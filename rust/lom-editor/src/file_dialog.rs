//! Personal source-directory memory shared by the editor's asset import dialogs.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) fn at(directory: Option<&Path>) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new();
    match directory.filter(|directory| directory.is_dir()) {
        Some(directory) => dialog.set_directory(directory),
        None => dialog,
    }
}

pub(crate) fn project_directory(project: &lom_core::project::Project) -> Option<&Path> {
    project
        .source
        .as_deref()
        .and_then(|source| {
            if source.is_dir() {
                Some(source)
            } else {
                source.parent()
            }
        })
        .filter(|directory| directory.is_dir())
}

fn settings_path() -> PathBuf {
    lom_core::content::default_repository_root()
        .parent()
        .expect("personal repository root")
        .join("rust/file-dialogs.json")
}

fn settings(path: &Path) -> Result<Value> {
    if !path.try_exists()? {
        return Ok(json!({}));
    }
    let settings = lom_core::load_json(path).context("无法读取上次素材来源目录")?;
    ensure!(settings.is_object(), "素材来源目录设置格式不正确");
    Ok(settings)
}

fn initial_directory(path: &Path, preferred: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(directory) = preferred.filter(|directory| directory.is_dir()) {
        return Ok(Some(std::path::absolute(directory)?));
    }
    let settings = settings(path)?;
    let Some(directory) = settings["asset_source_dir"].as_str() else {
        return Ok(None);
    };
    let mut directory = Path::new(directory);
    if !directory.is_absolute() {
        return Ok(None);
    }
    // A moved/deleted source folder can still start at its existing parent.
    // Reading it never replaces the saved choice with a guessed directory.
    while !directory.is_dir() {
        let Some(parent) = directory.parent() else {
            return Ok(None);
        };
        directory = parent;
    }
    Ok(Some(directory.to_path_buf()))
}

fn remember_selection(path: &Path, directory: Option<&Path>) -> Result<()> {
    let Some(directory) = directory else {
        return Ok(());
    };
    ensure!(directory.is_dir(), "所选素材来源目录已不存在");
    let directory = std::path::absolute(directory)?;
    let mut settings = settings(path)?;
    settings["asset_source_dir"] = json!(directory);
    lom_core::project::atomic_write(path, &lom_core::stable_json(&settings)?)
        .context("已选择素材，但无法记住素材来源目录")
}

fn dialog(path: &Path, preferred: Option<&Path>) -> Result<rfd::FileDialog> {
    let dialog = rfd::FileDialog::new();
    Ok(
        if let Some(directory) = initial_directory(path, preferred)? {
            dialog.set_directory(directory)
        } else {
            dialog
        },
    )
}

pub(crate) fn asset_files(
    label: &str,
    extensions: &[&str],
    multiple: bool,
) -> Result<Option<Vec<PathBuf>>> {
    asset_files_in(label, extensions, multiple, None)
}

/// Node image imports start in the selected preview library. Ordinary asset
/// imports keep using the last personal source directory.
pub(crate) fn asset_files_in(
    label: &str,
    extensions: &[&str],
    multiple: bool,
    preferred: Option<&Path>,
) -> Result<Option<Vec<PathBuf>>> {
    let settings = settings_path();
    let dialog = dialog(&settings, preferred)?.add_filter(label, extensions);
    let files = if multiple {
        dialog.pick_files()
    } else {
        dialog.pick_file().map(|path| vec![path])
    };
    remember_selection(
        &settings,
        files
            .as_ref()
            .and_then(|paths| paths.first())
            .and_then(|path| path.parent()),
    )?;
    Ok(files)
}

pub(crate) fn asset_folder() -> Result<Option<PathBuf>> {
    let settings = settings_path();
    let folder = dialog(&settings, None)?.pick_folder();
    remember_selection(&settings, folder.as_deref())?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_directory_survives_a_new_reader_and_preserves_other_personal_settings() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("personal/file-dialogs.json");
        let source = root.path().join("作者图片与音频");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(config.parent().unwrap()).unwrap();
        std::fs::write(&config, b"{\"other\":true}").unwrap();
        remember_selection(&config, Some(&source)).unwrap();
        assert_eq!(initial_directory(&config, None).unwrap(), Some(source));
        assert_eq!(settings(&config).unwrap()["other"], true);
    }

    #[test]
    fn cancelled_selection_does_not_create_or_change_the_saved_directory() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("file-dialogs.json");
        remember_selection(&config, None).unwrap();
        assert!(!config.exists());
        remember_selection(&config, Some(root.path())).unwrap();
        let original = std::fs::read(&config).unwrap();
        remember_selection(&config, None).unwrap();
        assert_eq!(std::fs::read(&config).unwrap(), original);
    }

    #[test]
    fn unavailable_source_uses_an_existing_parent_without_rewriting_the_record() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("file-dialogs.json");
        let missing = root.path().join("moved/source");
        std::fs::write(
            &config,
            lom_core::stable_json(&json!({"asset_source_dir":missing})).unwrap(),
        )
        .unwrap();
        let original = std::fs::read(&config).unwrap();
        assert_eq!(
            initial_directory(&config, None).unwrap(),
            Some(root.path().to_path_buf())
        );
        assert_eq!(std::fs::read(&config).unwrap(), original);
    }

    #[test]
    fn explicit_library_directory_precedes_memory_and_invalid_choices_use_memory() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("file-dialogs.json");
        let source = root.path().join("自定义图片");
        let library = root.path().join("已提取预览素材库");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&library).unwrap();
        remember_selection(&config, Some(&source)).unwrap();
        let original = std::fs::read(&config).unwrap();
        assert_eq!(
            initial_directory(&config, Some(&library)).unwrap(),
            Some(library)
        );
        assert_eq!(
            initial_directory(&config, None).unwrap(),
            Some(source.clone())
        );
        assert_eq!(
            initial_directory(&config, Some(&root.path().join("missing"))).unwrap(),
            Some(source.clone())
        );
        let file = root.path().join("not-a-directory.png");
        std::fs::write(&file, b"fixture").unwrap();
        assert_eq!(
            initial_directory(&config, Some(&file)).unwrap(),
            Some(source)
        );
        assert_eq!(std::fs::read(&config).unwrap(), original);
    }
}
