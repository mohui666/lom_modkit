//! Saved project versions kept outside the project and crash-recovery directories.
use super::*;
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::ErrorKind,
};

pub(super) const MAX_COUNT: usize = 20;

pub(super) struct Entry {
    pub(super) path: PathBuf,
    timestamp: u128,
}

fn project_directory(base: &Path, source: &Path) -> Result<PathBuf> {
    let source = source.canonicalize()?;
    let key = Sha256::digest(source.as_os_str().as_encoded_bytes());
    Ok(base.join(format!("{key:x}")))
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions and other reparse points.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn checked_directory(path: &Path) -> Result<Option<PathBuf>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && !is_link(&metadata),
                "检查点目录必须是普通目录：{}",
                path.display()
            );
            Ok(Some(path.canonicalize()?))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法读取检查点目录 {}", path.display())),
    }
}

fn directory_lock(directory: &Path) -> Result<File> {
    let path = directory.join(".lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) => ensure!(
            metadata.is_file() && !is_link(&metadata),
            "检查点锁路径无效"
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    lock.lock().context("无法锁定检查点目录")?;
    Ok(lock)
}

fn timestamp(name: &str) -> Option<u128> {
    let (time, id) = name.strip_prefix("checkpoint-")?.split_once('-')?;
    uuid::Uuid::parse_str(id).ok()?;
    time.parse().ok()
}

fn entries(directory: &Path) -> Result<Vec<Entry>> {
    let mut items = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let Some(time) = entry.file_name().to_str().and_then(timestamp) else {
            continue;
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.is_dir() && !is_link(&metadata) && path.join("manifest.json").is_file() {
            items.push(Entry {
                path,
                timestamp: time,
            });
        }
    }
    items.sort_unstable_by(|left, right| right.timestamp.cmp(&left.timestamp));
    Ok(items)
}

pub(super) fn list(base: &Path, source: &Path) -> Result<Vec<Entry>> {
    let Some(base) = checked_directory(base)? else {
        return Ok(Vec::new());
    };
    match checked_directory(&project_directory(&base, source)?)? {
        Some(directory) => entries(&directory),
        None => Ok(Vec::new()),
    }
}

fn remove_entries(directory: &Path, items: impl IntoIterator<Item = Entry>) -> Result<()> {
    for entry in items {
        let path = checked_directory(&entry.path)?.context("检查点目录已移动")?;
        ensure!(
            path.parent() == Some(directory),
            "检查点路径超出所属工程目录"
        );
        fs::remove_dir_all(&path).with_context(|| format!("无法移除检查点 {}", path.display()))?;
    }
    Ok(())
}

pub(super) fn clear(base: &Path, source: &Path) -> Result<usize> {
    let Some(base) = checked_directory(base)? else {
        return Ok(0);
    };
    let Some(directory) = checked_directory(&project_directory(&base, source)?)? else {
        return Ok(0);
    };
    let _lock = directory_lock(&directory)?;
    let items = entries(&directory)?;
    let count = items.len();
    remove_entries(&directory, items)?;
    Ok(count)
}

pub(super) fn write(base: &Path, source: &Path, project: &mut Project, limit: usize) -> Result<()> {
    if limit == 0 {
        return Ok(());
    }
    fs::create_dir_all(base)?;
    let base = checked_directory(base)?.context("找不到检查点目录")?;
    let directory = project_directory(&base, source)?;
    fs::create_dir_all(&directory)?;
    let directory = checked_directory(&directory)?.context("找不到工程检查点目录")?;
    let staging = tempfile::Builder::new()
        .prefix(".writing-")
        .tempdir_in(&directory)?;
    project.source = None;
    project.story_subdir = true;
    project.save_to(staging.path())?;

    let _lock = directory_lock(&directory)?;
    let previous = entries(&directory)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let time = previous
        .first()
        .map_or(now, |entry| now.max(entry.timestamp.saturating_add(1)));
    let destination = directory.join(format!("checkpoint-{time}-{}", uuid::Uuid::new_v4()));
    fs::rename(staging.path(), destination)?;
    remove_entries(&directory, previous.into_iter().skip(limit - 1))
}

pub(super) enum Action {
    Open(PathBuf),
    Restore(PathBuf),
    Clear,
}

impl App {
    pub(super) fn checkpoint_controls(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let Some(source) = &self.project.source else {
            ui.small(crate::i18n::key("editor.checkpoints.unsaved"));
            return None;
        };
        let saving = self.saving();
        let entries = self.checkpoint_entries.get_or_insert_with(|| {
            list(&self.checkpoint_base, source).map_err(crate::error_ui::capture)
        });
        let entries = match entries {
            Ok(entries) => entries,
            Err(error) => {
                crate::error_ui::show_with_actions(ui, error);
                return None;
            }
        };
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::message(
                "editor.checkpoints.current",
                &[("count", entries.len().to_string())],
            ));
            if ui
                .add_enabled(
                    !saving && !entries.is_empty(),
                    egui::Button::new(crate::i18n::key("editor.checkpoints.clear")),
                )
                .clicked()
            {
                action = Some(Action::Clear);
            }
        });
        if !entries.is_empty() {
            ui.collapsing(crate::i18n::key("editor.checkpoints.history"), |ui| {
                for (index, entry) in entries.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(crate::i18n::message(
                            "editor.checkpoints.entry",
                            &[
                                ("index", (index + 1).to_string()),
                                ("age", age_label(entry.timestamp)),
                            ],
                        ));
                        if ui
                            .button(crate::i18n::key("editor.checkpoints.open_copy"))
                            .clicked()
                        {
                            action = Some(Action::Open(entry.path.clone()));
                        }
                        if ui
                            .add_enabled(
                                !saving,
                                egui::Button::new(crate::i18n::key("editor.checkpoints.restore")),
                            )
                            .clicked()
                        {
                            action = Some(Action::Restore(entry.path.clone()));
                        }
                    });
                }
            });
        }
        action
    }

    pub(super) fn restore_checkpoint(&mut self, path: &Path) {
        let project = match Project::open(path) {
            Ok(project) => project,
            Err(error) => {
                self.error = Some(crate::error_ui::capture(
                    error.context(crate::i18n::key("editor.checkpoints.open_failed")),
                ));
                return;
            }
        };
        if !self.apply_pending_drafts() {
            return;
        }
        self.track_save_snapshot();
        self.flush();
        let checkpoint = snapshot(&project);
        if checkpoint != self.last {
            self.pending_history = Some(self.last.clone());
            self.flush();
            self.redo.clear();
            self.restore(checkpoint);
            self.stop_preview();
            self.last_auto_save = Instant::now();
        }
        self.status = crate::i18n::key("editor.checkpoints.restored");
    }
}

fn age_label(timestamp: u128) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let minutes = now.saturating_sub(timestamp) / 60_000_000_000;
    let (key, count) = match minutes {
        0 => return crate::i18n::key("editor.checkpoints.just_saved"),
        1..60 => ("editor.checkpoints.minutes_ago", minutes),
        60..1440 => ("editor.checkpoints.hours_ago", minutes / 60),
        _ => ("editor.checkpoints.days_ago", minutes / 1440),
    };
    crate::i18n::message(key, &[("count", count.to_string())])
}
