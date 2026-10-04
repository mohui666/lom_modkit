//! Save through the core ownership checks, then remove only verified, saved files.
use anyhow::{ensure, Context, Result};
use lom_core::{load_json, package, project::Project};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// The snapshots must describe the last successful save/open of this project.
/// Save-as and package export never remove files from the previous source.
pub fn save_project(
    project: &mut Project,
    target: &Path,
    saved_assets: &BTreeMap<String, Vec<u8>>,
    saved_stories: &BTreeMap<String, Value>,
    saved_paths: &BTreeMap<String, PathBuf>,
) -> Result<()> {
    let Some(root) = same_source_root(project, target)? else {
        return project.save_to(target);
    };
    let removed_assets: Vec<_> = saved_assets
        .iter()
        .filter(|(name, _)| !project.assets.contains_key(*name))
        .collect();
    let mut removed_stories = Vec::new();
    if target.extension().and_then(|s| s.to_str()) != Some("json") {
        let current_files: BTreeSet<_> = project
            .stories
            .keys()
            .map(|id| {
                project
                    .paths
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from(format!("{id}.json")))
                    .to_string_lossy()
                    .to_lowercase()
            })
            .collect();
        for (id, filename) in saved_paths {
            let name = filename.to_str().context("已保存章节文件名无效")?;
            if current_files.contains(&name.to_lowercase()) {
                continue;
            }
            package::canonical_archive_name(name)?;
            ensure!(
                !name.contains('/')
                    && filename.extension().and_then(|s| s.to_str()) == Some("json"),
                "已保存章节文件名无效：{name}"
            );
            let relative = if project.story_subdir {
                format!("story/{name}")
            } else {
                name.to_owned()
            };
            let expected = saved_stories.get(id).context("已保存章节快照不完整")?;
            removed_stories.push((relative, expected));
        }
    }

    // Validate every deletion before allowing any project documents to be written.
    for (name, expected) in &removed_assets {
        verify_asset(&root, name, expected)?;
    }
    for (name, expected) in &removed_stories {
        verify_story(&root, name, expected)?;
    }
    project.save_to(target)?;

    // Check again after saving: an external edit during the write must not be deleted.
    for (name, expected) in removed_assets {
        if let Some(path) = verify_asset(&root, name, expected)? {
            remove_owned_file(&path)?;
        }
    }
    for (name, expected) in removed_stories {
        if let Some(path) = verify_story(&root, &name, expected)? {
            remove_owned_file(&path)?;
        }
    }
    Ok(())
}

fn same_source_root(project: &Project, target: &Path) -> Result<Option<PathBuf>> {
    if target.extension().and_then(|s| s.to_str()) == Some("lommod") {
        return Ok(None);
    }
    let Some(source) = project.source.as_deref() else {
        return Ok(None);
    };
    let canonical = |path: &Path| -> Result<Option<PathBuf>> {
        match path.canonicalize() {
            Ok(path) => Ok(Some(path)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("无法确认保存路径：{}", path.display()))
            }
        }
    };
    let (Some(source), Some(target_path)) = (canonical(source)?, canonical(target)?) else {
        return Ok(None);
    };
    if source != target_path {
        return Ok(None);
    }
    if target.extension().and_then(|s| s.to_str()) == Some("json") {
        let parent = target_path.parent().context("无效项目保存路径")?;
        Ok(Some(
            if parent.file_name().and_then(|s| s.to_str()) == Some("story") {
                parent.parent().unwrap_or(parent)
            } else {
                parent
            }
            .to_owned(),
        ))
    } else {
        Ok(Some(target_path))
    }
}

fn owned_file(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    package::canonical_archive_name(name)?;
    ensure!(!root.is_symlink(), "拒绝通过符号链接删除项目文件");
    let mut path = root.to_owned();
    let mut parts = name.split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("无法检查待删除文件：{name}")),
        };
        ensure!(
            !metadata.file_type().is_symlink(),
            "拒绝通过符号链接删除项目文件：{name}"
        );
        ensure!(
            if parts.peek().is_some() {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
            "待删除项目路径不是普通文件或目录：{name}"
        );
    }
    package::resolve_confined_file(root, &path)?;
    Ok(Some(path))
}

fn verify_asset(root: &Path, name: &str, expected: &[u8]) -> Result<Option<PathBuf>> {
    ensure!(
        name.starts_with("assets/"),
        "待删除资源不在 assets 内：{name}"
    );
    let path = owned_file(root, name)?;
    if let Some(path) = &path {
        ensure!(
            fs::read(path)? == expected,
            "资源已在编辑器外修改，未删除：{name}"
        );
    }
    Ok(path)
}

fn verify_story(root: &Path, name: &str, expected: &Value) -> Result<Option<PathBuf>> {
    let path = owned_file(root, name)?;
    if let Some(path) = &path {
        ensure!(
            load_json(path)? == *expected,
            "章节已在编辑器外修改，未删除：{name}"
        );
    }
    Ok(path)
}

fn remove_owned_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("保存后清理已删除文件失败：{}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removed_assets_stay_removed_for_directory_and_both_single_json_layouts() {
        for layout in ["directory", "json", "story-json"] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let target = match layout {
                "directory" => root.to_owned(),
                "json" => root.join("main.json"),
                _ => root.join("story/main.json"),
            };
            let mut project = Project::new();
            project
                .assets
                .insert("assets/removed.bin".into(), b"owned".to_vec());
            project.save_to(&target).unwrap();
            let saved = project.clone();
            fs::write(root.join("assets/foreign.bin"), b"foreign").unwrap();
            project.assets.remove("assets/removed.bin");
            save_project(
                &mut project,
                &target,
                &saved.assets,
                &saved.stories,
                &saved.paths,
            )
            .unwrap();
            let reopened = Project::open(&target).unwrap();
            assert!(
                !reopened.assets.contains_key("assets/removed.bin"),
                "{layout}"
            );
            assert_eq!(reopened.assets["assets/foreign.bin"], b"foreign");
        }
    }

    #[test]
    fn externally_modified_asset_blocks_save_before_other_documents_change() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut project = Project::new();
        project
            .assets
            .insert("assets/removed.bin".into(), b"owned".to_vec());
        project.save_to(root).unwrap();
        let saved = project.clone();
        let original_manifest = fs::read(root.join("manifest.json")).unwrap();
        project.assets.clear();
        project.manifest["name"] = json!("not yet saved");
        fs::write(root.join("assets/removed.bin"), b"external edit").unwrap();
        assert!(save_project(
            &mut project,
            root,
            &saved.assets,
            &saved.stories,
            &saved.paths
        )
        .is_err());
        assert_eq!(
            fs::read(root.join("assets/removed.bin")).unwrap(),
            b"external edit"
        );
        assert_eq!(
            fs::read(root.join("manifest.json")).unwrap(),
            original_manifest
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_files_and_parent_directories_are_not_deleted_even_when_bytes_match() {
        for link_parent in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            let root = directory.path();
            let mut project = Project::new();
            project
                .assets
                .insert("assets/folder/item.bin".into(), b"owned".to_vec());
            project.save_to(root).unwrap();
            let saved = project.clone();
            project.assets.clear();
            let external_file = outside.path().join("item.bin");
            fs::write(&external_file, b"owned").unwrap();
            let link = if link_parent {
                let path = root.join("assets/folder");
                fs::remove_dir_all(&path).unwrap();
                std::os::unix::fs::symlink(outside.path(), &path).unwrap();
                path
            } else {
                let path = root.join("assets/folder/item.bin");
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink(&external_file, &path).unwrap();
                path
            };
            assert!(save_project(
                &mut project,
                root,
                &saved.assets,
                &saved.stories,
                &saved.paths
            )
            .is_err());
            assert!(link.is_symlink());
            assert_eq!(fs::read(external_file).unwrap(), b"owned");
        }
    }

    #[test]
    fn save_as_and_package_export_leave_removed_files_in_the_previous_source() {
        for package_export in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("original");
            let mut project = Project::new();
            project
                .assets
                .insert("assets/removed.bin".into(), b"owned".to_vec());
            project.save_to(&root).unwrap();
            let saved = project.clone();
            project.assets.clear();
            let target = directory.path().join(if package_export {
                "export.lommod"
            } else {
                "copy"
            });
            save_project(
                &mut project,
                &target,
                &saved.assets,
                &saved.stories,
                &saved.paths,
            )
            .unwrap();
            assert_eq!(fs::read(root.join("assets/removed.bin")).unwrap(), b"owned");
            assert!(!Project::open(&target)
                .unwrap()
                .assets
                .contains_key("assets/removed.bin"));
        }
    }

    #[test]
    fn removed_chapter_does_not_reappear_and_reused_filename_survives_rename() {
        for rename in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let mut project = Project::new();
            let mut chapter = project.stories["main"].clone();
            chapter["id"] = json!("chapter1");
            project.stories.insert("chapter1".into(), chapter);
            project
                .paths
                .insert("chapter1".into(), "owned-name.json".into());
            project.save_to(root).unwrap();
            let saved = project.clone();
            // A formatting-only external change does not change chapter ownership.
            fs::write(
                root.join("story/owned-name.json"),
                serde_json::to_vec(&saved.stories["chapter1"]).unwrap(),
            )
            .unwrap();
            let mut chapter = project.stories.remove("chapter1").unwrap();
            let path = project.paths.remove("chapter1").unwrap();
            if rename {
                chapter["id"] = json!("renamed");
                project.stories.insert("renamed".into(), chapter);
                project.paths.insert("renamed".into(), path);
            }
            fs::write(root.join("story/foreign.json"), b"{\"unrelated\":true}").unwrap();
            save_project(
                &mut project,
                root,
                &saved.assets,
                &saved.stories,
                &saved.paths,
            )
            .unwrap();
            let reopened = Project::open(root).unwrap();
            assert!(!reopened.stories.contains_key("chapter1"));
            assert_eq!(reopened.stories.contains_key("renamed"), rename);
            assert_eq!(root.join("story/owned-name.json").exists(), rename);
            assert!(root.join("story/foreign.json").is_file());
        }
    }

    #[test]
    fn externally_changed_chapter_is_preserved_and_already_missing_file_is_allowed() {
        for missing in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let mut project = Project::new();
            let mut chapter = project.stories["main"].clone();
            chapter["id"] = json!("extra");
            project.stories.insert("extra".into(), chapter);
            project.save_to(root).unwrap();
            let saved = project.clone();
            project.stories.remove("extra");
            project.paths.remove("extra");
            let file = root.join("story/extra.json");
            if missing {
                fs::remove_file(&file).unwrap();
            } else {
                let mut external = saved.stories["extra"].clone();
                external["title"] = json!("external edit");
                fs::write(&file, serde_json::to_vec(&external).unwrap()).unwrap();
            }
            let result = save_project(
                &mut project,
                root,
                &saved.assets,
                &saved.stories,
                &saved.paths,
            );
            assert_eq!(result.is_ok(), missing);
            if !missing {
                assert_eq!(load_json(file).unwrap()["title"], "external edit");
            }
        }
    }
}
