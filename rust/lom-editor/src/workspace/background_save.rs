//! One writer keeps disk IO and document reconstruction outside the UI thread.
use super::*;
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, TryRecvError},
};

#[derive(Default)]
pub(super) struct State {
    active: Option<Task>,
    queued: VecDeque<SaveRequest>,
    project_generation: u64,
    continuing: bool,
    discard: bool,
    clear_recovery: bool,
    recovery_revision: Option<RecoveryRevision>,
    repaint: Option<egui::Context>,
}

struct RecoveryRevision {
    document: u64,
    pending_content: bool,
}

struct SaveRequest {
    path: PathBuf,
    automatic: bool,
}

struct Task {
    generation: u64,
    kind: TaskKind,
    result: Receiver<Completion>,
}

enum TaskKind {
    Save {
        snapshot: Snapshot,
        path: PathBuf,
        automatic: bool,
    },
    Recovery {
        revision: u64,
        pending_content: bool,
    },
    ClearCheckpoints,
}

enum Completion {
    Save(Result<SavedProject, String>),
    Recovery(recovery::WrittenRecovery),
    ClearCheckpoints(Result<usize, String>),
}

struct SavedProject {
    metadata: ProjectMetadata,
    checkpoint_error: Option<String>,
}

pub(super) struct ProjectMetadata {
    pub(super) source: Option<PathBuf>,
    pub(super) paths: BTreeMap<String, PathBuf>,
    pub(super) story_subdir: bool,
    pub(super) has_manifest: bool,
}

impl ProjectMetadata {
    pub(super) fn capture(project: &Project) -> Self {
        Self {
            source: project.source.clone(),
            paths: project.paths.clone(),
            story_subdir: project.story_subdir,
            has_manifest: project.has_manifest,
        }
    }
}

pub(super) fn project_from_snapshot(snapshot: &Snapshot, metadata: ProjectMetadata) -> Project {
    Project {
        manifest: snapshot.manifest.clone(),
        scenario: snapshot.scenario.clone(),
        stories: snapshot
            .stories
            .iter()
            .map(|(id, story)| (id.clone(), story.to_value()))
            .collect::<BTreeMap<_, _>>()
            .into(),
        assets: snapshot.assets.as_ref().clone(),
        source: metadata.source,
        paths: metadata.paths,
        story_subdir: metadata.story_subdir,
        has_manifest: metadata.has_manifest,
    }
}

impl App {
    pub(super) fn saving(&self) -> bool {
        self.background_save.active.is_some() || !self.background_save.queued.is_empty()
    }

    pub(super) fn waiting_to_continue(&self) -> bool {
        self.background_save.continuing
    }

    pub(super) fn begin_pending_action(&mut self, action: Pending) {
        self.background_save.continuing = self.saving() && !self.dirty();
        self.background_save.discard = false;
        self.pending = Some(action);
    }

    pub(super) fn cancel_pending_action(&mut self) {
        self.pending = None;
        self.background_save.continuing = false;
        self.background_save.discard = false;
    }

    pub(super) fn save_and_continue(&mut self) {
        if self.save(false) {
            self.background_save.continuing = true;
            self.background_save.discard = false;
        }
    }

    pub(super) fn discard_and_continue(&mut self, ctx: &egui::Context) {
        // The active write cannot be interrupted midway through a project. Wait
        // before deleting recovery files or replacing the live editing session.
        self.background_save.queued.clear();
        if self.saving() {
            self.background_save.continuing = true;
            self.background_save.discard = true;
        } else if let Some(action) = self.pending.take() {
            self.perform(action, ctx);
        }
    }

    pub(super) fn reset_save_project(&mut self) {
        self.background_save.project_generation =
            self.background_save.project_generation.wrapping_add(1);
        self.background_save.queued.clear();
        self.reset_recovery_tracking();
        self.cancel_pending_action();
    }

    pub(super) fn reset_recovery_tracking(&mut self) {
        self.background_save.recovery_revision = None;
    }

    pub(super) fn recovery_needs_refresh(&self) -> bool {
        match &self.background_save.recovery_revision {
            Some(recovery) => {
                recovery.document != self.document_revision
                    || recovery.pending_content
                    || self.content_panel.has_pending()
            }
            None => self.dirty(),
        }
    }

    pub(super) fn defer_recovery_clear(&mut self) -> bool {
        if self.background_save.active.is_some() {
            self.background_save.clear_recovery = true;
            true
        } else {
            false
        }
    }

    pub(super) fn queue_project_save(&mut self, path: &Path, automatic: bool) -> bool {
        let request = SaveRequest {
            path: path.to_owned(),
            automatic,
        };
        if self.background_save.active.is_some() {
            // Repeated Ctrl+S requests need only the latest state, and must use
            // the ownership baseline produced by the writer already running.
            if self
                .background_save
                .queued
                .back()
                .is_some_and(|queued| queued.path == request.path)
            {
                *self.background_save.queued.back_mut().unwrap() = request;
            } else {
                self.background_save.queued.push_back(request);
            }
            self.status = "正在保存；完成后将保存最新修改".into();
            true
        } else {
            self.start_project_save(request)
        }
    }

    pub(super) fn track_save_snapshot(&mut self) {
        // A caller can add/remove content directly through the core API. Check
        // the small key set at a save boundary so a same-count rename cannot
        // leave the shared asset snapshot stale; do not scan bytes every frame.
        self.asset_dirty |= !self.last.assets.keys().eq(self.project.assets.keys());
        self.track();
    }

    fn start_project_save(&mut self, request: SaveRequest) -> bool {
        self.track_save_snapshot();
        self.flush();
        let snapshot = self.last.clone();
        let worker_snapshot = snapshot.clone();
        let baseline = self.saved.clone();
        let metadata = ProjectMetadata::capture(&self.project);
        let path = request.path.clone();
        let checkpoint_limit = if request.automatic {
            self.auto_save_checkpoints
        } else {
            0
        };
        let checkpoint_base = self.checkpoint_base.clone();
        let repaint = self.background_save.repaint.clone();
        let (send, result) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("project-save".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<SavedProject> {
                    let mut project = project_from_snapshot(&worker_snapshot, metadata);
                    crate::persistence::save_project(
                        &mut project,
                        &path,
                        baseline.assets.as_ref(),
                        &baseline.stories,
                        &baseline.paths,
                    )?;
                    let metadata = ProjectMetadata::capture(&project);
                    let checkpoint_error =
                        checkpoints::write(&checkpoint_base, &path, &mut project, checkpoint_limit)
                            .err()
                            .map(crate::error_ui::capture);
                    Ok(SavedProject {
                        metadata,
                        checkpoint_error,
                    })
                })()
                .map_err(crate::error_ui::capture);
                // The reconstructed document and byte buffers are dropped on
                // this thread before the UI receives its small completion.
                let _ = send.send(Completion::Save(result));
                if let Some(ctx) = repaint {
                    ctx.request_repaint();
                }
            });
        match worker {
            Ok(_) => {
                self.background_save.active = Some(Task {
                    generation: self.background_save.project_generation,
                    kind: TaskKind::Save {
                        snapshot,
                        path: request.path.clone(),
                        automatic: request.automatic,
                    },
                    result,
                });
                self.status = format!("正在保存全部章节 · {}", request.path.display());
                true
            }
            Err(error) => {
                self.error = Some(format!("无法启动保存；修改仍保留在编辑器中。\n{error}"));
                false
            }
        }
    }

    pub(super) fn clear_checkpoints(&mut self) {
        let Some(source) = self.project.source.clone() else {
            return;
        };
        if self.saving() {
            return;
        }
        let base = self.checkpoint_base.clone();
        let repaint = self.background_save.repaint.clone();
        let (send, result) = mpsc::channel();
        match std::thread::Builder::new()
            .name("clear-checkpoints".into())
            .spawn(move || {
                let result = checkpoints::clear(&base, &source).map_err(crate::error_ui::capture);
                let _ = send.send(Completion::ClearCheckpoints(result));
                if let Some(ctx) = repaint {
                    ctx.request_repaint();
                }
            }) {
            Ok(_) => {
                self.background_save.active = Some(Task {
                    generation: self.background_save.project_generation,
                    kind: TaskKind::ClearCheckpoints,
                    result,
                });
                self.settings_error = None;
            }
            Err(error) => self.settings_error = Some(format!("无法启动检查点清除。{error}")),
        }
    }

    pub(super) fn start_recovery_write(&mut self, input: recovery::RecoveryWrite) {
        let repaint = self.background_save.repaint.clone();
        let revision = self.document_revision;
        let pending_content = input.pending_content.is_some();
        let (send, result) = mpsc::channel();
        match std::thread::Builder::new()
            .name("project-recovery".into())
            .spawn(move || {
                let result = recovery::write_recovery(input);
                let _ = send.send(Completion::Recovery(result));
                if let Some(ctx) = repaint {
                    ctx.request_repaint();
                }
            }) {
            Ok(_) => {
                self.background_save.active = Some(Task {
                    generation: self.background_save.project_generation,
                    kind: TaskKind::Recovery {
                        revision,
                        pending_content,
                    },
                    result,
                });
            }
            Err(error) => self.status = format!("自动恢复副本写入失败。{error}"),
        }
    }

    pub(super) fn poll_background_save(&mut self, ctx: &egui::Context) {
        self.background_save.repaint = Some(ctx.clone());
        let Some(task) = &self.background_save.active else {
            return;
        };
        let result = match task.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        let task = self.background_save.active.take().unwrap();
        let mut save_failed = false;
        if task.generation == self.background_save.project_generation {
            match task.kind {
                TaskKind::Save {
                    mut snapshot,
                    path,
                    automatic,
                } => {
                    let result = match result {
                        Some(Completion::Save(result)) => result,
                        _ => Err("后台保存任务中断；修改仍保留在编辑器中。".into()),
                    };
                    match result {
                        Ok(SavedProject {
                            metadata,
                            checkpoint_error,
                        }) => {
                            // Track edits made after the worker's snapshot before
                            // applying path metadata. Never replace live content.
                            self.track();
                            for (id, filename) in &metadata.paths {
                                if self.project.stories.contains_key(id)
                                    && self.project.paths.get(id) == snapshot.paths.get(id)
                                {
                                    self.project.paths.insert(id.clone(), filename.clone());
                                }
                            }
                            self.project.source = metadata.source;
                            self.checkpoint_entries = None;
                            self.project.has_manifest = metadata.has_manifest;
                            self.last.paths = self.project.paths.clone();
                            snapshot.paths = metadata.paths;
                            self.saved = snapshot;
                            self.document_dirty = self.last != self.saved;
                            self.last_auto_save = Instant::now();
                            self.remember_source();
                            if !self.dirty() {
                                self.clear_recovery();
                            } else {
                                // A new save location also changes recovery's
                                // source metadata for the subsequent edits.
                                self.reset_recovery_tracking();
                            }
                            self.status = if automatic {
                                crate::i18n::message(
                                    "editor.autosave.saved",
                                    &[
                                        ("count", self.saved.stories.len().to_string()),
                                        ("path", path.display().to_string()),
                                    ],
                                )
                            } else {
                                format!(
                                    "已保存全部 {} 个章节 · {}",
                                    self.saved.stories.len(),
                                    path.display()
                                )
                            };
                            if self.dirty() {
                                self.status.push_str("；后续修改尚未保存");
                            }
                            if let Some(error) = checkpoint_error {
                                self.error = Some(crate::i18n::message(
                                    "editor.checkpoints.write_failed",
                                    &[("error", crate::error_ui::text(&error))],
                                ));
                            }
                        }
                        Err(error) => {
                            save_failed = true;
                            self.error = Some(if automatic {
                                crate::i18n::message(
                                    "editor.autosave.failed",
                                    &[("path", path.display().to_string()), ("error", error)],
                                )
                            } else {
                                format!("保存失败；修改仍保留在编辑器中。\n{error}")
                            });
                        }
                    }
                }
                TaskKind::Recovery {
                    revision,
                    pending_content,
                } => match result {
                    Some(Completion::Recovery(written)) => {
                        self.recovery_paths = written.paths;
                        self.recovery_assets = written.assets;
                        if written.result.is_ok() {
                            self.background_save.recovery_revision = Some(RecoveryRevision {
                                document: revision,
                                pending_content: pending_content
                                    && written.pending_warning.is_none(),
                            });
                        }
                        if let Err(error) = written.result {
                            self.status =
                                format!("自动恢复副本写入失败。{}", crate::error_ui::text(&error));
                        } else if let Some(error) = written.pending_warning {
                            self.status =
                                format!("素材输入尚未完成。{}", crate::error_ui::text(&error));
                        }
                    }
                    _ => self.status = "自动恢复副本写入失败。后台任务中断。".into(),
                },
                TaskKind::ClearCheckpoints => {
                    self.checkpoint_entries = None;
                    let result = match result {
                        Some(Completion::ClearCheckpoints(result)) => result,
                        _ => Err("后台任务中断".into()),
                    };
                    match result {
                        Ok(count) => {
                            self.status = crate::i18n::message(
                                "editor.checkpoints.cleared",
                                &[("count", count.to_string())],
                            )
                        }
                        Err(error) => {
                            let error = crate::i18n::message(
                                "editor.checkpoints.clear_failed",
                                &[("error", crate::error_ui::text(&error))],
                            );
                            if self.settings_open {
                                self.settings_error = Some(error);
                            } else {
                                self.error = Some(error);
                            }
                        }
                    }
                }
            }
        }
        if std::mem::take(&mut self.background_save.clear_recovery) {
            self.clear_recovery();
        }
        if save_failed {
            self.background_save.queued.clear();
            self.background_save.continuing = false;
            self.background_save.discard = false;
            return;
        }
        if let Some(request) = self.background_save.queued.pop_front() {
            if !self.start_project_save(request) {
                self.background_save.queued.clear();
                self.background_save.continuing = false;
                self.background_save.discard = false;
            }
            return;
        }
        if self.pending.is_some() && (self.background_save.discard || !self.dirty()) {
            let action = self.pending.take().unwrap();
            self.background_save.continuing = false;
            self.background_save.discard = false;
            self.perform(action, ctx);
        } else {
            // New edits or unfinished input require another decision. A finished
            // snapshot alone is never permission to discard those newer edits.
            self.background_save.continuing = false;
            self.background_save.discard = false;
        }
    }
}
