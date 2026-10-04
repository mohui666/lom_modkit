use crate::i18n::tr;
use eframe::egui::{self, Color32, RichText, Ui};
use serde_json::{json, Value};

pub struct Catalog {
    pub schema: Value,
    pub data: Value,
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            schema: serde_json::from_str(include_str!("../data/authoring.json"))
                .expect("embedded authoring schema"),
            data: serde_json::from_str(include_str!("../../../data/editor_data.json"))
                .expect("embedded editor data"),
        }
    }
    pub fn sync_assets(&mut self, assets: &std::collections::BTreeMap<String, Vec<u8>>) {
        self.data = serde_json::from_str(include_str!("../../../data/editor_data.json"))
            .expect("embedded metadata");
        self.data["user_images"] = json!([]);
        self.data["user_voices"] = json!([]);
        for (path, bytes) in assets {
            if !path.ends_with("/content.json") {
                continue;
            }
            let Ok(meta) = serde_json::from_slice::<Value>(bytes) else {
                continue;
            };
            let Some(id) = meta["id"].as_str() else {
                continue;
            };
            let mut entry = json!({"id":format!("user:{id}"),"name":meta["name"]});
            let key = match meta["type"].as_str().unwrap_or("") {
                "character" => {
                    entry["portraits"] = Value::Array(
                        meta["portraits"]
                            .as_object()
                            .map(|o| o.keys().map(|s| json!(s)).collect())
                            .unwrap_or_default(),
                    );
                    "characters"
                }
                "image" => "user_images",
                "audio" => {
                    if meta["character"].is_string() {
                        entry["character"] = meta["character"].clone();
                        self.data["user_voices"]
                            .as_array_mut()
                            .unwrap()
                            .push(entry.clone());
                    }
                    match meta["audio_kind"].as_str().unwrap_or("") {
                        "music" => "music",
                        "env" => "env_sounds",
                        _ => "sounds",
                    }
                }
                _ => continue,
            };
            if let Some(a) = self.data[key].as_array_mut() {
                a.push(entry);
            }
        }
    }
    pub fn label(&self, kind: &str) -> String {
        let key = format!("node.{kind}");
        let translated = crate::i18n::key(&key);
        if translated != key {
            translated
        } else {
            tr(self.schema["NODE_TYPE_CN_SRC"][kind]
                .as_str()
                .unwrap_or(kind))
        }
    }
    pub fn new_node(&self, kind: &str, id: String) -> Value {
        let mut node = self.schema["_NODE_DEFAULTS"][kind].clone();
        if !node.is_object() {
            node = json!({});
        }
        node["id"] = id.into();
        node["type"] = kind.into();
        if node.get("character").and_then(Value::as_str) == Some("") {
            node["character"] = "player".into();
        }
        node
    }
    pub fn options(
        &self,
        kind: &str,
        node: &Value,
        nodes: &[String],
        stories: &[String],
    ) -> Vec<(String, String)> {
        if let Some(set) = kind.strip_prefix("enum:") {
            return self.schema["ENUM_SETS_SRC"][set]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| {
                            Some((v[0].as_str()?.to_owned(), v[1].as_str()?.to_owned()))
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        if kind == "node_ref" {
            return nodes.iter().map(|s| (s.clone(), s.clone())).collect();
        }
        if kind == "story_ref" {
            return stories.iter().map(|s| (s.clone(), s.clone())).collect();
        }
        if kind == "facing" {
            return vec![
                ("left".into(), "朝左".into()),
                ("right".into(), "朝右".into()),
            ];
        }
        if kind == "branch_source" {
            return vec![
                ("mod".into(), "MOD 旗标".into()),
                ("condition".into(), "官方条件".into()),
                ("stat".into(), "属性".into()),
                ("game".into(), "官方检查点".into()),
                ("flag_value".into(), "游戏数值旗标".into()),
            ];
        }
        if kind == "portrait" {
            let id = node["character"].as_str().unwrap_or("");
            if let Some(row) = self.data["characters"]
                .as_array()
                .and_then(|a| a.iter().find(|c| c["id"].as_str() == Some(id)))
            {
                return rows(&row["portraits"]);
            }
            return vec![("normal".into(), "normal".into())];
        }
        if kind == "mode" {
            return self.schema["MODE_CN_SRC"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .filter_map(|(id, v)| Some((id.clone(), v.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default();
        }
        if kind == "item" {
            return rows(
                &self.data[match node["kind"]
                    .as_str()
                    .or_else(|| node["category"].as_str())
                    .unwrap_or("misc")
                {
                    "book" => "items_book",
                    "special" => "items_special",
                    _ => "items_misc",
                }],
            );
        }
        if kind == "voice" {
            return self.data["user_voices"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|v| v["character"] == node["character"])
                        .filter_map(|v| {
                            Some((
                                v["id"].as_str()?.to_owned(),
                                v["name"].as_str().unwrap_or("").to_owned(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        let key = match kind {
            "character" => "characters",
            "affinity_character" | "affinity_optional" => "affinity_characters",
            "position" => "positions",
            "view" => "views",
            "music" => "music",
            "sound_name" => {
                if node["kind"] == "env" {
                    "env_sounds"
                } else {
                    "sounds"
                }
            }
            "stat" => "stats",
            "talent" => "talents",
            "game_flag" => "game_flags",
            "battle_skill" => "battle_skills",
            "battle_faction" => "battle_factions",
            "mode" => "modes",
            "effect" => "effects",
            "menu_dialog" => "menu_dialogs",
            "death_id" => "death_ids",
            "user_image" | "ending_image" | "intro_image" => "user_images",
            "item" => "items",
            _ => "",
        };
        let result = rows(&self.data[key]);
        if kind == "mode" && result.is_empty() {
            return vec![
                ("character".into(), "对话".into()),
                ("think".into(), "内心独白".into()),
                ("narrative".into(), "旁白".into()),
                ("center".into(), "居中旁白".into()),
            ];
        }
        result
    }
    pub fn node_form(
        &self,
        ui: &mut Ui,
        node: &mut Value,
        node_ids: &[String],
        story_ids: &[String],
    ) -> bool {
        let kind = node["type"].as_str().unwrap_or("").to_owned();
        let fields = self.schema["NODE_SCHEMAS"][&kind]["fields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut changed = false;
        ui.heading(self.label(&kind));
        ui.collapsing(tr("技术信息"), |ui| {
            ui.label(format!("{} · {}", kind, node["id"].as_str().unwrap_or("")));
        });
        let help_key = format!("help.{kind}");
        let help = crate::i18n::key(&help_key);
        if help != help_key {
            ui.label(
                RichText::new(help)
                    .small()
                    .color(Color32::from_rgb(178, 185, 205)),
            );
        }
        ui.separator();
        let allow_goto = ![
            "choice",
            "branch",
            "dice",
            "end",
            "goto_scene",
            "death",
            "combat",
            "battle",
            "battle_result",
            "stat_check",
            "affinity_check",
            "item_check",
            "talent_check",
            "flag_check",
            "activity",
            "quest_check",
            "persistent_check",
        ]
        .contains(&kind.as_str());
        if !allow_goto && node.get("goto").is_some() {
            ui.colored_label(
                Color32::from_rgb(255, 107, 97),
                "此节点由自己的分支决定流程，不允许额外 goto。",
            );
            if ui.button(tr("移除非法 goto 字段")).clicked() {
                node.as_object_mut().unwrap().remove("goto");
                changed = true;
            }
        }
        let active_fields: Vec<Value> = fields
            .iter()
            .filter(|f| field_visible(&kind, node, f[0].as_str().unwrap_or("")))
            .cloned()
            .collect();
        for field in &active_fields {
            if let (Some(key), Some(label), Some(field_kind)) =
                (field[0].as_str(), field[1].as_str(), field[2].as_str())
            {
                changed |= self.field(
                    ui,
                    node,
                    key,
                    label,
                    if kind == "intro" && key == "character" && node["intro_source"] == "character"
                    {
                        "character"
                    } else {
                        field_kind
                    },
                    field[3].as_bool().unwrap_or(false),
                    node_ids,
                    story_ids,
                );
            }
        }
        if allow_goto {
            ui.collapsing(crate::i18n::key("form.advanced"), |ui| {
                changed |= self.field(
                    ui,
                    node,
                    "goto",
                    "完成后跳转",
                    "node_ref",
                    true,
                    node_ids,
                    story_ids,
                );
            });
        }
        if kind == "branch" && node["source"] == "stat" {
            changed |= self.field(
                ui,
                node,
                "stat",
                "检定属性",
                "stat",
                false,
                node_ids,
                story_ids,
            );
        }
        if kind == "branch" && changed {
            if node["source"] == "stat" {
                if let Some(map) = node.as_object_mut() {
                    map.remove("flag");
                    map.entry("stat").or_insert(json!("morale"));
                }
            } else if let Some(map) = node.as_object_mut() {
                map.remove("stat");
            }
        }
        let known: Vec<&str> = fields
            .iter()
            .filter_map(|f| f[0].as_str())
            .chain(["id", "type", "goto", "stat"])
            .collect();
        let extras: Vec<String> = node
            .as_object()
            .map(|o| {
                o.keys()
                    .filter(|k| !known.contains(&k.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !extras.is_empty() {
            ui.collapsing(
                tr("附加字段（保留项目中的全部数据）"),
                |ui| {
                    for key in extras {
                        ui.push_id(&key, |ui| {
                            if let Some(v) = node.get_mut(&key) {
                                changed |= value_editor(ui, &key, v, 0);
                            }
                        });
                    }
                },
            );
        }
        changed
    }
    pub fn field(
        &self,
        ui: &mut Ui,
        node: &mut Value,
        key: &str,
        label: &str,
        kind: &str,
        optional: bool,
        node_ids: &[String],
        story_ids: &[String],
    ) -> bool {
        let specific = format!("field.{}.{}", node["type"].as_str().unwrap_or(""), key);
        let generic = format!("field.{key}");
        let localized = crate::i18n::key(&specific);
        let label = if crate::i18n::locale() == "chs" {
            label.to_owned()
        } else if localized != specific {
            localized
        } else {
            let fallback = crate::i18n::key(&generic);
            if fallback != generic {
                fallback
            } else {
                tr(label)
            }
        };
        let label = label.as_str();
        let mut options = self.options(kind, node, node_ids, story_ids);
        if kind.starts_with("enum:") || ["mode", "facing", "branch_source"].contains(&kind) {
            for (_, label) in &mut options {
                *label = tr(label);
            }
        }
        let mut changed = false;
        ui.push_id(key, |ui| {
            let width = ui.available_width();
            let label_width = (width * 0.27).clamp(78.0, 120.0);
            let mut enabled = node.get(key).is_some();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(label_width);
                        ui.set_max_width(label_width);
                        if optional {
                            if ui.checkbox(&mut enabled, label).changed() {
                                if enabled {
                                    node[key] = self.schema["_NODE_DEFAULTS"]
                                        [node["type"].as_str().unwrap_or("")][key]
                                        .clone();
                                    if node[key].is_null() {
                                        node[key] = default_for(kind);
                                    }
                                } else if let Some(o) = node.as_object_mut() {
                                    o.remove(key);
                                }
                                changed = true;
                            }
                            ui.label(RichText::new(tr("可选")).small().color(Color32::GRAY));
                        } else {
                            ui.label(label);
                        }
                    },
                );
                ui.vertical(|ui| {
                    ui.set_width((width - label_width - 12.0).max(100.0));
                    if optional && !enabled {
                        ui.add_space(5.0);
                        return;
                    }
                    let mut value = node.get(key).cloned().unwrap_or_else(|| default_for(kind));
                    let field_changed = match kind {
                        "bool" => {
                            let mut b = value.as_bool().unwrap_or(false);
                            let c = ui.checkbox(&mut b, tr("启用")).changed();
                            if c {
                                value = b.into();
                            }
                            c
                        }
                        "int" | "bool_int" | "discount_toggle" => {
                            let mut n = value.as_i64().unwrap_or(0);
                            let c = ui.add(egui::DragValue::new(&mut n).speed(1)).changed();
                            if c {
                                value = n.into();
                            }
                            c
                        }
                        "float" | "percent_scale" | "percent_cg_scale" | "percent_position"
                        | "percent_offset" | "percent_opacity" => {
                            let mut n = value.as_f64().unwrap_or(0.0);
                            let c = ui.add(egui::DragValue::new(&mut n).speed(0.25)).changed();
                            if c {
                                value = json!(n);
                            }
                            c
                        }
                        "multiline" | "code" => {
                            let mut s = value.as_str().unwrap_or("").to_owned();
                            let c = ui
                                .add(
                                    egui::TextEdit::multiline(&mut s)
                                        .desired_rows(if kind == "code" { 8 } else { 4 })
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();
                            if c {
                                value = s.into();
                            }
                            c
                        }
                        "options"
                        | "cases"
                        | "dice_bands"
                        | "vars"
                        | "combat_talents"
                        | "battle_faction_list"
                        | "official_characters"
                        | "reward_entries"
                        | "reward_entries_optional"
                        | "custom_shop_items" => array_editor(ui, &mut value, kind, node_ids),
                        _ => {
                            let mut s = value.as_str().unwrap_or("").to_owned();
                            let mut c = false;
                            if options.is_empty() {
                                c |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut s)
                                            .desired_width(f32::INFINITY)
                                            .hint_text(tr("填写内容")),
                                    )
                                    .changed();
                            } else {
                                let display = options
                                    .iter()
                                    .find(|(v, _)| v == &s)
                                    .map(|(_, label)| label.as_str())
                                    .unwrap_or(&s)
                                    .to_owned();
                                egui::ComboBox::from_id_salt("select")
                                    .truncate()
                                    .selected_text(if display.is_empty() {
                                        tr("选择…")
                                    } else {
                                        display
                                    })
                                    .width(ui.available_width())
                                    .height(260.0)
                                    .show_ui(ui, |ui| {
                                        let search_id = ui.id().with("search");
                                        let mut search = ui.data_mut(|d| {
                                            d.get_temp::<String>(search_id).unwrap_or_default()
                                        });
                                        ui.add(
                                            egui::TextEdit::singleline(&mut search)
                                                .hint_text(tr("搜索名称或 ID")),
                                        );
                                        let lower = search.to_lowercase();
                                        for (id, name) in &options {
                                            if lower.is_empty()
                                                || id.to_lowercase().contains(&lower)
                                                || name.to_lowercase().contains(&lower)
                                            {
                                                c |= ui
                                                    .selectable_value(
                                                        &mut s,
                                                        id.clone(),
                                                        format!("{name} · {id}"),
                                                    )
                                                    .changed();
                                            }
                                        }
                                        ui.separator();
                                        c |= ui
                                            .add(
                                                egui::TextEdit::singleline(&mut s)
                                                    .hint_text(tr("内部 ID / 自定义引用")),
                                            )
                                            .changed();
                                        ui.data_mut(|d| d.insert_temp(search_id, search));
                                    });
                            }
                            if c {
                                value = s.into();
                            }
                            c
                        }
                    };
                    if field_changed {
                        node[key] = value;
                        changed = true;
                    }
                });
            });
            ui.add_space(4.0);
        });
        changed
    }
}

fn field_visible(kind: &str, node: &Value, key: &str) -> bool {
    if kind == "intro" {
        let custom = node["intro_source"] == "custom";
        return match key {
            "intro_source" => true,
            "character" => !custom,
            _ => custom,
        };
    }
    !(kind == "branch" && node["source"] == "stat" && key == "flag")
}

fn rows(value: &Value) -> Vec<(String, String)> {
    if let Some(a) = value.as_array() {
        return a
            .iter()
            .filter_map(|v| {
                if let Some(s) = v.as_str() {
                    Some((s.to_owned(), s.to_owned()))
                } else {
                    let id = v["id"].as_str().or_else(|| v["key"].as_str())?;
                    Some((id.to_owned(), v["name"].as_str().unwrap_or(id).to_owned()))
                }
            })
            .collect();
    }
    if let Some(o) = value.as_object() {
        return o
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or(k).to_owned()))
            .collect();
    }
    Vec::new()
}
fn default_for(kind: &str) -> Value {
    match kind {
        "bool" => json!(false),
        "int" | "float" | "bool_int" | "discount_toggle" => json!(0),
        "percent_scale" | "percent_cg_scale" | "percent_opacity" => json!(100),
        "percent_position" | "percent_offset" => json!(0),
        "options"
        | "cases"
        | "dice_bands"
        | "vars"
        | "combat_talents"
        | "battle_faction_list"
        | "official_characters"
        | "reward_entries"
        | "reward_entries_optional"
        | "custom_shop_items" => json!([]),
        _ => json!(""),
    }
}
fn row_template(kind: &str) -> Value {
    match kind {
        "options" => json!({"text":"新选项","goto":""}),
        "cases" => json!({"value":1,"goto":""}),
        "dice_bands" => json!({"text":"结果","goto":""}),
        "vars" => json!({"name":"","value":""}),
        "combat_talents" => json!({"key":"","level":1}),
        "battle_faction_list" => json!({"id":"","people":1}),
        "official_characters" => json!("player"),
        "custom_shop_items" => json!({"category":"misc","item":"","count":1}),
        _ => json!({"kind":"stat","key":"","amount":1}),
    }
}
fn array_editor(ui: &mut Ui, value: &mut Value, kind: &str, node_ids: &[String]) -> bool {
    let Some(array) = value.as_array_mut() else {
        return value_editor(ui, "内容", value, 0);
    };
    let mut changed = false;
    let mut remove = None;
    let mut swap = None;
    for i in 0..array.len() {
        ui.push_id(i, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("第 {} 项", i + 1));
                    if ui.small_button("↑").clicked() && i > 0 {
                        swap = Some((i, i - 1));
                    }
                    if ui.small_button("↓").clicked() && i + 1 < array.len() {
                        swap = Some((i, i + 1));
                    }
                    if ui.small_button(tr("删除")).clicked() {
                        remove = Some(i);
                    }
                });
                if let Some(obj) = array[i].as_object_mut() {
                    for (key, val) in obj.iter_mut() {
                        ui.push_id(key, |ui| {
                            if matches!(
                                key.as_str(),
                                "goto" | "win" | "lose" | "success" | "failure"
                            ) {
                                ui.label(key);
                                let mut s = val.as_str().unwrap_or("").to_owned();
                                egui::ComboBox::from_id_salt("target")
                                    .selected_text(&s)
                                    .show_ui(ui, |ui| {
                                        for n in node_ids {
                                            if ui.selectable_value(&mut s, n.clone(), n).changed() {
                                                changed = true;
                                            }
                                        }
                                    });
                                if ui.text_edit_singleline(&mut s).changed() {
                                    changed = true;
                                }
                                *val = s.into();
                            } else {
                                changed |= value_editor(ui, key, val, 1);
                            }
                        });
                    }
                    let key_id = ui.id().with("new-key");
                    let mut key = ui.data_mut(|d| d.get_temp::<String>(key_id).unwrap_or_default());
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut key)
                                .hint_text(tr("可选字段，如 upper / cost")),
                        );
                        if ui.small_button(tr("加字段")).clicked()
                            && !key.is_empty()
                            && !obj.contains_key(&key)
                        {
                            obj.insert(
                                key.clone(),
                                if matches!(
                                    key.as_str(),
                                    "upper" | "cost" | "price" | "level" | "value" | "amount"
                                ) {
                                    json!(0)
                                } else {
                                    json!("")
                                },
                            );
                            key.clear();
                            changed = true;
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(key_id, key));
                } else {
                    changed |= value_editor(ui, "值", &mut array[i], 1);
                }
            });
        });
    }
    if let Some(i) = remove {
        array.remove(i);
        changed = true;
    } else if let Some((a, b)) = swap {
        array.swap(a, b);
        changed = true;
    }
    if ui.button(tr("＋ 添加一项")).clicked() {
        array.push(row_template(kind));
        changed = true;
    }
    changed
}

/// A recursive structured editor for extension fields, manifests, and translation catalogs.
/// It retains every value and JSON type; text is edited as text, never as serialized JSON.
pub fn value_editor(ui: &mut Ui, label: &str, value: &mut Value, depth: usize) -> bool {
    let mut changed = false;
    if depth > 12 {
        ui.label(tr("嵌套过深，请在项目源文件中检查"));
        return false;
    }
    match value {
        Value::Bool(b) => {
            changed |= ui.checkbox(b, label).changed();
        }
        Value::Number(n) => {
            ui.horizontal(|ui| {
                ui.label(label);
                if let Some(mut v) = n.as_i64() {
                    if ui.add(egui::DragValue::new(&mut v)).changed() {
                        *n = v.into();
                        changed = true;
                    }
                } else if let Some(mut v) = n.as_f64() {
                    if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                        if let Some(num) = serde_json::Number::from_f64(v) {
                            *n = num;
                            changed = true;
                        }
                    }
                }
            });
        }
        Value::String(s) => {
            ui.label(label);
            if s.contains('\n') || matches!(label, "text" | "description" | "code") {
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(s)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
            } else {
                changed |= ui
                    .add(egui::TextEdit::singleline(s).desired_width(f32::INFINITY))
                    .changed();
            }
        }
        Value::Array(a) => {
            ui.collapsing(format!("{} · {} 项", label, a.len()), |ui| {
                let mut remove = None;
                for (i, v) in a.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("第 {} 项", i + 1));
                            if ui.small_button(tr("删除")).clicked() {
                                remove = Some(i);
                            }
                        });
                        changed |= value_editor(ui, "内容", v, depth + 1);
                        ui.separator();
                    });
                }
                if let Some(i) = remove {
                    a.remove(i);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("＋ 文本")).clicked() {
                        a.push(json!(""));
                        changed = true;
                    }
                    if ui.button(tr("＋ 对象")).clicked() {
                        a.push(json!({}));
                        changed = true;
                    }
                    if ui.button(tr("＋ 同类项")).clicked() {
                        a.push(a.last().cloned().unwrap_or(json!({})));
                        changed = true;
                    }
                });
            });
        }
        Value::Object(o) => {
            egui::CollapsingHeader::new(label)
                .default_open(depth < 2)
                .show(ui, |ui| {
                    let mut remove = None;
                    for (k, v) in o.iter_mut() {
                        ui.push_id(k, |ui| {
                            ui.horizontal(|ui| {
                                ui.strong(k);
                                if ui
                                    .small_button("×")
                                    .on_hover_text(tr("删除这个字段"))
                                    .clicked()
                                {
                                    remove = Some(k.clone());
                                }
                            });
                            changed |= value_editor(ui, k, v, depth + 1);
                            ui.add_space(4.0);
                        });
                    }
                    if let Some(k) = remove {
                        o.remove(&k);
                        changed = true;
                    }
                    let id = ui.id().with("key");
                    let mut key = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
                    let ty_id = ui.id().with("type");
                    let mut ty = ui.data_mut(|d| d.get_temp::<usize>(ty_id).unwrap_or_default());
                    ui.add(egui::TextEdit::singleline(&mut key).hint_text(tr("新字段名")));
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("value-type")
                            .selected_text(["文本", "数字", "布尔", "对象", "列表"][ty])
                            .show_ui(ui, |ui| {
                                for (i, n) in
                                    ["文本", "数字", "布尔", "对象", "列表"].iter().enumerate()
                                {
                                    ui.selectable_value(&mut ty, i, *n);
                                }
                            });
                        if ui.button(tr("添加字段")).clicked()
                            && !key.trim().is_empty()
                            && !o.contains_key(key.trim())
                        {
                            o.insert(
                                key.trim().to_owned(),
                                match ty {
                                    1 => json!(0),
                                    2 => json!(false),
                                    3 => json!({}),
                                    4 => json!([]),
                                    _ => json!(""),
                                },
                            );
                            key.clear();
                            changed = true;
                        }
                    });
                    ui.data_mut(|d| {
                        d.insert_temp(id, key);
                        d.insert_temp(ty_id, ty);
                    });
                });
        }
        Value::Null => {
            ui.horizontal(|ui| {
                ui.label(format!("{label}：空值"));
                if ui.button(tr("设为文本")).clicked() {
                    *value = json!("");
                    changed = true;
                }
            });
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_node_form_renders_without_modifying_untouched_document() {
        let catalog = Catalog::new();
        let context = egui::Context::default();
        for kind in catalog.schema["NODE_SCHEMAS"].as_object().unwrap().keys() {
            let mut node = catalog.new_node(kind, "n1".into());
            let before = node.clone();
            let _ = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    catalog.node_form(
                        ui,
                        &mut node,
                        &["n1".into(), "end1".into()],
                        &["main".into()],
                    );
                });
            });
            assert_eq!(node, before, "merely rendering {kind} mutated the project");
        }
    }
}

#[cfg(test)]
mod conditional_tests {
    use super::*;
    #[test]
    fn intro_sources_show_only_their_contract_fields() {
        let custom = json!({"intro_source":"custom"});
        assert!(!field_visible("intro", &custom, "character"));
        assert!(field_visible("intro", &custom, "text"));
        for source in ["official", "character"] {
            let node = json!({"intro_source":source});
            assert!(field_visible("intro", &node, "character"));
            assert!(!field_visible("intro", &node, "image"));
            assert!(!field_visible("intro", &node, "name"));
        }
        assert!(!field_visible("branch", &json!({"source":"stat"}), "flag"));
    }
}
