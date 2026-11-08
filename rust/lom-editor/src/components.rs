//! Shipped helpers live inside the editor. Only managed copies enter the cache.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
};
include!(concat!(env!("OUT_DIR"), "/components.rs"));

#[derive(Deserialize)]
struct Entry {
    path: String,
    size: u64,
}
static ROOT: OnceLock<std::result::Result<Option<PathBuf>, String>> = OnceLock::new();
pub fn root() -> Result<Option<PathBuf>> {
    match ROOT.get_or_init(|| materialize().map_err(|e| format!("{e:#}"))) {
        Ok(path) => Ok(path.clone()),
        Err(error) => anyhow::bail!("准备内置组件失败：{error}"),
    }
}
fn materialize() -> Result<Option<PathBuf>> {
    if PAYLOAD.is_empty() {
        return Ok(None);
    }
    let cache = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
        .context("无法确定组件缓存位置")?;
    let root = cache.join("lom_modkit/components").join(format!(
        "{}-{}",
        env!("CARGO_PKG_VERSION"),
        &KEY[..16]
    ));
    let mut archive = zip::ZipArchive::new(Cursor::new(PAYLOAD))?;
    let entries: Vec<Entry> = serde_json::from_reader(archive.by_name("components.json")?)?;
    let marker = root.join(".complete");
    let ready = fs::read_to_string(&marker).is_ok_and(|s| s == KEY)
        && entries.iter().all(|entry| {
            fs::metadata(root.join(&entry.path)).is_ok_and(|m| m.is_file() && m.len() == entry.size)
        });
    if ready {
        return Ok(Some(root));
    }
    fs::create_dir_all(&root).context("无法创建组件缓存，请检查系统磁盘空间和写入权限")?;
    for entry in entries {
        let mut file = archive.by_name(&entry.path)?;
        let relative = file.enclosed_name().context("内置组件路径无效")?;
        ensure!(
            file.is_file() && file.size() == entry.size,
            "内置组件清单不一致：{}",
            entry.path
        );
        let destination = root.join(relative);
        if fs::metadata(&destination).is_ok_and(|m| m.is_file() && m.len() == entry.size) {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size as usize);
        file.read_to_end(&mut bytes)?;
        lom_core::project::atomic_write(&destination, &bytes).with_context(|| {
            format!(
                "无法准备组件 {}，请关闭正在进行的试听或提取后重开编辑器",
                entry.path
            )
        })?;
        #[cfg(unix)]
        if entry.path.ends_with("lomc") || entry.path.ends_with("lom-asset-extractor") {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))?;
        }
    }
    lom_core::project::atomic_write(&marker, KEY.as_bytes())?;
    Ok(Some(root))
}
pub fn file(relative: &str) -> Result<PathBuf> {
    if let Some(root) = root()? {
        let path = root.join(relative);
        ensure!(path.is_file(), "内置组件缺失：{relative}");
        return Ok(path);
    }
    let exe = std::env::current_exe()?;
    let folder = exe.parent().context("无法确定编辑器位置")?;
    [
        folder.join(relative),
        folder.join("../Resources").join(relative),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .with_context(|| format!("开发目录缺少组件：{relative}；请使用发布版编辑器或运行构建脚本"))
}
pub fn runtime() -> Option<PathBuf> {
    // Startup reports materialization errors before constructing the workspace.
    root()
        .ok()
        .flatten()
        .map(|r| r.join("runtime"))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("runtime")))
                .filter(|p| p.join("MortalModHost.dll").is_file())
        })
}
pub fn is_managed_runtime(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "components")
        && path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|n| n == "components")
}
pub fn cli() -> Result<PathBuf> {
    file(if cfg!(windows) { "lomc.exe" } else { "lomc" })
}
pub fn extractor() -> Result<PathBuf> {
    file(if cfg!(windows) {
        "asset-extractor/lom-asset-extractor.exe"
    } else {
        "asset-extractor/lom-asset-extractor"
    })
}
pub fn decoder() -> Result<PathBuf> {
    file(if cfg!(windows) {
        "audio-decoder/vgmstream-cli.exe"
    } else {
        "audio-decoder/vgmstream-cli"
    })
}

pub fn task(args: &[String]) -> Result<i32> {
    let helper = extractor()?;
    let mut command = std::process::Command::new(helper);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command.status()?.code().unwrap_or(1))
}
