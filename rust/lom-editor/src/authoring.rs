//! Editing operations are planned on a copy and committed only after validation.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct BulkField {
    pub key: String,
    pub label: String,
    pub kind: String,
}
pub fn bulk_fields(schema: &Value, nodes: &[Value]) -> Vec<BulkField> {
    let safe = [
        "character",
        "portrait",
        "voice",
        "position",
        "from",
        "to",
        "facing",
        "duration",
        "fadeDuration",
        "moveDuration",
        "seconds",
        "fade",
        "scale",
        "opacity",
        "x",
        "y",
        "angle",
        "active",
        "dimmed",
        "play",
        "remove",
        "waitDisplay",
        "display",
        "update",
        "count",
        "level",
        "value",
        "mode",
        "layer",
        "action",
        "kind",
        "op",
        "source",
    ];
    let fields = |node: &Value| -> BTreeMap<String, BulkField> {
        schema["NODE_SCHEMAS"][node["type"].as_str().unwrap_or("")]["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| {
                let key = f[0].as_str()?;
                let kind = f[2].as_str()?;
                if !safe.contains(&key)
                    || ["options", "cases", "vars", "code", "multiline"].contains(&kind)
                {
                    return None;
                }
                Some((
                    key.into(),
                    BulkField {
                        key: key.into(),
                        label: f[1].as_str().unwrap_or(key).into(),
                        kind: kind.into(),
                    },
                ))
            })
            .collect()
    };
    let Some(first) = nodes.first() else {
        return vec![];
    };
    let mut common = fields(first);
    for node in &nodes[1..] {
        let other = fields(node);
        common.retain(|key, v| other.get(key).is_some_and(|f| f.kind == v.kind));
    }
    common.into_values().collect()
}
pub fn bulk_edit(
    story: &Value,
    indices: &BTreeSet<usize>,
    schema: &Value,
    key: &str,
    value: &Value,
) -> Result<Value> {
    let nodes = story["nodes"].as_array().context("章节没有步骤")?;
    ensure!(indices.len() >= 2, "至少选中两个步骤");
    let selected: Vec<Value> = indices
        .iter()
        .map(|i| nodes.get(*i).cloned().context("选中的步骤已不存在"))
        .collect::<Result<_>>()?;
    let fields = bulk_fields(schema, &selected);
    let field = fields
        .iter()
        .find(|f| f.key == key)
        .context("所选步骤没有类型一致的该字段")?;
    let valid = match field.kind.as_str() {
        "bool" => value.is_boolean(),
        "int" | "bool_int" | "discount_toggle" => value.is_i64() || value.is_u64(),
        "float" | "number" | "percent_scale" | "percent_cg_scale" | "percent_position"
        | "percent_offset" | "percent_opacity" => value.is_number(),
        kind if kind.starts_with("enum:") => schema["ENUM_SETS_SRC"][&kind[5..]]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| &v[0] == value)),
        _ => value.is_string(),
    };
    ensure!(valid, "{} 的值类型不符合 {}", field.label, field.kind);
    let mut candidate = story.clone();
    for i in indices {
        candidate["nodes"][*i][key] = value.clone();
    }
    // Validate the complete candidate so a field which depends on node type or
    // sibling fields cannot slip through a generic scalar widget.
    lom_core::validate::validate_story(&candidate)
        .context("整章校验失败，所有选中步骤均保持原值")?;
    Ok(candidate)
}
pub fn selection_block(
    story: &Value,
    indices: &BTreeSet<usize>,
) -> Result<(Vec<Value>, Vec<String>)> {
    ensure!(!indices.is_empty(), "没有选中的步骤");
    let first = *indices.first().unwrap();
    let last = *indices.last().unwrap();
    ensure!(
        last - first + 1 == indices.len(),
        "模板只能保存连续选中的步骤；请使用 Shift 选择连续范围"
    );
    let nodes = story["nodes"].as_array().context("章节没有步骤")?;
    ensure!(last < nodes.len(), "选中范围已失效");
    let block = nodes[first..=last].to_vec();
    validate_template(&block)?;
    let ids: BTreeSet<String> = block
        .iter()
        .filter_map(|n| n["id"].as_str().map(str::to_owned))
        .collect();
    let mut warnings = BTreeSet::new();
    for (index, node) in nodes.iter().enumerate().take(last + 1).skip(first) {
        let successors = lom_core::analysis::successors(story, index);
        for target in successors {
            if !ids.contains(&target) {
                warnings.insert(format!(
                    "{} 指向模板外部步骤 {target}；插入后请重新绑定",
                    node["id"].as_str().unwrap_or("")
                ));
            }
        }
    }
    // Sequential exit is implicit in the source document and will instead
    // continue into the destination's next node after insertion.
    if last + 1 < nodes.len() {
        let next = nodes[last + 1]["id"].as_str().unwrap_or("");
        let mut explicit = BTreeSet::new();
        collect_refs(&nodes[last], "", &mut explicit);
        if lom_core::analysis::successors(story, last)
            .iter()
            .any(|s| s == next)
            && !explicit.contains(next)
        {
            warnings.insert("末尾存在隐式顺序后继；插入后会继续到目标章节的下一步骤".into());
        }
    }
    Ok((block, warnings.into_iter().collect()))
}
fn collect_refs(v: &Value, key: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::String(s) if key == "goto" || key.starts_with("goto_") || key == "default" => {
            if !s.is_empty() {
                out.insert(s.clone());
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                collect_refs(v, k, out)
            }
        }
        Value::Array(a) => {
            for v in a {
                collect_refs(v, key, out)
            }
        }
        _ => {}
    }
}
pub fn template_warnings(block: &[Value]) -> Vec<String> {
    let ids: BTreeSet<_> = block.iter().filter_map(|v| v["id"].as_str()).collect();
    let mut refs = BTreeSet::new();
    for node in block {
        collect_refs(node, "", &mut refs);
    }
    refs.into_iter()
        .filter(|s| !ids.contains(s.as_str()))
        .map(|s| format!("外部引用 {s} 会保留，请确认目标章节中存在该 ID"))
        .collect()
}
pub fn validate_template(nodes: &[Value]) -> Result<()> {
    ensure!(!nodes.is_empty(), "模板没有步骤");
    let mut ids = BTreeSet::new();
    for node in nodes {
        let id = node["id"].as_str().context("模板步骤缺少 ID")?;
        ensure!(
            !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && ids.insert(id),
            "模板步骤 ID 不合法或重复：{id}"
        );
        ensure!(node["type"].is_string(), "模板步骤缺少类型");
        check_paths(node, "")?;
    }
    Ok(())
}
fn check_paths(v: &Value, key: &str) -> Result<()> {
    match v {
        Value::String(s)
            if [
                "asset",
                "audio",
                "background",
                "file",
                "image",
                "music",
                "path",
                "portrait",
                "sound",
                "voice",
            ]
            .contains(&key)
                || key.ends_with("_path")
                || key.ends_with("_file") =>
        {
            ensure!(
                !(s.starts_with('/') || s.starts_with('\\') || s.as_bytes().get(1) == Some(&b':')),
                "模板不能包含本机绝对资源路径：{key}"
            );
        }
        Value::Object(o) => {
            for (k, v) in o {
                check_paths(v, k)?
            }
        }
        Value::Array(a) => {
            for v in a {
                check_paths(v, key)?
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn bounds(story: &Value, item: &Value) -> Option<(usize, usize)> {
    let nodes = story["nodes"].as_array()?;
    let a = nodes.iter().position(|n| n["id"] == item["start"])?;
    let b = nodes.iter().position(|n| n["id"] == item["end"])?;
    Some((a.min(b), a.max(b)))
}
pub fn validate_sections(story: &Value) -> Result<()> {
    let mut occupied = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for section in story["_editor"]["sections"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let title = section["title"].as_str().unwrap_or("分区");
        let id = section["id"].as_str().context("分区缺少 ID")?;
        ensure!(ids.insert(id), "分区 ID 重复：{id}");
        let (a, b) =
            bounds(story, section).with_context(|| format!("分区「{title}」的起止步骤不存在"))?;
        for i in a..=b {
            ensure!(occupied.insert(i), "分区「{title}」与另一分区重叠");
        }
        let mut group_nodes = BTreeSet::new();
        let mut group_ids = BTreeSet::new();
        for group in section["groups"].as_array().into_iter().flatten() {
            let name = group["title"].as_str().unwrap_or("分组");
            let gid = group["id"].as_str().context("分组缺少 ID")?;
            ensure!(group_ids.insert(gid), "分组 ID 重复：{gid}");
            let (x, y) =
                bounds(story, group).with_context(|| format!("分组「{name}」起止步骤不存在"))?;
            ensure!(x >= a && y <= b, "分组「{name}」超出所在分区");
            for i in x..=y {
                ensure!(group_nodes.insert(i), "分组「{name}」与同分区其他分组重叠");
            }
        }
    }
    Ok(())
}
pub fn add_section(
    story: &Value,
    indices: &BTreeSet<usize>,
    parent: Option<usize>,
) -> Result<Value> {
    ensure!(!indices.is_empty(), "请先选择步骤范围");
    let a = *indices.first().unwrap();
    let b = *indices.last().unwrap();
    ensure!(b - a + 1 == indices.len(), "分区/分组必须使用连续步骤范围");
    let mut candidate = story.clone();
    let start = story["nodes"][a]["id"].clone();
    let end = story["nodes"][b]["id"].clone();
    ensure!(start.is_string() && end.is_string(), "选择范围已失效");
    if !candidate["_editor"].is_object() {
        candidate["_editor"] = json!({});
    }
    if !candidate["_editor"]["sections"].is_array() {
        candidate["_editor"]["sections"] = json!([]);
    }
    let list = if let Some(p) = parent {
        let section = candidate["_editor"]["sections"]
            .get_mut(p)
            .context("请选择所属分区")?;
        if !section["groups"].is_array() {
            section["groups"] = json!([]);
        }
        section["groups"].as_array_mut().unwrap()
    } else {
        candidate["_editor"]["sections"].as_array_mut().unwrap()
    };
    let mut n = 1;
    while list.iter().any(|v| v["id"] == format!("section{n}")) {
        n += 1;
    }
    list.push(json!({"id":format!("section{n}"),"title":if parent.is_some(){"新分组"}else{"新分区"},"start":start,"end":end,"collapsed":false,"groups":[]}));
    validate_sections(&candidate)?;
    Ok(candidate)
}
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Header {
        section: usize,
        group: Option<usize>,
        depth: usize,
        title: String,
        collapsed: bool,
    },
    Node {
        index: usize,
        depth: usize,
    },
}
pub fn section_rows(story: &Value) -> Vec<Row> {
    let count = story["nodes"].as_array().map(Vec::len).unwrap_or(0);
    if validate_sections(story).is_err() {
        return (0..count)
            .map(|index| Row::Node { index, depth: 0 })
            .collect();
    }
    let sections: Vec<_> = story["_editor"]["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, s)| Some((i, s, bounds(story, s)?)))
        .collect();
    let mut rows = vec![];
    let mut i = 0;
    while i < count {
        if let Some((si, s, (a, b))) = sections.iter().find(|(_, _, (a, b))| i >= *a && i <= *b) {
            let collapsed = s["collapsed"].as_bool().unwrap_or(false);
            if i == *a {
                rows.push(Row::Header {
                    section: *si,
                    group: None,
                    depth: 0,
                    title: s["title"].as_str().unwrap_or("分区").into(),
                    collapsed,
                });
            }
            if collapsed {
                i = b + 1;
                continue;
            }
            let group = s["groups"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .find_map(|(gi, g)| {
                    let (x, y) = bounds(story, g)?;
                    (i >= x && i <= y).then_some((gi, g, x, y))
                });
            if let Some((gi, g, x, y)) = group {
                let collapsed = g["collapsed"].as_bool().unwrap_or(false);
                if i == x {
                    rows.push(Row::Header {
                        section: *si,
                        group: Some(gi),
                        depth: 1,
                        title: g["title"].as_str().unwrap_or("分组").into(),
                        collapsed,
                    });
                }
                if collapsed {
                    i = y + 1;
                    continue;
                }
                rows.push(Row::Node { index: i, depth: 2 });
            } else {
                rows.push(Row::Node { index: i, depth: 1 });
            }
        } else {
            rows.push(Row::Node { index: i, depth: 0 });
        }
        i += 1;
    }
    rows
}
pub fn expand_for_node(story: &mut Value, index: usize) {
    let mut matches = vec![];
    for (si, s) in story["_editor"]["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if bounds(story, s).is_some_and(|(a, b)| index >= a && index <= b) {
            matches.push((si, None));
            for (gi, g) in s["groups"].as_array().into_iter().flatten().enumerate() {
                if bounds(story, g).is_some_and(|(a, b)| index >= a && index <= b) {
                    matches.push((si, Some(gi)));
                }
            }
        }
    }
    for (si, gi) in matches {
        let section = &mut story["_editor"]["sections"][si];
        if let Some(gi) = gi {
            section["groups"][gi]["collapsed"] = false.into();
        } else {
            section["collapsed"] = false.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn story() -> Value {
        json!({"id":"main","story_schema":2,"start":"a","nodes":[{"id":"a","type":"wait","seconds":1},{"id":"b","type":"wait","seconds":2},{"id":"c","type":"end"}]})
    }
    fn schema() -> Value {
        serde_json::from_str(include_str!("../data/authoring.json")).unwrap()
    }
    #[test]
    fn bulk_intersection_and_atomic_validation() {
        let s = story();
        let fields = bulk_fields(&schema(), s["nodes"].as_array().unwrap());
        assert!(fields.is_empty());
        let selection = BTreeSet::from([0, 1]);
        let changed = bulk_edit(&s, &selection, &schema(), "seconds", &json!(3)).unwrap();
        assert_eq!(changed["nodes"][0]["seconds"], 3);
        assert_eq!(changed["nodes"][1]["seconds"], 3);
        assert!(bulk_edit(&s, &selection, &schema(), "seconds", &json!(false)).is_err());
        assert_eq!(s["nodes"][0]["seconds"], 1);
    }
    #[test]
    fn block_template_requires_contiguous_selection_and_warns_external_flow() {
        let s = story();
        assert!(selection_block(&s, &BTreeSet::from([0, 2])).is_err());
        let (block, warnings) = selection_block(&s, &BTreeSet::from([0, 1])).unwrap();
        assert_eq!(block.len(), 2);
        assert!(warnings.iter().any(|s| s.contains("隐式")));
        assert!(validate_template(&[
            json!({"id":"x","type":"intro","image":"/Users/private.png"})
        ])
        .is_err());
    }
    #[test]
    fn section_validation_collapsing_and_expansion_preserve_range() {
        let s = add_section(&story(), &BTreeSet::from([0, 1]), None).unwrap();
        assert!(add_section(&s, &BTreeSet::from([1, 2]), None).is_err());
        assert!(add_section(&s, &BTreeSet::from([1, 2]), Some(0)).is_err());
        let mut s = add_section(&s, &BTreeSet::from([0]), Some(0)).unwrap();
        s["_editor"]["sections"][0]["collapsed"] = true.into();
        assert_eq!(section_rows(&s).len(), 2);
        expand_for_node(&mut s, 0);
        assert_eq!(section_rows(&s).len(), 5);
    }
}

/// Range controls use existing node IDs so typing an intermediate partial ID
/// cannot accidentally invalidate or lose a section.
pub fn section_editor(ui: &mut eframe::egui::Ui, story: &Value) -> Option<Result<Value>> {
    let mut candidate = story.clone();
    let ids: Vec<String> = story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect();
    let Some(sections) = candidate["_editor"]["sections"].as_array_mut() else {
        return None;
    };
    let mut changed = false;
    let mut remove = None;
    for (si, section) in sections.iter_mut().enumerate() {
        ui.push_id(("section", si), |ui| {
            eframe::egui::Frame::group(ui.style()).show(ui, |ui| {
                changed |= range_form(ui, section, &ids);
                if ui.small_button(crate::i18n::tr("删除分区")).clicked() {
                    remove = Some(si);
                }
                if let Some(groups) = section["groups"].as_array_mut() {
                    let mut remove_group = None;
                    for (gi, group) in groups.iter_mut().enumerate() {
                        ui.push_id(("group", gi), |ui| {
                            ui.indent("group", |ui| {
                                ui.label(crate::i18n::tr("分组"));
                                changed |= range_form(ui, group, &ids);
                                if ui.small_button(crate::i18n::tr("删除分组")).clicked() {
                                    remove_group = Some(gi);
                                }
                            });
                        });
                    }
                    if let Some(i) = remove_group {
                        groups.remove(i);
                        changed = true;
                    }
                }
            });
        });
    }
    if let Some(i) = remove {
        sections.remove(i);
        changed = true;
    }
    if !changed {
        return None;
    }
    Some(validate_sections(&candidate).map(|()| candidate))
}
fn range_form(ui: &mut eframe::egui::Ui, item: &mut Value, ids: &[String]) -> bool {
    let mut changed = false;
    let mut title = item["title"].as_str().unwrap_or("").to_owned();
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("标题"));
        if ui.text_edit_singleline(&mut title).changed() {
            item["title"] = title.into();
            changed = true;
        }
        let mut folded = item["collapsed"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut folded, crate::i18n::tr("折叠")).changed() {
            item["collapsed"] = folded.into();
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        for (key, label) in [("start", "起始步骤"), ("end", "结束步骤")] {
            ui.label(crate::i18n::tr(label));
            let old = item[key].as_str().unwrap_or("").to_owned();
            let mut selected = old.clone();
            eframe::egui::ComboBox::from_id_salt(key)
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for id in ids {
                        ui.selectable_value(&mut selected, id.clone(), id);
                    }
                });
            if selected != old {
                item[key] = selected.into();
                changed = true;
            }
        }
    });
    changed
}

#[cfg(test)]
mod constraint_tests {
    use super::*;
    #[test]
    fn per_node_overlay_range_rejects_entire_bulk_transaction() {
        let story = json!({"id":"main","start":"a","nodes":[{"id":"a","type":"overlay","action":"show","slot":"left","image":"user:demo.image","opacity":90},{"id":"b","type":"overlay","action":"show","slot":"right","image":"user:demo.image","opacity":80},{"id":"end","type":"end"}]});
        let schema: Value = serde_json::from_str(include_str!("../data/authoring.json")).unwrap();
        let result = bulk_edit(
            &story,
            &BTreeSet::from([0, 1]),
            &schema,
            "opacity",
            &json!(200),
        );
        assert!(result.is_err());
        assert_eq!(story["nodes"][0]["opacity"], 90);
        assert_eq!(story["nodes"][1]["opacity"], 80);
        let changed = bulk_edit(
            &story,
            &BTreeSet::from([0, 1]),
            &schema,
            "opacity",
            &json!(50),
        )
        .unwrap();
        assert_eq!(changed["nodes"][0]["opacity"], 50);
        assert_eq!(changed["nodes"][1]["opacity"], 50);
    }
}
