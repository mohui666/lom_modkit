//! Manual GitHub release checks and an in-place Windows bundle update.
use anyhow::{ensure, Context, Result};
use eframe::egui;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command, sync::mpsc};

const RELEASES: &str = "https://github.com/mohui666/lom_modkit/releases";
const LATEST: &str = "https://api.github.com/repos/mohui666/lom_modkit/releases/latest";
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
#[cfg(any(windows, test))]
const APPLY_SCRIPT: &str = include_str!("editor_update.ps1");

#[derive(Clone, Debug)]
struct Release {
    version: String,
    notes: String,
    url: String,
    files: Vec<Download>,
}

const PROGRAM_FILES: [(&str, &str); 4] = [
    ("lom-editor.exe", "lom-editor.exe"),
    ("lomc.exe", "lomc.exe"),
    ("MortalModHost.dll", "runtime/MortalModHost.dll"),
    ("NVorbis.dll", "runtime/NVorbis.dll"),
];

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
    ensure!(
        value["draft"] == false && value["prerelease"] == false,
        "最新发布不是正式版"
    );
    let tag = value["tag_name"].as_str().context("发布信息缺少版本")?;
    version(tag)?;
    let release_url = format!("{RELEASES}/tag/{tag}");
    ensure!(
        value["html_url"] == release_url,
        "发布页面不属于本工具的 GitHub 仓库"
    );

    let mut files = Vec::new();
    for (name, path) in PROGRAM_FILES {
        let expected = format!("{RELEASES}/download/{tag}/{name}");
        if let Some(asset) = value["assets"].as_array().and_then(|assets| {
            assets.iter().find(|asset| {
                asset["name"] == name
                    && asset["browser_download_url"] == expected
                    && asset["size"]
                        .as_u64()
                        .is_some_and(|n| n > 0 && n <= MAX_DOWNLOAD)
            })
        }) {
            if let Some(digest) = asset["digest"]
                .as_str()
                .and_then(|s| s.strip_prefix("sha256:"))
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
        version: tag.strip_prefix('v').unwrap_or(tag).into(),
        notes: value["body"].as_str().unwrap_or("").into(),
        url: release_url,
        files,
    })
}

#[cfg(windows)]
fn download(url: &str, destination: &Path, limit: u64) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let script = r#"
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$request = [Net.HttpWebRequest]::Create($env:LOM_UPDATE_URL)
$request.UserAgent = 'lom-modkit-editor-updater'
$request.Accept = 'application/vnd.github+json'
$request.Timeout = 180000; $request.ReadWriteTimeout = 180000
$response = $null; $inputStream = $null; $outputStream = $null
try {
    $response = $request.GetResponse()
    $limit = [long]$env:LOM_UPDATE_LIMIT
    if ($response.ContentLength -gt $limit) { throw 'Download exceeds size limit' }
    $inputStream = $response.GetResponseStream()
    $outputStream = [IO.File]::Open($env:LOM_UPDATE_FILE, [IO.FileMode]::CreateNew)
    $buffer = New-Object byte[] 65536; $total = 0
    while (($count = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
        $total += $count
        if ($total -gt $limit) { throw 'Download exceeds size limit' }
        $outputStream.Write($buffer, 0, $count)
    }
} finally {
    if ($outputStream) { $outputStream.Dispose() }
    if ($inputStream) { $inputStream.Dispose() }
    if ($response) { $response.Close() }
}
"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("LOM_UPDATE_URL", url)
        .env("LOM_UPDATE_FILE", destination)
        .env("LOM_UPDATE_LIMIT", limit.to_string())
        .creation_flags(0x08000000)
        .output()
        .context("无法启动更新下载")?;
    ensure!(
        output.status.success(),
        "无法从 GitHub 下载更新：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(fs::metadata(destination)?.len() > 0, "GitHub 返回了空文件");
    Ok(())
}

#[cfg(not(windows))]
fn download(url: &str, destination: &Path, limit: u64) -> Result<()> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "15",
            "--max-time",
            "180",
            "--max-filesize",
        ])
        .arg(limit.to_string())
        .args(["--user-agent", "lom-modkit-editor-updater", "--output"])
        .arg(destination)
        .arg(url)
        .output()
        .context("无法启动更新检查")?;
    ensure!(
        output.status.success(),
        "无法从 GitHub 下载更新：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        fs::metadata(destination)?.len() > 0 && fs::metadata(destination)?.len() <= limit,
        "下载文件大小不正确"
    );
    Ok(())
}

fn check() -> Result<Release> {
    let temp = lom_core::workdir::create()?;
    let response = temp.path().join("release.json");
    download(LATEST, &response, 1024 * 1024)?;
    parse_release(&serde_json::from_slice(&fs::read(response)?)?)
}

fn owned_file(name: &str) -> bool {
    PROGRAM_FILES.iter().any(|(_, path)| *path == name)
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

fn prepare(release: &Release) -> Result<Prepared> {
    ensure!(
        release.can_update(),
        "此发布缺少可直接更新的程序附件或 GitHub 校验信息"
    );
    let workspace = lom_core::workdir::create()?;
    let mut files = Vec::new();
    for file in &release.files {
        ensure!(owned_file(&file.path), "拒绝下载非程序文件：{}", file.path);
        let destination = workspace.path().join("bundle").join(&file.path);
        fs::create_dir_all(destination.parent().context("无效更新路径")?)?;
        download(&file.url, &destination, MAX_DOWNLOAD)?;
        verify_file(&destination, file)?;
        files.push(file.path.clone());
    }
    files.sort_by_key(|name| name == "lom-editor.exe");
    Ok(Prepared { workspace, files })
}

#[cfg(any(windows, test))]
fn validate_install(root: &Path, files: &[String]) -> Result<()> {
    ensure!(
        root.join("lomc.exe").is_file() && root.join("runtime/MortalModHost.dll").is_file(),
        "请从完整 Windows 发布包运行编辑器后，再使用内置更新"
    );
    for file in files {
        ensure!(owned_file(file), "拒绝更新非程序文件：{file}");
        let target = root.join(file);
        let mut ancestor = Some(target.as_path());
        while let Some(path) = ancestor {
            if let Ok(metadata) = fs::symlink_metadata(path) {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        "更新路径包含链接或联接：{}",
                        path.display()
                    );
                }
                ensure!(
                    !metadata.is_symlink(),
                    "更新路径包含符号链接：{}",
                    path.display()
                );
            }
            ancestor = path.parent();
        }
        if target.exists() {
            ensure!(target.is_file(), "程序目标不是文件：{}", target.display());
        }
    }
    Ok(())
}

enum Job {
    Checked(Release),
    Prepared(Prepared),
}

#[derive(Default)]
pub struct Updater {
    pub open: bool,
    release: Option<Release>,
    prepared: Option<Prepared>,
    job: Option<mpsc::Receiver<Result<Job, String>>>,
    downloading: bool,
    error: Option<String>,
}

impl Updater {
    pub fn check(&mut self, ctx: &egui::Context) {
        if self.job.is_some() {
            self.open = true;
            return;
        }
        self.open = true;
        self.release = None;
        self.prepared = None;
        self.error = None;
        self.downloading = false;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(check().map(Job::Checked).map_err(|e| format!("{e:#}")));
            ctx.request_repaint();
        });
    }

    pub fn download(&mut self, ctx: &egui::Context) {
        let Some(release) = self.release.clone() else {
            return;
        };
        self.open = true;
        self.downloading = true;
        self.error = None;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(
                prepare(&release)
                    .map(Job::Prepared)
                    .map_err(|e| format!("{e:#}")),
            );
            ctx.request_repaint();
        });
    }

    /// True only after a confirmed update has finished downloading and validating.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.job else { return false };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err("更新任务意外中断".into()),
        };
        self.job = None;
        self.downloading = false;
        match result {
            Ok(Job::Checked(release)) => self.release = Some(release),
            Ok(Job::Prepared(prepared)) => {
                self.prepared = Some(prepared);
                return true;
            }
            Err(error) => self.error = Some(error),
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
            else { close |= crate::shell::dialog_header(ui, &tr("检查更新")); }
            ui.label(format!("{} {}", tr("当前版本"), env!("CARGO_PKG_VERSION")));
            if self.job.is_some() {
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
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", APPLY_SCRIPT])
            .env("LOM_EDITOR_UPDATE_JOB", &job_path)
            .creation_flags(0x08000000)
            .spawn()
            .context("无法启动更新安装，编辑器尚未退出")?;
        let _ = self.prepared.take().unwrap().workspace.keep();
        self.open = false;
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn install(&mut self, _project: Option<&Path>) -> Result<()> {
        anyhow::bail!("当前平台请从 GitHub 发布页面手动安装")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Value {
        let tag = "v1.3.20";
        let assets = PROGRAM_FILES
            .iter()
            .map(|(name, _)| {
                json!({
                    "name": name, "size": 17,
                    "browser_download_url": format!("{RELEASES}/download/{tag}/{name}"),
                    "digest": format!("sha256:{:x}", Sha256::digest(b"new program bytes"))
                })
            })
            .collect::<Vec<_>>();
        json!({"tag_name":tag,"draft":false,"prerelease":false,"html_url":format!("{RELEASES}/tag/{tag}"),"body":"fixture release notes","assets":assets})
    }

    #[test]
    fn selects_only_stable_releases_and_matching_official_assets() {
        assert!(version("v1.3.20").unwrap() > version("1.3.9").unwrap());
        for invalid in ["vv1.2.3", "1.3", "1.3.3-beta", "../1.3.3"] {
            assert!(version(invalid).is_err());
        }
        let mut value = release();
        assert!(parse_release(&value).unwrap().can_update());
        value["prerelease"] = json!(true);
        assert!(parse_release(&value).is_err());
        value["prerelease"] = json!(false);
        value["assets"][0]["browser_download_url"] = json!("https://other.example/lom-editor.exe");
        assert!(!parse_release(&value).unwrap().can_update());
        value["html_url"] = json!("https://github.com/other/repo/releases/tag/v1.3.20");
        assert!(parse_release(&value).is_err());
        let mut value = release();
        value["assets"][0]["digest"] = json!("sha256:invalid");
        assert!(!parse_release(&value).unwrap().can_update());
    }

    fn fixture_files(output: &Path) -> Vec<String> {
        let mut files = Vec::new();
        for (_, name) in PROGRAM_FILES {
            let target = output.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"new program bytes").unwrap();
            files.push(name.into());
        }
        files.sort_by_key(|name| name == "lom-editor.exe");
        files
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
        let release = check().unwrap();
        let host = release
            .files
            .iter()
            .find(|file| file.path == "runtime/MortalModHost.dll")
            .unwrap();
        let temp = lom_core::workdir::create().unwrap();
        let path = temp.path().join("MortalModHost.dll");
        download(&host.url, &path, MAX_DOWNLOAD).unwrap();
        verify_file(&path, host).unwrap();
        if release.can_update() {
            let prepared = prepare(&release).unwrap();
            assert_eq!(prepared.files.len(), 4);
        }
    }

    #[cfg(windows)]
    #[test]
    fn installer_waits_for_editor_and_preserves_project_library_and_preferences() {
        use std::os::windows::process::CommandExt;
        use std::time::{Duration, Instant};
        for locked in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let local = temp.path().join("local");
            let work = local.join("lom_modkit/cache/work");
            fs::create_dir_all(&work).unwrap();
            let stage = tempfile::Builder::new()
                .prefix("lom-modkit-")
                .tempdir_in(&work)
                .unwrap();
            let root = temp.path().join("Editor 中文 [test]");
            fs::create_dir_all(root.join("runtime")).unwrap();
            fs::write(root.join("lomc.exe"), b"old").unwrap();
            fs::write(root.join("runtime/MortalModHost.dll"), b"old host").unwrap();
            let files = fixture_files(&stage.path().join("bundle"));
            for (path, data) in [
                ("project/story/main.json", "my story"),
                (
                    "assets/user/character/my.person/content.json",
                    "my character",
                ),
                (
                    "repository/assets/user/music/my.theme/theme.ogg",
                    "my music",
                ),
                ("preferences.json", "my settings"),
            ] {
                fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
                fs::write(root.join(path), data).unwrap();
            }
            fs::write(root.join("lom-editor.exe"), b"old editor").unwrap();
            let mut parent = Command::new("powershell.exe")
                .args(["-NoProfile", "-Command", "Start-Sleep -Milliseconds 700"])
                .creation_flags(0x08000000)
                .spawn()
                .unwrap();
            let job = stage.path().join("install.json");
            fs::write(&job, serde_json::to_vec(&json!({"root":root,"stage":stage.path(),"files":files,"parent_pid":parent.id(),"restart":false})).unwrap()).unwrap();
            let lock = if locked {
                use std::os::windows::fs::OpenOptionsExt;
                Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(root.join("lom-editor.exe"))
                        .unwrap(),
                )
            } else {
                None
            };
            let started = Instant::now();
            let result = Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", APPLY_SCRIPT])
                .env("LOM_EDITOR_UPDATE_JOB", &job)
                .env("LOCALAPPDATA", &local)
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            let _ = parent.wait();
            assert!(started.elapsed() >= Duration::from_millis(500));
            assert_eq!(
                result.status.success(),
                !locked,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                fs::read(root.join("lomc.exe")).unwrap(),
                if locked {
                    b"old".as_slice()
                } else {
                    b"new program bytes".as_slice()
                }
            );
            assert_eq!(
                fs::read(root.join("project/story/main.json")).unwrap(),
                b"my story"
            );
            assert_eq!(
                fs::read(root.join("assets/user/character/my.person/content.json")).unwrap(),
                b"my character"
            );
            assert_eq!(
                fs::read(root.join("repository/assets/user/music/my.theme/theme.ogg")).unwrap(),
                b"my music"
            );
            assert_eq!(
                fs::read(root.join("preferences.json")).unwrap(),
                b"my settings"
            );
            assert!(!stage.path().exists());
            drop(lock);
        }
    }
}
