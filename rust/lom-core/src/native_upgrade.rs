//! Explicit, lossless upgrades of legacy imported chapters with linear edits.
use crate::vanilla_import::{self, Origin};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn source_spans(origins: &[Origin]) -> Vec<Option<(usize, usize)>> {
    let mut offset = 0;
    origins
        .iter()
        .map(|origin| {
            let tokens: Vec<_> = crate::vanilla_patch::tokens(&origin.code)
                .into_iter()
                .filter(|token| token.string || token.value != ";")
                .collect();
            let span = tokens
                .first()
                .zip(tokens.last())
                .map(|(first, last)| (offset + first.start, offset + last.end));
            offset += origin.code.len();
            span
        })
        .collect()
}

/// Return a validated candidate. The caller commits it only after this succeeds.
pub fn upgrade(story: &Value) -> Result<Value> {
    let imported = vanilla_import::parse(story)?.context("当前章节不是原版整章项目")?;
    ensure!(imported.schema == 1, "当前章节已经使用结构化原版格式");
    crate::validate::validate_story(story)?;
    let nodes = story["nodes"].as_array().context("原版章节缺少步骤")?;
    let associations = imported.associations(nodes);
    let claims: Vec<_> = associations
        .iter()
        .enumerate()
        .filter_map(|(index, origin)| {
            origin.map(|_| (index, vanilla_import::origin_index(&nodes[index]).unwrap()))
        })
        .collect();
    ensure!(
        claims
            .iter()
            .map(|(_, origin)| *origin)
            .eq(0..imported.origins.len()),
        "来源步骤已被删除或重排，无法保证转换后完整保留这些流程；请先核对来源步骤"
    );

    let mut translations = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        let id = node["id"].as_str().unwrap_or("");
        let next = nodes.get(index + 1).and_then(|node| node["id"].as_str());
        ensure!(
            node["goto"].is_null() || node["goto"].as_str() == next,
            "步骤“{id}”有自定义跳转，转换前需核对其分支去向"
        );
        if let Some(origin) = associations[index] {
            if origin.node["type"] == "raw" {
                ensure!(
                    vanilla_import::Imported::unchanged(origin, node),
                    "步骤“{id}”的高级 Lua 已修改，当前转换无法无损迁移，请先核对这段修改"
                );
            } else {
                ensure!(
                    origin.node["type"] == node["type"],
                    "来源步骤“{id}”已改变类型，当前转换无法无损迁移"
                );
                if origin.node["type"] == "say" {
                    for site in crate::vanilla_patch::text_sites(&origin.code) {
                        if let Some(key) = site.key {
                            translations
                                .insert(key, origin.node["text"].as_str().unwrap_or("").to_owned());
                        }
                    }
                }
            }
        } else {
            let kind = node["type"].as_str().unwrap_or("");
            ensure!(kind != "raw" && kind != "free_trigger"
                && !crate::validate::node_schema()["no_goto_types"].as_array().unwrap().iter().any(|value| value == kind),
                "新增步骤“{id}”包含高级 Lua 或多出口流程，当前转换仅支持普通线性插入，请先核对该步骤");
        }
    }

    let source = imported.source();
    let canonical = crate::native_flow::import(
        &source,
        story["id"].as_str().unwrap_or(""),
        story["title"].as_str().unwrap_or(""),
        &imported.target,
        &translations,
    )?;
    let baseline = vanilla_import::parse(&canonical)?.context("转换结果缺少原版来源")?;
    let old_spans = source_spans(&imported.origins);
    let new_spans = source_spans(&baseline.origins);
    let mut by_origin = BTreeMap::new();
    let mut preserved = BTreeMap::new();
    for &(old_index, origin_index) in &claims {
        let origin = &imported.origins[origin_index];
        let kind = origin.node["type"].as_str().unwrap_or("");
        let candidates: Vec<_> = baseline
            .origins
            .iter()
            .enumerate()
            .filter(|(index, new)| {
                if kind == "end" && old_spans[origin_index].is_none() {
                    return new.node["type"] == "end" && new_spans[*index].is_none();
                }
                match (old_spans[origin_index], new_spans[*index]) {
                    (Some((old_start, old_end)), Some((new_start, new_end))) if kind == "raw" => {
                        new_start >= old_start && new_end <= old_end
                    }
                    (Some(old), Some(new_span)) => {
                        old == new_span && new.node["type"] == origin.node["type"]
                    }
                    _ => false,
                }
            })
            .map(|(index, _)| index)
            .collect();
        let selected = if kind == "raw" {
            candidates.first().copied()
        } else {
            (candidates.len() == 1).then(|| candidates[0])
        }
        .with_context(|| {
            format!(
                "来源步骤“{}”没有唯一可对应的结构化位置，请核对该步骤",
                nodes[old_index]["id"].as_str().unwrap_or("")
            )
        })?;
        ensure!(
            !preserved.contains_key(&selected),
            "两个来源步骤对应同一位置，当前转换无法无损迁移"
        );
        by_origin.insert(origin_index, selected);
        preserved.insert(selected, old_index);
    }

    let mut candidate = story.clone();
    candidate["nodes"] = canonical["nodes"].clone();
    candidate["start"] = canonical["start"].clone();
    candidate["vanilla_import"] = canonical["vanilla_import"].clone();
    if imported.external {
        candidate["vanilla_import"]["external"] = json!(true);
    }
    let mut used: BTreeSet<_> = nodes
        .iter()
        .filter_map(|node| node["id"].as_str().map(str::to_owned))
        .collect();
    let mut ids = BTreeMap::new();
    for (index, node) in canonical["nodes"].as_array().unwrap().iter().enumerate() {
        let id = if let Some(&old_index) = preserved.get(&index) {
            nodes[old_index]["id"].as_str().unwrap().to_owned()
        } else {
            let id = (1..)
                .map(|number| format!("converted_{number}"))
                .find(|id| !used.contains(id))
                .unwrap();
            used.insert(id.clone());
            id
        };
        ids.insert(node["id"].as_str().unwrap().to_owned(), id);
    }
    crate::editing::retarget(&mut candidate["nodes"], &ids);
    crate::editing::retarget(&mut candidate["vanilla_import"]["origins"], &ids);
    candidate["start"] = json!(ids[canonical["start"].as_str().unwrap()]);
    for index in 0..baseline.origins.len() {
        let id = &ids[baseline.origins[index].node["id"].as_str().unwrap()];
        candidate["nodes"][index]["id"] = json!(id);
        candidate["vanilla_import"]["origins"][index]["node"]["id"] = json!(id);
        if let Some(&old_index) = preserved.get(&index) {
            let old = &nodes[old_index];
            let mut metadata = candidate["nodes"][index]["_editor"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            if old["type"] != "raw" {
                let flow = candidate["nodes"][index]["goto"].clone();
                candidate["nodes"][index] = old.clone();
                candidate["nodes"][index]
                    .as_object_mut()
                    .unwrap()
                    .remove("goto");
                if !flow.is_null() {
                    candidate["nodes"][index]["goto"] = flow;
                }
            }
            if let Some(previous) = old["_editor"].as_object() {
                metadata.extend(previous.clone());
            }
            metadata.insert("vanilla_origin".into(), json!(index));
            candidate["nodes"][index]["_editor"] = Value::Object(metadata);
        }
    }

    let mut index = 0;
    while index < nodes.len() {
        if associations[index].is_some() {
            index += 1;
            continue;
        }
        let start = index;
        while index < nodes.len() && associations[index].is_none() {
            index += 1;
        }
        ensure!(
            start > 0 && index < nodes.len(),
            "章节首尾的新增步骤缺少可确认的原版连接位置，请先核对"
        );
        let previous = vanilla_import::origin_index(&nodes[start - 1]).unwrap();
        let following = vanilla_import::origin_index(&nodes[index]).unwrap();
        let previous_id = candidate["vanilla_import"]["origins"][by_origin[&previous]]["node"]
            ["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let following_id = candidate["vanilla_import"]["origins"][by_origin[&following]]["node"]
            ["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let anchor = candidate["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["id"] == previous_id)
            .unwrap();
        let before = imported.origins[previous].node["type"] == "raw";
        let continuation = if before {
            following_id.clone()
        } else {
            let next = crate::analysis::successors(&candidate, anchor);
            ensure!(
                next.len() == 1,
                "新增步骤前的原版步骤没有唯一后继，请先核对"
            );
            next[0].clone()
        };
        let first = nodes[start]["id"].clone();
        let insertion = if before {
            crate::editing::retarget(
                &mut candidate["nodes"],
                &BTreeMap::from([(following_id.clone(), first.as_str().unwrap().to_owned())]),
            );
            if candidate["start"] == following_id {
                candidate["start"] = first.clone();
            }
            candidate["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .position(|node| node["id"] == following_id)
                .unwrap()
        } else {
            candidate["nodes"][anchor]["goto"] = first;
            anchor + 1
        };
        let mut inserted = nodes[start..index].to_vec();
        for offset in 0..inserted.len() {
            inserted[offset]["goto"] = inserted
                .get(offset + 1)
                .map(|node| node["id"].clone())
                .unwrap_or(json!(continuation));
            if let Some(metadata) = inserted[offset]
                .get_mut("_editor")
                .and_then(Value::as_object_mut)
            {
                metadata.remove("vanilla_origin");
            }
        }
        candidate["nodes"]
            .as_array_mut()
            .unwrap()
            .splice(insertion..insertion, inserted);
    }
    ensure!(
        vanilla_import::parse(&candidate)?.unwrap().source() == source,
        "转换后的原版来源不一致"
    );
    crate::validate::validate_story(&candidate)?;
    Ok(candidate)
}
