//! Manual GitHub release checks and an in-place Windows bundle update.
use anyhow::{ensure, Context, Result};
use eframe::egui;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Read, path::Path, process::Command, sync::mpsc};

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
    archive: Option<(String, String, String)>,
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
    let number = tag.trim_start_matches('v');
    let filename = format!("lom_modkit-v{number}_windows_x64.zip");
    let asset_url = |name: &str| -> Option<String> {
        let expected = format!("{RELEASES}/download/{tag}/{name}");
        value["assets"].as_array()?.iter().find_map(|asset| {
            (asset["name"] == name
                && asset["browser_download_url"] == expected
                && asset["size"]
                    .as_u64()
                    .is_some_and(|n| n > 0 && n <= MAX_DOWNLOAD))
            .then_some(expected.clone())
        })
    };
    let archive = asset_url(&filename)
        .zip(asset_url(&format!("{filename}.sha256")))
        .map(|(zip, checksum)| (filename, zip, checksum));
    Ok(Release {
        version: number.into(),
        notes: value["body"].as_str().unwrap_or("").into(),
        url: release_url,
        archive,
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
    matches!(
        name,
        "lom-editor.exe"
            | "lomc.exe"
            | "LICENSE"
            | "README-Windows.md"
            | "runtime/MortalModHost.dll"
            | "runtime/NVorbis.dll"
            | "assets/doorstop/win-x86-doorstop.dll"
            | "assets/doorstop/win-x64-doorstop.dll"
            | "assets/doorstop/README.md"
            | "assets/doorstop/LICENSE.UnityDoorstop"
    ) || name.starts_with("asset-extractor/")
        || (!name.contains('/') && name.starts_with("RELEASE_NOTES_v") && name.ends_with(".md"))
}

fn unpack(archive: &Path, output: &Path) -> Result<Vec<String>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    ensure!(zip.len() <= 4096, "更新包文件过多");
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let path = lom_core::package::canonical_archive_name(entry.name())?;
        ensure!(
            entry.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            "更新包不能包含符号链接"
        );
        let relative = path
            .strip_prefix("lom_modkit/")
            .context("更新包目录结构不正确")?;
        ensure!(
            seen.insert(relative.to_lowercase()),
            "更新包含重复路径：{relative}"
        );
        if entry.is_dir() {
            continue;
        }
        ensure!(owned_file(relative), "更新包包含非程序文件：{relative}");
        total = total.checked_add(entry.size()).context("更新包大小溢出")?;
        ensure!(
            total <= 512 * 1024 * 1024 && entry.size() <= MAX_DOWNLOAD,
            "更新包解压大小超限"
        );
        let target = output.join(relative);
        fs::create_dir_all(target.parent().context("无效更新路径")?)?;
        let mut destination = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let copied = std::io::copy(&mut entry.by_ref().take(MAX_DOWNLOAD + 1), &mut destination)?;
        ensure!(copied == entry.size(), "更新文件长度不正确：{relative}");
        files.push(relative.into());
    }
    for required in [
        "lom-editor.exe",
        "lomc.exe",
        "runtime/MortalModHost.dll",
        "runtime/NVorbis.dll",
        "asset-extractor/lom-asset-extractor.exe",
        "assets/doorstop/win-x64-doorstop.dll",
        "assets/doorstop/win-x86-doorstop.dll",
    ] {
        ensure!(files.iter().any(|s| s == required), "更新包缺少 {required}");
    }
    // Replace the editor last, after all companion files are installed.
    files.sort_by_key(|name| name == "lom-editor.exe");
    Ok(files)
}

struct Prepared {
    workspace: tempfile::TempDir,
    files: Vec<String>,
}

fn verify_archive(archive: &Path, checksum: &str, filename: &str) -> Result<()> {
    let expected = checksum.split_whitespace().collect::<Vec<_>>();
    ensure!(
        expected.len() == 2 && expected[1] == filename && expected[0].len() == 64,
        "发布校验文件格式不正确"
    );
    let actual = format!("{:x}", Sha256::digest(fs::read(archive)?));
    ensure!(
        actual.eq_ignore_ascii_case(expected[0]),
        "更新包校验失败，请重新检查更新"
    );
    Ok(())
}

fn prepare(release: &Release) -> Result<Prepared> {
    let (filename, archive_url, checksum_url) = release
        .archive
        .as_ref()
        .context("此发布没有完整 Windows 更新包及校验文件")?;
    let workspace = lom_core::workdir::create()?;
    let archive = workspace.path().join(filename);
    let checksum = workspace.path().join("checksum.txt");
    download(archive_url, &archive, MAX_DOWNLOAD)?;
    download(checksum_url, &checksum, 4096)?;
    verify_archive(&archive, &fs::read_to_string(checksum)?, filename)?;
    let files = unpack(&archive, &workspace.path().join("bundle"))?;
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
                    if cfg!(windows) && release.archive.is_some() {
                        offered = true;
                        crate::shell::dialog_buttons(ui, |ui| {
                            update = ui.button(tr("更新并重启")).clicked();
                            close |= ui.button(tr("暂不更新")).clicked();
                        });
                    } else { ui.label(tr("当前平台暂无内置安装包，请到发布页面查看。")); }
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
    use std::io::Write;

    fn release() -> Value {
        let tag = "v1.3.20";
        let filename = "lom_modkit-v1.3.20_windows_x64.zip";
        json!({"tag_name":tag,"draft":false,"prerelease":false,"html_url":format!("{RELEASES}/tag/{tag}"),"body":"fixture release notes","assets":[
            {"name":filename,"size":100,"browser_download_url":format!("{RELEASES}/download/{tag}/{filename}")},
            {"name":format!("{filename}.sha256"),"size":100,"browser_download_url":format!("{RELEASES}/download/{tag}/{filename}.sha256")}
        ]})
    }

    #[test]
    fn selects_only_stable_releases_and_matching_official_assets() {
        assert!(version("v1.3.20").unwrap() > version("1.3.9").unwrap());
        for invalid in ["vv1.2.3", "1.3", "1.3.3-beta", "../1.3.3"] {
            assert!(version(invalid).is_err());
        }
        let mut value = release();
        assert!(parse_release(&value).unwrap().archive.is_some());
        value["prerelease"] = json!(true);
        assert!(parse_release(&value).is_err());
        value["prerelease"] = json!(false);
        value["assets"][0]["browser_download_url"] = json!("https://other.example/update.zip");
        assert!(parse_release(&value).unwrap().archive.is_none());
        value["html_url"] = json!("https://github.com/other/repo/releases/tag/v1.3.20");
        assert!(parse_release(&value).is_err());
    }

    fn fixture_zip(path: &Path, extra: Option<&str>) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for name in [
            "lom-editor.exe",
            "lomc.exe",
            "runtime/MortalModHost.dll",
            "runtime/NVorbis.dll",
            "asset-extractor/lom-asset-extractor.exe",
            "assets/doorstop/win-x64-doorstop.dll",
            "assets/doorstop/win-x86-doorstop.dll",
        ]
        .into_iter()
        .chain(extra)
        {
            zip.start_file(
                format!("lom_modkit/{name}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(b"new program bytes").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn rejects_wrong_checksum_traversal_and_user_data_in_update_packages() {
        let temp = tempfile::tempdir().unwrap();
        for (i, extra) in [
            None,
            Some("../outside.txt"),
            Some("assets/user/character/test.person/content.json"),
            Some("project/story/main.json"),
            Some("LOM-EDITOR.EXE"),
        ]
        .into_iter()
        .enumerate()
        {
            let archive = temp.path().join(format!("{i}.zip"));
            fixture_zip(&archive, extra);
            let checksum = format!("{:x}  {i}.zip", Sha256::digest(fs::read(&archive).unwrap()));
            verify_archive(&archive, &checksum, &format!("{i}.zip")).unwrap();
            assert!(verify_archive(&archive, &checksum, "wrong.zip").is_err());
            assert!(verify_archive(
                &archive,
                &format!("{}  {i}.zip", "0".repeat(64)),
                &format!("{i}.zip")
            )
            .is_err());
            let result = unpack(&archive, &temp.path().join(format!("stage-{i}")));
            assert_eq!(result.is_ok(), extra.is_none(), "{extra:?}");
            if let Ok(files) = result {
                assert_eq!(files.last().unwrap(), "lom-editor.exe");
            }
        }
        assert!(!temp.path().join("outside.txt").exists());
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
        let prepared = prepare(&release).unwrap();
        assert!(prepared.files.iter().any(|name| name == "lom-editor.exe"));
        assert!(prepared
            .workspace
            .path()
            .join("bundle/runtime/MortalModHost.dll")
            .is_file());
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
            let archive = stage.path().join("fixture.zip");
            fixture_zip(&archive, None);
            let files = unpack(&archive, &stage.path().join("bundle")).unwrap();
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
