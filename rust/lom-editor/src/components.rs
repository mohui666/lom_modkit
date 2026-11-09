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
        clean_old_components(&root);
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
    clean_old_components(&root);
    Ok(Some(root))
}

/// Only generated sibling directories belong to us. Never traverse reparse
/// points or remove a version that another process is still using.
fn clean_old_components(current: &Path) {
    let Some(parent) = current.parent() else {
        return;
    };
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("无法清理旧组件缓存：{error}");
            return;
        }
    };
    let managed = regex::Regex::new(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?-[0-9a-f]{16}$").unwrap();
    for entry in entries {
        let result = (|| -> Result<()> {
            let entry = entry?;
            let path = entry.path();
            if path == current || !managed.is_match(&entry.file_name().to_string_lossy()) {
                return Ok(());
            }
            let mut files = Vec::new();
            collect_cleanup_files(&path, &mut files)?;
            // Keep these handles until deletion completes. Windows denies new
            // readers and loaded executables/DLLs deny the DELETE access below.
            #[cfg(windows)]
            let _locks = lock_cleanup_files(&files)?;
            fs::remove_dir_all(&path).with_context(|| format!("清理 {} 失败", path.display()))?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("旧组件暂时保留（可能正在使用或无删除权限）：{error:#}");
        }
    }
}

fn collect_cleanup_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "不清理链接路径：{}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "不清理重解析点：{}",
            path.display()
        );
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_cleanup_files(&entry?.path(), files)?;
        }
    } else {
        ensure!(metadata.is_file(), "不清理特殊文件：{}", path.display());
        files.push(path.to_path_buf());
    }
    Ok(())
}

#[cfg(windows)]
fn lock_cleanup_files(files: &[PathBuf]) -> Result<Vec<fs::File>> {
    use std::os::windows::fs::OpenOptionsExt;
    files
        .iter()
        .map(|path| {
            fs::OpenOptions::new()
                .access_mode(0x00010000) // DELETE
                .share_mode(0x00000004) // FILE_SHARE_DELETE; retain locks during removal
                .open(path)
                .with_context(|| format!("组件被占用或无法删除：{}", path.display()))
        })
        .collect()
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

/// The distributed editor also exposes the embedded compiler's headless CLI.
pub fn cli_task(args: &[String]) -> Result<i32> {
    let mut command = std::process::Command::new(cli()?);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command.status()?.code().unwrap_or(1))
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[test]
    fn removes_only_old_generated_directories() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("1.4.19-0123456789abcdef");
        let old = temp.path().join("1.4.18-abcdef0123456789");
        let same_version = temp.path().join("1.4.19-abcdef0123456789");
        let unrelated = temp.path().join("my-files");
        for folder in [&current, &old, &same_version, &unrelated] {
            fs::create_dir_all(folder.join("runtime")).unwrap();
            fs::write(folder.join("runtime/helper.dll"), b"test").unwrap();
        }
        clean_old_components(&current);
        assert!(current.join("runtime/helper.dll").exists());
        assert!(unrelated.join("runtime/helper.dll").exists());
        assert!(!old.exists());
        assert!(!same_version.exists());
    }

    #[cfg(windows)]
    #[test]
    fn occupied_old_version_is_preserved_until_released() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("1.4.19-0123456789abcdef");
        let old = temp.path().join("1.4.18-abcdef0123456789");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("a.txt"), b"keep the whole folder").unwrap();
        fs::write(old.join("helper.exe"), b"busy").unwrap();
        let handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(old.join("helper.exe"))
            .unwrap();
        clean_old_components(&current);
        assert!(old.join("a.txt").exists());
        assert!(old.join("helper.exe").exists());
        drop(handle);
        clean_old_components(&current);
        assert!(!old.exists());
    }

    #[test]
    fn linked_old_folder_is_never_traversed() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("1.4.19-0123456789abcdef");
        let old = temp.path().join("1.4.18-abcdef0123456789");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, old.join("linked")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, old.join("linked")).unwrap();
        clean_old_components(&current);
        assert!(old.exists());
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
    }
}
