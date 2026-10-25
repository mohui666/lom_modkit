//! Disposable work files. Windows staging must not depend on a shared TEMP ACL.
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn create() -> Result<tempfile::TempDir> {
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .filter(|value| !value.is_empty())
                    .map(|home| std::path::PathBuf::from(home).join("AppData/Local"))
            })
            .context("无法确定当前用户的临时工作目录：缺少 LOCALAPPDATA / USERPROFILE")?;
        anyhow::ensure!(
            local.is_absolute(),
            "用户缓存目录必须是绝对路径：{}",
            local.display()
        );
        create_in(&local.join("lom_modkit/cache/work"))
    }
    #[cfg(not(windows))]
    {
        create_in(&std::env::temp_dir())
    }
}

fn create_in(parent: &Path) -> Result<tempfile::TempDir> {
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "无法创建临时工作目录，请检查该目录的写入权限：{}",
            parent.display()
        )
    })?;
    tempfile::Builder::new()
        .prefix("lom-modkit-")
        .tempdir_in(parent)
        .with_context(|| {
            format!(
                "无法在临时工作目录创建文件，请检查该目录的写入权限：{}",
                parent.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspaces_are_private_and_cleaned_without_removing_other_files() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("keep.txt");
        fs::write(&marker, b"keep").unwrap();
        let a = create_in(root.path()).unwrap();
        let b = create_in(root.path()).unwrap();
        assert_ne!(a.path(), b.path());
        let path = a.path().to_owned();
        fs::write(path.join("stage.bin"), b"stage").unwrap();
        drop(a);
        assert!(!path.exists());
        assert!(b.path().is_dir());
        assert_eq!(fs::read(marker).unwrap(), b"keep");
    }

    #[test]
    fn unusable_work_directory_reports_its_actual_path() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("not-a-directory");
        fs::write(&file, b"keep").unwrap();
        let error = format!("{:#}", create_in(&file.join("work")).unwrap_err());
        assert!(error.contains("临时工作目录"));
        assert!(error.contains(&file.display().to_string()));
        assert_eq!(fs::read(file).unwrap(), b"keep");
    }
}
