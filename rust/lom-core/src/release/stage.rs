//! Pure data repairs. A proposal keeps both snapshots so the editor can undo it.
use super::{nodes, text, Stories};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn required_character(node: &Value) -> Option<&str> {
    let kind = text(node, "type");
    let needs = [
        "move", "face", "hide", "focus", "offset", "shock", "dim", "rotate",
    ]
    .contains(&kind)
        || kind == "say" && ["", "character", "think"].contains(&text(node, "mode"));
    needs
        .then(|| text(node, "character"))
        .filter(|s| !s.is_empty())
}
pub fn missing_stage_linear(nodes: &[Value], index: usize) -> Option<String> {
    let cid = required_character(nodes.get(index)?)?;
    for prev in nodes[..index]
        .iter()
        .rev()
        .filter(|n| text(n, "character") == cid)
    {
        match text(prev, "type") {
            "show" => return None,
            "hide" => return Some(cid.into()),
            _ => {}
        }
    }
    Some(cid.into())
}
pub(crate) fn reached(story: &Value) -> BTreeSet<String> {
    let by_id: BTreeMap<_, _> = nodes(story)
        .iter()
        .enumerate()
        .map(|(i, n)| (text(n, "id"), i))
        .collect();
    let mut pending = vec![text(story, "start").to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if let Some(&index) = by_id.get(id.as_str()) {
            if seen.insert(id) {
                pending.extend(crate::analysis::successors(story, index));
            }
        }
    }
    seen
}
pub fn find_stage_issues(story: &Value) -> Vec<(String, String)> {
    let ns: Vec<_> = nodes(story)
        .iter()
        .filter(|n| !text(n, "id").is_empty())
        .collect();
    let by_id: BTreeMap<_, _> = ns.iter().map(|n| (text(n, "id"), *n)).collect();
    let universe: BTreeSet<String> = ns
        .iter()
        .filter_map(|n| n["character"].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let mut preds: BTreeMap<&str, BTreeSet<&str>> =
        by_id.keys().map(|id| (*id, BTreeSet::new())).collect();
    for (i, n) in nodes(story).iter().enumerate() {
        for target in crate::analysis::successors(story, i) {
            if let Some((&id, _)) = by_id.get_key_value(target.as_str()) {
                preds.get_mut(id).unwrap().insert(text(n, "id"));
            }
        }
    }
    let start = text(story, "start");
    let mut ins: BTreeMap<&str, BTreeSet<String>> = by_id
        .keys()
        .map(|id| {
            (
                *id,
                if *id == start {
                    BTreeSet::new()
                } else {
                    universe.clone()
                },
            )
        })
        .collect();
    loop {
        let mut changed = false;
        for n in &ns {
            let id = text(n, "id");
            if id == start || preds[id].is_empty() {
                continue;
            }
            let mut meet: Option<BTreeSet<String>> = None;
            for pred in &preds[id] {
                let Some(previous) = by_id.get(pred) else {
                    continue;
                };
                let mut out = ins[pred].clone();
                let cid = text(previous, "character");
                if !cid.is_empty() {
                    match text(previous, "type") {
                        "show" => {
                            out.insert(cid.into());
                        }
                        "hide" => {
                            out.remove(cid);
                        }
                        _ => {}
                    }
                }
                meet = Some(match meet {
                    None => out,
                    Some(old) => old.intersection(&out).cloned().collect(),
                });
            }
            if let Some(meet) = meet {
                if meet != ins[id] {
                    ins.insert(id, meet);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let reachable = reached(story);
    ns.into_iter()
        .filter_map(|n| {
            let id = text(n, "id");
            let cid = required_character(n)?;
            (reachable.contains(id) && !ins[id].contains(cid)).then(|| (id.into(), cid.into()))
        })
        .collect()
}
pub fn ensure_stage(story: &mut Value, node_id: &str) -> Option<Value> {
    let ns = nodes(story);
    let index = ns.iter().position(|n| text(n, "id") == node_id)?;
    let cid = required_character(&ns[index])?.to_owned();
    let used: BTreeSet<_> = ns.iter().map(|n| text(n, "id")).collect();
    let id = (1..)
        .map(|i| format!("show{i}"))
        .find(|s| !used.contains(s.as_str()))
        .unwrap();
    let show = json!({"id":id,"type":"show","character":cid,"position":"M"});
    let ns = story.get_mut("nodes")?.as_array_mut()?;
    ns.insert(index, show.clone());
    for node in ns {
        if text(node, "id") == id || text(node, "id") == node_id {
            continue;
        }
        // Includes every native control-flow edge, preserving target self-loops.
        for key in ["goto", "win", "lose", "success", "failure"] {
            if node[key] == node_id {
                node[key] = json!(id);
            }
        }
        for collection in ["options", "bands", "cases"] {
            if let Some(items) = node.get_mut(collection).and_then(Value::as_array_mut) {
                for item in items {
                    for key in ["goto", "goto_大成功", "goto_成功", "goto_失败"] {
                        if item[key] == node_id {
                            item[key] = json!(id);
                        }
                    }
                }
            }
        }
    }
    if story["start"] == node_id {
        story["start"] = json!(id);
    }
    Some(show)
}
#[derive(Clone, Debug)]
pub struct FixProposal {
    pub before: Stories,
    pub after: Stories,
    pub changes: Vec<String>,
}
impl FixProposal {
    pub fn apply(&self, stories: &mut Stories) -> Result<()> {
        ensure!(*stories == self.before, "项目已变化，请重新生成修复建议");
        *stories = self.after.clone();
        Ok(())
    }
    pub fn undo(&self, stories: &mut Stories) -> Result<()> {
        ensure!(
            *stories == self.after,
            "项目已变化，请通过编辑器撤销历史恢复"
        );
        *stories = self.before.clone();
        Ok(())
    }
}
pub fn propose_safe_fixes(stories: &Stories) -> FixProposal {
    let mut after = stories.clone();
    let mut changes = Vec::new();
    let catalog = crate::validate::editor_data();
    let valid: BTreeSet<_> = catalog["characters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().or_else(|| v["id"].as_str()))
        .collect();
    let display = regex::Regex::new(r"^.*（([a-zA-Z0-9_\-]+)）$").unwrap();
    for (sid, story) in &mut after {
        if let Some(ns) = story.get_mut("nodes").and_then(Value::as_array_mut) {
            for node in ns {
                if let Some(m) = display.captures(text(node, "character").trim()) {
                    let id = m[1].to_owned();
                    if valid.contains(id.as_str()) {
                        node["character"] = json!(id);
                        changes.push(format!("章节 {sid}：修正人物内部 ID"));
                    }
                }
            }
        }
        let count = nodes(story).len();
        for _ in 0..=count {
            let Some((nid, cid)) = find_stage_issues(story).into_iter().next() else {
                break;
            };
            if ensure_stage(story, &nid).is_none() {
                break;
            }
            changes.push(format!(
                "章节 {sid} / 步骤 {nid}：在前面自动插入 {cid} 的登场"
            ));
        }
        if !nodes(story).is_empty() && !nodes(story).iter().any(|n| n["id"] == story["start"]) {
            if let Some(first) = nodes(story)
                .first()
                .and_then(|n| n["id"].as_str())
                .map(str::to_owned)
            {
                story["start"] = json!(first);
                changes.push(format!("章节 {sid}：把开头恢复为第一个步骤 {first}"));
            }
        }
        if let Some(ns) = story.get_mut("nodes").and_then(Value::as_array_mut) {
            for node in ns {
                if !node.is_object() {
                    continue;
                }
                let label = format!("章节 {sid} / 步骤 {}", text(node, "id"));
                for key in ["goto", "next_script"] {
                    if node[key] == "" && (key == "goto" || node["type"] == "end") {
                        node.as_object_mut().unwrap().remove(key);
                        changes.push(format!("{label}：移除空的 {key}"));
                    }
                }
                if node["type"] == "dice" {
                    if let Some(options) = node.get_mut("options").and_then(Value::as_array_mut) {
                        for opt in options {
                            if let Some(o) = opt.as_object_mut() {
                                let removed =
                                    o.remove("text").is_some() | o.remove("threshold").is_some();
                                if removed {
                                    changes.push(format!("{label}：移除已废弃的骰子字段"));
                                }
                            }
                        }
                    }
                }
                let kind = text(node, "type");
                let scene = text(node, "scene");
                let end = kind == "goto_scene" && scene == "End";
                if (end || kind == "death" || kind == "goto_scene" && scene == "GameOver")
                    && !node["next"].is_null()
                    && !["", "Title"].contains(&text(node, "next"))
                    && !(end && node["next"] == "Story")
                {
                    node["next"] = json!("Title");
                    changes.push(format!("{label}：把无效的结局去向恢复为标题画面"));
                }
            }
        }
    }
    FixProposal {
        before: stories.clone(),
        after,
        changes,
    }
}
