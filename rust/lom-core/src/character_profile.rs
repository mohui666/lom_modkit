//! Author-owned social state and native biography-page limits.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

pub const MAX_PAGES: i64 = 4;
pub const MAX_LEVEL: i64 = 10;
pub const OPERATIONS: &[&str] = &["meet", "forget", "unlock_biography", "lock_biography"];

pub fn page_operation(node: &Value) -> bool {
    matches!(
        node["op"].as_str(),
        Some("unlock_biography" | "lock_biography")
    )
}

pub fn normalize_metadata(raw: &Value, normalized: &mut Value) -> Result<()> {
    let profile_fields = ["initially_known", "biography_pages"];
    if profile_fields.iter().any(|key| raw.get(key).is_some()) {
        ensure!(
            normalized["extends"].as_str().unwrap_or("").is_empty(),
            "认识状态与分级传记只支持独立自定义人物，不能用于原版人物扩展"
        );
    }
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
    has_biography(metadata) || metadata.get("initially_known").is_some()
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
        .context("人物资料操作必须指定独立自定义人物")?;
    ensure!(
        crate::content::parse_content_ref(character)?.is_some(),
        "人物资料操作只支持本作品的独立 user: 人物，不能用于原版人物"
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
    ensure!(
        metadata["type"] == "character" && metadata["extends"].as_str().unwrap_or("").is_empty(),
        "人物资料操作只支持独立自定义人物，不能用于原版人物扩展"
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
