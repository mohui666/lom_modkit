//! Transactional content editing. Shared-library removal is recoverable.
use crate::{
    content, content_library, editing, package,
    project::{self, Project},
    stable_json,
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub fn update(
    project: &mut Project,
    key: &str,
    metadata: &Value,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    package::canonical_archive_name(key)?;
    let old: Value = serde_json::from_slice(project.assets.get(key).context("内容不存在")?)?;
    ensure!(
        metadata["id"] == old["id"] && metadata["type"] == old["type"],
        "内容 ID 与类型不能原地改变"
    );
    let normalized = content::normalize_content_metadata(metadata)?;
    let prefix = format!(
        "{}/",
        key.strip_suffix("/content.json").context("内容路径无效")?
    );
    let mut assets = project.assets.clone();
    for (name, bytes) in files {
        package::canonical_archive_name(name)?;
        ensure!(
            !name.contains('/') && !name.contains('\\'),
            "素材文件不能包含目录"
        );
        let ext = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let limit = if ["wav", "ogg"].contains(&ext.as_str()) {
            20 * 1024 * 1024
        } else {
            8 * 1024 * 1024
        };
        ensure!(
            !bytes.is_empty() && bytes.len() <= limit,
            "素材为空或超过大小限制"
        );
        ensure!(
            ["wav", "ogg", "png", "jpg", "jpeg"].contains(&ext.as_str()),
            "不支持的素材格式"
        );
        assets.insert(format!("{prefix}{name}"), bytes.clone());
    }
    for name in content::listed_content_files(&normalized) {
        ensure!(
            assets.contains_key(&format!("{prefix}{name}")),
            "缺少素材 {name}"
        );
    }
    assets.insert(
        key.into(),
        stable_json(&content::content_metadata_payload(&normalized))?,
    );
    project.assets = assets;
    Ok(())
}
pub fn remove(project: &mut Project, key: &str) -> Result<()> {
    let meta: Value = serde_json::from_slice(project.assets.get(key).context("内容不存在")?)?;
    let symbol = format!("user:{}", meta["id"].as_str().context("缺少内容 ID")?);
    let refs = editing::references(
        &project.stories,
        &project.manifest,
        "content",
        &symbol,
        None,
    );
    ensure!(
        refs.is_empty(),
        "仍有 {} 处剧情引用，请先解除引用",
        refs.len()
    );
    let prefix = format!(
        "{}/",
        key.strip_suffix("/content.json").context("内容路径无效")?
    );
    for (name, data) in &project.assets {
        if name.ends_with("/content.json") && !name.starts_with(&prefix) {
            if let Ok(m) = serde_json::from_slice::<Value>(data) {
                ensure!(m["character"] != symbol, "仍有关联配音，请先解除角色绑定");
            }
        }
    }
    project.assets.retain(|name, _| !name.starts_with(&prefix));
    Ok(())
}
fn confined_destination(root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(root).context("目标必须位于内容库中")?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        ensure!(
            matches!(part, std::path::Component::Normal(_)),
            "内容路径无效"
        );
        current.push(part);
        if let Ok(meta) = fs::symlink_metadata(&current) {
            ensure!(
                !meta.file_type().is_symlink(),
                "拒绝修改符号链接路径 {}",
                current.display()
            );
        }
    }
    Ok(())
}
fn backup_path(root: &Path) -> PathBuf {
    root.join(".trash").join(uuid::Uuid::new_v4().to_string())
}
pub fn remove_shared(root: &Path, id: &str) -> Result<PathBuf> {
    let rec = content_library::get_content(root, id)?;
    confined_destination(root, &rec.folder)?;
    confined_destination(root, &root.join(".trash"))?;
    ensure!(!rec.folder.is_symlink(), "拒绝修改符号链接目录");
    let backup = backup_path(root);
    fs::create_dir_all(backup.parent().unwrap())?;
    fs::rename(&rec.folder, &backup)?;
    if let Err(e) = content_library::rebuild_index(root) {
        fs::rename(&backup, &rec.folder)?;
        return Err(e);
    }
    Ok(backup)
}
pub fn store_shared(
    root: &Path,
    project: &Project,
    key: &str,
    replace: bool,
) -> Result<Option<PathBuf>> {
    let meta: Value = serde_json::from_slice(project.assets.get(key).context("内容不存在")?)?;
    let normalized = content::normalize_content_metadata(&meta)?;
    let id = normalized["id"].as_str().context("缺少内容 ID")?;
    let kind = normalized["type"].as_str().context("缺少内容类型")?;
    let relative = content::package_content_dir(kind, id)?;
    let folder = root.join(&relative);
    confined_destination(root, &folder)?;
    confined_destination(root, &root.join(".trash"))?;
    for other in ["character", "image", "audio"] {
        let path = root.join(content::package_content_dir(other, id)?);
        ensure!(other == kind || !path.exists(), "内容 ID 已被其他类型占用");
    }
    ensure!(!folder.is_symlink(), "拒绝覆盖符号链接目录");
    if let Ok(existing) = content_library::get_content(root, id) {
        ensure!(
            replace && existing.content_type == kind,
            "共享库已有该 ID；请明确选择替换"
        );
    }
    ensure!(!folder.exists() || replace, "共享库已有该内容");
    fs::create_dir_all(folder.parent().unwrap())?;
    let temp = tempfile::tempdir_in(folder.parent().unwrap())?;
    let prefix = key.strip_suffix("content.json").context("内容路径无效")?;
    let share = folder.join(".lomcontent.json");
    if share.exists() {
        package::resolve_confined_file(root, &share)?;
        fs::copy(&share, temp.path().join(".lomcontent.json"))?;
    }
    for name in content::listed_content_files(&normalized) {
        let bytes = project
            .assets
            .get(&format!("{prefix}{name}"))
            .with_context(|| format!("缺少素材 {name}"))?;
        project::atomic_write(&temp.path().join(name), bytes)?;
    }
    project::atomic_write(
        &temp.path().join("content.json"),
        &stable_json(&content::content_metadata_payload(&normalized))?,
    )?;
    let backup = if folder.exists() {
        let backup = backup_path(root);
        fs::create_dir_all(backup.parent().unwrap())?;
        fs::rename(&folder, &backup)?;
        Some(backup)
    } else {
        None
    };
    if let Err(e) = fs::rename(temp.path(), &folder) {
        if let Some(ref b) = backup {
            fs::rename(b, &folder)?;
        }
        return Err(e.into());
    }
    if let Err(e) = content_library::rebuild_index(root) {
        fs::remove_dir_all(&folder)?;
        if let Some(ref b) = backup {
            fs::rename(b, &folder)?;
        }
        return Err(e);
    }
    Ok(backup)
}
