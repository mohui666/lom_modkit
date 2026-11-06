//! Editable chapters that retain the installed script's native scopes and calls.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub code: String,
    pub node: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Imported {
    pub schema: u32,
    pub target: String,
    pub origins: Vec<Origin>,
}

pub fn parse(story: &Value) -> Result<Option<Imported>> {
    let Some(value) = story.get("vanilla_import") else {
        return Ok(None);
    };
    ensure!(
        story.get("vanilla_patch").is_none(),
        "同一章节不能同时使用原文补丁和整章覆盖"
    );
    let imported: Imported =
        serde_json::from_value(value.clone()).context("原版整章导入信息无效")?;
    ensure!(
        imported.schema == 1 && crate::vanilla_patch::valid_target(&imported.target),
        "原版整章导入的版本或章节编号无效"
    );
    ensure!(
        !imported.origins.is_empty() && imported.origins.len() <= 16384,
        "原版来源步骤数量无效"
    );
    ensure!(
        imported
            .origins
            .iter()
            .map(|origin| origin.code.len())
            .sum::<usize>()
            <= 4 * 1024 * 1024,
        "原版来源文本超过 4 MiB"
    );
    for origin in &imported.origins {
        ensure!(
            origin.node.is_object() && origin.node["type"].is_string(),
            "原版来源步骤缺少类型"
        );
    }
    Ok(Some(imported))
}

pub fn comparable(node: &Value) -> Value {
    let mut node = node.clone();
    if let Some(fields) = node.as_object_mut() {
        fields.remove("id");
        fields.remove("_editor");
    }
    node
}

pub fn origin_index(node: &Value) -> Option<usize> {
    node["_editor"]["vanilla_origin"]
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
}

pub fn resume_index(story: &Value, from: usize) -> Option<usize> {
    let count = story["vanilla_import"]["origins"].as_array()?.len();
    let mut claimed = BTreeSet::new();
    story["nodes"]
        .as_array()?
        .iter()
        .enumerate()
        .find_map(|(index, node)| {
            let origin = origin_index(node).filter(|origin| *origin < count)?;
            (claimed.insert(origin) && index > from).then_some(index)
        })
}

impl Imported {
    pub fn source(&self) -> String {
        self.origins
            .iter()
            .map(|origin| origin.code.as_str())
            .collect()
    }

    /// An origin can be claimed once. Additional pasted copies are new steps.
    pub fn associations<'a>(&'a self, nodes: &[Value]) -> Vec<Option<&'a Origin>> {
        let mut seen = BTreeSet::new();
        nodes
            .iter()
            .map(|node| {
                let index = origin_index(node)?;
                self.origins.get(index).filter(|_| seen.insert(index))
            })
            .collect()
    }

    pub fn native_raw(origin: Option<&Origin>, node: &Value) -> bool {
        origin.is_some_and(|origin| origin.node["type"] == "raw") && node["type"] == "raw"
    }

    pub fn unchanged(origin: &Origin, node: &Value) -> bool {
        comparable(&origin.node) == comparable(node)
    }

    pub fn validate_flow(&self, story: &Value) -> Result<()> {
        let nodes = story["nodes"].as_array().context("原版覆盖章节缺少步骤")?;
        ensure!(
            nodes
                .first()
                .is_some_and(|node| node["id"] == story["start"]),
            "原版覆盖章节从原版入口开始，不能跳过原生初始化步骤"
        );
        let origins = self.associations(nodes);
        let mut start = 0;
        while start < nodes.len() {
            if Self::native_raw(origins[start], &nodes[start]) {
                ensure!(
                    nodes[start]["goto"].is_null(),
                    "原生逻辑块的跳转由原 Lua 控制，请在块外添加分支步骤"
                );
                start += 1;
                continue;
            }
            let end = (start..nodes.len())
                .find(|&index| Self::native_raw(origins[index], &nodes[index]))
                .unwrap_or(nodes.len());
            let allowed: BTreeSet<_> = nodes[start..end]
                .iter()
                .map(|node| node["id"].as_str().unwrap_or(""))
                .chain(nodes.get(end).and_then(|node| node["id"].as_str()))
                .collect();
            for index in start..end {
                ensure!(
                    !matches!(nodes[index]["type"].as_str(), Some("combat" | "battle")),
                    "步骤 {} 会切换战斗场景，当前原版整章的局部状态无法跨场景续接",
                    nodes[index]["id"]
                );
                for target in crate::analysis::successors(story, index) {
                    ensure!(
                        allowed.contains(target.as_str()),
                        "步骤 {} 的跳转跨过了原版逻辑块；请连接本段步骤或返回原版流程",
                        nodes[index]["id"]
                    );
                }
            }
            start = end;
        }
        Ok(())
    }
}

pub fn bookmark_node(story: &Value, key: &str, text: &str, mod_id: &str) -> Option<usize> {
    let imported = parse(story).ok()??;
    let nodes = story["nodes"].as_array()?;
    let associations = imported.associations(nodes);
    nodes.iter().zip(associations).position(|(node, origin)| {
        let native_key = origin
            .into_iter()
            .flat_map(|origin| crate::vanilla_patch::text_sites(&origin.code))
            .any(|site| !key.is_empty() && site.key.as_deref() == Some(key));
        native_key
            || (!key.is_empty()
                && key
                    == format!(
                        "MOD_{mod_id}_{}_{}",
                        story["id"].as_str().unwrap_or(""),
                        node["id"].as_str().unwrap_or("")
                    ))
            || (!text.is_empty() && node["text"].as_str() == Some(text))
    })
}

pub fn target(story: &Value) -> Option<&str> {
    story["vanilla_import"]["target"]
        .as_str()
        .or_else(|| story["vanilla_patch"]["target"].as_str())
}

pub fn import(
    source: &str,
    id: &str,
    title: &str,
    target: &str,
    translations: &BTreeMap<String, String>,
) -> Result<Value> {
    ensure!(
        crate::vanilla_patch::valid_target(target),
        "原版章节编号无效"
    );
    crate::lua_import::import_vanilla(source, id, title, target, translations)
}

/// Convert existing anchored edits along with the original chapter.
pub fn import_patch(
    story: &Value,
    source: &str,
    translations: &BTreeMap<String, String>,
) -> Result<Value> {
    let patch = crate::vanilla_patch::parse(story)?.context("当前章节不是原版补丁")?;
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    crate::vanilla_patch::apply(source, &patch.edits)?;
    let mut imported = import(
        source,
        story["id"].as_str().unwrap_or(""),
        story["title"].as_str().unwrap_or(""),
        &patch.target,
        translations,
    )?;
    let baseline = parse(&imported)?.context("缺少原版来源")?;
    let nodes = imported["nodes"].as_array().context("缺少导入步骤")?;
    let positioned_edits: Vec<_> = patch
        .edits
        .iter()
        .enumerate()
        .map(|(index, edit)| {
            let at = source
                .match_indices(&edit.find)
                .nth(edit.occurrence - 1)
                .expect("validated source edit")
                .0;
            (index, at, edit)
        })
        .collect();
    let mut converted = Vec::new();
    let mut offset = 0;
    for (index, (origin, node)) in baseline.origins.iter().zip(nodes).enumerate() {
        let end = offset + origin.code.len();
        let edits: Vec<_> = positioned_edits
            .iter()
            .copied()
            .filter(|(_, at, edit)| *at >= offset && *at + edit.find.len() <= end)
            .collect();
        let mut node = node.clone();
        if origin.node["type"] == "say" && edits.len() == 1 && edits[0].2.kind == "say" {
            let (ordinal, _, edit) = edits[0];
            if let Some(text) = &edit.text {
                node["text"] = json!(text);
            }
            let anchor = node["id"].as_str().unwrap_or("");
            append_insertion(
                &mut converted,
                ordinal,
                "before",
                &edit.before,
                edit.before_condition.as_ref(),
                anchor,
            );
            converted.push(node.clone());
            let next = nodes
                .get(index + 1)
                .and_then(|node| node["id"].as_str())
                .context("原版对白缺少后续入口")?;
            append_insertion(
                &mut converted,
                ordinal,
                "after",
                &edit.after,
                edit.after_condition.as_ref(),
                next,
            );
        } else {
            ensure!(
                edits.is_empty() || node["type"] == "raw",
                "旧修改无法转换到对应步骤，请先核对原文定位"
            );
            let mut code = origin.code.clone();
            let mut edits = edits;
            edits.sort_by_key(|(_, at, _)| *at);
            for (_, at, edit) in edits.into_iter().rev() {
                code.replace_range(
                    at - offset..at - offset + edit.find.len(),
                    &edit.replacement(),
                );
            }
            if node["type"] == "raw" {
                node["code"] = json!(code);
            }
            converted.push(node);
        }
        offset = end;
    }
    imported["start"] = converted.first().context("原版章节没有步骤")?["id"].clone();
    imported["nodes"] = json!(converted);
    crate::validate::validate_story(&imported)?;
    Ok(imported)
}

fn append_insertion(
    nodes: &mut Vec<Value>,
    ordinal: usize,
    position: &str,
    text: &str,
    condition: Option<&crate::vanilla_patch::Condition>,
    next: &str,
) {
    if text.is_empty() {
        return;
    }
    let id = format!("insert_{ordinal}_{position}");
    if let Some(condition) = condition {
        let mut check = condition.node();
        check["id"] = json!(format!("{id}_check"));
        check["success"] = json!(id);
        check["failure"] = json!(next);
        nodes.push(check);
    }
    nodes.push(json!({"id":id,"type":"say","text":text,"inherit_dialog":true}));
}
