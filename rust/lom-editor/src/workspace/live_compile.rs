//! Refresh diagnostics from an immutable snapshot without blocking input.
use super::*;
use lom_core::{
    project::Stories,
    release::{self, PreflightIssue},
};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(super) const DEBOUNCE: Duration = Duration::from_millis(400);

pub(super) struct Task {
    revision: u64,
    chapter: String,
    full: bool,
    result: Receiver<Output>,
}

struct Output {
    flags: Value,
    diagnostics: Vec<PreflightIssue>,
    lua: Option<String>,
}

fn project_flags(stories: &Stories) -> Value {
    json!(stories
        .values()
        .flat_map(|story| story["nodes"].as_array().into_iter().flatten())
        .filter(|node| node["type"] == "flag")
        .filter_map(|node| node["flag"].as_str())
        .collect::<BTreeSet<_>>())
}

fn compiler_issues(stories: &Stories, catalog: &Value) -> Vec<PreflightIssue> {
    let mut diagnostics = Vec::new();
    for (id, story) in stories {
        diagnostics.extend(release::compiler_issues_with_catalog(id, story, catalog));
        for (node, character) in release::find_stage_issues(story) {
            diagnostics.push(PreflightIssue {
                severity: "warning".into(),
                code: "stage_missing".into(),
                story_id: id.clone(),
                node_id: node,
                message: crate::i18n::message(
                    "usability.problems.stage_missing",
                    &[("character", character)],
                ),
                fixable: false,
            });
        }
    }
    diagnostics
}

fn display_lua(result: anyhow::Result<String>) -> String {
    result.unwrap_or_else(|error| crate::error_ui::text(&crate::error_ui::capture(error)))
}

impl App {
    fn refresh_branch_labels(&mut self) {
        if self.story()["vanilla_import"]["schema"] == 2 {
            if let Some(story) = self.project.stories.get_mut(&self.current) {
                lom_core::native_flow::refresh_branch_labels(story);
            }
        }
    }

    pub(super) fn compile(&mut self) {
        self.refresh_branch_labels();
        self.catalog.data["project_flags"] = project_flags(&self.project.stories);
        if self.preflight_tracked {
            self.refresh_full_problems();
        } else {
            self.diagnostics = compiler_issues(&self.project.stories, &self.catalog.data);
        }
        self.lua = display_lua(compile_with_assets(&self.project, self.story()));
        self.needs_compile = false;
    }

    pub(super) fn update_live_compile(&mut self, ctx: &egui::Context) {
        let show_lua = self.story_panels_visible() && matches!(self.view, View::Lua | View::Checks);
        if let Some(task) = self.live_compile.take() {
            let current = self.needs_compile
                && task.revision == self.document_revision
                && task.chapter == self.current
                && task.full == self.preflight_tracked;
            match task.result.try_recv() {
                Ok(output) if current && (!show_lua || output.lua.is_some()) => {
                    self.catalog.data["project_flags"] = output.flags;
                    self.diagnostics = output.diagnostics;
                    if task.full {
                        self.error_issues = self
                            .diagnostics
                            .iter()
                            .filter(|issue| issue.severity == "error")
                            .cloned()
                            .collect();
                    }
                    if let Some(lua) = output.lua {
                        self.lua = lua;
                    }
                    self.needs_compile = false;
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => {
                    self.live_compile = Some(task);
                    return;
                }
                Err(TryRecvError::Disconnected) if current => {
                    self.error = Some("后台剧情检查意外退出".into());
                    self.needs_compile = false;
                    ctx.request_repaint();
                }
                _ => {}
            }
        }
        if !self.needs_compile || (self.frame != 1 && self.last_edit.elapsed() < DEBOUNCE) {
            return;
        }
        self.refresh_branch_labels();
        self.track();
        let snapshot = self.last.clone();
        let full = self.preflight_tracked;
        let metadata = full.then(|| background_save::ProjectMetadata::capture(&self.project));
        let chapter = self.current.clone();
        let repaint = ctx.clone();
        let (sender, result) = mpsc::channel();
        match std::thread::Builder::new()
            .name("lom-live-compile".into())
            .spawn(move || {
                let (flags, diagnostics) = if let Some(metadata) = metadata {
                    let project = background_save::project_from_snapshot(&snapshot, metadata);
                    (
                        project_flags(&project.stories),
                        release::run_preflight(
                            &project,
                            release::Profile::Editing,
                            env!("CARGO_PKG_VERSION"),
                        ),
                    )
                } else {
                    let catalog = lom_core::validate::catalog_with_assets(&snapshot.assets).ok();
                    let catalog = catalog
                        .as_ref()
                        .unwrap_or_else(|| lom_core::validate::editor_data());
                    let stories = snapshot.materialize_stories();
                    let flags = project_flags(&stories);
                    (flags, compiler_issues(&stories, catalog))
                };
                let lua = show_lua.then(|| {
                    let story = snapshot
                        .stories
                        .get(&chapter)
                        .map_or(Value::Null, |story| story.to_value());
                    display_lua(project_io::compile_lua(
                        &story,
                        &snapshot.manifest,
                        &snapshot.assets,
                    ))
                });
                let _ = sender.send(Output {
                    flags,
                    diagnostics,
                    lua,
                });
                repaint.request_repaint();
            }) {
            Ok(_) => {
                self.live_compile = Some(Task {
                    revision: self.document_revision,
                    chapter: self.current.clone(),
                    full,
                    result,
                });
            }
            Err(error) => {
                self.error = Some(crate::error_ui::capture(error));
                self.needs_compile = false;
                ctx.request_repaint();
            }
        }
    }
}
