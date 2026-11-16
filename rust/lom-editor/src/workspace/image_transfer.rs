//! Copy user image files and their authored layouts without merging story prose.
use super::*;

pub(super) struct CopyReport {
    pub images: usize,
    pub chapters: usize,
    pub first_asset: String,
}

pub(super) fn copy_images(
    destination: &mut Project,
    incoming: &Project,
) -> anyhow::Result<CopyReport> {
    let mut additions = BTreeMap::new();
    let mut image_refs = BTreeSet::new();
    let mut characters = BTreeSet::new();
    let mut first_asset = String::new();
    for (path, bytes) in &incoming.assets {
        if !path.starts_with("assets/user/") || !path.ends_with("/content.json") {
            continue;
        }
        let meta: Value = serde_json::from_slice(bytes).with_context(|| path.clone())?;
        let kind = meta["type"].as_str().unwrap_or("");
        if !matches!(kind, "image" | "character") {
            continue;
        }
        let normalized =
            lom_core::content::normalize_content_metadata(&meta).with_context(|| path.clone())?;
        let id = normalized["id"].as_str().context("图片素材缺少 id")?;
        let directory = format!("{}/", lom_core::content::package_content_dir(kind, id)?);
        anyhow::ensure!(
            path == &format!("{directory}content.json"),
            "素材 ID 与路径不一致：{path}"
        );
        anyhow::ensure!(!destination.assets.keys().chain(additions.keys()).any(|name|
            name.starts_with("assets/user/")
                && name.split('/').nth(3).is_some_and(|existing| existing.eq_ignore_ascii_case(id))
                && !name.starts_with(&directory)),
            "当前作品中的其他类型素材已使用 ID {id}。请使用空白新作品接收图片，或先处理当前作品的同名素材；本次未改动任何素材或章节。");
        for filename in lom_core::content::listed_content_files(&normalized) {
            anyhow::ensure!(
                incoming
                    .assets
                    .contains_key(&format!("{directory}{filename}")),
                "图片素材 {id} 缺少文件 {filename}"
            );
        }
        image_refs.insert(format!("user:{id}"));
        if kind == "character" {
            characters.insert(format!("user:{id}"));
            if let Some(extends) = meta["extends"].as_str().filter(|s| !s.is_empty()) {
                characters.insert(extends.to_owned());
            }
        }
        if first_asset.is_empty() {
            first_asset = path.clone();
        }
        for (name, data) in incoming.assets.range(directory.clone()..) {
            if !name.starts_with(&directory) {
                break;
            }
            if let Some((existing, existing_data)) = destination
                .assets
                .iter()
                .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            {
                anyhow::ensure!(existing == name && existing_data == data,
                    "当前作品已有不同内容的同名图片素材：{id}（{name}）。请使用空白新作品接收图片，或先处理当前作品的同名素材；本次未改动任何素材或章节。");
            } else {
                additions.insert(name.clone(), data.clone());
            }
        }
    }
    anyhow::ensure!(
        !image_refs.is_empty(),
        "所选作品没有可复制的自定义图片或人物立绘。"
    );
    anyhow::ensure!(destination.assets.values().chain(additions.values()).map(Vec::len).sum::<usize>() <= 128 * 1024 * 1024,
        "复制后的作品素材超过项目的 128 MiB 上限；请先移除不需要的素材，或复制到空白新作品。本次未改动任何素材或章节。");
    // Validate extension ownership using metadata only; image bytes are already
    // held in the two projects and need not be cloned again for this check.
    let metadata: BTreeMap<_, _> = destination
        .assets
        .iter()
        .chain(additions.iter())
        .filter(|(name, _)| name.ends_with("/content.json"))
        .map(|(name, bytes)| (name.clone(), bytes.clone()))
        .collect();
    lom_core::content::character_extensions(&metadata)?;

    let mut chapters = Vec::new();
    let mut reserved: BTreeSet<_> = destination
        .stories
        .keys()
        .map(|id| id.to_ascii_lowercase())
        .collect();
    for story in incoming.stories.values() {
        let mut nodes = layout_nodes(story, &image_refs, &characters);
        if nodes.is_empty() {
            continue;
        }
        let id = (1..)
            .map(|index| format!("image_layout{index}"))
            .find(|id| reserved.insert(id.clone()))
            .unwrap();
        for (index, node) in nodes.iter_mut().enumerate() {
            node["id"] = json!(format!("layout{}", index + 1));
            node.as_object_mut().unwrap().remove("goto");
        }
        nodes.push(json!({"id":"layout_end","type":"end"}));
        let title = story["title"]
            .as_str()
            .or_else(|| story["id"].as_str())
            .unwrap_or("");
        chapters.push((id.clone(), json!({
            "story_schema":2,"id":id,
            "title":crate::i18n::message("workspace.assets.layout_title", &[("title", title.to_owned())]),
            "start":"layout1","nodes":nodes,
            "_editor":{"group":crate::i18n::key("workspace.assets.layout_group")}
        })));
    }
    let report = CopyReport {
        images: image_refs.len(),
        chapters: chapters.len(),
        first_asset,
    };
    destination.assets.extend(additions);
    for (id, story) in chapters {
        destination.stories.insert(id, story);
    }
    Ok(report)
}

fn layout_nodes(
    story: &Value,
    images: &BTreeSet<String>,
    characters: &BTreeSet<String>,
) -> Vec<Value> {
    let actions: Vec<_> = lom_core::parallel::story_actions(story)
        .map(|(_, action)| action)
        .collect();
    let slots: BTreeSet<_> = actions
        .iter()
        .filter(|action| {
            action["type"] == "overlay" && images.contains(action["image"].as_str().unwrap_or(""))
        })
        .filter_map(|action| action["slot"].as_str())
        .collect();
    let has_cg = actions.iter().any(|action| {
        action["type"] == "custom_cg" && images.contains(action["image"].as_str().unwrap_or(""))
    });
    let has_background = actions.iter().any(|action| {
        action["type"] == "background" && images.contains(action["image"].as_str().unwrap_or(""))
    });
    let picture_kinds: BTreeSet<_> = actions
        .iter()
        .filter(|action| {
            action["type"] == "cg" && images.contains(action["key"].as_str().unwrap_or(""))
        })
        .filter_map(|action| action["kind"].as_str())
        .collect();
    let has_character = actions
        .iter()
        .any(|action| characters.contains(action["character"].as_str().unwrap_or("")));
    let select = |action: &Value| -> Option<Value> {
        let kind = action["type"].as_str().unwrap_or("");
        let character = action["character"].as_str().unwrap_or("");
        let selected = match kind {
            "show" | "portrait" | "move" | "face" | "hide" | "focus" | "offset" | "rotate"
            | "dim" | "shock" | "mood" => characters.contains(character),
            "say" => characters.contains(character) && action["portrait"].is_string(),
            "hide_all" => has_character,
            "overlay" => {
                images.contains(action["image"].as_str().unwrap_or(""))
                    || action["action"] == "hide"
                        && slots.contains(action["slot"].as_str().unwrap_or(""))
            }
            "custom_cg" => {
                images.contains(action["image"].as_str().unwrap_or(""))
                    || action["action"] == "hide" && has_cg
            }
            "background" => {
                images.contains(action["image"].as_str().unwrap_or(""))
                    || matches!(action["action"].as_str(), Some("hide" | "clear")) && has_background
            }
            "cg" => {
                images.contains(action["key"].as_str().unwrap_or(""))
                    || picture_kinds.contains(action["kind"].as_str().unwrap_or(""))
                        && (action["action"] == "hide" || action["background_only"] == true)
            }
            "render_priority" => match action["target"].as_str().unwrap_or("") {
                "character" => characters.contains(character),
                "overlay" => slots.contains(action["slot"].as_str().unwrap_or("")),
                "custom_cg" => has_cg,
                "background" => has_background,
                _ => false,
            },
            _ => false,
        };
        selected.then(|| {
            if kind == "say" {
                // Dialogue can select an expression too. Preserve its visual
                // instruction while leaving the old prose and voice behind.
                let mut portrait =
                    json!({"type":"portrait","character":character,"portrait":action["portrait"]});
                for key in ["appearance", "brighten"] {
                    if let Some(value) = action.get(key) {
                        portrait[key] = value.clone();
                    }
                }
                portrait
            } else {
                action.clone()
            }
        })
    };
    story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            if node["type"] == "parallel" {
                let actions: Vec<_> = lom_core::parallel::actions(node)
                    .iter()
                    .filter_map(&select)
                    .collect();
                if actions.is_empty() {
                    return None;
                }
                let mut group = node.clone();
                group["actions"] = json!(actions);
                Some(group)
            } else {
                select(node)
            }
        })
        .collect()
}
