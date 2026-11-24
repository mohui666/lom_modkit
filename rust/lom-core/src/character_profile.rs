//! Author-owned social state and native biography-page limits.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

pub const MAX_PAGES: i64 = 4;
pub const MAX_LEVEL: i64 = 10;
pub const OPERATIONS: &[&str] = &["meet", "forget", "unlock_biography", "lock_biography"];

pub fn uses_native_affinity(id: &str) -> bool {
    // These RelationshipStatType IDs are native even though the affinity
    // picker omits the protagonist and alternate character appearances.
    matches!(id, "player" | "girl2_3_1" | "girl2_4" | "girl5_11" | "special215_1" | "special805_0" | "special831")
        || crate::validate::editor_data()["affinity_characters"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|entry| entry.as_str() == Some(id))
}

pub fn affinity_reference(metadata: &Value) -> Option<String> {
    if let Some(target) = metadata["extends"]
        .as_str()
        .filter(|target| !target.is_empty())
    {
        return Some(if uses_native_affinity(target) {
            target.to_owned()
        } else {
            format!("{}{target}", crate::content::STORY_AFFINITY_PREFIX)
        });
    }
    metadata["id"].as_str().map(|id| format!("user:{id}"))
}

pub fn page_operation(node: &Value) -> bool {
    matches!(
        node["op"].as_str(),
        Some("unlock_biography" | "lock_biography")
    )
}

pub fn normalize_metadata(raw: &Value, normalized: &mut Value) -> Result<()> {
    if let Some(known) = raw.get("initially_known") {
        ensure!(
            known.is_boolean(),
            "initially_known（初始是否认识）必须是布尔值"
        );
        normalized["initially_known"] = known.clone();
    }
    if let Some(value) = raw.get("biography_pages") {
        let pages = value
            .as_array()
            .context("biography_pages（分级传记）必须是数组")?;
        ensure!(
            pages.len() <= MAX_PAGES as usize,
            "原版人物传记最多支持 {MAX_PAGES} 页"
        );
        let mut result = Vec::with_capacity(pages.len());
        for (index, page) in pages.iter().enumerate() {
            let number = index + 1;
            let fields = page
                .as_object()
                .with_context(|| format!("传记第 {number} 页必须是对象"))?;
            ensure!(
                fields
                    .keys()
                    .all(|key| ["text", "unlock_level"].contains(&key.as_str())),
                "传记第 {number} 页只支持 text 与 unlock_level"
            );
            let text = page["text"]
                .as_str()
                .with_context(|| format!("传记第 {number} 页 text 必须是文字"))?;
            let level = page["unlock_level"]
                .as_i64()
                .filter(|level| (0..=MAX_LEVEL).contains(level))
                .with_context(|| {
                    format!("传记第 {number} 页 unlock_level 必须是 0～{MAX_LEVEL} 的整数")
                })?;
            result.push(json!({"text":text,"unlock_level":level}));
        }
        normalized["biography_pages"] = json!(result);
    }
    Ok(())
}

pub fn has_biography(metadata: &Value) -> bool {
    metadata["biography"]
        .as_str()
        .is_some_and(|text| !text.trim().is_empty())
        || metadata["biography_pages"]
            .as_array()
            .is_some_and(|pages| !pages.is_empty())
}

pub fn has_profile(metadata: &Value) -> bool {
    has_biography(metadata)
        || metadata.get("initially_known").is_some()
        || metadata["social_thumbnail"]
            .as_str()
            .is_some_and(|file| !file.trim().is_empty())
}

pub fn validate_for_export(metadata: &Value) -> Result<()> {
    ensure!(!(metadata["biography"].as_str().is_some_and(|text| !text.trim().is_empty())
        && metadata["biography_pages"].as_array().is_some_and(|pages| !pages.is_empty())),
        "人物 user:{} 同时填写了旧 biography 与 biography_pages。两份文稿已保留；请在人物传记编辑中明确选择一份或合并后移除旧字段，再导出。",
        metadata["id"].as_str().unwrap_or(""));
    Ok(())
}

pub fn validate_node(node: &Value) -> Result<()> {
    let character = node["character"]
        .as_str()
        .context("人物资料操作必须指定作品人物")?;
    let story_character = character
        .strip_prefix(crate::content::STORY_AFFINITY_PREFIX)
        .is_some_and(|id| {
            !uses_native_affinity(id)
                && crate::validate::editor_data()["characters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|entry| entry["id"] == id)
        });
    ensure!(
        story_character || crate::content::parse_content_ref(character)?.is_some(),
        "人物资料操作需要本作品的自定义人物或使用作品好感的剧情人物"
    );
    ensure!(
        OPERATIONS.contains(&node["op"].as_str().unwrap_or("")),
        "人物资料 op 必须是 meet/forget/unlock_biography/lock_biography"
    );
    if page_operation(node) {
        ensure!(
            node["page"]
                .as_i64()
                .is_some_and(|page| (1..=MAX_PAGES).contains(&page)),
            "传记页码必须是 1～{MAX_PAGES} 的整数"
        );
    } else {
        ensure!(
            node.get("page").is_none(),
            "认识／忘记人物不使用 page，请移除页码字段"
        );
    }
    Ok(())
}

pub fn validate_reference(node: &Value, metadata: &Value) -> Result<()> {
    let character = node["character"].as_str().unwrap_or("");
    let story_character = character.starts_with(crate::content::STORY_AFFINITY_PREFIX);
    ensure!(
        metadata["type"] == "character"
            && affinity_reference(metadata).as_deref() == Some(character)
            && (!story_character || has_profile(metadata)),
        "请在本作品的人物素材中为 {character} 设置人际资料"
    );
    if page_operation(node) {
        let count = metadata["biography_pages"]
            .as_array()
            .filter(|pages| !pages.is_empty())
            .map(Vec::len)
            .unwrap_or_else(|| usize::from(metadata["biography"].is_string()));
        ensure!(
            node["page"]
                .as_u64()
                .is_some_and(|page| page <= count as u64),
            "人物 {} 当前仅配置 {count} 页传记，无法操作第 {} 页；请修改页码或补全该页",
            node["character"],
            node["page"]
        );
    }
    Ok(())
}
