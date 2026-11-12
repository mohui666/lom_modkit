//! Merge authored projects without changing either source on disk.
use crate::{project::Project, stable_json};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct MergeReport {
    pub chapters: usize,
    pub assets: usize,
    pub events: usize,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn namespaced(prefix: &str, id: &str) -> String {
    format!("{prefix}_{id}")
}
fn uses_namespace(value: &Value, prefix: &str) -> bool {
    match value {
        Value::String(value) => value.to_ascii_lowercase().starts_with(prefix),
        Value::Array(items) => items.iter().any(|item| uses_namespace(item, prefix)),
        Value::Object(object) => object.iter().any(|(key, item)| {
            key.to_ascii_lowercase().starts_with(prefix)
                || !matches!(
                    key.as_str(),
                    "text" | "description" | "code" | "_editor" | "localization"
                ) && uses_namespace(item, prefix)
        }),
        _ => false,
    }
}
fn declared_flags(value: &Value, flags: &mut BTreeSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                declared_flags(item, flags);
            }
        }
        Value::Object(object) => {
            if value["type"] == "flag" {
                if let Some(flag) = value["flag"].as_str().or_else(|| value["key"].as_str()) {
                    flags.insert(flag.to_owned());
                }
            }
            for (key, item) in object {
                if !matches!(key.as_str(), "_editor" | "localization") {
                    declared_flags(item, flags);
                }
            }
        }
        _ => {}
    }
}
fn rewrite(
    value: &mut Value,
    stories: &BTreeMap<String, String>,
    assets: &BTreeMap<String, String>,
    prefix: &str,
    flags: &BTreeSet<String>,
) -> Result<()> {
    match value {
        Value::Array(items) => {
            for item in items {
                rewrite(item, stories, assets, prefix, flags)?;
            }
        }
        Value::Object(object) => {
            let kind = object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let source = object
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            for (key, item) in object.iter_mut() {
                if let Some(raw) = item.as_str() {
                    let mapped = if matches!(key.as_str(), "next_script" | "script") {
                        stories.get(raw).cloned()
                    } else if matches!(
                        key.as_str(),
                        "image"
                            | "voice"
                            | "character"
                            | "mood_character"
                            | "name"
                            | "item"
                            | "talent"
                            | "key"
                            | "avatar"
                    ) {
                        assets.get(raw).cloned()
                    } else {
                        None
                    };
                    if let Some(mapped) = mapped {
                        *item = json!(mapped);
                        continue;
                    }
                    let private = key == "quest"
                        || key == "key"
                            && (matches!(kind.as_str(), "persistent_var" | "persistent_check")
                                || matches!(source.as_str(), "variable" | "task" | "mod")
                                || source == "flag" && flags.contains(raw)
                                || kind == "flag")
                        || key == "flag"
                            && (kind == "flag" || kind == "flag_check" && source == "mod")
                        || key == "pool"
                        || matches!(key.as_str(), "when_flag_set" | "when_flag_clear")
                            && flags.contains(raw);
                    if private && !raw.is_empty() {
                        let scoped = namespaced(prefix, raw);
                        if key == "key" && matches!(source.as_str(), "variable" | "task") {
                            ensure!(
                                scoped.len() <= 64,
                                "合并后变量或任务名称超过64位：{raw}；请缩短该名称后再合并"
                            );
                        }
                        *item = json!(scoped);
                    }
                }
                if key == "variables" || key == "tasks" {
                    if let Some(map) = item.as_object_mut() {
                        let entries = std::mem::take(map);
                        for (id, data) in entries {
                            let scoped = namespaced(prefix, &id);
                            ensure!(
                                scoped.len() <= 64,
                                "合并后变量或任务名称超过64位：{id}；请缩短该名称后再合并"
                            );
                            map.insert(scoped, data);
                        }
                    }
                }
                // Dialogue, native Lua, translations and editor snapshots are author-owned text.
                if !matches!(
                    key.as_str(),
                    "text" | "description" | "code" | "_editor" | "localization"
                ) {
                    rewrite(item, stories, assets, prefix, flags)?;
                }
            }
        }
        Value::String(raw) => {
            if let Some(mapped) = assets.get(raw) {
                *raw = mapped.clone();
            }
        }
        _ => {}
    }
    Ok(())
}

/// Changes the destination only after the entire merge validates. Its work/save
/// identity stays the destination's; imported files are read-only inputs.
pub fn merge(destination: &mut Project, incoming: &Project) -> Result<MergeReport> {
    let mode = text(&destination.manifest, "play_mode");
    ensure!(
        mode == text(&incoming.manifest, "play_mode"),
        "两个项目的玩法模式不同，请先在作品设置中统一为附加剧情或独立作品，再合并。"
    );
    ensure!(
        destination.manifest.get("legacy_extensions").is_none()
            && incoming.manifest.get("legacy_extensions").is_none(),
        "此项目含旧式运行时扩展，请先将扩展整理到同一项目；不能自动合并其代码与数据归属。"
    );
    for (id, story) in &incoming.stories {
        ensure!(!crate::parallel::story_actions(story).any(|(_, n)| n["type"] == "raw") && story.get("vanilla_import").is_none(),
            "章节 {id} 含原始 Lua 或整章原版代码，无法自动改写其中的跨项目引用；请先转为可编辑节点或在同一工程中整理。");
    }
    let prefix = (1..)
        .map(|n| format!("merge{n}"))
        .find(|p| {
            let prefix = format!("{p}_");
            !destination.stories.iter().any(|(id, story)| {
                id.to_ascii_lowercase().starts_with(&prefix) || uses_namespace(story, &prefix)
            }) && !uses_namespace(&destination.manifest, &prefix)
                && !destination
                    .scenario
                    .as_ref()
                    .is_some_and(|config| uses_namespace(config, &prefix))
        })
        .unwrap();
    let mut merged = destination.clone();
    let mut flags = BTreeSet::new();
    for story in incoming.stories.values() {
        declared_flags(story, &mut flags);
    }
    if let Some(config) = &incoming.scenario {
        declared_flags(config, &mut flags);
    }
    let mut reserved: BTreeSet<_> = merged
        .stories
        .keys()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let mut chapter_map = BTreeMap::new();
    for id in incoming.stories.keys() {
        let mut candidate: String = namespaced(&prefix, id).chars().take(64).collect();
        let base = candidate.clone();
        for n in 2.. {
            if reserved.insert(candidate.to_ascii_lowercase()) {
                break;
            }
            candidate = namespaced(&format!("{prefix}_{n}"), &base)
                .chars()
                .take(64)
                .collect();
        }
        chapter_map.insert(id.clone(), candidate);
    }
    let mut assets = BTreeMap::new();
    let mut content_ids = BTreeMap::new();
    let existing_ids: BTreeSet<_> = merged
        .assets
        .keys()
        .filter_map(|path| {
            let parts: Vec<_> = path.split('/').collect();
            (parts.len() >= 5 && parts[0] == "assets" && parts[1] == "user")
                .then(|| parts[3].to_ascii_lowercase())
        })
        .collect();
    let mut reserved_content = existing_ids.clone();
    for path in incoming.assets.keys() {
        let parts: Vec<_> = path.split('/').collect();
        if parts.len() >= 5 && parts[0] == "assets" && parts[1] == "user" {
            reserved_content.insert(parts[3].to_ascii_lowercase());
        }
    }
    for (path, bytes) in &incoming.assets {
        if path.starts_with("assets/user/") && path.ends_with("/content.json") {
            let meta: Value = serde_json::from_slice(bytes).with_context(|| path.clone())?;
            let id = text(&meta, "id");
            if existing_ids.contains(&id.to_ascii_lowercase()) {
                let (namespace, name) = id.split_once('.').context("素材 ID 缺少命名空间")?;
                for n in 1.. {
                    let scope = if n == 1 {
                        format!("{prefix}_{namespace}")
                    } else {
                        format!("{prefix}_{n}_{namespace}")
                    };
                    let candidate =
                        format!("{}.{}", scope.chars().take(32).collect::<String>(), name);
                    if reserved_content.insert(candidate.to_ascii_lowercase()) {
                        crate::content::validate_content_id(&candidate)?;
                        content_ids.insert(id.to_owned(), candidate);
                        break;
                    }
                }
            }
        }
    }
    for path in incoming.assets.keys() {
        let parts: Vec<_> = path.split('/').collect();
        let target = if parts.len() >= 5 && parts[0] == "assets" && parts[1] == "user" {
            if let Some(id) = content_ids.get(parts[3]) {
                format!("assets/user/{}/{}/{}", parts[2], id, parts[4..].join("/"))
            } else {
                path.clone()
            }
        } else if merged
            .assets
            .keys()
            .any(|key| key.eq_ignore_ascii_case(path))
        {
            format!(
                "assets/{prefix}/{}",
                path.strip_prefix("assets/")
                    .context("素材路径必须位于 assets/")?
            )
        } else {
            path.clone()
        };
        assets.insert(path.clone(), target);
    }
    for (old, new) in &content_ids {
        assets.insert(format!("user:{old}"), format!("user:{new}"));
    }
    for (id, story) in &incoming.stories {
        let mut story = story.clone();
        rewrite(&mut story, &chapter_map, &assets, &prefix, &flags)?;
        let new_id = &chapter_map[id];
        story["id"] = json!(new_id);
        for node in story["nodes"].as_array_mut().context("章节缺少 nodes")? {
            if mode == "sandbox" && node["type"] == "end" && node["next_script"].is_null() {
                node["event_result"] = json!(namespaced(
                    &prefix,
                    node["event_result"].as_str().unwrap_or("completed")
                ));
            }
        }
        merged
            .paths
            .insert(new_id.clone(), format!("{new_id}.json").into());
        merged.stories.insert(new_id.clone(), story);
    }
    for (path, bytes) in &incoming.assets {
        let bytes = if path.ends_with("/content.json") {
            let mut meta: Value = serde_json::from_slice(bytes)?;
            if let Some(id) = content_ids.get(text(&meta, "id")) {
                meta["id"] = json!(id);
            }
            rewrite(&mut meta, &chapter_map, &assets, &prefix, &flags)?;
            stable_json(&meta)?
        } else {
            bytes.clone()
        };
        let target = &assets[path];
        ensure!(
            !merged
                .assets
                .keys()
                .any(|key| key.eq_ignore_ascii_case(target)),
            "素材路径冲突：{target}"
        );
        merged.assets.insert(target.clone(), bytes);
    }
    crate::content::character_extensions(&merged.assets)?;
    let mut imported_entry = None;
    let mut event_count = 0;
    if let Some(config) = &incoming.scenario {
        let mut config = config.clone();
        rewrite(&mut config, &chapter_map, &assets, &prefix, &flags)?;
        for field in ["events", "tasks"] {
            for entry in config[field].as_array_mut().context("事件配置缺少列表")? {
                entry["id"] = json!(namespaced(&prefix, text(entry, "id")));
                if field == "events" && mode == "sandbox" {
                    let outcomes = entry["outcomes"]
                        .as_object_mut()
                        .context("事件缺少 outcomes")?;
                    let values = std::mem::take(outcomes);
                    for (id, effects) in values {
                        outcomes.insert(namespaced(&prefix, &id), effects);
                    }
                    if entry["trigger"] == "start" {
                        imported_entry = Some(entry.clone());
                        continue;
                    }
                }
                merged.scenario.as_mut().context("当前项目缺少事件配置")?[field]
                    .as_array_mut()
                    .context("当前事件配置缺少列表")?
                    .push(entry.clone());
                if field == "events" {
                    event_count += 1;
                }
            }
        }
    }
    if let Some(triggers) = incoming.manifest["campaign"]["triggers"].as_array() {
        let mut triggers = json!(triggers);
        rewrite(&mut triggers, &chapter_map, &assets, &prefix, &flags)?;
        if !merged.manifest["campaign"]["triggers"].is_array() {
            merged.manifest["campaign"]["triggers"] = json!([]);
        }
        merged.manifest["campaign"]["triggers"]
            .as_array_mut()
            .unwrap()
            .extend(triggers.as_array().unwrap().clone());
    }
    if mode != "addon" {
        let old_entry = text(&merged.manifest, "entry").to_owned();
        let new_entry = chapter_map
            .get(text(&incoming.manifest, "entry"))
            .context("合入项目的开场章节不存在")?
            .clone();
        let menu_base = namespaced(&prefix, "entries");
        let mut menu_id = menu_base.clone();
        let mut suffix = 2;
        while merged
            .stories
            .keys()
            .any(|id| id.eq_ignore_ascii_case(&menu_id))
        {
            menu_id = format!("{menu_base}_{suffix}");
            suffix += 1;
        }
        let mut menu = json!({"story_schema":2,"id":menu_id,"title":"作品入口","start":"select_work","nodes":[
            {"id":"select_work","type":"choice","options":[
                {"text":destination.manifest["name"],"goto":"original_work"},
                {"text":incoming.manifest["name"],"goto":"imported_work"}]},
            {"id":"original_work","type":"end","next_script":old_entry},
            {"id":"imported_work","type":"end","next_script":new_entry}]});
        menu["_editor"] = json!({"group":"合并作品入口"});
        merged
            .paths
            .insert(menu_id.clone(), format!("{menu_id}.json").into());
        merged.stories.insert(menu_id.clone(), menu);
        merged.manifest["entry"] = json!(menu_id);
        if mode == "sandbox" {
            let config = merged.scenario.as_mut().context("独立作品缺少事件配置")?;
            let entry_id = config["entry_event"].clone();
            let entry = config["events"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|e| e["id"] == entry_id)
                .context("找不到开场事件")?;
            entry["script"] = json!(menu_id);
            entry["outcomes"].as_object_mut().unwrap().extend(
                imported_entry.context("合入项目缺少开场事件")?["outcomes"]
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        }
    }
    // Save the merged work to a new destination, retaining both source projects.
    merged.source = None;
    merged.paths.clear();
    merged.story_subdir = true;
    merged.has_manifest = true;
    for (id, story) in &merged.stories {
        crate::validate::validate_story(story).with_context(|| format!("合并后章节 {id}"))?;
    }
    if let Some(config) = &merged.scenario {
        crate::scenario::validate(config, &merged.manifest, &merged.stories)?;
    }
    crate::validate::validate_manifest(&merged.manifest)?;
    let report = MergeReport {
        chapters: incoming.stories.len(),
        assets: incoming.assets.len(),
        events: event_count,
    };
    *destination = merged;
    Ok(report)
}
