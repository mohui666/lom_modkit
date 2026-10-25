//! Everyday event authoring. Edits only the conditions the controls represent.
use crate::{forms::Catalog, i18n::tr};
use eframe::egui;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Names {
    pub stories: Vec<(String, String)>,
    pub tasks: Vec<(String, String)>,
    pub states: BTreeMap<String, Vec<String>>,
    results: BTreeMap<String, BTreeMap<String, String>>,
}
impl Names {
    pub fn new(stories: &BTreeMap<String, Value>, config: Option<&Value>) -> Self {
        let mut result = Self {
            stories: vec![],
            tasks: vec![],
            states: BTreeMap::new(),
            results: BTreeMap::new(),
        };
        for (id, story) in stories {
            let name = story["title"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(id);
            result.stories.push((id.clone(), name.to_owned()));
            let mut labels = BTreeMap::new();
            for node in story["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|n| n["type"] == "choice")
            {
                for option in node["options"].as_array().into_iter().flatten() {
                    if let Some(end) = story["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|n| n["id"] == option["goto"] && n["type"] == "end")
                    {
                        let code = end["event_result"].as_str().unwrap_or("completed");
                        if let Some(text) = option["text"].as_str() {
                            labels.insert(code.to_owned(), format!("选择「{text}」后"));
                        }
                    }
                }
            }
            result.results.insert(id.clone(), labels);
        }
        for task in config
            .into_iter()
            .flat_map(|c| c["tasks"].as_array().into_iter().flatten())
        {
            let id = task["id"].as_str().unwrap_or("").to_owned();
            let name = task["name"]
                .as_str()
                .filter(|name| !name.is_empty() && *name != id)
                .unwrap_or("未命名任务")
                .to_owned();
            result.tasks.push((id.clone(), name));
            result.states.insert(
                id,
                task["states"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            );
        }
        result
    }
    fn story(&self, id: &str) -> String {
        label(&self.stories, id, "尚未选择剧情")
    }
    fn task(&self, id: &str) -> String {
        label(&self.tasks, id, "找不到这个任务")
    }
    pub fn event(&self, e: &Value) -> String {
        e["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && Some(*s) != e["id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| self.story(e["script"].as_str().unwrap_or("")))
    }
    fn result(&self, script: &str, code: &str) -> String {
        if code == "completed" {
            return tr("正常结束后");
        }
        self.results
            .get(script)
            .and_then(|r| r.get(code))
            .cloned()
            .unwrap_or_else(|| state_name(code))
    }
}
fn label(items: &[(String, String)], id: &str, missing: &str) -> String {
    items
        .iter()
        .find(|(key, _)| key == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| tr(missing))
}
pub fn state_name(id: &str) -> String {
    match id {
        "locked" | "inactive" => tr("未开始"),
        "available" => tr("可接取"),
        "active" => tr("进行中"),
        "accepted" => tr("已接取"),
        "done" | "completed" => tr("已完成"),
        "failed" => tr("已失败"),
        "declined" => tr("已拒绝"),
        "cancelled" => tr("已取消"),
        _ => format!("{}（{id}）", tr("自定义状态")),
    }
}
fn field_row<R>(ui: &mut egui::Ui, title: &str, edit: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if ui.available_width() < 112.0 + ui.spacing().item_spacing.x + 190.0 {
        return ui
            .vertical(|ui| {
                ui.label(tr(title));
                edit(ui)
            })
            .inner;
    }
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(112.0, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(112.0);
                ui.add(egui::Label::new(tr(title)).truncate());
            },
        );
        edit(ui)
    })
    .inner
}
fn named_choice(
    ui: &mut egui::Ui,
    v: &mut Value,
    key: &str,
    title: &str,
    items: &[(String, String)],
    missing: &str,
) -> bool {
    let mut selected = v[key].as_str().unwrap_or("").to_owned();
    let before = selected.clone();
    field_row(ui, title, |ui| {
        egui::ComboBox::from_id_salt(key)
            .width(190.0_f32.min(ui.available_width()))
            .truncate()
            .selected_text(label(items, &selected, missing))
            .show_ui(ui, |ui| {
                for (id, name) in items {
                    ui.selectable_value(&mut selected, id.clone(), tr(name));
                }
            });
    });
    if selected != before {
        v[key] = json!(selected);
        true
    } else {
        false
    }
}
pub fn task_state(
    ui: &mut egui::Ui,
    v: &mut Value,
    key: &str,
    title: &str,
    states: &[String],
) -> bool {
    named_choice(
        ui,
        v,
        key,
        title,
        &states
            .iter()
            .map(|s| (s.clone(), state_name(s)))
            .collect::<Vec<_>>(),
        "请选择状态",
    )
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SimpleWhen {
    year: Option<i64>,
    month: Option<i64>,
    stage: Option<i64>,
    task: Option<(String, String)>,
    other: Vec<Value>,
}
impl SimpleWhen {
    fn read(v: &Value) -> Option<Self> {
        if v.get("any").is_some() || v.get("not").is_some() {
            return None;
        }
        let mut parts = vec![];
        fn flatten(v: &Value, out: &mut Vec<Value>) {
            if let Some(all) = v["all"].as_array() {
                for c in all {
                    flatten(c, out);
                }
            } else {
                out.push(v.clone());
            }
        }
        flatten(v, &mut parts);
        let mut s = Self::default();
        for c in parts {
            if c["source"] == "time" && c["op"] == "eq" {
                let (target, max) = match c["key"].as_str() {
                    Some("year") => (&mut s.year, 9),
                    Some("month") => (&mut s.month, 12),
                    Some("stage") => (&mut s.stage, 3),
                    _ => {
                        s.other.push(c);
                        continue;
                    }
                };
                if let Some(n) = c["value"].as_i64().filter(|n| (1..=max).contains(n)) {
                    if target.is_some() {
                        return None;
                    }
                    *target = Some(n);
                    continue;
                }
            }
            if c["source"] == "task" && c["op"] == "eq" {
                if let (Some(id), Some(state)) = (c["key"].as_str(), c["value"].as_str()) {
                    if s.task.is_some() {
                        return None;
                    }
                    s.task = Some((id.to_owned(), state.to_owned()));
                    continue;
                }
            }
            s.other.push(c);
        }
        Some(s)
    }
    fn write(&self) -> Value {
        let mut all = self.other.clone();
        for (key, n) in [
            ("year", self.year),
            ("month", self.month),
            ("stage", self.stage),
        ] {
            if let Some(n) = n {
                all.push(json!({"source":"time","key":key,"op":"eq","value":n}));
            }
        }
        if let Some((id, state)) = &self.task {
            all.push(json!({"source":"task","key":id,"op":"eq","value":state}));
        }
        json!({"all":all})
    }
    fn date(&self) -> String {
        let mut s = String::new();
        if let Some(n) = self.year {
            s += &format!("第{n}年");
        }
        if let Some(n) = self.month {
            s += &format!("{n}月");
        }
        if let Some(n) = self.stage {
            s += match n {
                1 => "上旬",
                2 => "中旬",
                _ => "下旬",
            };
        }
        if s.is_empty() {
            tr("时间不限")
        } else {
            s
        }
    }
}
fn date_part(ui: &mut egui::Ui, key: &str, value: &mut Option<i64>, none: &str, max: i64) -> bool {
    let before = *value;
    let name = |n: i64| -> String {
        match key {
            "year" => format!("第{n}年"),
            "month" => format!("{n}月"),
            _ => tr(match n {
                1 => "上旬",
                2 => "中旬",
                _ => "下旬",
            }),
        }
    };
    egui::ComboBox::from_id_salt(key)
        .width(112.0_f32.min(ui.available_width()))
        .truncate()
        .selected_text(value.map(name).unwrap_or_else(|| tr(none)))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, tr(none));
            for n in 1..=max {
                ui.selectable_value(value, Some(n), name(n));
            }
        });
    before != *value
}
pub fn dates(ui: &mut egui::Ui, when: &mut Value, show_task: bool, names: &Names) -> bool {
    let Some(mut simple) = SimpleWhen::read(when) else {
        ui.label(tr("此处已有复杂条件，请在高级设置中调整。"));
        return false;
    };
    let before = simple.clone();
    ui.label(tr("发生时间"));
    ui.horizontal_wrapped(|ui| {
        for (key, value, none, max) in [
            ("year", &mut simple.year, "不限年份", 9),
            ("month", &mut simple.month, "不限月份", 12),
            ("stage", &mut simple.stage, "不限旬", 3),
        ] {
            // Reserve each date control before drawing it so the parent can wrap.
            ui.allocate_ui_with_layout(
                egui::vec2(112.0, ui.spacing().interact_size.y),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| date_part(ui, key, value, none, max),
            );
        }
    });
    ui.small(tr("每月分上旬、中旬、下旬，游戏没有单独的“几号”。"));
    if show_task && !names.tasks.is_empty() {
        let title = match &simple.task {
            Some((id, state)) => format!("前置任务：{} · {}", names.task(id), state_name(state)),
            None => tr("前置任务（可选）"),
        };
        let mut title_job = egui::text::LayoutJob::simple(
            title.clone(),
            egui::TextStyle::Button.resolve(ui.style()),
            ui.visuals().text_color(),
            (ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x).max(0.0),
        );
        title_job.wrap.max_rows = 1;
        title_job.wrap.overflow_character = Some('…');
        let title_galley = ui.fonts_mut(|fonts| fonts.layout_job(title_job));
        let response = egui::CollapsingHeader::new(title_galley).id_salt("require_task").show(ui,|ui| {
            let mut id=simple.task.as_ref().map(|(id,_)|id.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("required_task").width(190.0_f32.min(ui.available_width())).truncate().selected_text(if id.is_empty(){tr("不要求任务")}else{names.task(&id)}).show_ui(ui,|ui| {
                ui.selectable_value(&mut id,String::new(),tr("不要求任务"));
                for (key,name) in &names.tasks {ui.selectable_value(&mut id,key.clone(),name);}
            });
            if id.is_empty(){simple.task=None;}
            else {
                let states=names.states.get(&id).cloned().unwrap_or_default();
                let mut value=json!({"state":simple.task.as_ref().filter(|(key,_)|key==&id).map(|(_,s)|s.clone()).unwrap_or_else(||states.first().cloned().unwrap_or_default())});
                task_state(ui,&mut value,"state","需要处于",&states);
                simple.task=Some((id,value["state"].as_str().unwrap_or("").into()));
            }
        });
        response.header_response.on_hover_text(title);
    }
    if !simple.other.is_empty() {
        ui.small(format!(
            "另有 {} 项高级条件；修改日期会保留它们。",
            simple.other.len()
        ));
    }
    if simple != before {
        *when = simple.write();
        true
    } else {
        false
    }
}
pub fn event(
    ui: &mut egui::Ui,
    e: &mut Value,
    names: &Names,
    catalog: &Catalog,
    story_ids: &[String],
) {
    ui.push_id("easy", |ui| {
        let mut name = names.event(e);
        field_row(ui, "事件名称", |ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width(190.0_f32.min(ui.available_width())),
                )
                .changed()
            {
                e["name"] = json!(name);
            }
        });
        let start = e["trigger"] == "start";
        if start {
            ui.label(tr("自由剧本开场时，播放下面这段剧情。"));
        } else {
            dates(ui, &mut e["when"], true, names);
            let mut trigger = e["trigger"].as_str().unwrap_or("auto").to_owned();
            let before = trigger.clone();
            field_row(ui, "怎样遇到", |ui| {
                egui::ComboBox::from_id_salt("trigger")
                    .width(190.0_f32.min(ui.available_width()))
                    .truncate()
                    .selected_text(tr(if trigger == "position" {
                        "去某地点时"
                    } else {
                        "时间到了自动发生"
                    }))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut trigger, "auto".into(), tr("时间到了自动发生"));
                        ui.selectable_value(&mut trigger, "position".into(), tr("去某地点时"));
                    });
            });
            if trigger != before {
                e["trigger"] = json!(trigger);
                if e["trigger"] == "position" {
                    e["position"] = json!("Center");
                } else {
                    e.as_object_mut().unwrap().remove("position");
                }
            }
            if e["trigger"] == "position" {
                let places = catalog.options("free_position", e, &[], story_ids);
                named_choice(ui, e, "position", "地点", &places, "请选择地点");
            }
        }
        named_choice(
            ui,
            e,
            "script",
            "播放哪段剧情",
            &names.stories,
            "请选择剧情",
        );
        if !start {
            field_row(ui, "发生概率", |ui| {
                let mut chance = e["chance"].as_f64().unwrap_or(100.0);
                if ui
                    .add(
                        egui::DragValue::new(&mut chance)
                            .range(0.0..=100.0)
                            .suffix("%"),
                    )
                    .changed()
                {
                    e["chance"] = json!(chance);
                }
            });
            let mut repeat = !e["once"].as_bool().unwrap_or(true);
            if ui.checkbox(&mut repeat, tr("可以重复发生")).changed() {
                e["once"] = json!(!repeat);
            }
            let choices = vec![
                ("action".into(), tr("每次行动")),
                ("stage".into(), tr("每旬")),
                ("month".into(), tr("每月")),
            ];
            let mut cadence = json!({"check":e.get("check").cloned().unwrap_or(json!("stage"))});
            if named_choice(
                ui,
                &mut cadence,
                "check",
                if repeat {
                    "重复发生间隔"
                } else {
                    "再次尝试间隔"
                },
                &choices,
                "每旬",
            ) {
                e["check"] = cadence["check"].clone();
            }
            if e.get("pool").is_some() {
                ui.small(tr("此事件会与同组事件随机选一件，具体权重在高级设置中。"));
            }
        }
        let date = SimpleWhen::read(&e["when"])
            .map(|s| s.date())
            .unwrap_or_else(|| tr("按高级条件"));
        let mode = if start {
            tr("开场时")
        } else if e["trigger"] == "position" {
            tr("前往所选地点时")
        } else {
            tr("时间到来时")
        };
        ui.add_space(5.0);
        let story = names.story(e["script"].as_str().unwrap_or(""));
        if start {
            ui.label(format!("开场时播放《{story}》。"));
        } else {
            let chance = e["chance"].as_f64().unwrap_or(100.0);
            let repeat = if e["once"] == false {
                "可以重复发生"
            } else {
                "成功播放后不再发生"
            };
            ui.label(format!(
                "{date}，{mode}以 {chance}% 概率播放《{story}》，{repeat}。"
            ));
        }
    });
}
pub fn task(ui: &mut egui::Ui, t: &mut Value, names: &Names) {
    ui.push_id("easy", |ui| {
        let mut name = t["name"].as_str().unwrap_or("新的任务").to_owned();
        field_row(ui, "任务名称", |ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width(190.0_f32.min(ui.available_width())),
                )
                .changed()
            {
                t["name"] = json!(name);
            }
        });
        let states = t["states"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        task_state(ui, t, "initial", "开始时的状态", &states);
        let count = t["transitions"].as_array().map_or(0, Vec::len);
        if count == 0 {
            ui.small(tr("由事件结束后的结果更新进度；不必设置自动变化。"));
        }
        if ui.button(tr("添加自动变化（可选）")).clicked() {
            if !t["transitions"].is_array() {
                t["transitions"] = json!([]);
            }
            let from = t["initial"].clone();
            let to = states
                .iter()
                .find(|s| json!(s) != from)
                .cloned()
                .unwrap_or_default();
            t["transitions"]
                .as_array_mut()
                .unwrap()
                .push(json!({"from":from,"to":to,"when":{"all":[]}}));
        }
        let mut remove = None;
        for (i, r) in t
            .get_mut("transitions")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .enumerate()
        {
            ui.push_id(i, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    task_state(ui, r, "from", "当前是", &states);
                    task_state(ui, r, "to", "自动变成", &states);
                    dates(ui, &mut r["when"], false, names);
                    if ui.small_button(tr("移除这条变化")).clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            t["transitions"].as_array_mut().unwrap().remove(i);
        }
    });
}
pub fn results(ui: &mut egui::Ui, e: &mut Value, names: &Names) {
    let script = e["script"].as_str().unwrap_or("").to_owned();
    let Some(outcomes) = e["outcomes"].as_object_mut() else {
        return;
    };
    ui.collapsing(tr("剧情结束后做什么（可选）"), |ui| {
        for (code, effects) in outcomes {
            ui.push_id(code, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(names.result(&script, code));
                    if names.tasks.is_empty() {
                        ui.small(tr("未关联任务，只播放剧情即可。"));
                    } else {
                        let mut add = String::new();
                        egui::ComboBox::from_id_salt("add_progress")
                            .width(190.0_f32.min(ui.available_width()))
                            .truncate()
                            .selected_text(tr("让某个任务更新进度…"))
                            .show_ui(ui, |ui| {
                                for (id, name) in &names.tasks {
                                    ui.selectable_value(&mut add, id.clone(), name);
                                }
                            });
                        if !add.is_empty() && effects["tasks"].get(&add).is_none() {
                            if !effects["tasks"].is_object() {
                                effects["tasks"] = json!({});
                            }
                            let states = names.states.get(&add).cloned().unwrap_or_default();
                            let state = states
                                .iter()
                                .find(|s| s.as_str() == "completed" || s.as_str() == "done")
                                .cloned()
                                .or_else(|| states.last().cloned())
                                .unwrap_or_default();
                            effects["tasks"][&add] = json!(state);
                        }
                    }
                    let keys = effects["tasks"]
                        .as_object()
                        .map(|v| v.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default();
                    for id in keys {
                        ui.push_id(&id, |ui| {
                            let states = names.states.get(&id).cloned().unwrap_or_default();
                            task_state(
                                ui,
                                &mut effects["tasks"],
                                &id,
                                &format!("{}变成", names.task(&id)),
                                &states,
                            );
                            if ui.small_button(tr("不更新这个任务")).clicked() {
                                effects["tasks"].as_object_mut().unwrap().remove(&id);
                            }
                        });
                    }
                    let actions = effects["actions"].as_array().map_or(0, Vec::len);
                    if actions > 0 {
                        ui.small(format!(
                            "另有 {actions} 项奖励或其他结果动作，可在高级设置中修改。"
                        ));
                    }
                });
            });
        }
    });
}

/// Names in navigation are derived from the config; no new ownership is implied.
pub fn related_tasks(event: &Value) -> Vec<String> {
    fn conditions(v: &Value, out: &mut std::collections::BTreeSet<String>) {
        if v["source"] == "task" {
            if let Some(id) = v["key"].as_str() {
                out.insert(id.into());
            }
        }
        if let Some(map) = v.as_object() {
            for value in map.values() {
                conditions(value, out);
            }
        }
        if let Some(values) = v.as_array() {
            for value in values {
                conditions(value, out);
            }
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    conditions(&event["when"], &mut ids);
    for modifier in event["weight_modifiers"].as_array().into_iter().flatten() {
        conditions(&modifier["when"], &mut ids);
    }
    for effects in event["outcomes"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let Some(tasks) = effects["tasks"].as_object() {
            ids.extend(tasks.keys().cloned());
        }
    }
    ids.into_iter().collect()
}
pub fn event_summary(e: &Value) -> String {
    if e["trigger"] == "start" {
        return tr("自由剧本开场");
    }
    let date = SimpleWhen::read(&e["when"])
        .map(|s| s.date())
        .unwrap_or_else(|| tr("高级条件"));
    let repeat = if e["once"] == false {
        tr("可重复")
    } else {
        tr("一次性")
    };
    format!("{date} · {repeat}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn date_edits_preserve_other_conditions_and_task_requirements() {
        let raw = json!({"all":[{"source":"time","key":"month","op":"eq","value":7},{"source":"task","key":"letter","op":"eq","value":"active"},{"source":"stat","key":"money","op":"ge","value":10}]});
        let mut simple = SimpleWhen::read(&raw).unwrap();
        simple.year = Some(2);
        simple.stage = Some(1);
        let result = simple.write();
        assert!(result["all"].as_array().unwrap().contains(&raw["all"][2]));
        let read = SimpleWhen::read(&result).unwrap();
        assert_eq!(read.task, Some(("letter".into(), "active".into())));
        assert_eq!(read.date(), "第2年7月上旬");
        assert!(SimpleWhen::read(&json!({"any":[raw]})).is_none());
    }
    #[test]
    fn names_and_choice_results_are_readable_without_changing_identifiers() {
        let project = ProjectFixture::project();
        let names = Names::new(&project.stories, project.scenario.as_ref());
        assert_eq!(names.story("meeting"), "七月来信");
        assert_eq!(names.result("meeting", "accepted"), "选择「接下信件」后");
        assert_eq!(state_name("completed"), "已完成");
    }
    struct ProjectFixture;
    impl ProjectFixture {
        fn project() -> lom_core::project::Project {
            lom_core::project::Project::open(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/event-addon"),
            )
            .unwrap()
        }
    }
}
