//! Structured project search, references and transactional cross-chapter editing.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use unicode_casefold::UnicodeCaseFold;

pub const CATEGORIES: &[&str] = &[
    "story",
    "node",
    "text",
    "character",
    "portrait",
    "voice",
    "image",
    "variable",
    "flag",
    "goto",
    "content_ref",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub category: String,
    pub story_id: String,
    pub node_id: Option<String>,
    pub field: String,
    pub value: String,
    pub preview: String,
}
fn hit(category: &str, story: &str, node: Option<&str>, field: &str, value: &str) -> SearchHit {
    SearchHit {
        category: category.into(),
        story_id: story.into(),
        node_id: node.map(str::to_owned),
        field: field.into(),
        value: value.into(),
        preview: if value.chars().count() > 120 {
            format!("{}…", value.chars().take(117).collect::<String>())
        } else {
            value.into()
        },
    }
}
fn leaves(value: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::String(s) => out.push((path.into(), s.clone())),
        Value::Object(o) => {
            for (k, v) in o {
                leaves(
                    v,
                    &if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    },
                    out,
                )
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                leaves(v, &format!("{path}[{i}]"), out)
            }
        }
        _ => (),
    }
}
fn category(node: &Value, field: &str, value: &str) -> Option<&'static str> {
    let leaf = field.rsplit('.').next()?.split('[').next()?;
    let kind = node["type"].as_str().unwrap_or("");
    if value.starts_with("user:") {
        return Some(match leaf {
            "character" => "character",
            "voice" => "voice",
            "image" => "image",
            _ if matches!(kind, "music" | "sound") => "voice",
            _ => "content_ref",
        });
    }
    Some(match leaf {
        "character" => "character",
        "portrait" => "portrait",
        "voice" => "voice",
        "image" => "image",
        "name" if matches!(kind, "music" | "sound") => "voice",
        "goto" | "next_script" | "win" | "lose" | "success" | "failure" => "goto",
        _ if leaf.starts_with("goto_") => "goto",
        "flag" => "flag",
        "var" | "variable" | "vars" => "variable",
        "name" if kind == "block" && field.starts_with("vars[") => "variable",
        "key" | "stat" if matches!(kind, "stat" | "stat_set" | "branch") => "variable",
        "text" | "title" | "desc" | "name" | "band_texts" => "text",
        _ if field.contains(".text") => "text",
        _ => return None,
    })
}
pub fn index_project(stories: &BTreeMap<String, Value>) -> Vec<SearchHit> {
    let mut hits = vec![];
    for (sid, story) in stories {
        hits.push(hit("story", sid, None, "story", sid));
        if let Some(title) = story["title"].as_str().filter(|s| !s.is_empty()) {
            hits.push(hit("story", sid, None, "title", title));
        }
        for node in story["nodes"].as_array().into_iter().flatten() {
            let nid = node["id"].as_str().unwrap_or("");
            hits.push(hit("node", sid, Some(nid), "id", nid));
            let mut fields = vec![];
            leaves(node, "", &mut fields);
            for (field, value) in fields {
                if value.is_empty() || matches!(field.as_str(), "id" | "type") {
                    continue;
                }
                if let Some(cat) = category(node, &field, &value) {
                    hits.push(hit(cat, sid, Some(nid), &field, &value));
                }
            }
        }
    }
    hits
}
pub fn search(hits: &[SearchHit], query: &str, category: &str) -> Vec<SearchHit> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|s| s.case_fold().collect())
        .collect();
    hits.iter()
        .filter(|h| {
            let haystack = format!(
                "{} {} {} {} {} {}",
                h.category,
                h.story_id,
                h.node_id.as_deref().unwrap_or(""),
                h.field,
                h.value,
                h.preview
            )
            .case_fold()
            .collect::<String>();
            (category.is_empty() || h.category == category)
                && terms.iter().all(|t| haystack.contains(t))
        })
        .cloned()
        .collect()
}
pub fn references(
    stories: &BTreeMap<String, Value>,
    manifest: &Value,
    kind: &str,
    symbol: &str,
    story: Option<&str>,
) -> Vec<SearchHit> {
    let mut found: Vec<_> = index_project(stories)
        .into_iter()
        .filter(|h| {
            h.value == symbol
                && match kind {
                    "content" => matches!(
                        h.category.as_str(),
                        "character" | "voice" | "image" | "content_ref"
                    ),
                    "node" => {
                        h.category == "goto"
                            && h.field != "next_script"
                            && story.is_none_or(|s| s == h.story_id)
                    }
                    "story" => h.category == "goto" && h.field == "next_script",
                    "character" | "variable" | "flag" => h.category == kind,
                    _ => false,
                }
        })
        .collect();
    if kind == "story" && manifest["entry"] == symbol {
        found.insert(0, hit("story", symbol, None, "manifest.entry", symbol));
    }
    for (i, t) in manifest["campaign"]["triggers"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let sid = t["script"].as_str().unwrap_or("?");
        for (cat, field, val) in [
            ("story", "script", &t["script"]),
            ("flag", "when_flag_set", &t["when_flag_set"]),
            ("flag", "when_flag_clear", &t["when_flag_clear"]),
            (
                "character",
                "when_affinity.character",
                &t["when_affinity"]["character"],
            ),
        ] {
            if (cat == kind || (kind == "content" && cat == "character")) && val == symbol {
                found.push(hit(
                    cat,
                    sid,
                    None,
                    &format!("manifest.campaign.triggers[{i}].{field}"),
                    symbol,
                ));
            }
        }
    }
    found
}
pub fn retarget(value: &mut Value, mapping: &BTreeMap<String, String>) {
    match value {
        Value::Array(a) => {
            for v in a {
                retarget(v, mapping)
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "goto"
                        | "start"
                        | "end"
                        | "win"
                        | "lose"
                        | "success"
                        | "failure"
                        | "goto_大成功"
                        | "goto_成功"
                        | "goto_失败"
                ) {
                    if let Some(to) = v.as_str().and_then(|s| mapping.get(s)) {
                        *v = json!(to);
                    }
                } else if k != "next_script" {
                    retarget(v, mapping)
                }
            }
        }
        _ => (),
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Transfer {
    pub source_story: String,
    pub target_story: String,
    pub first_index: usize,
    pub count: usize,
    pub id_mapping: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    pub after: Value,
}
pub fn transfer(
    stories: &BTreeMap<String, Value>,
    source: &str,
    start: usize,
    end: usize,
    target: &str,
    at: usize,
) -> Result<Transfer> {
    ensure!(source != target, "跨章节复制必须选择不同章节");
    let src = stories.get(source).context("来源章节不存在")?;
    let dst = stories.get(target).context("目标章节不存在")?;
    let ns = src["nodes"].as_array().context("来源没有节点")?;
    let (lo, hi) = (start.min(end), start.max(end));
    ensure!(hi < ns.len(), "复制范围越界");
    let mut selected = ns[lo..=hi].to_vec();
    let ids: BTreeSet<_> = selected.iter().filter_map(|n| n["id"].as_str()).collect();
    ensure!(
        ids.len() == selected.len() && ids.iter().all(|s| valid_id(s)),
        "来源节点 ID 不合法或重复"
    );
    let mut warnings = vec![];
    for n in &selected {
        let mut fields = vec![];
        leaves(n, "", &mut fields);
        for (field, value) in fields {
            if category(n, &field, &value) == Some("goto")
                && field != "next_script"
                && !ids.contains(value.as_str())
            {
                warnings.push(format!(
                    "{}.{} 指向范围外的节点 {}，目标章节需核对",
                    n["id"].as_str().unwrap_or(""),
                    field,
                    value
                ));
            }
        }
        if let Some(next) = n["next_script"].as_str().filter(|s| !s.is_empty()) {
            if !stories.contains_key(next) {
                warnings.push(format!("next_script 指向不存在的章节 {next}"));
            }
        }
    }
    if hi + 1 < ns.len()
        && crate::analysis::successors(src, hi)
            .iter()
            .any(|id| !ids.contains(id.as_str()))
    {
        warnings.push("范围末尾原本连接其他步骤；目标章节的后继可能不同".into());
    }
    let mut used: BTreeSet<String> = dst["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["id"].as_str().map(str::to_owned))
        .collect();
    let mut mapping = BTreeMap::new();
    for n in &mut selected {
        let kind = n["type"].as_str().unwrap_or("n");
        let id = (1..)
            .map(|i| format!("{kind}{i}"))
            .find(|v| !used.contains(v))
            .unwrap();
        mapping.insert(n["id"].as_str().unwrap().to_owned(), id.clone());
        used.insert(id.clone());
        n["id"] = json!(id);
    }
    for n in &mut selected {
        retarget(n, &mapping)
    }
    let mut after = dst.clone();
    let array = after["nodes"].as_array_mut().context("目标没有节点")?;
    let at = at.min(array.len());
    array.splice(at..at, selected);
    Ok(Transfer {
        source_story: source.into(),
        target_story: target.into(),
        first_index: at,
        count: hi - lo + 1,
        id_mapping: mapping,
        warnings,
        after,
    })
}
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
