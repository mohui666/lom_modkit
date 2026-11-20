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
    content::character_extensions(&assets)?;
    project.assets = assets;
    Ok(())
}

pub fn story_references(project: &Project, metadata: &Value) -> Vec<editing::SearchHit> {
    let symbol = format!("user:{}", metadata["id"].as_str().unwrap_or(""));
    let mut refs = editing::references(
        &project.stories,
        &project.manifest,
        "content",
        &symbol,
        None,
    );
    refs.extend(extension_portrait_references(project, metadata, false));
    refs
}

fn extension_target(metadata: &Value) -> Option<&str> {
    metadata["extends"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

fn official_portraits(character: &str) -> &[Value] {
    crate::validate::editor_data()["characters"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"] == character)
        .and_then(|row| row["portraits"].as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn extension_portrait_references(
    project: &Project,
    metadata: &Value,
    required_only: bool,
) -> Vec<editing::SearchHit> {
    let Some(character) = extension_target(metadata) else {
        return Vec::new();
    };
    let official = official_portraits(character);
    let symbol = format!("user:{}", metadata["id"].as_str().unwrap_or(""));
    let mut refs = Vec::new();
    for (story_id, story) in &project.stories {
        for parent in story["nodes"].as_array().into_iter().flatten() {
            for (index, node) in crate::parallel::actions(parent).iter().enumerate() {
                let official_actor = node["character"] == character;
                // References by user: ID are already in the ordinary content list.
                if !(official_actor || required_only && node["character"] == symbol)
                    || !matches!(node["type"].as_str(), Some("show" | "say" | "portrait"))
                {
                    continue;
                }
                let explicit = node["portrait"].as_str();
                let portrait = explicit.unwrap_or("normal");
                if metadata["portraits"].get(portrait).is_none() {
                    continue;
                }
                if explicit.is_none()
                    && node["type"] == "say"
                    && matches!(
                        node["mode"].as_str(),
                        Some("narrative" | "center" | "offscreen")
                    )
                {
                    continue;
                }
                // An omitted portrait can use the game's default again. An explicit
                // portrait must still exist in the base catalog to pass validation.
                if required_only
                    && official_actor
                    && (explicit.is_none() || official.iter().any(|p| p == portrait))
                {
                    continue;
                }
                let field = if parent["type"] == "parallel" {
                    format!("actions[{index}].portrait")
                } else {
                    "portrait".into()
                };
                refs.push(editing::hit(
                    "portrait",
                    story_id,
                    parent["id"].as_str(),
                    &field,
                    portrait,
                ));
            }
        }
    }
    refs
}

fn ensure_portraits_removable(
    project: &Project,
    metadata: &Value,
    portrait: Option<&str>,
) -> Result<()> {
    if let Some(hit) = extension_portrait_references(project, metadata, true)
        .into_iter()
        .find(|hit| portrait.is_none_or(|id| hit.value == id))
    {
        anyhow::bail!(
            "story/{}.json: 节点 \"{}\"({}) 仍使用扩展表情 {}，请先为该步骤选择其他表情",
            hit.story_id,
            hit.node_id.as_deref().unwrap_or("?"),
            hit.field,
            hit.value
        );
    }
    Ok(())
}

fn clear_portrait_alignment(fields: &mut serde_json::Map<String, Value>, portrait: &str) {
    if fields
        .get("portrait_reference")
        .is_some_and(|value| value == portrait)
    {
        fields.remove("portrait_reference");
    }
    if let Some(layouts) = fields
        .get_mut("portrait_layouts")
        .and_then(Value::as_object_mut)
    {
        layouts.remove(portrait);
        if layouts.is_empty() {
            fields.remove("portrait_layouts");
        }
    }
}

pub fn remove_extension_portrait(project: &mut Project, key: &str, portrait: &str) -> Result<()> {
    let mut meta: Value = serde_json::from_slice(project.assets.get(key).context("内容不存在")?)?;
    ensure!(
        extension_target(&meta).is_some(),
        "仅支持移除原版人物的扩展表情"
    );
    ensure_portraits_removable(project, &meta, Some(portrait))?;
    let portraits = meta["portraits"].as_object_mut().context("缺少扩展表情")?;
    let filename = portraits
        .remove(portrait)
        .and_then(|file| file.as_str().map(str::to_owned))
        .context("找不到要移除的扩展表情")?;
    let Some(next_main) = portraits.values().next().cloned() else {
        return remove(project, key);
    };
    if meta["files"]["main"] == filename {
        meta["files"]["main"] = next_main;
    }
    clear_portrait_alignment(meta.as_object_mut().unwrap(), portrait);
    let prefix = key.strip_suffix("content.json").context("内容路径无效")?;
    update(project, key, &meta, &BTreeMap::new())?;
    if !content::listed_content_files(&meta).contains(&filename) {
        project.assets.remove(&format!("{prefix}{filename}"));
    }
    Ok(())
}

fn is_editor_replacement_filename(filename: &str) -> bool {
    let Some((stem, extension)) = filename.rsplit_once('.') else {
        return false;
    };
    matches!(extension, "png" | "jpg" | "jpeg")
        && stem
            .strip_prefix("asset_")
            .and_then(|s| s.split_once('_'))
            .is_some_and(|(process, time)| {
                !process.is_empty()
                    && !time.is_empty()
                    && process.bytes().all(|b| b.is_ascii_digit())
                    && time.bytes().all(|b| b.is_ascii_digit())
            })
}

/// Correct the legacy main-slot write without changing imported image bytes.
/// Apply this as an undoable editor edit after the original project has loaded.
pub fn repair_legacy_main_portrait_overrides(project: &mut Project) -> Result<usize> {
    let mut repaired = BTreeMap::new();
    for (key, bytes) in &project.assets {
        if !key.starts_with("assets/user/character/") || !key.ends_with("/content.json") {
            continue;
        }
        let Ok(mut meta) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        let Some(character) = extension_target(&meta) else {
            continue;
        };
        let Some(portraits) = meta["portraits"].as_object() else {
            continue;
        };
        let Some(normal) = portraits.get("normal").and_then(Value::as_str) else {
            continue;
        };
        // Main-slot replacements generated one filename for both fields.
        // Explicit portrait-slot replacements generate a separate filename.
        if portraits.len() < 2
            || meta["files"]["main"] != normal
            || !is_editor_replacement_filename(normal)
            || !official_portraits(character).iter().any(|p| p == "normal")
            || extension_portrait_references(project, &meta, true)
                .iter()
                .any(|r| r.value == "normal")
        {
            continue;
        }
        meta["portraits"].as_object_mut().unwrap().remove("normal");
        clear_portrait_alignment(meta.as_object_mut().unwrap(), "normal");
        repaired.insert(key.clone(), stable_json(&meta)?);
    }
    let count = repaired.len();
    project.assets.extend(repaired);
    Ok(count)
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
    ensure_portraits_removable(project, &meta, None)?;
    if let Some(config) = &project.scenario {
        ensure!(
            !content::scenario_gameplay_nodes(config)
                .iter()
                .any(|node| node["item"] == symbol
                    || node["talent"] == symbol
                    || node["character"] == symbol),
            "事件或任务结果仍引用此内容，请先解除引用"
        );
    }
    let prefix = format!(
        "{}/",
        key.strip_suffix("/content.json").context("内容路径无效")?
    );
    for (name, data) in &project.assets {
        if name.ends_with("/content.json") && !name.starts_with(&prefix) {
            if let Ok(m) = serde_json::from_slice::<Value>(data) {
                ensure!(m["character"] != symbol, "仍有关联配音，请先解除角色绑定");
                ensure!(
                    !m["levels"].as_array().into_iter().flatten().any(|level| {
                        level["talents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|grant| grant["talent"] == symbol)
                    }),
                    "仍有功法升级效果引用此天赋，请先解除引用"
                );
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
    for &other in content::CONTENT_TYPES {
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
