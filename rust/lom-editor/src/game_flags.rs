//! Presentation metadata only. Projects keep the original flag IDs and numbers.
use crate::{forms, i18n};
use eframe::egui::{self, Ui};
use serde_json::{json, Value};

#[derive(Clone, Default)]
pub(crate) struct ReferenceRequest {
    pub script: String,
    pub flag: String,
    pub scripts: Vec<String>,
}

pub(crate) fn take_reference(ctx: &egui::Context) -> Option<ReferenceRequest> {
    ctx.data_mut(|data| data.remove_temp(egui::Id::new("flag-story-reference")))
}

pub(crate) fn row<'a>(data: &'a Value, id: &str) -> Option<&'a Value> {
    data["game_flags"]
        .as_array()?
        .iter()
        .find(|row| row["id"] == id)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

pub(crate) fn prose(value: &str) -> String {
    if i18n::locale() == "chs" {
        i18n::simplified_label(value)
    } else {
        value.into()
    }
}

pub(crate) fn label(row: &Value) -> String {
    if text(row, "name").is_empty() || row["name"] == row["id"] {
        i18n::key("game_flags.unknown")
    } else {
        i18n::term("game_flags", text(row, "id"), text(row, "name"))
    }
}

pub(crate) fn matches(row: &Value, query: &str) -> bool {
    let normalize = |text: &str| i18n::simplified_label(text).to_lowercase();
    let haystack = normalize(&format!(
        "{} {} {}",
        text(row, "id"),
        label(row),
        text(row, "description")
    ));
    normalize(query)
        .split_whitespace()
        .all(|term| haystack.contains(term))
}

pub(crate) fn original_chapter_matches(id: &str, title: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    static REFERENCES: std::sync::OnceLock<std::collections::BTreeMap<String, String>> =
        std::sync::OnceLock::new();
    let references = REFERENCES.get_or_init(|| {
        let mut entries = std::collections::BTreeMap::<String, String>::new();
        for row in lom_core::validate::editor_data()["game_flags"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let note = i18n::simplified_label(&format!(
                "{} {} {}",
                text(row, "id"),
                text(row, "name"),
                text(row, "description")
            ))
            .to_lowercase();
            for id in row["scripts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let terms = entries.entry(id.to_ascii_lowercase()).or_default();
                terms.push(' ');
                terms.push_str(&note);
            }
        }
        entries
    });
    let id = id.to_ascii_lowercase();
    let title = i18n::simplified_label(title).to_lowercase();
    let notes = references.get(&id).map(String::as_str).unwrap_or("");
    query
        .split_whitespace()
        .all(|term| id.contains(term) || title.contains(term) || notes.contains(term))
}

fn advanced(row: &Value) -> bool {
    row["temporary"] == true || text(row, "name").is_empty() || row["name"] == row["id"]
}

pub(crate) fn node_flag(node: &Value) -> Option<&str> {
    if node["type"] == "game_flag" || node["source"] == "flag_value" {
        Some(text(node, "flag"))
    } else if node["source"] == "game_flag" {
        Some(text(node, "key"))
    } else {
        None
    }
}

pub(crate) fn step_uses(node: &Value) -> Vec<(&str, &'static str)> {
    fn collect<'a>(value: &'a Value, uses: &mut Vec<(&'a str, &'static str)>) {
        let argument = &value["args"][0];
        let usage = if let Some(id) = node_flag(value) {
            let kind = match (text(value, "type"), text(value, "op")) {
                ("game_flag", "add") => "game_flags.context_add",
                ("game_flag", _) => "game_flags.context_set",
                _ => "game_flags.context_read",
            };
            Some((id, kind))
        } else if value["kind"] == "call" && argument["kind"] == "literal" {
            let kind = match crate::native_forms::call_name(value).as_str() {
                "statmodifymanager.SetFlag" => Some("game_flags.context_set"),
                "statmodifymanager.AddFlag" => Some("game_flags.context_add"),
                "luamanager.GetFlagData" => Some("game_flags.context_read"),
                _ => None,
            };
            argument["value"].as_str().zip(kind)
        } else {
            None
        };
        if let Some(usage) = usage.filter(|(id, _)| !id.is_empty()) {
            if !uses.contains(&usage) {
                uses.push(usage);
            }
        }
        match value {
            Value::Array(items) => items.iter().for_each(|item| collect(item, uses)),
            Value::Object(fields) => fields.values().for_each(|item| collect(item, uses)),
            _ => {}
        }
    }
    let mut uses = Vec::new();
    collect(node, &mut uses);
    uses
}

pub(crate) fn find_steps<'a>(
    data: &'a Value,
    nodes: &'a [Value],
    query: &str,
) -> Vec<(usize, &'a str, &'static str, Option<&'a Value>)> {
    let mut flags = std::collections::BTreeMap::new();
    let mut hits = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        for (id, usage) in step_uses(node) {
            let (metadata, matched) = *flags.entry(id).or_insert_with(|| {
                let metadata = row(data, id);
                (
                    metadata,
                    metadata.map_or_else(
                        || id.to_lowercase().contains(query),
                        |row| matches(row, query),
                    ),
                )
            });
            if matched {
                hits.push((index, id, usage, metadata));
            }
        }
    }
    hits
}

#[test]
fn find_steps_locates_structured_flag_reads_and_writes_by_purpose() {
    let story = lom_core::vanilla_import::import(
        "statmodifymanager.SetFlag('M20008', 1)\nif luamanager.GetFlagData('M20008') == 1 then\nsay('test')\nend\ncheckpointmanager.Switch('M20008')",
        "main",
        "test",
        "DEMO",
        &Default::default(),
    )
    .unwrap();
    let data = json!({"game_flags":[{"id":"M20008","name":"路线开关","description":"试验标记"}]});
    let nodes = story["nodes"].as_array().unwrap();
    let hits = find_steps(&data, nodes, "试验标记");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].2, "game_flags.context_set");
    assert_eq!(hits[1].2, "game_flags.context_read");
    assert_eq!(hits, find_steps(&data, nodes, "m20008"));
}

pub(crate) fn field(
    ui: &mut Ui,
    data: &Value,
    node: &mut Value,
    key: &str,
    title: &str,
    optional: bool,
) -> bool {
    let before = node.get(key).cloned();
    let mut id = text(node, key).to_owned();
    let mut edited = false;
    let mut clear = false;
    let response = ui.push_id(key, |ui| {
        ui.label(if optional {
            i18n::tr(title)
        } else {
            format!("{} *", i18n::tr(title))
        });
        let display = row(data, &id).map(label).unwrap_or_else(|| {
            if id.is_empty() {
                i18n::key("editor.form.choose")
            } else {
                i18n::key("game_flags.manual")
            }
        });
        forms::resource_combo(ui, "flag-selector", &display, |ui| {
            let search_id = ui.id().with("flag-search");
            let mut search =
                ui.data_mut(|data| data.get_temp::<String>(search_id).unwrap_or_default());
            ui.add(
                egui::TextEdit::singleline(&mut search)
                    .desired_width(f32::INFINITY)
                    .hint_text(i18n::key("game_flags.search")),
            );
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if optional
                        && ui
                            .selectable_label(before.is_none(), i18n::key("game_flags.clear"))
                            .clicked()
                    {
                        clear = true;
                        ui.close();
                    }
                    let visible: Vec<_> = data["game_flags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|row| matches(row, &search))
                        .collect();
                    let mut choose = |ui: &mut Ui, row: &Value| {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(id == text(row, "id"), label(row))
                                .on_hover_text(prose(text(row, "description")))
                                .clicked()
                            {
                                id = text(row, "id").into();
                                edited = true;
                                ui.close();
                            }
                            ui.weak(text(row, "id"));
                        });
                    };
                    for row in visible.iter().filter(|row| !advanced(row)) {
                        choose(ui, row);
                    }
                    if visible.iter().any(|row| advanced(row)) {
                        egui::CollapsingHeader::new(i18n::key("game_flags.advanced"))
                            .open((!search.trim().is_empty()).then_some(true))
                            .show(ui, |ui| {
                                for row in visible.iter().filter(|row| advanced(row)) {
                                    choose(ui, row)
                                }
                            });
                    }
                    ui.collapsing(i18n::key("game_flags.manual"), |ui| {
                        let manual_id = ui.id().with("manual-id");
                        let mut manual = ui.data_mut(|data| {
                            data.get_temp::<String>(manual_id)
                                .unwrap_or_else(|| id.clone())
                        });
                        ui.add(
                            egui::TextEdit::singleline(&mut manual)
                                .hint_text(i18n::key("game_flags.code")),
                        );
                        if ui.button(i18n::key("game_flags.apply_id")).clicked() {
                            id = manual.clone();
                            edited = true;
                            ui.close();
                        }
                        ui.data_mut(|data| data.insert_temp(manual_id, manual));
                    });
                });
            ui.data_mut(|data| data.insert_temp(search_id, search));
        });
        if !id.is_empty() {
            ui.horizontal(|ui| {
                ui.weak(&id);
                if ui.small_button(i18n::key("game_flags.copy_id")).clicked() {
                    ui.ctx().copy_text(id.clone());
                }
            });
            details(ui, data, &id);
            if initial_default_condition(data, node, &id) {
                ui.small(i18n::key("game_flags.initial_condition_hint"));
            }
        }
        forms::unavailable_game_flag_warning(ui, &id);
    });
    if ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("problem-field")))
        .as_deref()
        == Some(key)
    {
        response.response.scroll_to_me(Some(egui::Align::Center));
        ui.painter().rect_stroke(
            response.response.rect.expand(3.0),
            3.0,
            egui::Stroke::new(2.0_f32, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Outside,
        );
        ui.ctx()
            .data_mut(|data| data.remove::<String>(egui::Id::new("problem-field")));
    }
    if clear {
        node.as_object_mut().unwrap().remove(key);
    } else if edited {
        node[key] = json!(id);
    }
    node.get(key) != before.as_ref()
}

fn initial_default_condition(data: &Value, node: &Value, id: &str) -> bool {
    node["type"] != "game_flag"
        && node["source"] == "game_flag"
        && node["op"] == "eq"
        && row(data, id)
            .and_then(|row| row["default"].as_f64())
            .zip(node["value"].as_f64())
            .is_some_and(|(default, value)| default == value)
}

fn details(ui: &mut Ui, data: &Value, id: &str) {
    let Some(row) = row(data, id) else {
        ui.weak(i18n::key("game_flags.unknown_detail"));
        return;
    };
    if row["temporary"] == true {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            i18n::key("game_flags.temporary"),
        );
    }
    let referenced_use = row["description_source"] == "event_database";
    if !text(row, "description").is_empty() {
        ui.small(prose(text(row, "description")));
        if !referenced_use {
            ui.weak(i18n::key(if row["description_source"] == "story" {
                "game_flags.story_source"
            } else {
                "game_flags.note_source"
            }));
        }
    } else {
        ui.weak(i18n::key("game_flags.unknown_detail"));
    }
    if referenced_use || row["values_source"] == "event_database" {
        ui.hyperlink_to(
            i18n::key("game_flags.event_database"),
            format!("https://mortal.eraser.moe/zh-cn/flag/{id}"),
        );
    }
    if let Some(scripts) = row["scripts"]
        .as_array()
        .filter(|scripts| !scripts.is_empty())
    {
        let scripts: Vec<_> = scripts
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let ctx = ui.ctx().clone();
        let open = |script: &str| {
            ctx.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("flag-story-reference"),
                    ReferenceRequest {
                        script: script.into(),
                        flag: id.into(),
                        scripts: scripts.clone(),
                    },
                )
            });
        };
        if let Some(first) = scripts.first() {
            if ui.button(i18n::key("game_flags.view_context")).clicked() {
                open(first);
            }
        }
        egui::CollapsingHeader::new(i18n::message(
            "game_flags.references",
            &[("count", scripts.len().to_string())],
        ))
        .show(ui, |ui| {
            ui.weak(i18n::key("game_flags.reference_hint"));
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for script in &scripts {
                        if ui.link(script).clicked() {
                            open(script);
                        }
                    }
                });
        });
    }
}

/// Suggestions are only for absolute states/thresholds. An increment stays a
/// number, and unlisted historical values always remain directly editable.
pub(crate) fn number(
    ui: &mut Ui,
    data: &Value,
    flag: &str,
    value: &mut Value,
    absolute: bool,
) -> bool {
    let current = value
        .as_i64()
        .unwrap_or_else(|| value.as_f64().unwrap_or(0.0) as i64);
    let mut number = current;
    let mut edited = false;
    let metadata = row(data, flag);
    let choices = metadata
        .and_then(|row| row["values"].as_array())
        .filter(|items| !items.is_empty());
    if let Some(choices) = choices.filter(|_| absolute) {
        let title = |item: &Value| format!("{}（{}）", prose(text(item, "name")), item["value"]);
        let display = choices
            .iter()
            .find(|item| item["value"].as_i64() == Some(current))
            .map(title)
            .unwrap_or_else(|| {
                i18n::message("game_flags.current_value", &[("value", value.to_string())])
            });
        forms::resource_combo(ui, "flag-value", &display, |ui| {
            forms::search_menu(ui, |ui, query| {
                for choice in choices {
                    if !forms::resource_matches(&choice["value"].to_string(), &title(choice), query)
                    {
                        continue;
                    }
                    let selected = choice["value"].as_i64().expect("flag value suggestion");
                    if ui
                        .selectable_label(current == selected, title(choice))
                        .clicked()
                    {
                        number = selected;
                        edited = true;
                        ui.close();
                    }
                }
                if let Some(default) =
                    metadata
                        .and_then(|row| row["default"].as_i64())
                        .filter(|default| {
                            !choices
                                .iter()
                                .any(|item| item["value"].as_i64() == Some(*default))
                        })
                {
                    if ui
                        .selectable_label(
                            current == default,
                            i18n::message(
                                "game_flags.initial_value",
                                &[("value", default.to_string())],
                            ),
                        )
                        .clicked()
                    {
                        number = default;
                        edited = true;
                        ui.close();
                    }
                }
            })
        });
    }
    edited |= ui
        .add(
            egui::DragValue::new(&mut number)
                .speed(1)
                .range(i32::MIN as i64..=i32::MAX as i64)
                .clamp_existing_to_range(false),
        )
        .changed();
    if edited {
        *value = json!(number);
    }
    edited
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        mut show: impl FnMut(&mut Ui),
    ) -> egui::FullOutput {
        ctx.enable_accesskit();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui));
            },
        )
    }
    fn click(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        title: &str,
    ) -> Vec<egui::Event> {
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let target = nodes
            .iter()
            .find_map(|(id, node)| {
                (node.role() == role
                    && (node.label() == Some(title) || node.value() == Some(title)))
                .then_some(*id)
            })
            .unwrap_or_else(|| panic!("missing {role:?} {title}"));
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target,
                data: None,
            },
        )]
    }

    #[test]
    fn game_flag_names_and_search_use_purpose_without_changing_ids() {
        i18n::set_locale("chs");
        let data = &forms::Catalog::new().data;
        let kongtong = row(data, "M20011").unwrap();
        assert_eq!(label(kongtong), "崆峒所选门派");
        assert!(matches(kongtong, "铁拳门"));
        assert!(matches(kongtong, "崆峒 M20011"));
        assert!(matches(kongtong, "崆峒 鐵拳門"));
        assert!(!matches(kongtong, "青城"));
        assert!(matches(row(data, "Back_02_501_01").unwrap(), "后山 龙湘"));
        assert_eq!(kongtong["id"], "M20011");
        assert!(advanced(row(data, "A00001").unwrap()));
        assert!(!advanced(kongtong));

        let meeting = row(data, "Meeting_Option_101_01").unwrap();
        assert_eq!(label(meeting), "已开发杀人扑满");
        assert!(matches(meeting, "杀人机关扑满"));
        assert!(matches(meeting, "殺人撲滿 meeting_option_101_01"));
        assert!(!advanced(meeting));
        assert_eq!(meeting["default"], 0);
        assert_eq!(
            meeting["values"],
            json!([{"value":0,"name":"否"},{"value":1,"name":"是"}])
        );

        let facility = row(data, "Facility_001").unwrap();
        assert_eq!(label(facility), "外堡等级");
        assert!(facility["values"].is_null());
        assert!(!advanced(facility));

        let gate = row(data, "P_Alchemy_01").unwrap();
        assert!(matches(gate, "炼丹房 p_alchemy_01"));
        assert_eq!(
            gate["values"],
            json!([{"value":0,"name":"開放"},{"value":1,"name":"關閉"}])
        );

        let reused = row(data, "A10001").unwrap();
        assert!(matches(reused, "江陵 对练"));
        assert!(advanced(reused));
        assert!(reused["values"].is_null());
    }

    #[test]
    fn initial_default_hint_only_describes_equal_numeric_game_flag_conditions_without_edits() {
        i18n::set_locale("chs");
        let data = json!({"game_flags":[
            {"id":"M30077","name":"小竹參戰江陵大戰","default":0},
            {"id":"negative","name":"旧条件","default":-2},
            {"id":"unknown","name":"未知默认值"}
        ]});
        for (mut node, expected) in [
            (
                json!({"type":"flag_check","source":"game_flag","key":"M30077","op":"eq","value":0}),
                true,
            ),
            (
                json!({"source":"game_flag","key":"M30077","op":"eq","value":0.0}),
                true,
            ),
            (
                json!({"source":"game_flag","key":"negative","op":"eq","value":-2}),
                true,
            ),
            (
                json!({"source":"flag_value","key":"M30077","op":"eq","value":0}),
                false,
            ),
            (
                json!({"source":"game_flag","key":"M30077","op":"ge","value":0}),
                false,
            ),
            (
                json!({"source":"game_flag","key":"M30077","op":"eq","value":1}),
                false,
            ),
            (
                json!({"source":"game_flag","key":"M30077","op":"eq"}),
                false,
            ),
            (
                json!({"source":"game_flag","key":"unknown","op":"eq","value":0}),
                false,
            ),
            (
                json!({"type":"game_flag","source":"game_flag","key":"M30077","op":"eq","value":0}),
                false,
            ),
        ] {
            let before = node.clone();
            assert_eq!(
                initial_default_condition(&data, &node, node["key"].as_str().unwrap()),
                expected
            );
            let output = frame(&egui::Context::default(), vec![], |ui| {
                assert!(!field(ui, &data, &mut node, "key", "原版旗标", false));
            });
            let hint = i18n::key("game_flags.initial_condition_hint");
            let shown = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| {
                    node.label() == Some(hint.as_str()) || node.value() == Some(hint.as_str())
                });
            assert_eq!(shown, expected);
            assert_eq!(node, before);
        }
    }

    #[test]
    fn game_flag_state_choices_write_only_original_numbers_and_additions_stay_numeric() {
        i18n::set_locale("chs");
        let data = &forms::Catalog::new().data;
        let ctx = egui::Context::default();
        let mut value = json!(1);
        let output = frame(&ctx, vec![], |ui| {
            assert!(!number(ui, data, "M20008", &mut value, true));
        });
        let output = frame(
            &ctx,
            click(&output, egui::accesskit::Role::ComboBox, "青城（1）"),
            |ui| {
                number(ui, data, "M20008", &mut value, true);
            },
        );
        frame(
            &ctx,
            click(&output, egui::accesskit::Role::Button, "崆峒（2）"),
            |ui| {
                number(ui, data, "M20008", &mut value, true);
            },
        );
        assert_eq!(value, json!(2));
        let ctx = egui::Context::default();
        let output = frame(&ctx, vec![], |ui| {
            assert!(!number(ui, data, "M20008", &mut value, false));
        });
        assert!(!output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == egui::accesskit::Role::ComboBox));
    }

    #[test]
    fn game_flag_reading_preserves_unlisted_values_and_legacy_field_presence() {
        i18n::set_locale("chs");
        let catalog = forms::Catalog::new();
        for mut node in [
            json!({"id":"flag","type":"game_flag","flag":"M20008","value":99,"op":"set"}),
            json!({"id":"flag","type":"game_flag","flag":"A00001","value":1.0,"op":"add"}),
            json!({"id":"check","type":"flag_check","source":"flag_value","flag":"M20011","success":"end","failure":"end"}),
            json!({"id":"flag","type":"game_flag","flag":"unknown_legacy","value":-17,"op":"set"}),
        ] {
            let before = node.clone();
            let ctx = egui::Context::default();
            frame(&ctx, vec![], |ui| {
                assert!(!catalog.field(
                    ui,
                    &mut node,
                    "flag",
                    "原版旗标",
                    "game_flag",
                    false,
                    &[],
                    &[]
                ));
                assert!(!catalog.field(ui, &mut node, "value", "数值", "int", true, &[], &[]));
            });
            assert_eq!(node, before);
        }
    }

    #[test]
    fn game_flag_search_reveals_temporary_ids_and_selection_stores_only_the_id() {
        i18n::set_locale("chs");
        let data = &forms::Catalog::new().data;
        let ctx = egui::Context::default();
        let mut node = json!({"type":"game_flag","flag":"M20008","value":99,"op":"set"});
        let output = frame(&ctx, vec![], |ui| {
            field(ui, data, &mut node, "flag", "原版数值旗标", false);
        });
        let output = frame(
            &ctx,
            click(&output, egui::accesskit::Role::ComboBox, "留学去向"),
            |ui| {
                field(ui, data, &mut node, "flag", "原版数值旗标", false);
            },
        );
        let target = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(id, node)| (node.role() == egui::accesskit::Role::TextInput).then_some(*id))
            .unwrap();
        let events = vec![
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Focus,
                target,
                data: None,
            }),
            egui::Event::Text("A00001".into()),
        ];
        frame(&ctx, events, |ui| {
            field(ui, data, &mut node, "flag", "原版数值旗标", false);
        });
        let output = frame(&ctx, vec![], |ui| {
            field(ui, data, &mut node, "flag", "原版数值旗标", false);
        });
        frame(
            &ctx,
            click(&output, egui::accesskit::Role::Button, "旗标暂存1"),
            |ui| {
                field(ui, data, &mut node, "flag", "原版数值旗标", false);
            },
        );
        assert_eq!(
            node,
            json!({"type":"game_flag","flag":"A00001","value":99,"op":"set"})
        );
    }
}
