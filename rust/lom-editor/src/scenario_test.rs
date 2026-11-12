//! Temporary event test state; never stored in the authored project.
use crate::{forms::Catalog, i18n};
use anyhow::{ensure, Context, Result};
use eframe::egui;
use lom_core::project::Project;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn collect(when: &Value, values: &mut Vec<Value>, seen: &mut BTreeSet<(String, String)>) {
    if let Some(source) = when["source"].as_str() {
        let key = when["key"].as_str().unwrap_or("");
        if source != "time" && seen.insert((source.to_owned(), key.to_owned())) {
            values.push(json!({"source":source,"key":key,"value":when["value"]}));
        }
    }
    for field in ["all", "any"] {
        for child in when[field].as_array().into_iter().flatten() {
            collect(child, values, seen);
        }
    }
    if let Some(child) = when.get("not") {
        collect(child, values, seen);
    }
}

fn reference_date(when: &Value, draft: &mut Value) {
    if when["source"] == "time" && matches!(when["op"].as_str(), Some("eq" | "ge")) {
        if let Some(key @ ("year" | "month" | "stage")) = when["key"].as_str() {
            draft[key] = when["value"].clone();
        }
    }
    for field in ["all", "any"] {
        for child in when[field].as_array().into_iter().flatten() {
            reference_date(child, draft);
        }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    project: &Project,
    event_id: Option<&str>,
    catalog: &Catalog,
) -> Option<Value> {
    let config = project.scenario.as_ref()?;
    let event = config["events"]
        .as_array()?
        .iter()
        .find(|e| e["id"].as_str() == event_id)
        .or_else(|| config["events"].as_array()?.first())?;
    if event["trigger"] == "start" {
        return None;
    }
    let id = egui::Id::new((
        "event-test-state",
        project.manifest["id"].as_str(),
        event["id"].as_str(),
    ));
    let mut draft = ui
        .ctx()
        .data(|d| d.get_temp::<Value>(id))
        .unwrap_or_else(|| {
            let mut values = Vec::new();
            collect(&event["when"], &mut values, &mut BTreeSet::new());
            let mut draft =
                json!({"event_id":event["id"],"year":1,"month":5,"stage":1,"values":values});
            reference_date(&event["when"], &mut draft);
            draft
        });
    let mut current = Vec::new();
    collect(&event["when"], &mut current, &mut BTreeSet::new());
    for row in &mut current {
        if let Some(previous) = draft["values"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|old| old["source"] == row["source"] && old["key"] == row["key"])
        {
            row["value"] = previous["value"].clone();
        }
    }
    draft["values"] = json!(current);
    let mut run = false;
    egui::CollapsingHeader::new(i18n::key("editor.event_test.title")).show(ui, |ui| {
        ui.small(i18n::key("editor.event_test.hint"));
        let mut original = draft["include_official_events"].as_bool().unwrap_or(false);
        if ui
            .checkbox(
                &mut original,
                i18n::key("editor.event_test.original_events"),
            )
            .changed()
        {
            draft["include_official_events"] = json!(original);
        }
        for (key, label, high) in [("year", "年", 9), ("month", "月", 12), ("stage", "旬", 3)] {
            ui.horizontal(|ui| {
                ui.label(i18n::tr(label));
                let mut n = draft[key].as_i64().unwrap_or(1);
                if ui
                    .add(egui::DragValue::new(&mut n).range(1..=high))
                    .changed()
                {
                    draft[key] = json!(n);
                }
            });
        }
        for (index, row) in draft["values"]
            .as_array_mut()
            .into_iter()
            .flatten()
            .enumerate()
        {
            ui.push_id(index, |ui| {
                let source = row["source"].as_str().unwrap_or("");
                let key = row["key"].as_str().unwrap_or("");
                let kind = match source {
                    "stat" => "stat",
                    "affinity" => "affinity_character",
                    "game_flag" => "game_flag",
                    _ => "",
                };
                let name = if source == "task" {
                    config["tasks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|t| t["id"] == key)
                        .and_then(|t| t["name"].as_str())
                        .unwrap_or(key)
                        .to_owned()
                } else {
                    catalog
                        .options(kind, &Value::Null, &[], &[])
                        .into_iter()
                        .find(|(id, _)| id == key)
                        .map(|(_, name)| name)
                        .unwrap_or_else(|| key.to_owned())
                };
                ui.label(name).on_hover_text(key);
                if source == "task" {
                    let task = config["tasks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|t| t["id"] == key);
                    let mut state = row["value"].as_str().unwrap_or("").to_owned();
                    egui::ComboBox::from_id_salt("task-state")
                        .selected_text(crate::scenario_quick::state_name(&state))
                        .show_ui(ui, |ui| {
                            for option in task
                                .into_iter()
                                .flat_map(|t| t["states"].as_array())
                                .flatten()
                                .filter_map(Value::as_str)
                            {
                                ui.selectable_value(
                                    &mut state,
                                    option.into(),
                                    crate::scenario_quick::state_name(option),
                                );
                            }
                        });
                    row["value"] = json!(state);
                } else {
                    let mut n = row["value"].as_i64().unwrap_or(0);
                    let range = match source {
                        "flag" => 0..=1,
                        "affinity" => 0..=100,
                        _ => i32::MIN as i64..=i32::MAX as i64,
                    };
                    if ui.add(egui::DragValue::new(&mut n).range(range)).changed() {
                        row["value"] = json!(n);
                    }
                }
            });
        }
        ui.small(i18n::key("editor.event_test.values_hint"));
        run = ui.button(i18n::key("editor.event_test.run")).clicked();
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, draft.clone()));
    run.then_some(draft)
}

pub fn prepare(project: &Project, spec: &Value) -> Result<Project> {
    let mut preview = project.clone();
    let config = preview.scenario.as_mut().context("项目没有事件配置")?;
    let event_id = spec["event_id"].as_str().context("缺少测试事件")?;
    ensure!(
        config["events"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|e| e["id"] == event_id && e["trigger"] != "start"),
        "测试事件不存在或为开场事件"
    );
    let setup = (1..)
        .map(|i| format!("event_test_setup_{i}"))
        .find(|id| !preview.stories.contains_key(id))
        .unwrap();
    let entry = (1..)
        .map(|i| format!("event_test_entry_{i}"))
        .find(|id| {
            !config["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == *id)
        })
        .unwrap();
    let mut actions = vec![
        json!({"type":"time","year":spec["year"],"month":spec["month"],"stage":spec["stage"]}),
    ];
    let mut variables = json!({});
    let mut tasks = json!({});
    for row in spec["values"].as_array().context("测试状态必须是列表")? {
        let key = row["key"].as_str().context("测试状态缺少名称")?;
        match row["source"].as_str().unwrap_or("") {
            "stat" | "affinity" | "game_flag" => actions
                .push(json!({"type":row["source"],"key":key,"op":"set","value":row["value"]})),
            "flag" => {
                if row["value"] == 1 {
                    actions.push(json!({"type":"flag","key":key}));
                }
            }
            "variable" => variables[key] = json!({"op":"set","value":row["value"]}),
            "task" => tasks[key] = row["value"].clone(),
            _ => anyhow::bail!("不支持的测试状态来源：{}", row["source"]),
        }
    }
    config["events"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["trigger"] != "start");
    config["events"].as_array_mut().unwrap().push(json!({"id":entry,"name":"事件测试准备","trigger":"start","script":setup,"when":{"all":[]},"outcomes":{"completed":{"actions":actions,"variables":variables,"tasks":tasks}}}));
    config["entry_event"] = json!(entry);
    preview.stories.insert(
        setup.clone(),
        json!({"id":setup,"story_schema":2,"start":"notice","nodes":[
        {"id":"notice","type":"message","text":i18n::key("editor.event_test.game_notice")},
        {"id":"ready","type":"end"}]}),
    );
    preview.manifest = json!({"format":3,"package_format":3,"story_schema":2,"content_schema":1,"id":lom_core::game_tools::PREVIEW_ID,"campaign_id":lom_core::game_tools::PREVIEW_ID,"name":"编辑器事件测试","version":"0.0.0-preview","author":"lom_modkit","description":"临时测试状态","entry":setup,"play_mode":"sandbox","min_host_version":"1.5.11","campaign":{"new_game":true,"disable_official_events":!spec["include_official_events"].as_bool().unwrap_or(false)}});
    lom_core::scenario::validate(config, &preview.manifest, &preview.stories)?;
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_test_preserves_conditions_and_author_source_in_an_isolated_campaign() {
        let mut project = Project::new_scenario("addon");
        let event = &mut project.scenario.as_mut().unwrap()["events"][0];
        event["trigger"] = json!("story_entry");
        event.as_object_mut().unwrap().remove("position");
        event["target"] = json!("section_01_free_03");
        event["from"] = json!("section_01_free_start_02");
        event["chance"] = json!(37);
        event["when"] = json!({"all":[{"source":"time","key":"year","op":"ge","value":3},{"source":"game_flag","key":"original_flag","op":"eq","value":2}]});
        let original = project.clone();
        let spec = json!({"event_id":"opening","year":3,"month":11,"stage":2,"values":[{"source":"game_flag","key":"original_flag","value":2}]});
        let preview = prepare(&project, &spec).unwrap();
        assert_eq!(
            preview.scenario.as_ref().unwrap()["events"][0],
            original.scenario.as_ref().unwrap()["events"][0]
        );
        assert_eq!(preview.manifest["id"], lom_core::game_tools::PREVIEW_ID);
        assert_eq!(
            preview.manifest["campaign_id"],
            lom_core::game_tools::PREVIEW_ID
        );
        assert_eq!(preview.manifest["play_mode"], "sandbox");
        assert_eq!(
            preview.manifest["campaign"]["disable_official_events"],
            true
        );
        let mut alongside_original = spec.clone();
        alongside_original["include_official_events"] = json!(true);
        assert_eq!(
            prepare(&project, &alongside_original).unwrap().manifest["campaign"]
                ["disable_official_events"],
            false
        );
        let actions =
            &preview.scenario.as_ref().unwrap()["events"][1]["outcomes"]["completed"]["actions"];
        assert_eq!(actions[0]["year"], 3);
        assert_eq!(
            actions[1],
            json!({"type":"game_flag","key":"original_flag","op":"set","value":2})
        );
        let dir = tempfile::tempdir().unwrap();
        preview.export(&dir.path().join("test.lommod")).unwrap();
        assert_eq!(
            Project::open(&dir.path().join("test.lommod"))
                .unwrap()
                .scenario,
            preview.scenario
        );
        assert_eq!(project, original);
    }
}
