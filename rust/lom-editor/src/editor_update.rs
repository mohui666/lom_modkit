//! Manual GitHub release checks and an in-place Windows bundle update.
use anyhow::{ensure, Context, Result};
use eframe::egui;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::process::Command;
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RELEASES: &str = "https://github.com/mohui666/lom_modkit/releases";
const LATEST: &str = "https://api.github.com/repos/mohui666/lom_modkit/releases/latest";
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
use lom_core::editor_update::validate_install;
use lom_core::editor_update::{owned_file, PROGRAM_FILES};

#[derive(Clone, Debug)]
struct Release {
    version: String,
    notes: String,
    url: String,
    files: Vec<Download>,
    repair_needed: bool,
    embedded_components: bool,
    self_installing: bool,
}

#[derive(Clone, Debug)]
struct Download {
    path: String,
    url: String,
    size: u64,
    digest: String,
}

struct FileProgress {
    path: String,
    size: u64,
    received: Arc<AtomicU64>,
    verified: AtomicBool,
}

struct UpdateProgress {
    files: Vec<FileProgress>,
    started: Instant,
}

impl UpdateProgress {
    fn new(release: &Release) -> Self {
        Self {
            files: release
                .files
                .iter()
                .map(|file| FileProgress {
                    path: file.path.clone(),
                    size: file.size,
                    received: Arc::new(AtomicU64::new(0)),
                    verified: AtomicBool::new(false),
                })
                .collect(),
            started: Instant::now(),
        }
    }
}

fn byte_size(bytes: f64) -> String {
    if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KiB", bytes / 1024.0)
    } else {
        format!("{:.1} MiB", bytes / (1024.0 * 1024.0))
    }
}

impl Release {
    fn can_update(&self) -> bool {
        if self.self_installing {
            self.files.len() == 1 && self.files[0].path == "lom-editor.exe"
        } else {
            self.files.len() == PROGRAM_FILES.len()
        }
    }

    fn inspect_installation(&mut self, root: &Path) -> Result<()> {
        self.repair_needed = false;
        if self.version != env!("CARGO_PKG_VERSION") || !self.can_update() {
            return Ok(());
        }
        let paths = self
            .files
            .iter()
            .filter(|file| !self.embedded_components || file.path == "lom-editor.exe")
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        validate_install(root, &paths)?;
        self.repair_needed = self
            .files
            .iter()
            .filter(|file| !self.embedded_components || file.path == "lom-editor.exe")
            .any(|file| verify_file(&root.join(&file.path), file).is_err());
        Ok(())
    }
}

fn version(text: &str) -> Result<[u64; 3]> {
    let parts = text
        .strip_prefix('v')
        .unwrap_or(text)
        .split('.')
        .collect::<Vec<_>>();
    ensure!(parts.len() == 3, "无法识别发布版本：{text}");
    let mut result = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        ensure!(
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()),
            "无法识别发布版本：{text}"
        );
        result[index] = part.parse()?;
    }
    Ok(result)
}

fn parse_release(value: &Value) -> Result<Release> {
    if value.get("tag_name").is_some() {
        return parse_github_release(value);
    }
    let number = value["version"].as_str().context("更新清单缺少版本")?;
    version(number)?;
    let tag = format!("v{}", number.strip_prefix('v').unwrap_or(number));
    let release_url = format!("{RELEASES}/tag/{tag}");

    let mut files = Vec::new();
    for (name, path) in PROGRAM_FILES {
        let expected = format!("{RELEASES}/download/{tag}/{name}");
        if let Some(asset) = value["files"].as_array().and_then(|assets| {
            assets.iter().find(|asset| {
                asset["name"] == name
                    && asset["size"]
                        .as_u64()
                        .is_some_and(|n| n > 0 && n <= MAX_DOWNLOAD)
            })
        }) {
            if let Some(digest) = asset["sha256"]
                .as_str()
                .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                files.push(Download {
                    path: path.into(),
                    url: expected,
                    size: asset["size"].as_u64().unwrap(),
                    digest: digest.into(),
                });
            }
        }
    }
    Ok(Release {
        version: number.strip_prefix('v').unwrap_or(number).into(),
        notes: value["notes"].as_str().unwrap_or("").into(),
        url: release_url,
        files,
        repair_needed: false,
        embedded_components: value["embedded_components"].as_bool().unwrap_or(false),
        self_installing: false,
    })
}

fn parse_github_release(value: &Value) -> Result<Release> {
    ensure!(
        value["draft"] == false && value["prerelease"] == false,
        "只支持正式发布版本"
    );
    let number = value["tag_name"].as_str().context("发布信息缺少版本")?;
    let parsed = version(number)?;
    let number = number.strip_prefix('v').unwrap_or(number);
    let mut files = vec![];
    // Earlier editors cannot act as their own native installation helper.
    if parsed >= [1, 4, 19] {
        if let Some(asset) = value["assets"]
            .as_array()
            .filter(|items| parsed >= [1, 5, 0] || items.len() == 1)
            .filter(|items| {
                items
                    .iter()
                    .filter(|asset| asset["name"] == "lom-editor.exe")
                    .count()
                    == 1
            })
            .and_then(|items| items.iter().find(|a| a["name"] == "lom-editor.exe"))
        {
            if let (Some(size), Some(digest)) = (
                asset["size"]
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= MAX_DOWNLOAD),
                asset["digest"]
                    .as_str()
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())),
            ) {
                files.push(Download {
                    path: "lom-editor.exe".into(),
                    url: format!("{RELEASES}/download/v{number}/lom-editor.exe"),
                    size,
                    digest: digest.into(),
                });
            }
        }
    }
    Ok(Release {
        version: number.into(),
        notes: value["body"].as_str().unwrap_or("").into(),
        url: format!("{RELEASES}/tag/v{number}"),
        files,
        repair_needed: false,
        embedded_components: true,
        self_installing: true,
    })
}

fn download(url: &str, destination: &Path, limit: u64, cancel: &AtomicBool) -> Result<()> {
    crate::http_download::download(url, destination, limit, cancel)
}

fn check(cancel: &AtomicBool) -> Result<Release> {
    let temp = lom_core::workdir::create()?;
    let response = temp.path().join("release.json");
    download(LATEST, &response, 1024 * 1024, cancel)?;
    parse_release(&serde_json::from_slice(&fs::read(response)?)?)
}

struct Prepared {
    workspace: tempfile::TempDir,
    files: Vec<String>,
    installer: &'static str,
}

fn verify_file(path: &Path, file: &Download) -> Result<()> {
    ensure!(
        fs::metadata(path)?.len() == file.size,
        "更新文件长度不正确：{}",
        file.path
    );
    let actual = format!("{:x}", Sha256::digest(fs::read(path)?));
    ensure!(
        actual.eq_ignore_ascii_case(&file.digest),
        "更新文件校验失败：{}",
        file.path
    );
    Ok(())
}

fn prepare(release: &Release, cancel: &AtomicBool, progress: &UpdateProgress) -> Result<Prepared> {
    ensure!(
        release.can_update(),
        "此发布缺少可直接更新的程序附件或校验信息"
    );
    let workspace = lom_core::workdir::create()?;
    let first_error = std::sync::Mutex::new(None);
    thread::scope(|scope| {
        let mut workers = Vec::new();
        for (file, progress) in release.files.iter().zip(&progress.files) {
            let workspace = workspace.path();
            let first_error = &first_error;
            workers.push(scope.spawn(move || {
                let result = (|| -> Result<()> {
                    ensure!(owned_file(&file.path), "拒绝下载非程序文件：{}", file.path);
                    let destination = workspace.join("bundle").join(&file.path);
                    fs::create_dir_all(destination.parent().context("无效更新路径")?)?;
                    crate::http_download::download_update(
                        &file.url,
                        &destination,
                        file.size,
                        MAX_DOWNLOAD,
                        cancel,
                        &progress.received,
                    )
                    .with_context(|| format!("下载更新文件失败：{}", file.path))?;
                    verify_file(&destination, file)?;
                    progress.verified.store(true, Ordering::Relaxed);
                    Ok(())
                })();
                if let Err(error) = result {
                    if !cancel.swap(true, Ordering::Relaxed) {
                        *first_error.lock().unwrap() = Some(error);
                    }
                }
            }));
        }
        for worker in workers {
            if worker.join().is_err() {
                cancel.store(true, Ordering::Relaxed);
                let mut error = first_error.lock().unwrap();
                if error.is_none() {
                    *error = Some(anyhow::anyhow!("更新下载线程意外中断"));
                }
            }
        }
    });
    if let Some(error) = first_error.into_inner().unwrap() {
        return Err(error);
    }
    ensure!(!cancel.load(Ordering::Relaxed), "更新已取消");
    let mut files: Vec<_> = release
        .files
        .iter()
        .filter(|file| !release.embedded_components || file.path == "lom-editor.exe")
        .map(|file| file.path.clone())
        .collect();
    files.sort_by_key(|name| name == "lom-editor.exe");
    Ok(Prepared {
        workspace,
        files,
        installer: if release.self_installing {
            "lom-editor.exe"
        } else {
            "lomc.exe"
        },
    })
}

enum Job {
    Checked(Release),
    Prepared(Prepared),
}

#[derive(Default)]
pub struct Updater {
    pub open: bool,
    completed: bool,
    release: Option<Release>,
    prepared: Option<Prepared>,
    job: Option<mpsc::Receiver<Result<Job, String>>>,
    downloading: bool,
    silent: bool,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    progress: Option<Arc<UpdateProgress>>,
    last_received: u64,
    last_activity: Option<Instant>,
    cancelling: bool,
}

impl Updater {
    pub fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("更新任务在退出时意外中断");
            }
        }
        self.job = None;
        self.prepared = None;
        self.downloading = false;
        self.progress = None;
    }

    pub fn show_completed(&mut self) {
        self.completed = true;
        self.open = true;
    }

    pub fn check(&mut self, ctx: &egui::Context) {
        self.start_check(ctx, false);
    }

    pub fn check_on_startup(&mut self, ctx: &egui::Context) {
        self.start_check(ctx, true);
    }

    fn start_check(&mut self, ctx: &egui::Context, silent: bool) {
        if self.job.is_some() {
            if !silent {
                self.open = true;
                self.silent = false;
            }
            return;
        }
        self.open = !silent;
        self.silent = silent;
        self.completed = false;
        self.release = None;
        self.prepared = None;
        self.error = None;
        self.downloading = false;
        self.cancelling = false;
        self.progress = None;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        let ctx = ctx.clone();
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        self.worker = Some(thread::spawn(move || {
            let result = check(&cancel).and_then(|mut release| {
                if cfg!(windows) {
                    let exe = std::env::current_exe()?;
                    release
                        .inspect_installation(exe.parent().context("无法确定编辑器安装位置")?)?;
                }
                Ok(Job::Checked(release))
            });
            let _ = tx.send(result.map_err(|e| crate::error_ui::capture(e)));
            if !cancel.load(Ordering::Relaxed) {
                ctx.request_repaint();
            }
        }));
    }

    pub fn download(&mut self, ctx: &egui::Context) {
        if self.job.is_some() {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        self.open = true;
        self.downloading = true;
        self.silent = false;
        self.error = None;
        self.cancelling = false;
        self.last_received = 0;
        self.last_activity = None;
        let progress = Arc::new(UpdateProgress::new(&release));
        self.progress = Some(progress.clone());
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        let ctx = ctx.clone();
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        self.worker = Some(thread::spawn(move || {
            let _ = tx.send(
                prepare(&release, &cancel, &progress)
                    .map(Job::Prepared)
                    .map_err(|e| crate::error_ui::capture(e)),
            );
            if !cancel.load(Ordering::Relaxed) {
                ctx.request_repaint();
            }
        }));
    }

    /// True only after a confirmed update has finished downloading and validating.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.job else { return false };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err("更新任务意外中断".into()),
        };
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                self.job = None;
                self.downloading = false;
                self.error = Some("更新任务意外中断".into());
                return false;
            }
        }
        self.job = None;
        self.downloading = false;
        self.progress = None;
        // A cancel click must also win a race with the final verified download.
        if self.cancelling {
            self.error = Some(crate::i18n::key("common.update.cancelled"));
            return false;
        }
        match result {
            Ok(Job::Checked(release)) => {
                self.open = !self.silent
                    || release.repair_needed
                    || version(&release.version).ok() > version(env!("CARGO_PKG_VERSION")).ok();
                self.release = Some(release);
            }
            Ok(Job::Prepared(prepared)) => {
                self.prepared = Some(prepared);
                return true;
            }
            Err(error) => {
                if self.silent {
                    self.open = false;
                } else {
                    self.error = Some(error);
                }
            }
        }
        false
    }

    fn cancel_job(&mut self) {
        self.cancelling = true;
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn show_progress(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::{key, message};
        let Some(progress) = &self.progress else {
            return;
        };
        let received: Vec<_> = progress
            .files
            .iter()
            .map(|file| file.received.load(Ordering::Relaxed).min(file.size))
            .collect();
        let total: u64 = progress.files.iter().map(|file| file.size).sum();
        let downloaded: u64 = received.iter().sum();
        let now = Instant::now();
        if downloaded != self.last_received {
            self.last_received = downloaded;
            self.last_activity = Some(now);
        }
        let elapsed = progress.started.elapsed();
        let speed = downloaded as f64 / elapsed.as_secs_f64().max(0.001);
        ui.add(egui::ProgressBar::new(downloaded as f32 / total.max(1) as f32).show_percentage());
        ui.label(message(
            "common.update.transfer",
            &[
                ("received", byte_size(downloaded as f64)),
                ("total", byte_size(total as f64)),
                ("speed", byte_size(speed)),
                (
                    "elapsed",
                    format!("{}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60),
                ),
            ],
        ));
        for (file, received) in progress.files.iter().zip(received) {
            let status = if file.verified.load(Ordering::Relaxed) {
                "common.update.file_ready"
            } else if received == file.size {
                "common.update.file_verifying"
            } else if received == 0 {
                "common.update.file_connecting"
            } else {
                "common.update.file_downloading"
            };
            ui.small(format!(
                "{} · {} / {} · {}",
                file.path,
                byte_size(received as f64),
                byte_size(file.size as f64),
                key(status)
            ));
        }
        if downloaded < total
            && now.duration_since(self.last_activity.unwrap_or(progress.started))
                >= Duration::from_secs(15)
        {
            ui.label(key("common.update.no_data"));
        }
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    /// Returns true when the user chooses “update and restart”.
    pub fn dialog(&mut self, ctx: &egui::Context) -> bool {
        use crate::i18n::tr;
        let mut update = false;
        let mut close = false;
        let mut offered = false;
        egui::Modal::new(egui::Id::new("editor-update"))
            .frame(crate::shell::dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(480.0_f32.min(ctx.content_rect().width() - 60.0));
                if self.job.is_some() {
                    ui.heading(tr("检查更新"));
                } else {
                    close |= crate::shell::dialog_header(
                        ui,
                        &tr(if self.completed {
                            "更新完成"
                        } else {
                            "检查更新"
                        }),
                    );
                }
                ui.label(crate::i18n::message(
                    "common.update.current_version",
                    &[("version", env!("CARGO_PKG_VERSION").to_owned())],
                ));
                if self.completed {
                    ui.label(tr("编辑器已更新完成，可以继续编辑。"));
                } else if self.job.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr(if self.downloading {
                            "正在下载并准备更新…"
                        } else {
                            "正在检查 GitHub 最新正式版…"
                        }));
                    });
                    if self.downloading {
                        egui::ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() - 210.0).max(80.0))
                            .show(ui, |ui| self.show_progress(ui));
                    }
                    ui.hyperlink_to(tr("前往 GitHub 发布页面"), RELEASES);
                    if self.cancelling {
                        ui.label(crate::i18n::key("common.update.cancelling"));
                    } else if ui
                        .button(crate::i18n::key("common.update.cancel"))
                        .clicked()
                    {
                        self.cancel_job();
                    }
                } else if let Some(error) = &self.error {
                    crate::error_ui::show_with_actions(ui, error);
                    ui.hyperlink_to(tr("前往 GitHub 发布页面"), RELEASES);
                    if ui.button(tr("重新检查")).clicked() {
                        self.check(ctx);
                    }
                } else if let Some(release) = &self.release {
                    let newer =
                        version(&release.version).ok() > version(env!("CARGO_PKG_VERSION")).ok();
                    if newer || release.repair_needed {
                        if release.repair_needed {
                            ui.label(crate::i18n::key("common.update.repair_needed"));
                        } else {
                            ui.label(crate::i18n::message(
                                "common.update.found_version",
                                &[("version", release.version.clone())],
                            ));
                        }
                        egui::ScrollArea::vertical()
                            .max_height(210.0)
                            .show(ui, |ui| {
                                ui.label(&release.notes);
                            });
                        ui.add_space(8.0);
                        ui.label(tr(if release.repair_needed {
                            "是否修复当前版本？完成后会自动重启编辑器，并重新打开当前项目。"
                        } else {
                            "是否更新？安装完成后会自动重启编辑器，并重新打开当前项目。"
                        }));
                        ui.small(crate::i18n::key("common.update.host_hint"));
                        ui.hyperlink_to(tr("查看完整更新说明"), &release.url);
                        if cfg!(windows) && release.can_update() {
                            offered = true;
                            crate::shell::dialog_buttons(ui, |ui| {
                                update = ui
                                    .button(tr(if release.repair_needed {
                                        "修复并重启"
                                    } else {
                                        "更新并重启"
                                    }))
                                    .clicked();
                                close |= ui.button(tr("暂不更新")).clicked();
                            });
                        } else {
                            ui.label(tr("此发布暂无适用的独立更新文件，请到发布页面查看。"));
                        }
                    } else {
                        ui.label(tr("当前已是最新版本"));
                        ui.hyperlink_to(tr("查看发布页面"), &release.url);
                    }
                }
                if self.job.is_none() && !offered {
                    close |= ui.button(tr("关闭")).clicked();
                }
            });
        if close || update {
            self.open = false;
        }
        update
    }

    #[cfg(windows)]
    pub fn install(&mut self, project: Option<&Path>) -> Result<()> {
        use std::os::windows::process::CommandExt;
        let prepared = self.prepared.as_ref().context("更新包尚未准备完成")?;
        let exe = std::env::current_exe()?;
        let root = exe.parent().context("无法确定编辑器安装位置")?;
        validate_install(root, &prepared.files)?;
        let job_path = prepared.workspace.path().join("install.json");
        let job = json!({"root":root,"stage":prepared.workspace.path(),"files":prepared.files,"parent_pid":std::process::id(),"project":project,"restart":true});
        fs::write(&job_path, serde_json::to_vec(&job)?)?;
        // The verified new editor is also the update helper; no other asset is needed.
        let installer = prepared
            .workspace
            .path()
            .join("bundle")
            .join(prepared.installer);
        Command::new(&installer)
            .arg("--apply-editor-update")
            .arg(&job_path)
            .current_dir(
                prepared
                    .workspace
                    .path()
                    .parent()
                    .context("无法确定更新工作目录")?,
            )
            .creation_flags(0x08000000)
            .spawn()
            .context("无法启动 Rust 更新助手，编辑器尚未退出")?;
        let _ = self.prepared.take().unwrap().workspace.keep();
        self.open = false;
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn install(&mut self, _project: Option<&Path>) -> Result<()> {
        anyhow::bail!("当前平台请从 GitHub 发布页面手动安装")
    }
}

impl Drop for Updater {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Value {
        let files = PROGRAM_FILES
            .iter()
            .map(|(name, _)| {
                json!({
                    "name": name, "size": 17,
                    "sha256": format!("{:x}", Sha256::digest(b"new program bytes"))
                })
            })
            .collect::<Vec<_>>();
        json!({"version":"1.3.20","notes":"fixture release notes","files":files})
    }

    fn single_release() -> Value {
        json!({"tag_name":"v1.4.19","draft":false,"prerelease":false,"body":"single exe release",
            "assets":[{"name":"lom-editor.exe","size":17,"digest":format!("sha256:{:x}",Sha256::digest(b"new program bytes"))}]})
    }
    #[test]
    fn github_single_asset_metadata_requires_stable_release_and_verified_editor() {
        let value = single_release();
        let parsed = parse_release(&value).unwrap();
        assert!(parsed.can_update() && parsed.self_installing);
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(
            parsed.files[0].url,
            format!("{RELEASES}/download/v1.4.19/lom-editor.exe")
        );
        for field in ["draft", "prerelease"] {
            let mut bad = value.clone();
            bad[field] = json!(true);
            assert!(parse_release(&bad).is_err());
        }
        for (field, bad_value) in [
            ("name", json!("../lom-editor.exe")),
            ("size", json!(0)),
            ("digest", json!("sha256:bad")),
        ] {
            let mut bad = value.clone();
            bad["assets"][0][field] = bad_value;
            assert!(!parse_release(&bad).unwrap().can_update());
        }
        let mut old = value;
        old["tag_name"] = json!("v1.4.18");
        assert!(!parse_release(&old).unwrap().can_update());
        let mut previous = single_release();
        previous["assets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"lomc.exe","size":17}));
        assert!(
            !parse_release(&previous).unwrap().can_update(),
            "the previous multi-asset 1.4.19 cannot install itself"
        );
    }
    #[test]
    fn github_release_with_skill_attachment_updates_only_the_editor() {
        let mut value = single_release();
        value["tag_name"] = json!("v1.5.0");
        value["assets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"lom-modkit-authoring-v1.5.0.zip","size":2048}));
        let release = parse_release(&value).unwrap();
        assert!(release.can_update());
        assert_eq!(release.files.len(), 1);
        assert_eq!(release.files[0].path, "lom-editor.exe");
        assert!(release.files[0].url.ends_with("/v1.5.0/lom-editor.exe"));
        let duplicate = value["assets"][0].clone();
        value["assets"].as_array_mut().unwrap().push(duplicate);
        assert!(!parse_release(&value).unwrap().can_update());
    }
    #[test]
    fn single_asset_download_uses_the_verified_editor_as_installer() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let n = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).contains("/lom-editor.exe"));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\nnew program bytes").unwrap();
        });
        let mut release = parse_release(&single_release()).unwrap();
        release.files[0].url = format!("http://{address}/lom-editor.exe");
        let prepared = prepare(
            &release,
            &AtomicBool::new(false),
            &UpdateProgress::new(&release),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(prepared.files, vec!["lom-editor.exe"]);
        assert_eq!(prepared.installer, "lom-editor.exe");
        assert_eq!(
            fs::read(prepared.workspace.path().join("bundle/lom-editor.exe")).unwrap(),
            b"new program bytes"
        );
        assert!(!prepared.workspace.path().join("bundle/lomc.exe").exists());
    }

    #[test]
    fn selects_only_stable_versions_and_fixed_program_files() {
        assert!(version("v1.3.20").unwrap() > version("1.3.9").unwrap());
        for invalid in ["vv1.2.3", "1.3", "1.3.3-beta", "../1.3.3"] {
            assert!(version(invalid).is_err());
        }
        let mut value = release();
        let parsed = parse_release(&value).unwrap();
        assert!(parsed.can_update());
        assert!(parsed.files.iter().all(|file| file
            .url
            .starts_with(&format!("{RELEASES}/download/v1.3.20/"))));
        value["version"] = json!("1.3.20-beta");
        assert!(parse_release(&value).is_err());
        let mut value = release();
        value["files"][0]["name"] = json!("../lom-editor.exe");
        assert!(!parse_release(&value).unwrap().can_update());
        let mut value = release();
        value["files"][0]["sha256"] = json!("invalid");
        assert!(!parse_release(&value).unwrap().can_update());
    }

    #[test]
    fn startup_check_opens_only_for_updates_or_a_damaged_current_installation() {
        for (latest, repair, opens) in [
            ("0.1.0", false, false),
            (env!("CARGO_PKG_VERSION"), false, false),
            (env!("CARGO_PKG_VERSION"), true, true),
            ("99.0.0", false, true),
        ] {
            let mut release = parse_release(&release()).unwrap();
            release.version = latest.into();
            release.repair_needed = repair;
            let (tx, rx) = mpsc::channel();
            let mut updater = Updater::default();
            updater.silent = true;
            updater.job = Some(rx);
            tx.send(Ok(Job::Checked(release))).unwrap();
            assert!(!updater.poll());
            assert_eq!(updater.open, opens, "{latest}");
        }
        for silent in [true, false] {
            let (tx, rx) = mpsc::channel();
            let mut updater = Updater::default();
            updater.silent = silent;
            updater.open = !silent;
            updater.job = Some(rx);
            tx.send(Err("network unavailable".into())).unwrap();
            assert!(!updater.poll());
            assert_eq!(updater.open, !silent);
            assert_eq!(updater.error.is_some(), !silent);
        }
    }

    #[test]
    fn rejects_modified_downloads_and_never_selects_user_files() {
        let temp = tempfile::tempdir().unwrap();
        let release = parse_release(&release()).unwrap();
        for file in &release.files {
            let path = temp.path().join("download");
            fs::write(&path, b"new program bytes").unwrap();
            verify_file(&path, file).unwrap();
            fs::write(&path, b"bad program bytes").unwrap();
            assert!(verify_file(&path, file).is_err());
        }
        for name in [
            "../outside.txt",
            "assets/user/character/test.person/content.json",
            "project/story/main.json",
            "preferences.json",
        ] {
            assert!(!owned_file(name));
        }
    }

    #[test]
    fn all_update_files_are_requested_before_any_file_finishes() {
        for embedded in [false, true] {
            use std::{
                io::{BufRead, BufReader, Write},
                net::TcpListener,
                time::{Duration, Instant},
            };
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut connections = Vec::new();
                while connections.len() < PROGRAM_FILES.len() {
                    let mut connection = match listener.accept() {
                        Ok((connection, _)) => connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "update files were downloaded serially"
                            );
                            thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    connection.set_nonblocking(false).unwrap();
                    connection
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    {
                        let mut request = BufReader::new(&mut connection);
                        let mut line = String::new();
                        loop {
                            assert!(request.read_line(&mut line).unwrap() > 0);
                            if line == "\r\n" {
                                break;
                            }
                            line.clear();
                        }
                    }
                    connections.push(connection);
                }
                for mut connection in connections {
                    connection.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\nnew program bytes").unwrap();
                }
            });
            let mut release = parse_release(&release()).unwrap();
            release.embedded_components = embedded;
            for file in &mut release.files {
                file.url = format!("http://{address}/{}", file.path);
            }
            let progress = UpdateProgress::new(&release);
            let prepared = prepare(&release, &AtomicBool::new(false), &progress).unwrap();
            server.join().unwrap();
            assert!(progress
                .files
                .iter()
                .all(|file| file.received.load(Ordering::Relaxed) == file.size
                    && file.verified.load(Ordering::Relaxed)));
            assert_eq!(
                prepared.files.len(),
                if embedded { 1 } else { PROGRAM_FILES.len() }
            );
            assert!(
                prepared.workspace.path().join("bundle/lomc.exe").is_file(),
                "the verified installer is still required for single-exe updates"
            );
            for file in prepared.files {
                assert_eq!(
                    fs::read(prepared.workspace.path().join("bundle").join(file)).unwrap(),
                    b"new program bytes"
                );
            }
        }
    }

    #[test]
    fn embedded_installation_checks_only_the_editor_beside_user_projects() {
        let root = tempfile::tempdir().unwrap();
        let mut value = release();
        value["version"] = json!(env!("CARGO_PKG_VERSION"));
        value["embedded_components"] = json!(true);
        let mut release = parse_release(&value).unwrap();
        fs::write(root.path().join("lom-editor.exe"), b"new program bytes").unwrap();
        release.inspect_installation(root.path()).unwrap();
        assert!(!release.repair_needed);
        fs::write(root.path().join("lom-editor.exe"), b"broken").unwrap();
        release.inspect_installation(root.path()).unwrap();
        assert!(release.repair_needed);
    }

    #[test]
    fn download_dialog_shows_transfer_states_and_no_data_hint() {
        let release = parse_release(&release()).unwrap();
        let mut progress = UpdateProgress::new(&release);
        progress.started = Instant::now() - Duration::from_secs(16);
        for (file, received) in progress.files.iter().zip([0, 1, 17, 17]) {
            file.received.store(received, Ordering::Relaxed);
        }
        progress
            .files
            .last()
            .unwrap()
            .verified
            .store(true, Ordering::Relaxed);
        let (_tx, rx) = mpsc::channel();
        let mut updater = Updater::default();
        updater.job = Some(rx);
        updater.downloading = true;
        updater.last_received = 35;
        updater.last_activity = Some(progress.started);
        updater.progress = Some(Arc::new(progress));
        let context = egui::Context::default();
        context.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 560.0),
            )),
            ..Default::default()
        };
        let _ = context.run(input.clone(), |context| {
            updater.dialog(context);
        });
        let output = context.run(input, |context| {
            assert!(!updater.dialog(context));
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        for key in [
            "common.update.file_connecting",
            "common.update.file_downloading",
            "common.update.file_verifying",
            "common.update.file_ready",
            "common.update.no_data",
            "common.update.cancel",
        ] {
            let text = crate::i18n::key(key);
            assert!(
                labels.iter().any(|label| label.contains(&text)),
                "missing {key}: {labels:?}"
            );
        }
        assert!(labels.iter().any(|label| label.contains("KiB/s")));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 400.0),
            )),
            ..Default::default()
        };
        let _ = context.run(input.clone(), |context| {
            updater.dialog(context);
        });
        let output = context.run(input, |context| {
            updater.dialog(context);
        });
        let cancel = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find_map(|(_, node)| {
                (node.label() == Some(crate::i18n::key("common.update.cancel").as_str()))
                    .then(|| node.bounds())
                    .flatten()
            })
            .unwrap();
        assert!(
            cancel.x0 >= 0.0 && cancel.y0 >= 0.0 && cancel.x1 <= 640.0 && cancel.y1 <= 400.0,
            "cancel button must stay visible on small screens: {cancel:?}"
        );
    }

    #[test]
    fn cancelling_a_completed_download_does_not_start_installation() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().to_path_buf();
        let (tx, rx) = mpsc::channel();
        let mut updater = Updater::default();
        updater.job = Some(rx);
        updater.downloading = true;
        tx.send(Ok(Job::Prepared(Prepared {
            workspace,
            files: Vec::new(),
            installer: "lom-editor.exe",
        })))
        .unwrap();
        updater.cancel_job();
        assert!(!updater.poll());
        assert!(updater.prepared.is_none());
        assert!(updater.job.is_none());
        assert!(!updater.downloading);
        assert_eq!(
            updater.error.as_deref(),
            Some(crate::i18n::key("common.update.cancelled").as_str())
        );
        assert!(
            !path.exists(),
            "the cancelled staged download must be released"
        );
    }

    #[test]
    fn recognizes_partial_editor_installs_but_rejects_unknown_roots_and_user_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        assert!(validate_install(root.as_path(), &["lom-editor.exe".into()]).is_err());
        fs::write(root.as_path().join("lom-editor.exe"), b"fixture").unwrap();
        let files = PROGRAM_FILES
            .iter()
            .map(|(_, path)| path.to_string())
            .collect::<Vec<_>>();
        assert!(validate_install(root.as_path(), &files).is_ok());
        assert!(validate_install(root.as_path(), &["assets/user/test.json".into()]).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn damaged_current_version_offers_repair_instead_of_reporting_up_to_date() {
        let mut release = parse_release(&release()).unwrap();
        release.version = env!("CARGO_PKG_VERSION").into();
        release.repair_needed = true;
        let mut updater = Updater::default();
        updater.release = Some(release);
        let context = egui::Context::default();
        context.enable_accesskit();
        let output = context.run(Default::default(), |context| {
            assert!(!updater.dialog(context));
        });
        let nodes = &output.platform_output.accesskit_update.unwrap().nodes;
        assert!(nodes
            .iter()
            .any(|(_, node)| node.role() == egui::accesskit::Role::Button
                && node.label()
                    == Some(crate::i18n::key("common.update.repair_restart").as_str())));
        assert!(!nodes
            .iter()
            .any(|(_, node)| node.label() == Some(crate::i18n::tr("当前已是最新版本").as_str())));
    }

    #[test]
    fn same_version_check_detects_missing_and_mixed_components_without_downgrading() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut release = parse_release(&release()).unwrap();
        release.version = env!("CARGO_PKG_VERSION").into();
        for (index, file) in release.files.iter_mut().enumerate() {
            let bytes = format!("current component {index}").into_bytes();
            file.size = bytes.len() as u64;
            file.digest = format!("{:x}", Sha256::digest(&bytes));
            let path = root.as_path().join(&file.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        release.inspect_installation(root.as_path()).unwrap();
        assert!(!release.repair_needed);
        for path in [
            "lomc.exe",
            "runtime/MortalModHost.dll",
            "runtime/NVorbis.dll",
        ] {
            let target = root.as_path().join(path);
            let original = fs::read(&target).unwrap();
            fs::remove_file(&target).unwrap();
            release.inspect_installation(root.as_path()).unwrap();
            assert!(release.repair_needed, "missing {path}");
            fs::write(&target, vec![b'x'; original.len()]).unwrap();
            release.inspect_installation(root.as_path()).unwrap();
            assert!(release.repair_needed, "same-size stale {path}");
            fs::write(&target, original).unwrap();
            release.inspect_installation(root.as_path()).unwrap();
            assert!(!release.repair_needed, "restored {path}");
        }
        fs::remove_file(root.as_path().join("lomc.exe")).unwrap();
        release.inspect_installation(root.as_path()).unwrap();
        assert!(release.repair_needed);
        release.version = "0.1.0".into();
        release.inspect_installation(root.as_path()).unwrap();
        assert!(
            !release.repair_needed,
            "an older public release must not replace a newer editor"
        );
    }

    #[test]
    #[ignore = "requires the public GitHub release and a network download"]
    fn github_release_download_and_validation() {
        let cancel = AtomicBool::new(false);
        let release = check(&cancel).unwrap();
        assert_eq!(
            release.version,
            env!("CARGO_PKG_VERSION"),
            "publish the tested version first"
        );
        assert!(release.self_installing && release.can_update());
        let temp = lom_core::workdir::create().unwrap();
        if release.can_update() {
            let prepared = prepare(&release, &cancel, &UpdateProgress::new(&release)).unwrap();
            assert_eq!(prepared.files.len(), 1);
            #[cfg(windows)]
            if release.embedded_components {
                use std::os::windows::process::CommandExt;
                let output = Command::new(prepared.workspace.path().join("bundle/lom-editor.exe"))
                    .args(["--component-task", "--self-check"])
                    .env("LOCALAPPDATA", temp.path().join("isolated-components"))
                    .creation_flags(0x08000000)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let result: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(result["engine"], "rust");
                assert_eq!(result["sprite_decoder_ready"], true);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn shutdown_and_drop_cancel_pending_downloads_and_release_their_files() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;
        use std::time::Instant;

        for downloading_body in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = tempfile::tempdir_in(temp.path()).unwrap();
            let cache = workspace.path().to_path_buf();
            let destination = cache.join("download.bin");
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/update.json", listener.local_addr().unwrap());
            let mut updater = Updater::default();
            let cancel = updater.cancel.clone();
            let file = destination.clone();
            let (tx, rx) = mpsc::channel();
            updater.worker = Some(thread::spawn(move || {
                let _workspace = workspace;
                let _ = tx.send(download(&url, &file, 1024, &cancel));
            }));
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut connection = loop {
                match listener.accept() {
                    Ok((connection, _)) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "download helper did not connect");
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            {
                let mut request = BufReader::new(&mut connection);
                let mut line = String::new();
                loop {
                    assert!(request.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
            }
            if downloading_body {
                connection
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\n{",
                    )
                    .unwrap();
                while !destination.exists() {
                    assert!(
                        Instant::now() < deadline,
                        "download did not open its output file"
                    );
                    thread::sleep(Duration::from_millis(20));
                }
            }
            let started = Instant::now();
            if !downloading_body {
                updater.shutdown();
                assert!(updater.worker.is_none());
                // The real exit hook and Drop may both run during normal shutdown.
                updater.shutdown();
            }
            drop(updater);
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "closing waited for the HTTP timeout"
            );
            let error = format!(
                "{:#}",
                rx.recv_timeout(Duration::from_secs(1))
                    .unwrap()
                    .unwrap_err()
            );
            assert!(error.contains("下载已取消"), "{error}");
            assert!(
                !cache.exists(),
                "the helper must release the output file before closing"
            );
            let mut remaining = Vec::new();
            if let Err(error) = connection.read_to_end(&mut remaining) {
                assert!(
                    matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ),
                    "the helper still holds its HTTP connection: {error}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn pending_download_does_not_keep_the_editor_directory_locked() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;
        use std::time::{Duration, Instant};

        const PROBE: &str = "LOM_TEST_UPDATE_DOWNLOAD_URL";
        if let Ok(url) = std::env::var(PROBE) {
            let destination = std::path::PathBuf::from(
                std::env::var_os("LOM_TEST_UPDATE_DOWNLOAD_FILE").unwrap(),
            );
            let cache = destination.parent().unwrap().to_path_buf();
            let job = std::thread::spawn(move || {
                download(&url, &destination, 1024, &AtomicBool::new(false))
            });
            // Release the parent's directory only after its helper has connected.
            std::io::stdin().read_exact(&mut [0]).unwrap();
            std::env::set_current_dir(cache).unwrap();
            println!("download-probe-ready");
            std::io::stdout().flush().unwrap();
            job.join().unwrap().unwrap();
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Editor 中文 [test]");
        let cache = temp.path().join("cache");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&cache).unwrap();
        fs::write(root.join("project.json"), b"keep my project").unwrap();
        let destination = cache.join("release.json");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "editor_update::tests::pending_download_does_not_keep_the_editor_directory_locked",
                "--nocapture",
            ])
            .env(
                PROBE,
                format!("http://{}/update.json", listener.local_addr().unwrap()),
            )
            .env("LOM_TEST_UPDATE_DOWNLOAD_FILE", &destination)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut connection = loop {
            match listener.accept() {
                Ok((connection, _)) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline || parent.try_wait().unwrap().is_some() {
                        let _ = parent.kill();
                        let output = parent.wait_with_output().unwrap();
                        panic!(
                            "download helper did not connect: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("{error}"),
            }
        };
        connection
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        {
            let mut request = BufReader::new(&mut connection);
            let mut line = String::new();
            loop {
                assert!(request.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                line.clear();
            }
        }
        parent.stdin.take().unwrap().write_all(b"1").unwrap();
        let mut stdout = BufReader::new(parent.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            assert!(stdout.read_line(&mut line).unwrap() > 0);
            if line.trim() == "download-probe-ready" {
                break;
            }
            line.clear();
        }
        // The HTTP request is still waiting, but the editor has released its cwd.
        let moved = temp.path().join("Moved editor");
        let rename = fs::rename(&root, &moved);
        connection
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        drop(connection);
        let output = parent.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        rename.expect("pending PowerShell download must not lock the editor directory");
        assert_eq!(
            fs::read(moved.join("project.json")).unwrap(),
            b"keep my project"
        );
        assert_eq!(fs::read(destination).unwrap(), b"{}");
    }
}
