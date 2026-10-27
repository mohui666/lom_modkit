//! A focused view of ordinary chapter-entry actions, with no separate game state.
use crate::{forms::Catalog, i18n::tr};
use eframe::egui;
use lom_core::project::Project;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const KINDS: &[&str] = &[
    "time",
    "stat_set",
    "stat",
    "affinity",
    "talent",
    "item",
    "flag",
    "game_flag",
    "mission",
];

fn next(nodes: &[Value], index: usize) -> String {
    nodes[index]
        .get("goto")
        .and_then(Value::as_str)
        .or_else(|| nodes.get(index + 1)?.get("id")?.as_str())
        .unwrap_or("")
        .to_owned()
}
fn prefix(story: &Value) -> Vec<usize> {
    let Some(nodes) = story["nodes"].as_array() else {
        return vec![];
    };
    let mut id = story["start"].as_str().unwrap_or("").to_owned();
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    while seen.insert(id.clone()) {
        let Some(index) = nodes.iter().position(|node| node["id"] == id) else {
            break;
        };
        if !KINDS.contains(&nodes[index]["type"].as_str().unwrap_or("")) {
            break;
        }
        result.push(index);
        id = next(nodes, index);
    }
    result
}
fn insert(story: &mut Value, mut node: Value) {
    let leading = prefix(story);
    let nodes = story["nodes"].as_array_mut().unwrap();
    let (at, target) = if let Some(&last) = leading.last() {
        let target = next(nodes, last);
        nodes[last]["goto"] = node["id"].clone();
        (last + 1, target)
    } else {
        let target = story["start"].as_str().unwrap_or("").to_owned();
        let at = story["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|n| n["id"] == target)
            .unwrap_or(0);
        story["start"] = node["id"].clone();
        (at, target)
    };
    if !target.is_empty() {
        node["goto"] = target.into();
    }
    story["nodes"].as_array_mut().unwrap().insert(at, node);
}
fn remove(story: &mut Value, index: usize) {
    let nodes = story["nodes"].as_array().unwrap();
    let old = nodes[index]["id"].as_str().unwrap().to_owned();
    let target = next(nodes, index);
    if target.is_empty() || target == old {
        return;
    }
    crate::workspace::retarget(story, &BTreeMap::from([(old, target)]));
    story["nodes"].as_array_mut().unwrap().remove(index);
}

pub fn show(ui: &mut egui::Ui, project: &mut Project, catalog: &Catalog, current: &mut String) {
    ui.heading(tr("初始设置"));
    ui.label(tr(
        "这些设置就是章节入口处的普通步骤，按顺序在对白之前执行。",
    ));
    ui.small(tr(
        "每次播放此章节都会执行；附加剧情请在剧情安排中设置开场事件只发生一次。",
    ));
    egui::ComboBox::from_id_salt("setup-chapter")
        .selected_text(
            project
                .stories
                .get(current)
                .and_then(|s| s["title"].as_str())
                .unwrap_or(current),
        )
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (id, story) in &project.stories {
                ui.selectable_value(current, id.clone(), story["title"].as_str().unwrap_or(id));
            }
        });
    let story_ids = project.stories.keys().cloned().collect::<Vec<_>>();
    let Some(story) = project.stories.get_mut(current) else {
        return;
    };
    let node_ids = story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    ui.add_space(10.0);
    let mut add = None;
    ui.menu_button(tr("＋ 添加初始设置"), |ui| {
        for kind in KINDS {
            if ui.button(catalog.label(kind)).clicked() {
                add = Some(*kind);
                ui.close();
            }
        }
    });
    if let Some(kind) = add {
        let id = (1..)
            .map(|n| format!("initial_{kind}{n}"))
            .find(|id| !node_ids.contains(id))
            .unwrap();
        let mut node = catalog.new_node(kind, id);
        if kind == "time" {
            node["op"] = json!("set");
            node["year"] = json!(1);
            node["month"] = json!(1);
            node["stage"] = json!(1);
        }
        insert(story, node);
    }
    let leading = prefix(story);
    if leading.is_empty() {
        ui.label(tr("尚无开场设置。可添加时间、属性设置、物品等步骤。"));
    }
    let mut deletion = None;
    for index in leading {
        let mut node = story["nodes"][index].clone();
        let id = node["id"].as_str().unwrap_or("").to_owned();
        egui::CollapsingHeader::new(format!(
            "{} · {id}",
            catalog.label(node["type"].as_str().unwrap_or(""))
        ))
        .id_salt(("initial", current.as_str(), &id))
        .default_open(true)
        .show(ui, |ui| {
            // Entry routing is managed here; the full step editor remains available.
            let target = node.as_object_mut().unwrap().remove("goto");
            catalog.node_form(ui, &mut node, &node_ids, &story_ids);
            if let Some(target) = target {
                node["goto"] = target;
            }
            let target = next(story["nodes"].as_array().unwrap(), index);
            if ui
                .add_enabled(
                    !target.is_empty() && target != id,
                    egui::Button::new(tr("移除此设置")),
                )
                .on_disabled_hover_text(tr("请先在此设置后连接一个剧情或结束步骤。"))
                .clicked()
            {
                deletion = Some(index);
            }
        });
        story["nodes"][index] = node;
    }
    if let Some(index) = deletion {
        remove(story, index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_settings_preserve_nonfirst_entry_and_routes() {
        let mut story = json!({"id":"main","start":"intro","nodes":[
            {"id":"unused","type":"end"},{"id":"intro","type":"say","text":"开场"},
            {"id":"loop","type":"wait","seconds":0,"goto":"intro"}]});
        insert(
            &mut story,
            json!({"id":"setup1","type":"stat_set","stat":"morale","value":20}),
        );
        insert(
            &mut story,
            json!({"id":"setup2","type":"item","item":"1001","count":1}),
        );
        assert_eq!(story["start"], "setup1");
        assert_eq!(prefix(&story), vec![1, 2]);
        assert_eq!(story["nodes"][1]["goto"], "setup2");
        assert_eq!(story["nodes"][2]["goto"], "intro");
        assert_eq!(story["nodes"][4]["goto"], "intro");
        remove(&mut story, 1);
        assert_eq!(story["start"], "setup2");
        assert_eq!(story["nodes"][1]["goto"], "intro");
        assert_eq!(story["nodes"][3]["goto"], "intro");
    }
}
