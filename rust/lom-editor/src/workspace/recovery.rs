//! Autosave and recovery of the current editor session.
use super::*;

impl App {
    pub(super) fn recover(&mut self, path: &Path) {
        if recovery_owner_active(path) {
            self.error = Some("此恢复副本可能属于仍在运行的编辑器实例，暂不读取或移除。".into());
            return;
        }
        match Project::open(path) {
            Ok(mut project) => {
                let session = lom_core::load_json(path.join("session.json")).unwrap_or(json!({}));
                project.source = session["source"].as_str().map(PathBuf::from);
                if let Ok(paths) =
                    serde_json::from_value::<BTreeMap<String, PathBuf>>(session["paths"].clone())
                {
                    project.paths = paths;
                }
                if let Some(story_subdir) = session["story_subdir"].as_bool() {
                    project.story_subdir = story_subdir;
                }
                if let Some(has_manifest) = session["has_manifest"].as_bool() {
                    project.has_manifest = has_manifest;
                }
                self.install(project);
                self.saved = Snapshot {
                    manifest: json!({}),
                    scenario: None,
                    stories: BTreeMap::new(),
                    paths: BTreeMap::new(),
                    assets: std::sync::Arc::new(BTreeMap::new()),
                };
                self.document_dirty = true;
                self.current = session["current"]
                    .as_str()
                    .filter(|id| self.project.stories.contains_key(*id))
                    .unwrap_or(&self.current)
                    .to_owned();
                self.selected = (session["selected"].as_u64().unwrap_or(0) as usize).min(
                    self.story()["nodes"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or(1)
                        .saturating_sub(1),
                );
                self.recovery_candidates
                    .retain(|candidate| candidate != path);
                self.status = "已恢复创作副本，请检查并保存到原项目或新目录".into();
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    pub(super) fn clear_recovery(&mut self) {
        if self.recovery_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.recovery_dir);
        }
        self.recovery_paths.clear();
        self.recovery_assets.clear();
    }
    pub(super) fn idle_repaint_delay(&self) -> Duration {
        let mut delay = Duration::MAX;
        if self.needs_compile {
            delay = delay.min(Duration::from_millis(400).saturating_sub(self.last_edit.elapsed()));
        }
        if self.pending_history.is_some() {
            delay = delay.min(Duration::from_millis(650).saturating_sub(self.last_edit.elapsed()));
        }
        if self.dirty() {
            delay = delay.min(Duration::from_secs(30).saturating_sub(self.last_recovery.elapsed()));
            if self.auto_save_project
                && !self.saved.stories.is_empty()
                && self.save_target().is_some()
            {
                delay = delay.min(
                    Duration::from_secs(self.auto_save_seconds)
                        .saturating_sub(self.last_auto_save.elapsed()),
                );
            }
        }
        if self.audio.playing() {
            delay = delay.min(Duration::from_millis(200));
        }
        if self.screenshot.is_some() {
            delay = delay.min(Duration::from_millis(70));
        }
        delay
    }
    pub(super) fn autosave(&mut self) {
        if self.auto_save_project
            && self.dirty()
            && !self.saved.stories.is_empty()
            && self.last_auto_save.elapsed() >= Duration::from_secs(self.auto_save_seconds)
        {
            // Do not commit half-entered drafts or resolve a pending save/switch dialog.
            self.last_auto_save = Instant::now();
            if self.pending.is_none()
                && self.error.is_none()
                && !self.content_panel.has_pending()
                && !self.advanced.has_pending()
            {
                if let Some(path) = self.save_target() {
                    self.flush();
                    if self.save_at(&path, true) {
                        return;
                    }
                }
            }
        }
        if !self.dirty() || self.last_recovery.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.last_recovery = Instant::now();
        let mut p = self.project.clone();
        let pending_warning = self.content_panel.apply_pending_to_snapshot(&mut p).err();
        p.paths = self.recovery_paths.clone();
        p.story_subdir = true;
        // Only earlier successful writes establish ownership of recovery files.
        // Never borrow the original project's filenames or trust a preexisting directory.
        p.source = if self.recovery_paths.is_empty() {
            None
        } else {
            Some(
                self.recovery_dir
                    .canonicalize()
                    .unwrap_or_else(|_| self.recovery_dir.clone()),
            )
        };
        let result = (|| -> anyhow::Result<()> {
            p.save_to(&self.recovery_dir)?;
            let previous_paths = self.recovery_paths.clone();
            let previous_assets = self.recovery_assets.clone();
            self.recovery_paths.extend(p.paths.clone());
            self.recovery_assets.extend(p.assets.keys().cloned());
            // Undoing chapter creation/renaming must not resurrect an obsolete
            // chapter when recovery reopens the directory. Only remove owned files.
            for (id, filename) in previous_paths {
                if !p.paths.values().any(|path| path == &filename) {
                    let file = self.recovery_dir.join("story").join(&filename);
                    if file.exists() {
                        std::fs::remove_file(file)?;
                    }
                    self.recovery_paths.remove(&id);
                }
            }
            for name in previous_assets {
                if !p.assets.contains_key(&name) {
                    let file = self.recovery_dir.join(&name);
                    if file.exists() {
                        let file =
                            lom_core::package::resolve_confined_file(&self.recovery_dir, &file)?;
                        anyhow::ensure!(
                            file == self.recovery_dir.canonicalize()?.join(&name),
                            "恢复素材路径已改变，未移除：{name}"
                        );
                        std::fs::remove_file(file)?;
                    }
                    self.recovery_assets.remove(&name);
                }
            }
            let session = json!({"source":self.project.source,"paths":self.project.paths,"story_subdir":self.project.story_subdir,"has_manifest":self.project.has_manifest,"current":self.current,"selected":self.selected});
            lom_core::project::atomic_write(
                &self.recovery_dir.join("session.json"),
                &lom_core::stable_json(&session)?,
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            self.status = format!(
                "自动恢复副本写入失败。{}",
                crate::error_ui::text(&crate::error_ui::capture(e))
            );
        } else if let Some(error) = pending_warning {
            self.status = format!(
                "素材输入尚未完成。{}",
                crate::error_ui::text(&crate::error_ui::capture(error))
            );
        }
    }
}

pub(super) fn recovery_owner_active(path: &Path) -> bool {
    let Some(pid) = path
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse::<u32>().ok())
    else {
        return true;
    };
    #[cfg(not(windows))]
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        return std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .map(|out| !out.stdout.is_empty())
            .unwrap_or(true);
    }
    #[cfg(windows)]
    {
        let Some(stamp) = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split_once('-'))
            .and_then(|(_, stamp)| stamp.parse::<u128>().ok())
        else {
            return true;
        };
        windows_recovery_owner_active(pid, stamp)
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}
#[cfg(windows)]
pub(super) fn windows_recovery_owner_active(pid: u32, session_created_ms: u128) -> bool {
    use std::ffi::c_void;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn GetProcessTimes(
            handle: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: i32 = 87;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    const WINDOWS_TO_UNIX_EPOCH_MS: u64 = 11_644_473_600_000;

    // PID 0 is the system idle process, never a valid editor owner.
    if pid == 0 {
        return true;
    }
    // SAFETY: We only query this non-inherited process handle. OwnedHandle closes
    // every successful OpenProcess result, and the FILETIME outputs are valid.
    unsafe {
        let raw = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if raw.is_null() {
            // A missing PID is safe to recover; access denied and unknown errors
            // must keep a potentially active editor's recovery files protected.
            return std::io::Error::last_os_error().raw_os_error() != Some(ERROR_INVALID_PARAMETER);
        }
        let handle = OwnedHandle::from_raw_handle(raw);
        match WaitForSingleObject(handle.as_raw_handle(), 0) {
            WAIT_OBJECT_0 => false,
            WAIT_TIMEOUT => {
                let mut creation = FileTime::default();
                let mut exit = FileTime::default();
                let mut kernel = FileTime::default();
                let mut user = FileTime::default();
                if GetProcessTimes(
                    handle.as_raw_handle(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                ) == 0
                {
                    return true;
                }
                let ticks = (u64::from(creation.high) << 32) | u64::from(creation.low);
                // A process created after the session cannot own it: Windows
                // has reused an earlier editor's PID, including our own PID.
                (ticks / 10_000)
                    .checked_sub(WINDOWS_TO_UNIX_EPOCH_MS)
                    .map(|created_ms| u128::from(created_ms) <= session_created_ms)
                    .unwrap_or(true)
            }
            _ => true,
        }
    }
}
