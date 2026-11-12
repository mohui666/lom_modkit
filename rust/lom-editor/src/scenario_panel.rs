use crate::{forms::Catalog, i18n::tr};
use eframe::egui;
use lom_core::project::Project;
use serde_json::{json, Value};

#[derive(Default)]
pub struct Selection {
    pub event: Option<String>,
    pub task: Option<String>,
}

pub enum Navigate {
    Story(String),
    Event(String),
    Task(String),
}

fn selected<'a>(config: &'a Value, task_page: bool, selection: &Selection) -> Option<&'a Value> {
    let (key, id) = if task_page {
        ("tasks", &selection.task)
    } else {
        ("events", &selection.event)
    };
    let items = config[key].as_array()?;
    id.as_ref()
        .and_then(|id| items.iter().find(|v| v["id"] == *id))
        .or_else(|| items.first())
}

pub fn navigation(
    ui: &mut egui::Ui,
    project: &mut Project,
    task_page: bool,
    selection: &mut Selection,
) -> Option<bool> {
    let names = crate::scenario_quick::Names::new(&project.stories, project.scenario.as_ref());
    let script = project
        .stories
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "main".into());
    let config = project.scenario.as_mut()?;
    if !config["events"].is_array() || !config["tasks"].is_array() {
        return None;
    }
    let mut target = None;
    ui.horizontal(|ui| {
        ui.strong(crate::i18n::key("common.scenario.event"));
        ui.label(config["events"].as_array().unwrap().len().to_string());
        if ui
            .small_button("＋")
            .on_hover_text(crate::i18n::key("common.scenario.add_event"))
            .clicked()
        {
            let id = unique(config["events"].as_array().unwrap(), "event");
            config["events"]
                .as_array_mut()
                .unwrap()
                .push(new_event(&id, &script));
            selection.event = Some(id);
            target = Some(false);
        }
    });
    ui.small(crate::i18n::key("common.scenario.arrange_chance"));
    let active = selected(config, false, selection)
        .and_then(|e| e["id"].as_str())
        .map(str::to_owned);
    for e in config["events"].as_array().unwrap() {
        let id = e["id"].as_str().unwrap_or("");
        let mut caption = egui::text::LayoutJob::default();
        caption.wrap.max_width = (ui.available_width() - 24.0).max(40.0);
        caption.wrap.max_rows = 2;
        caption.append(
            &names.event(e),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(14.0),
                color: ui.visuals().text_color(),
                ..Default::default()
            },
        );
        caption.append(
            &format!("\n{}", crate::scenario_quick::event_summary(e)),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(12.0),
                color: ui.visuals().weak_text_color(),
                ..Default::default()
            },
        );
        let caption = ui.fonts_mut(|fonts| fonts.layout_job(caption));
        if ui
            .add_sized(
                [ui.available_width(), 48.0],
                egui::Button::selectable(!task_page && active.as_deref() == Some(id), caption)
                    .right_text(""),
            )
            .clicked()
        {
            selection.event = Some(id.into());
            target = Some(false);
        }
    }
    ui.add_space(12.0);
    egui::CollapsingHeader::new(crate::i18n::key("common.scenario.task_optional")).id_salt("task_navigation").default_open(true).show(ui, |ui| {
        ui.small(crate::i18n::key("common.scenario.multi_event_task"));
        let active = selected(config, true, selection).and_then(|t|t["id"].as_str()).map(str::to_owned);
        for t in config["tasks"].as_array().unwrap() {
            let id = t["id"].as_str().unwrap_or("");
            if ui.selectable_label(task_page && active.as_deref()==Some(id), t["name"].as_str().unwrap_or(&crate::i18n::key("common.scenario.unnamed_task"))).clicked() {
                selection.task = Some(id.into()); target=Some(true);
            }
        }
        if ui.small_button(crate::i18n::key("common.scenario.add_task")).clicked() {
            let id=unique(config["tasks"].as_array().unwrap(),"task");
            config["tasks"].as_array_mut().unwrap().push(json!({"id":id,"name":"新的任务","initial":"locked","states":["locked","active","completed"],"transitions":[]}));
            selection.task=Some(id);target=Some(true);
        }
    });
    target
}

pub fn show(
    ui: &mut egui::Ui,
    project: &mut Project,
    catalog: &Catalog,
    task_page: bool,
    selection: &Selection,
) {
    if !task_page {
        crate::scenario_settings::opening(ui, project);
    }
    let names = crate::scenario_quick::Names::new(&project.stories, project.scenario.as_ref());
    let stories = project.stories.keys().cloned().collect::<Vec<_>>();
    let Some(config) = &mut project.scenario else {
        ui.heading(crate::i18n::key("common.scenario.arrangement"));
        ui.label(crate::i18n::key("common.scenario.old_project"));
        return;
    };
    if !config.is_object() || !config["events"].is_array() || !config["tasks"].is_array() {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            crate::i18n::key("common.scenario.invalid_root"),
        );
        return;
    }
    let states = names
        .states
        .iter()
        .map(|(id, states)| (id.clone(), states.clone()))
        .collect::<Vec<_>>();
    let key = if task_page { "tasks" } else { "events" };
    let id = selected(config, task_page, selection)
        .and_then(|e| e["id"].as_str())
        .unwrap_or("")
        .to_owned();
    let entry = config["entry_event"].as_str().unwrap_or("").to_owned();
    let Some(index) = config[key]
        .as_array()
        .unwrap()
        .iter()
        .position(|e| e["id"] == id)
    else {
        ui.heading(tr(if task_page {
            "任务进度"
        } else {
            "事件安排"
        }));
        ui.label(tr(if task_page {
            "只有多步流程才需要任务。从左侧添加一个任务。"
        } else {
            "从左侧添加事件，再选择时间与要播放的剧情。"
        }));
        return;
    };
    let e = &mut config[key][index];
    let mut remove = false;
    ui.push_id((&id, task_page), |ui| {
        ui.heading(if task_page {
            e["name"]
                .as_str()
                .unwrap_or(&crate::i18n::key("common.scenario.unnamed_task"))
                .to_owned()
        } else {
            names.event(e)
        });
        ui.label(tr(if task_page {
            "任务进度 · 可由多个事件共同推进"
        } else {
            "事件安排 · 决定剧情什么时候发生"
        }));
        ui.add_space(10.0);
        if !e.is_object() {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                crate::i18n::key("common.scenario.invalid_object"),
            );
            return;
        }
        if task_page {
            crate::scenario_quick::task(ui, e, &names);
            ui.add_space(12.0);
            ui.collapsing(tr("高级设置"), |ui| {
                task_advanced(ui, e, &states, catalog, &stories)
            });
        } else {
            bool_field(ui, e, "enabled", "启用这个事件", true);
            let before = e["script"].clone();
            crate::scenario_quick::event(ui, e, &names, catalog, &stories);
            crate::scenario_quick::results(ui, e, &names);
            ui.add_space(12.0);
            ui.collapsing(tr("高级设置"), |ui| {
                event_advanced(
                    ui,
                    e,
                    &states,
                    catalog,
                    &stories,
                    project.manifest["play_mode"] == "sandbox",
                )
            });
            if id == entry && before != e["script"] {
                project.manifest["entry"] = e["script"].clone();
            }
        }
        ui.add_space(20.0);
        ui.separator();
        if id == entry && !task_page {
            ui.small(crate::i18n::key("common.scenario.start_hint"));
        } else if ui
            .small_button(tr(if task_page {
                "删除这个任务"
            } else {
                "删除这个事件"
            }))
            .clicked()
        {
            remove = true;
        }
    });
    if remove {
        config[key].as_array_mut().unwrap().remove(index);
    }
    if let Err(error) = lom_core::scenario::validate(config, &project.manifest, &project.stories) {
        ui.separator();
        crate::error_ui::show_with_actions(ui, &crate::error_ui::capture(error));
    }
}

pub fn context(
    ui: &mut egui::Ui,
    project: &Project,
    task_page: bool,
    selection: &Selection,
) -> Option<Navigate> {
    let config = project.scenario.as_ref()?;
    let e = selected(config, task_page, selection)?;
    let names = crate::scenario_quick::Names::new(&project.stories, Some(config));
    let mut navigate = None;
    if task_page {
        ui.heading(crate::i18n::key("common.scenario.related_events"));
        ui.label(crate::i18n::key("common.scenario.related_check"));
        let id = e["id"].as_str().unwrap_or("");
        let related = config["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| {
                crate::scenario_quick::related_tasks(e)
                    .iter()
                    .any(|key| key == id)
            })
            .collect::<Vec<_>>();
        if related.is_empty() {
            ui.small(crate::i18n::key("common.scenario.related_connect"));
        }
        for event in related {
            if ui.button(names.event(event)).clicked() {
                navigate = Some(Navigate::Event(event["id"].as_str().unwrap_or("").into()));
            }
        }
    } else {
        ui.heading(crate::i18n::key("common.scenario.play_story"));
        let script = e["script"].as_str().unwrap_or("");
        if let Some(story) = project.stories.get(script) {
            ui.strong(story["title"].as_str().unwrap_or(script));
            ui.small(crate::i18n::message(
                "common.scenario.step_count",
                &[(
                    "count",
                    story["nodes"].as_array().map_or(0, Vec::len).to_string(),
                )],
            ));
            if ui
                .button(crate::i18n::key("common.scenario.edit_story"))
                .clicked()
            {
                navigate = Some(Navigate::Story(script.into()));
            }
        } else {
            ui.label(crate::i18n::key("common.scenario.story_missing"));
        }
        ui.add_space(16.0);
        ui.separator();
        ui.strong(crate::i18n::key("common.scenario.related_tasks"));
        let related = crate::scenario_quick::related_tasks(e);
        if related.is_empty() {
            ui.small(crate::i18n::key("common.scenario.independent"));
        }
        for id in related {
            if let Some((_, name)) = names.tasks.iter().find(|(key, _)| key == &id) {
                if ui.button(name).clicked() {
                    navigate = Some(Navigate::Task(id));
                }
            }
        }
    }
    navigate
}

fn text(ui: &mut egui::Ui, v: &mut Value, key: &str, label: &str) {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        let mut value = v[key].as_str().unwrap_or("").to_owned();
        if ui.text_edit_singleline(&mut value).changed() {
            v[key] = json!(value);
        }
    });
}
fn select(
    ui: &mut egui::Ui,
    v: &mut Value,
    key: &str,
    label: &str,
    items: &[(&str, &str)],
    default: &str,
) {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        let mut value = v[key].as_str().unwrap_or(default).to_owned();
        let before = value.clone();
        egui::ComboBox::from_id_salt(key)
            .selected_text(
                items
                    .iter()
                    .find(|(id, _)| *id == value)
                    .map(|(_, name)| tr(name))
                    .unwrap_or_else(|| value.clone()),
            )
            .show_ui(ui, |ui| {
                for (id, name) in items {
                    ui.selectable_value(&mut value, (*id).into(), tr(name));
                }
            });
        if value != before {
            v[key] = json!(value);
        }
    });
}
fn integer(
    ui: &mut egui::Ui,
    v: &mut Value,
    key: &str,
    label: &str,
    default: i64,
    min: i64,
    max: i64,
) {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        let mut value = v[key].as_i64().unwrap_or(default);
        if ui
            .add(egui::DragValue::new(&mut value).range(min..=max))
            .changed()
        {
            v[key] = json!(value);
        }
    });
}
fn bool_field(ui: &mut egui::Ui, v: &mut Value, key: &str, label: &str, default: bool) {
    let mut b = v[key].as_bool().unwrap_or(default);
    if ui.checkbox(&mut b, tr(label)).changed() {
        v[key] = json!(b);
    }
}
fn new_event(id: &str, script: &str) -> Value {
    json!({"id":id,"name":"新的事件","trigger":"position","position":"Center","script":script,"once":true,"check":"stage","priority":0,"chance":100,"weight":1,"when":{"all":[]},"outcomes":{"completed":{"variables":{},"tasks":{}}}})
}
fn unique(items: &[Value], prefix: &str) -> String {
    (1..)
        .map(|i| format!("{prefix}_{i}"))
        .find(|id| !items.iter().any(|v| v["id"] == *id))
        .unwrap()
}
fn event_advanced(
    ui: &mut egui::Ui,
    e: &mut Value,
    task_states: &[(String, Vec<String>)],
    catalog: &Catalog,
    stories: &[String],
    sandbox: bool,
) {
    text(ui, e, "id", "事件 ID");
    text(ui, e, "name", "名称");
    bool_field(ui, e, "enabled", "启用", true);
    let before = e["trigger"].clone();
    let story_entry_label = crate::i18n::key("common.quick.original_trigger");
    let mut kinds = vec![
        ("auto", "自动触发"),
        ("position", "点击地点"),
        ("story_entry", story_entry_label.as_str()),
    ];
    if sandbox {
        kinds.push(("start", "开场"));
    }
    select(ui, e, "trigger", "入口", &kinds, "position");
    if before != e["trigger"] {
        crate::scenario_quick::trigger_changed(e);
    }
    if e["trigger"] == "position" {
        catalog.field(
            ui,
            e,
            "position",
            "位置",
            "free_position",
            false,
            &[],
            stories,
        );
    }
    if e["trigger"] == "story_entry" {
        crate::scenario_quick::story_entry_fields(ui, e, None);
    }
    catalog.field(
        ui,
        e,
        "script",
        "演出章节",
        "story_ref",
        false,
        &[],
        stories,
    );
    bool_field(ui, e, "once", "成功演出后不再重复", true);
    select(
        ui,
        e,
        "check",
        "判定周期",
        &[
            ("month", "每月一次"),
            ("stage", "每旬一次"),
            ("action", "每次行动一次"),
        ],
        "stage",
    );
    ui.small(crate::i18n::key("common.scenario.attempt_used"));
    integer(ui, e, "priority", "优先级", 0, -100000, 100000);
    ui.horizontal(|ui| {
        ui.label(crate::i18n::key("common.scenario.chance"));
        let mut chance = e["chance"].as_f64().unwrap_or(100.0);
        if ui
            .add(
                egui::DragValue::new(&mut chance)
                    .range(0.0..=100.0)
                    .speed(0.5),
            )
            .changed()
        {
            e["chance"] = json!(chance);
        }
    });
    let mut pooled = e.get("pool").is_some();
    if ui
        .checkbox(
            &mut pooled,
            crate::i18n::key("common.scenario.weighted_pool"),
        )
        .changed()
    {
        if pooled {
            e["pool"] = json!("pool_1");
        } else {
            e.as_object_mut().unwrap().remove("pool");
        }
    }
    if pooled {
        text(ui, e, "pool", "事件池 ID");
        integer(ui, e, "weight", "基础权重", 1, 0, 100000);
        ui.small(crate::i18n::key("common.scenario.pool_hint"));
    }
    ui.collapsing(
        crate::i18n::key("common.scenario.trigger_conditions"),
        |ui| condition(ui, &mut e["when"], task_states, catalog, stories, 0),
    );
    ui.collapsing(
        crate::i18n::key("common.scenario.weight_adjustments"),
        |ui| {
            if ui
                .button(crate::i18n::key("common.scenario.add_adjustment"))
                .clicked()
            {
                if !e["weight_modifiers"].is_array() {
                    e["weight_modifiers"] = json!([]);
                }
                e["weight_modifiers"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"when":{"all":[]},"add":0}));
            }
            let mut delete = None;
            for (j, r) in e
                .get_mut("weight_modifiers")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
                .enumerate()
            {
                ui.push_id(j, |ui| {
                    integer(ui, r, "add", "权重增减", 0, -100000, 100000);
                    condition(ui, &mut r["when"], task_states, catalog, stories, 0);
                    if ui
                        .button(crate::i18n::key("common.scenario.remove_adjustment"))
                        .clicked()
                    {
                        delete = Some(j);
                    }
                });
            }
            if let Some(j) = delete {
                e["weight_modifiers"].as_array_mut().unwrap().remove(j);
            }
        },
    );
    ui.collapsing(crate::i18n::key("common.scenario.outcome_tasks"), |ui| {
        outcomes(ui, &mut e["outcomes"], task_states)
    });
}
fn task_advanced(
    ui: &mut egui::Ui,
    t: &mut Value,
    states: &[(String, Vec<String>)],
    catalog: &Catalog,
    stories: &[String],
) {
    text(ui, t, "id", "任务 ID");
    text(ui, t, "name", "名称");
    let choices = crate::scenario_task_states::states(t);
    crate::scenario_quick::task_state(ui, t, "initial", "初始状态", &choices);
    let transition = crate::scenario_task_states::transition(t);
    if ui
        .add_enabled(
            transition.is_some(),
            egui::Button::new(crate::i18n::key("common.scenario.add_condition_transition")),
        )
        .clicked()
    {
        if !t["transitions"].is_array() {
            t["transitions"] = json!([]);
        }
        t["transitions"]
            .as_array_mut()
            .unwrap()
            .push(transition.unwrap());
    }
    let mut delete = None;
    for (j, r) in t
        .get_mut("transitions")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .enumerate()
    {
        ui.push_id(j, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                crate::scenario_quick::task_state(ui, r, "from", "从状态", &choices);
                crate::scenario_quick::task_state(ui, r, "to", "到状态", &choices);
                condition(ui, &mut r["when"], states, catalog, stories, 0);
                if ui
                    .button(crate::i18n::key(
                        "common.scenario.remove_condition_transition",
                    ))
                    .clicked()
                {
                    delete = Some(j);
                }
            });
        });
    }
    if let Some(j) = delete {
        t["transitions"].as_array_mut().unwrap().remove(j);
    }
}

fn condition(
    ui: &mut egui::Ui,
    c: &mut Value,
    tasks: &[(String, Vec<String>)],
    catalog: &Catalog,
    stories: &[String],
    depth: usize,
) {
    if depth > 12 {
        ui.label(crate::i18n::key("common.scenario.depth"));
        return;
    }
    if !c.is_object() {
        ui.label(crate::i18n::key("common.scenario.condition_object"));
        return;
    }
    let kind = if c.get("all").is_some() {
        "all"
    } else if c.get("any").is_some() {
        "any"
    } else if c.get("not").is_some() {
        "not"
    } else {
        "compare"
    };
    if kind == "all" || kind == "any" {
        let mut group = kind;
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("以下条件"));
            egui::ComboBox::from_id_salt("condition-group")
                .selected_text(tr(if kind == "all" {
                    "全部满足（并且）"
                } else {
                    "任一满足（或者）"
                }))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut group, "all", tr("全部满足（并且）"));
                    ui.selectable_value(&mut group, "any", tr("任一满足（或者）"));
                });
        });
        if group != kind {
            let children = c.as_object_mut().unwrap().remove(kind).unwrap();
            c[group] = children;
        }
        let kind = group;
        if c[kind].as_array().is_some_and(Vec::is_empty) {
            ui.weak(tr(if kind == "all" {
                "没有额外条件，按上方入口与时间触发。"
            } else {
                "尚无可满足的条件，请添加至少一项。"
            }));
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(crate::i18n::key("common.scenario.add_condition"))
                .clicked()
            {
                c[kind]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"source":"time","key":"month","op":"eq","value":7}));
            }
            if ui.small_button(tr("添加条件组")).clicked() {
                c[kind]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"any":[{"source":"time","key":"month","op":"eq","value":7}]}));
            }
        });
        let mut delete = None;
        for (i, child) in c[kind].as_array_mut().into_iter().flatten().enumerate() {
            ui.push_id(i, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("{} {}", tr("条件"), i + 1));
                        if ui
                            .small_button(crate::i18n::key("common.scenario.delete_condition"))
                            .clicked()
                        {
                            delete = Some(i);
                        }
                    });
                    condition(ui, child, tasks, catalog, stories, depth + 1);
                });
            });
        }
        if let Some(i) = delete {
            c[kind].as_array_mut().unwrap().remove(i);
        }
    } else if kind == "not" {
        ui.label(tr("以下条件不满足时通过"));
        ui.push_id("not", |ui| {
            condition(ui, &mut c["not"], tasks, catalog, stories, depth + 1)
        });
    } else {
        let before = c["source"].clone();
        select(
            ui,
            c,
            "source",
            "判断项目",
            &[
                ("time", "时间"),
                ("stat", "角色数值"),
                ("affinity", "人物好感"),
                ("flag", "剧情记录"),
                ("game_flag", "原版数值旗标"),
                ("variable", "事件变量"),
                ("task", "任务状态"),
            ],
            "time",
        );
        if before != c["source"] {
            c["key"] = json!(if c["source"] == "time" { "month" } else { "" });
            c["value"] = if c["source"] == "task" {
                json!("")
            } else {
                json!(0)
            };
        }
        match c["source"].as_str().unwrap_or("") {
            "time" => select(
                ui,
                c,
                "key",
                "时间项",
                &[
                    ("year", "年"),
                    ("month", "月"),
                    ("stage", "旬"),
                    ("action", "剩余行动次数"),
                ],
                "month",
            ),
            "affinity" => {
                catalog.field(
                    ui,
                    c,
                    "key",
                    "人物",
                    "affinity_character",
                    false,
                    &[],
                    stories,
                );
            }
            "stat" => {
                catalog.field(ui, c, "key", "属性", "stat", false, &[], stories);
            }
            "game_flag" => {
                catalog.field(ui, c, "key", "原版旗标", "game_flag", false, &[], stories);
            }
            "flag" => text(ui, c, "key", "剧情记录名称"),
            "variable" => text(ui, c, "key", "事件变量名称"),
            "task" => {
                let values: Vec<_> = tasks
                    .iter()
                    .map(|(id, _)| (id.as_str(), id.as_str()))
                    .collect();
                select(ui, c, "key", "任务", &values, "");
            }
            _ => text(ui, c, "key", "属性或任务 ID"),
        }
        select(
            ui,
            c,
            "op",
            "比较",
            &[
                ("eq", "等于"),
                ("ne", "不等于"),
                ("lt", "小于"),
                ("le", "小于等于"),
                ("gt", "大于"),
                ("ge", "大于等于"),
            ],
            "eq",
        );
        if c["source"] == "task" {
            let key = c["key"].as_str().unwrap_or("");
            let options = tasks
                .iter()
                .find(|(id, _)| id == key)
                .map(|(_, states)| states.clone())
                .unwrap_or_default();
            crate::scenario_quick::task_state(ui, c, "value", "状态", &options);
        } else {
            integer(ui, c, "value", "数值", 0, i32::MIN as i64, i32::MAX as i64);
        }
        if c["source"] == "game_flag" {
            ui.weak(tr("填写旗标状态数值；原版旗标沿用游戏判定，不是开关。"));
        }
        ui.menu_button(tr("组合条件…"), |ui| {
            for (key, label) in [
                ("all", "与其他条件同时满足"),
                ("any", "与其他条件满足其一"),
                ("not", "反转此条件"),
            ] {
                if ui.button(tr(label)).clicked() {
                    *c = if key == "not" {
                        json!({"not":c.clone()})
                    } else {
                        json!({key:[c.clone()]})
                    };
                    ui.close();
                }
            }
        });
    }
}
fn outcomes(ui: &mut egui::Ui, v: &mut Value, tasks: &[(String, Vec<String>)]) {
    if !v.is_object() {
        ui.label(crate::i18n::key("common.scenario.outcomes_object"));
        return;
    }
    if ui
        .button(crate::i18n::key("common.scenario.add_outcome"))
        .clicked()
    {
        let id = (1..)
            .map(|i| format!("result_{i}"))
            .find(|id| v.get(id).is_none())
            .unwrap();
        v[&id] = json!({"variables":{},"tasks":{}});
    }
    let results = v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    for result in results {
        ui.push_id(&result, |ui| {
            ui.collapsing(&result, |ui| {
                if result != "completed" {
                    let mut renamed = result.clone();
                    if ui.text_edit_singleline(&mut renamed).changed()
                        && !renamed.is_empty()
                        && renamed != result
                        && v.get(&renamed).is_none()
                    {
                        let value = v.as_object_mut().unwrap().remove(&result).unwrap();
                        v[&renamed] = value;
                        return;
                    }
                }
                let effects = &mut v[&result];
                if !effects.is_object() {
                    ui.label(crate::i18n::key("common.scenario.result_object"));
                    return;
                }
                ui.collapsing(crate::i18n::key("common.scenario.misc_effects"), |ui| {
                    result_actions(ui, effects)
                });
                if ui
                    .button(crate::i18n::key("common.scenario.add_variable_effect"))
                    .clicked()
                {
                    if !effects["variables"].is_object() {
                        effects["variables"] = json!({});
                    }
                    let id = (1..)
                        .map(|i| format!("var_{i}"))
                        .find(|id| effects["variables"].get(id).is_none())
                        .unwrap();
                    effects["variables"][&id] = json!({"op":"set","value":1});
                }
                let vars = effects["variables"]
                    .as_object()
                    .map(|m| m.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                for key in vars {
                    ui.push_id(&key, |ui| {
                        let mut renamed = key.clone();
                        if ui.text_edit_singleline(&mut renamed).changed()
                            && !renamed.is_empty()
                            && renamed != key
                            && effects["variables"].get(&renamed).is_none()
                        {
                            let value = effects["variables"]
                                .as_object_mut()
                                .unwrap()
                                .remove(&key)
                                .unwrap();
                            effects["variables"][&renamed] = value;
                            return;
                        }
                        select(
                            ui,
                            &mut effects["variables"][&key],
                            "op",
                            "操作",
                            &[("set", "设置"), ("add", "增加")],
                            "set",
                        );
                        integer(
                            ui,
                            &mut effects["variables"][&key],
                            "value",
                            "值",
                            1,
                            i32::MIN as i64,
                            i32::MAX as i64,
                        );
                        if ui
                            .small_button(crate::i18n::key(
                                "common.scenario.delete_variable_effect",
                            ))
                            .clicked()
                        {
                            effects["variables"].as_object_mut().unwrap().remove(&key);
                        }
                    });
                }
                if !tasks.is_empty()
                    && ui
                        .button(crate::i18n::key("common.scenario.add_task_effect"))
                        .clicked()
                {
                    if !effects["tasks"].is_object() {
                        effects["tasks"] = json!({});
                    }
                    let (id, states) = &tasks[0];
                    effects["tasks"][id] = json!(states.last().cloned().unwrap_or_default());
                }
                let updates = effects["tasks"]
                    .as_object()
                    .map(|m| m.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                for key in updates {
                    ui.push_id(&key, |ui| {
                        let mut id = key.clone();
                        egui::ComboBox::from_id_salt("task")
                            .selected_text(&id)
                            .show_ui(ui, |ui| {
                                for (candidate, _) in tasks {
                                    ui.selectable_value(&mut id, candidate.clone(), candidate);
                                }
                            });
                        if id != key && effects["tasks"].get(&id).is_none() {
                            let value = effects["tasks"]
                                .as_object_mut()
                                .unwrap()
                                .remove(&key)
                                .unwrap();
                            effects["tasks"][&id] = value;
                            return;
                        }
                        let states = tasks
                            .iter()
                            .find(|(id, _)| id == &key)
                            .map(|(_, states)| states.clone())
                            .unwrap_or_default();
                        crate::scenario_quick::task_state(
                            ui,
                            &mut effects["tasks"],
                            &key,
                            "推进到状态",
                            &states,
                        );
                        if ui
                            .small_button(crate::i18n::key("common.scenario.delete_task_effect"))
                            .clicked()
                        {
                            effects["tasks"].as_object_mut().unwrap().remove(&key);
                        }
                    });
                }
                if result != "completed"
                    && ui
                        .small_button(crate::i18n::key("common.scenario.delete_outcome"))
                        .clicked()
                {
                    v.as_object_mut().unwrap().remove(&result);
                }
            });
        });
    }
}
fn result_actions(ui: &mut egui::Ui, effects: &mut Value) {
    if ui
        .button(crate::i18n::key("common.scenario.add_effect"))
        .clicked()
    {
        if !effects["actions"].is_array() {
            effects["actions"] = json!([]);
        }
        effects["actions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"stat","key":"money","op":"add","value":10}));
    }
    let mut delete = None;
    for (i, a) in effects
        .get_mut("actions")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .enumerate()
    {
        ui.push_id(i, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let before = a["type"].clone();
                select(
                    ui,
                    a,
                    "type",
                    "动作",
                    &[
                        ("stat", "角色数值"),
                        ("affinity", "人物好感"),
                        ("item", "物品"),
                        ("talent", "技能"),
                        ("flag", "记录剧情标记"),
                        ("game_flag", "原版数值旗标"),
                        ("time", "设置游戏时间"),
                    ],
                    "stat",
                );
                if before != a["type"] {
                    let kind = a["type"].clone();
                    *a = match kind.as_str().unwrap_or("") {
                        "flag" => json!({"type":kind,"key":"event_seen"}),
                        "time" => json!({"type":kind,"year":1,"month":4,"stage":1}),
                        "talent" => json!({"type":kind,"key":"","value":1}),
                        "item" => json!({"type":kind,"key":"","kind":"book","op":"add","value":1}),
                        _ => json!({"type":kind,"key":"","op":"add","value":1}),
                    };
                }
                let kind = a["type"].as_str().unwrap_or("").to_owned();
                if kind == "time" {
                    integer(ui, a, "year", "年", 1, 1, 9);
                    integer(ui, a, "month", "月", 4, 1, 12);
                    integer(ui, a, "stage", "旬", 1, 1, 3);
                } else {
                    text(ui, a, "key", "属性、人物或物品 ID");
                    if kind == "item" {
                        select(
                            ui,
                            a,
                            "kind",
                            "物品类别",
                            &[
                                ("book", "书籍"),
                                ("misc", "杂物"),
                                ("special", "贵重品"),
                                ("consumable", "消耗品"),
                            ],
                            "book",
                        );
                    }
                    if kind != "flag" && kind != "talent" {
                        let ops = if kind == "item" {
                            vec![("add", "增加"), ("remove", "减少")]
                        } else {
                            vec![("add", "增加"), ("set", "设置")]
                        };
                        select(ui, a, "op", "操作", &ops, "add");
                    }
                    if kind != "flag" {
                        integer(
                            ui,
                            a,
                            "value",
                            "数值",
                            1,
                            if kind == "item" { 1 } else { i32::MIN as i64 },
                            i32::MAX as i64,
                        );
                    }
                }
                if ui
                    .small_button(crate::i18n::key("common.scenario.delete_effect"))
                    .clicked()
                {
                    delete = Some(i);
                }
            });
        });
    }
    if let Some(i) = delete {
        effects["actions"].as_array_mut().unwrap().remove(i);
    }
}

#[cfg(test)]
mod windows_merge_tests {
    use super::*;

    fn condition_frame(
        ctx: &egui::Context,
        catalog: &Catalog,
        when: &mut Value,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.enable_accesskit();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 2400.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let names = crate::scenario_quick::Names::new(&Default::default(), None);
                    crate::scenario_quick::dates(ui, when, false, &names);
                    ui.push_id("advanced", |ui| condition(ui, when, &[], catalog, &[], 0));
                });
            },
        )
    }

    fn condition_click(
        output: &egui::FullOutput,
        label: &str,
        role: egui::accesskit::Role,
        occurrence: usize,
    ) -> Vec<egui::Event> {
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let mut matches: Vec<_> = nodes
            .iter()
            .filter(|(_, node)| {
                node.role() == role && (node.label() == Some(label) || node.value() == Some(label))
            })
            .collect();
        matches.sort_by(|(_, left), (_, right)| {
            left.bounds()
                .map(|r| r.y0)
                .unwrap_or(f64::INFINITY)
                .total_cmp(&right.bounds().map(|r| r.y0).unwrap_or(f64::INFINITY))
        });
        let (_, target) = matches
            .into_iter()
            .nth(occurrence)
            .unwrap_or_else(|| panic!("Missing {role:?} {label}"));
        let bounds = target.bounds().unwrap();
        let pos = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        );
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![egui::Event::PointerMoved(pos), button(true), button(false)]
    }

    fn select_condition_option(
        ctx: &egui::Context,
        catalog: &Catalog,
        when: &mut Value,
        selected: &str,
        option: &str,
        occurrence: usize,
    ) {
        use egui::accesskit::Role;
        let output = condition_frame(ctx, catalog, when, vec![]);
        let events = condition_click(&output, &tr(selected), Role::ComboBox, occurrence);
        condition_frame(ctx, catalog, when, events);
        let output = condition_frame(ctx, catalog, when, vec![]);
        let events = condition_click(&output, &tr(option), Role::Button, 0);
        condition_frame(ctx, catalog, when, events);
        condition_frame(ctx, catalog, when, vec![]);
    }

    #[test]
    fn switching_condition_groups_keeps_every_nested_date_and_sibling_after_quick_form_refresh() {
        let catalog = Catalog::new();
        let ctx = egui::Context::default();
        let children = json!([
            {"all":[
                {"source":"time","key":"year","op":"ge","value":2},
                {"source":"time","key":"month","op":"eq","value":7},
                {"source":"time","key":"stage","op":"le","value":2}]},
            {"source":"stat","key":"money","op":"ge","value":100},
            {"source":"game_flag","key":"original_flag","op":"eq","value":3}
        ]);
        let mut when = json!({"all":children});
        select_condition_option(
            &ctx,
            &catalog,
            &mut when,
            "全部满足（并且）",
            "任一满足（或者）",
            0,
        );
        assert_eq!(when, json!({"any":children}));
        select_condition_option(
            &ctx,
            &catalog,
            &mut when,
            "任一满足（或者）",
            "全部满足（并且）",
            0,
        );
        assert_eq!(when, json!({"all":children}));
        select_condition_option(
            &ctx,
            &catalog,
            &mut when,
            "全部满足（并且）",
            "任一满足（或者）",
            1,
        );
        assert_eq!(when["all"][0]["any"], children[0]["all"]);
        assert_eq!(when["all"][1], children[1]);
        assert_eq!(when["all"][2], children[2]);
    }

    #[test]
    fn wrapping_a_time_comparison_preserves_it_and_every_outer_condition() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        for (option, kind) in [
            ("与其他条件同时满足", "all"),
            ("与其他条件满足其一", "any"),
            ("反转此条件", "not"),
        ] {
            let ctx = egui::Context::default();
            let children = json!([
                {"source":"time","key":"year","op":"eq","value":2},
                {"source":"time","key":"month","op":"eq","value":7},
                {"source":"time","key":"stage","op":"eq","value":3},
                {"source":"stat","key":"money","op":"ge","value":100}
            ]);
            let mut when = json!({"all":children});
            let output = condition_frame(&ctx, &catalog, &mut when, vec![]);
            let events = condition_click(&output, &tr("组合条件…"), Role::Button, 1);
            condition_frame(&ctx, &catalog, &mut when, events);
            let output = condition_frame(&ctx, &catalog, &mut when, vec![]);
            let events = condition_click(&output, &tr(option), Role::Button, 0);
            condition_frame(&ctx, &catalog, &mut when, events);
            condition_frame(&ctx, &catalog, &mut when, vec![]);
            let mut expected = children.clone();
            expected[1] = if kind == "not" {
                json!({"not":children[1]})
            } else {
                json!({kind:[children[1]]})
            };
            assert_eq!(when, json!({"all":expected}), "{option}");
        }
    }

    #[test]
    fn released_date_conditions_survive_group_clicks_save_export_and_reopen() {
        use lom_core::{package, project::Project};
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lom-core/tests/fixtures/released-1.5.0-scenario-dates.lommod");
        let original = std::fs::read(&fixture).unwrap();
        let mut project = Project::open(&fixture).unwrap();
        let before = project.scenario.as_ref().unwrap()["events"][0]["when"].clone();
        let ctx = egui::Context::default();
        let catalog = Catalog::new();
        let when = &mut project.scenario.as_mut().unwrap()["events"][0]["when"];
        select_condition_option(
            &ctx,
            &catalog,
            when,
            "全部满足（并且）",
            "任一满足（或者）",
            0,
        );
        assert_eq!(when["any"], before["all"]);
        select_condition_option(
            &ctx,
            &catalog,
            when,
            "任一满足（或者）",
            "全部满足（并且）",
            0,
        );
        assert_eq!(*when, before);
        let dir = tempfile::tempdir().unwrap();
        project.save_to(&dir.path().join("edited")).unwrap();
        let edited = Project::open(&dir.path().join("edited")).unwrap();
        let output = dir.path().join("edited.lommod");
        edited.export(&output).unwrap();
        assert_eq!(Project::open(&output).unwrap().scenario, edited.scenario);
        assert_eq!(std::fs::read(&fixture).unwrap(), original);
        let mut entries = package::read_package(&output).unwrap();
        entries.remove("package-content.sha256");
        entries
            .get_mut("lua/main.lua")
            .unwrap()
            .extend_from_slice(b"\nerror('unrelated Lua')\n");
        let tampered = dir.path().join("tampered.lommod");
        package::write_package(entries, &tampered).unwrap();
        assert!(Project::open(&tampered).is_err());
    }

    #[test]
    fn browsing_optional_event_and_task_controls_preserves_source() {
        for mode in ["addon", "sandbox"] {
            let mut project = Project::new_scenario(mode);
            let config = project.scenario.as_mut().unwrap();
            config["tasks"] = json!([{
                "id":"delivery", "name":"送信任务", "initial":"locked",
                "states":["locked","active","completed"]
            }]);
            config["events"][0]["outcomes"]["completed"] = json!({});
            let before = project.scenario.clone();
            lom_core::scenario::validate(
                project.scenario.as_ref().unwrap(),
                &project.manifest,
                &project.stories,
            )
            .unwrap();
            let catalog = Catalog::new();
            for task_page in [false, true] {
                let ctx = egui::Context::default();
                ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
                let _ = ctx.run(Default::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show(ui, &mut project, &catalog, task_page, &Selection::default());
                    });
                });
                assert_eq!(project.scenario, before, "mode={mode}, task={task_page}");
                lom_core::scenario::validate(
                    project.scenario.as_ref().unwrap(),
                    &project.manifest,
                    &project.stories,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn quick_forms_fit_narrow_columns_with_windows_button_spacing() {
        let catalog = Catalog::new();
        for width in [240.0, 280.0, 360.0] {
            for task_page in [false, true] {
                let mut project = Project::new_scenario("addon");
                project.stories.get_mut("main").unwrap()["title"] =
                    json!("七月来信与送信任务的很长中文演出章节名称");
                project.scenario.as_mut().unwrap()["tasks"] = json!([{
                    "id":"delivery", "name":"跨事件送信任务的中文名称", "initial":"locked",
                    "states":["locked","active","completed"]
                }]);
                project.scenario.as_mut().unwrap()["events"][0]["when"] = json!({"all":[
                    {"source":"task","key":"delivery","op":"eq","value":"active"}
                ]});
                let names =
                    crate::scenario_quick::Names::new(&project.stories, project.scenario.as_ref());
                let ids = project.stories.keys().cloned().collect::<Vec<_>>();
                let ctx = egui::Context::default();
                crate::shell::style(&ctx);
                ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 1400.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let limit = ui.max_rect().right();
                            let config = project.scenario.as_mut().unwrap();
                            if task_page {
                                crate::scenario_quick::task(ui, &mut config["tasks"][0], &names);
                            } else {
                                crate::scenario_quick::event(
                                    ui,
                                    &mut config["events"][0],
                                    &names,
                                    &catalog,
                                    &ids,
                                );
                            }
                            assert!(
                                ui.min_rect().right() <= limit + 1.0,
                                "width={width}, task={task_page}, actual={:?}",
                                ui.min_rect()
                            );
                        });
                    },
                );
            }
        }
    }
}
