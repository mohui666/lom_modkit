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
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
};

const RELEASES: &str = "https://github.com/mohui666/lom_modkit/releases";
const LATEST: &str = "https://github.com/mohui666/lom_modkit/releases/latest/download/update.json";
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
#[cfg(any(windows, test))]
use lom_core::editor_update::validate_install;
use lom_core::editor_update::{owned_file, PROGRAM_FILES};

#[derive(Clone, Debug)]
struct Release {
    version: String,
    notes: String,
    url: String,
    files: Vec<Download>,
}

#[derive(Clone, Debug)]
struct Download {
    path: String,
    url: String,
    size: u64,
    digest: String,
}

impl Release {
    fn can_update(&self) -> bool {
        self.files.len() == PROGRAM_FILES.len()
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

fn prepare(release: &Release, cancel: &AtomicBool) -> Result<Prepared> {
    ensure!(
        release.can_update(),
        "此发布缺少可直接更新的程序附件或校验信息"
    );
    let workspace = lom_core::workdir::create()?;
    let mut files = Vec::new();
    for file in &release.files {
        ensure!(owned_file(&file.path), "拒绝下载非程序文件：{}", file.path);
        let destination = workspace.path().join("bundle").join(&file.path);
        fs::create_dir_all(destination.parent().context("无效更新路径")?)?;
        download(&file.url, &destination, MAX_DOWNLOAD, cancel)?;
        verify_file(&destination, file)?;
        files.push(file.path.clone());
    }
    files.sort_by_key(|name| name == "lom-editor.exe");
    Ok(Prepared { workspace, files })
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
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        let ctx = ctx.clone();
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        self.worker = Some(thread::spawn(move || {
            let _ = tx.send(
                check(&cancel)
                    .map(Job::Checked)
                    .map_err(|e| format!("{e:#}")),
            );
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
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        let ctx = ctx.clone();
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        self.worker = Some(thread::spawn(move || {
            let _ = tx.send(
                prepare(&release, &cancel)
                    .map(Job::Prepared)
                    .map_err(|e| format!("{e:#}")),
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
        match result {
            Ok(Job::Checked(release)) => {
                self.open = !self.silent
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

    /// Returns true when the user chooses “update and restart”.
    pub fn dialog(&mut self, ctx: &egui::Context) -> bool {
        use crate::i18n::tr;
        let mut update = false;
        let mut close = false;
        let mut offered = false;
        egui::Modal::new(egui::Id::new("editor-update")).frame(crate::shell::dialog_frame()).show(ctx, |ui| {
            ui.set_width(480.0_f32.min(ctx.content_rect().width() - 60.0));
            if self.job.is_some() { ui.heading(tr("检查更新")); }
            else { close |= crate::shell::dialog_header(ui, &tr(if self.completed { "更新完成" } else { "检查更新" })); }
            ui.label(format!("{} {}", tr("当前版本"), env!("CARGO_PKG_VERSION")));
            if self.completed {
                ui.label(tr("编辑器已更新完成，可以继续编辑。"));
            } else if self.job.is_some() {
                ui.horizontal(|ui| { ui.spinner(); ui.label(tr(if self.downloading { "正在下载并准备更新…" } else { "正在检查 GitHub 最新正式版…" })); });
            } else if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(179, 55, 49), error);
                ui.hyperlink_to(tr("前往 GitHub 发布页面"), RELEASES);
                if ui.button(tr("重新检查")).clicked() { self.check(ctx); }
            } else if let Some(release) = &self.release {
                let newer = version(&release.version).ok() > version(env!("CARGO_PKG_VERSION")).ok();
                if newer {
                    ui.label(format!("{} v{}", tr("发现新版本"), release.version));
                    egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| { ui.label(&release.notes); });
                    ui.add_space(8.0);
                    ui.label(tr("是否更新？安装完成后会自动重启编辑器，并重新打开当前项目。"));
                    ui.small(tr("项目、用户内容库和预览库会保留。游戏中的 Host 需在“游戏与 MOD”中另行更新。"));
                    ui.hyperlink_to(tr("查看完整更新说明"), &release.url);
                    if cfg!(windows) && release.can_update() {
                        offered = true;
                        crate::shell::dialog_buttons(ui, |ui| {
                            update = ui.button(tr("更新并重启")).clicked();
                            close |= ui.button(tr("暂不更新")).clicked();
                        });
                    } else { ui.label(tr("此发布暂无适用的独立更新文件，请到发布页面查看。")); }
                } else {
                    ui.label(tr("当前已是最新版本"));
                    ui.hyperlink_to(tr("查看发布页面"), &release.url);
                }
            }
            if self.job.is_none() && !offered { close |= ui.button(tr("关闭")).clicked(); }
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
        let installer = prepared.workspace.path().join("installer.exe");
        fs::copy(root.join("lomc.exe"), &installer).context("无法准备 Rust 更新助手")?;
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
    fn startup_check_is_silent_except_when_a_newer_release_exists() {
        for (latest, opens) in [
            ("0.1.0", false),
            (env!("CARGO_PKG_VERSION"), false),
            ("99.0.0", true),
        ] {
            let mut release = parse_release(&release()).unwrap();
            release.version = latest.into();
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
    fn refuses_source_builds_and_non_program_install_paths() {
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_install(temp.path(), &["lom-editor.exe".into()]).is_err());
        fs::create_dir(temp.path().join("runtime")).unwrap();
        fs::write(temp.path().join("lomc.exe"), b"fixture").unwrap();
        fs::write(temp.path().join("runtime/MortalModHost.dll"), b"fixture").unwrap();
        assert!(validate_install(temp.path(), &["assets/user/test.json".into()]).is_err());
    }

    #[test]
    #[ignore = "requires the public GitHub release and a network download"]
    fn github_release_download_and_validation() {
        let cancel = AtomicBool::new(false);
        let release = check(&cancel).unwrap();
        let host = release
            .files
            .iter()
            .find(|file| file.path == "runtime/MortalModHost.dll")
            .unwrap();
        let temp = lom_core::workdir::create().unwrap();
        let path = temp.path().join("MortalModHost.dll");
        download(&host.url, &path, MAX_DOWNLOAD, &cancel).unwrap();
        verify_file(&path, host).unwrap();
        if release.can_update() {
            let prepared = prepare(&release, &cancel).unwrap();
            assert_eq!(prepared.files.len(), 4);
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
