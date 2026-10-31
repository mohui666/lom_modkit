use crate::i18n::tr;
use eframe::egui::{self, Color32, RichText, Ui};
use serde_json::{json, Value};

pub struct Catalog {
    pub schema: Value,
    pub data: Value,
}

pub struct NodeFormResponse {
    pub changed: bool,
    pub help: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Requirement {
    Optional,
    Required,
    OneOf,
}

// Use compiler requirements, rather than the form's reset/default flag.
fn field_requirement(node: &Value, key: &str, optional: bool) -> Requirement {
    use Requirement::*;
    let kind = node["type"].as_str().unwrap_or("");
    let spec = &lom_core::validate::node_schema()["nodes"][kind];
    let required = if spec.is_object() {
        spec["required"].get(key).is_some()
    } else if matches!(kind, "character" | "audio" | "image") {
        key == "name"
    } else {
        !optional
    };
    let conditional = match (kind, key) {
        ("say", "character") => Some(!matches!(
            node["mode"].as_str(),
            Some("narrative" | "center")
        )),
        ("intro", "character") => Some(node["intro_source"] != "custom"),
        ("intro", "name" | "text") => Some(node["intro_source"] == "custom"),
        ("background", "image") => Some(matches!(
            node["action"].as_str(),
            Some("set" | "show" | "replace" | "fadein")
        )),
        ("custom_cg" | "overlay", "image") => Some(node["action"] == "show"),
        ("cg", "key") => Some(node["action"] == "show"),
        ("cg", "key2") => Some(
            node["action"] == "show" && matches!(node["kind"].as_str(), Some("map" | "family")),
        ),
        ("cg", "n1" | "n2") => Some(node["action"] == "show" && node["kind"] == "family"),
        ("enemy", "value") => Some(node["op"] != "id"),
        ("battle_skill", "key") => Some(matches!(
            node["op"].as_str(),
            Some("set" | "active" | "level")
        )),
        ("flag_check", "op" | "value") => Some(node["source"] == "flag_value"),
        ("time", "year" | "month" | "stage") => {
            Some(matches!(node["op"].as_str(), Some("set" | "mission")))
        }
        ("time", "name") => Some(node["op"] == "mission"),
        ("panel", "key") => Some(matches!(
            node["panel"].as_str(),
            Some("cg" | "cgvideo" | "endgame")
        )),
        ("branch", "stat") => Some(node["source"] == "stat"),
        ("branch", "flag") => Some(node["source"] != "stat"),
        ("dice", "check" | "options") => {
            Some(node.get("check").is_some() || node.get("options").is_some())
        }
        ("dice", "max" | "header" | "bands") => {
            Some(node.get("check").is_none() && node.get("options").is_none())
        }
        (
            "battle",
            "friend_factions" | "friend_characters" | "enemy_factions" | "enemy_characters",
        ) => return OneOf,
        ("goto_scene", "title" | "desc" | "image") => {
            let id = node["key"].as_str().unwrap_or("");
            let digits = !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
            let significant = id.trim_start_matches('0');
            let custom = digits
                && (significant.len() > 6 || (significant.len() == 6 && significant >= "900000"));
            if (node["scene"] == "GameOver" && custom && key != "image")
                || (node["scene"] == "End" && (id.is_empty() || custom))
            {
                return OneOf;
            }
            Some(false)
        }
        _ => None,
    };
    if conditional.unwrap_or(required) {
        Required
    } else {
        Optional
    }
}

fn requirement_label(label: String, requirement: Requirement) -> String {
    match requirement {
        Requirement::Optional => label,
        Requirement::Required => format!("{label} *"),
        Requirement::OneOf => format!("{label} * ({})", tr("至少填一项")),
    }
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
        let mut options = self.raw_options(kind, node, nodes, stories);
        for (id, name) in &mut options {
            if kind.starts_with("enum:") || ["mode", "facing", "branch_source"].contains(&kind) {
                *name = tr(name);
            } else if kind == "portrait"
                && !node["character"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("user:")
            {
                *name = crate::i18n::term("portraits", id, name);
            } else if kind == "game_flag" && id == name {
                *name = format!("{} {id}", tr("检查点"));
            }
        }
        // A character keeps the same number in the full list and in restricted
        // affinity/battle lists. Search only filters these already named options.
        if is_character_selector(kind) && kind != "character_group" {
            let mut characters = localized_rows(&self.data["characters"], "characters");
            number_duplicate_labels(&mut characters);
            let names: std::collections::BTreeMap<_, _> = characters.into_iter().collect();
            for (id, name) in &mut options {
                if let Some(label) = names.get(id) {
                    *name = label.clone();
                }
            }
        }
        number_duplicate_labels(&mut options);
        options
    }
    fn raw_options(
        &self,
        kind: &str,
        node: &Value,
        nodes: &[String],
        stories: &[String],
    ) -> Vec<(String, String)> {
        if kind == "character_group" {
            let rows: Vec<Value> = self.data["characters"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|row| {
                    row["appearance_group"]
                        .as_str()
                        .is_none_or(|group| row["id"] == group)
                })
                .cloned()
                .collect();
            return localized_rows(&json!(rows), "characters");
        }
        if kind == "character_appearance" {
            return self.appearance_options(node);
        }
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
            if id == "player" && node["appearance"] == "beautified" {
                return rows(&self.data["player_appearance_portraits"]["beautified"]);
            }
            if let Some(row) = self.data["characters"]
                .as_array()
                .and_then(|a| a.iter().find(|c| c["id"].as_str() == Some(id)))
            {
                return rows(&row["portraits"]);
            }
            return vec![("normal".into(), "normal".into())];
        }
        if kind == "official_picture" {
            let catalog: Value =
                serde_json::from_str(include_str!("../../../data/picture_keys.json"))
                    .expect("picture keys");
            let preferred = match node["kind"].as_str() {
                Some("item") => "Items",
                Some("big") => "BigPicture",
                _ => "Picture",
            };
            let mut result: Vec<_> = catalog["pictures"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    Some((
                        row["key"].as_str()?.to_owned(),
                        row["category"].as_str()?.to_owned(),
                    ))
                })
                .collect();
            result.sort_by_key(|(key, category)| (category != preferred, key.clone()));
            return result
                .into_iter()
                .map(|(key, category)| (key.clone(), format!("{key} · {category}")))
                .collect();
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
        if kind == "camera" {
            return rows(&self.schema["CAMERA_PRESETS"]);
        }
        if kind == "flag_ref" {
            return std::iter::once((String::new(), tr("不限")))
                .chain(rows(&self.data["project_flags"]))
                .collect();
        }
        if kind == "battle_character" || kind == "battle_faction" {
            let (key, category) = if kind == "battle_character" {
                ("VERIFIED_BATTLE_CHARACTER_IDS", "characters")
            } else {
                ("VERIFIED_BATTLE_FACTION_IDS", "battle_factions")
            };
            let names = rows(&self.data[category]);
            return self.schema[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .map(|id| {
                    let name = names
                        .iter()
                        .find(|(k, _)| k == id)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or(id);
                    (id.into(), crate::i18n::term(category, id, name))
                })
                .collect();
        }
        if kind == "item" {
            let category = match node["category"]
                .as_str()
                .or_else(|| node["kind"].as_str())
                .unwrap_or("misc")
            {
                "book" => "items_book",
                "special" => "items_special",
                _ => "items_misc",
            };
            return localized_rows(&self.data[category], category);
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
            "combat_skill" => "combat_talents",
            "game_flag" => "game_flags",
            "free_position" => "free_positions",
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
        let result = localized_rows(
            &self.data[key],
            if key == "affinity_characters" {
                "characters"
            } else {
                key
            },
        );
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
    ) -> NodeFormResponse {
        let kind = node["type"].as_str().unwrap_or("").to_owned();
        let fields = self.schema["NODE_SCHEMAS"][&kind]["fields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut changed = false;
        let mut help_request = None;
        ui.horizontal(|ui| {
            ui.heading(self.label(&kind));
            let help_key = format!("help.{kind}");
            let help = crate::i18n::key(&help_key);
            if help != help_key {
                if ui.small_button(tr("说明")).on_hover_text(help).clicked() {
                    help_request = Some(kind.clone());
                }
            }
        });
        ui.add_space(6.0);
        if kind == "parallel" {
            changed |= self.parallel_actions(ui, node);
            ui.collapsing(crate::i18n::key("form.advanced"), |ui| {
                changed |= self.field(
                    ui,
                    node,
                    "goto",
                    "整组完成后跳转",
                    "node_ref",
                    true,
                    node_ids,
                    story_ids,
                );
            });
            return NodeFormResponse {
                changed,
                help: help_request,
            };
        }
        if kind == "battle" {
            for side in ["friend", "enemy"] {
                let total = node[format!("{side}_factions")]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| r["people"].as_i64().unwrap_or(1))
                    .sum::<i64>()
                    + node[format!("{side}_characters")]
                        .as_array()
                        .map_or(0, Vec::len) as i64;
                ui.label(format!(
                    "{}: {total}",
                    if side == "friend" {
                        tr("我方总人数")
                    } else {
                        tr("敌方总人数")
                    }
                ));
            }
        }
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
        for field in &fields {
            if let (Some(key), Some(label), Some(field_kind)) =
                (field[0].as_str(), field[1].as_str(), field[2].as_str())
            {
                if !field_visible(&kind, node, key) {
                    continue;
                }
                changed |= self.field(
                    ui,
                    node,
                    key,
                    if kind == "cg" && key == "key" && node["kind"] == "title" {
                        "标题文字"
                    } else if kind == "cg"
                        && key == "key"
                        && matches!(node["kind"].as_str(), Some("picture" | "item" | "big"))
                    {
                        "图片"
                    } else {
                        label
                    },
                    if kind == "intro" && key == "character" && node["intro_source"] == "character"
                    {
                        "character"
                    } else if kind == "cg"
                        && key == "key"
                        && matches!(node["kind"].as_str(), Some("picture" | "item" | "big"))
                    {
                        "official_picture"
                    } else {
                        field_kind
                    },
                    field[3].as_bool().unwrap_or(false),
                    node_ids,
                    story_ids,
                );
            }
        }
        ui.add_space(4.0);
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
                        let label =
                            requirement_label(key.clone(), field_requirement(node, &key, true));
                        ui.push_id(&key, |ui| {
                            if let Some(v) = node.get_mut(&key) {
                                changed |= value_editor(ui, &label, v, 0);
                            }
                        });
                    }
                },
            );
        }
        NodeFormResponse {
            changed,
            help: help_request,
        }
    }
    fn parallel_actions(&self, ui: &mut Ui, node: &mut Value) -> bool {
        ui.label(tr("这些动作会一起开始，全部完成后才继续下一步。"));
        ui.small(tr(
            "音乐、音效和循环特效在启动后即视为完成；可添加“等待”控制整组停留时间。",
        ));
        let Some(actions) = node.get("actions").and_then(Value::as_array) else {
            ui.colored_label(
                Color32::from_rgb(179, 55, 49),
                tr("同时动作数据不是数组，请先修正原始数据。"),
            );
            return false;
        };
        let mut actions = actions.clone();
        let mut changed = false;
        let mut remove = None;
        let mut swap = None;
        for (index, action) in actions.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(format!("{} {}", tr("同时动作"), index + 1));
                        let old = action["type"].as_str().unwrap_or("").to_owned();
                        let mut selected = old.clone();
                        egui::ComboBox::from_id_salt("parallel-action-type")
                            .selected_text(self.label(&old))
                            .show_ui(ui, |ui| {
                                for kind in lom_core::parallel::ACTION_TYPES {
                                    ui.selectable_value(
                                        &mut selected,
                                        (*kind).into(),
                                        self.label(kind),
                                    );
                                }
                            });
                        if selected != old {
                            *action = self.new_node(&selected, String::new());
                            action.as_object_mut().unwrap().remove("id");
                            changed = true;
                        }
                        if ui
                            .add_enabled(index > 0, egui::Button::new(tr("上移")))
                            .clicked()
                        {
                            swap = Some((index, index - 1));
                        }
                        if ui.small_button(tr("移除")).clicked() {
                            remove = Some(index);
                        }
                    });
                    let kind = action["type"].as_str().unwrap_or("");
                    if !lom_core::parallel::ACTION_TYPES.contains(&kind) {
                        ui.colored_label(
                            Color32::from_rgb(179, 55, 49),
                            tr("这个步骤不能同时执行，请改为演出动作或移到组外。"),
                        );
                        return;
                    }
                    let fields = self.schema["NODE_SCHEMAS"][kind]["fields"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    for field in fields {
                        if field[0] == "appearance" || field[0] == "wait_for_completion" {
                            continue;
                        }
                        if let (Some(key), Some(label), Some(field_kind)) =
                            (field[0].as_str(), field[1].as_str(), field[2].as_str())
                        {
                            changed |= self.field(
                                ui,
                                action,
                                key,
                                label,
                                field_kind,
                                field[3].as_bool().unwrap_or(false),
                                &[],
                                &[],
                            );
                        }
                    }
                    for key in ["id", "goto", "appearance"] {
                        if action.get(key).is_some() {
                            ui.colored_label(
                                Color32::from_rgb(179, 55, 49),
                                format!("{}: {key}", tr("组内不支持此字段")),
                            );
                            if ui
                                .small_button(format!("{} {key}", tr("移除非法字段")))
                                .clicked()
                            {
                                action.as_object_mut().unwrap().remove(key);
                                changed = true;
                            }
                        }
                    }
                });
            });
        }
        if let Some(index) = remove {
            actions.remove(index);
            changed = true;
        } else if let Some((a, b)) = swap {
            actions.swap(a, b);
            changed = true;
        }
        ui.add_enabled_ui(actions.len() < lom_core::parallel::MAX_ACTIONS, |ui| {
            ui.menu_button(tr("添加同时动作"), |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("parallel-add-actions")
                    .max_height((ui.ctx().content_rect().height() * 0.6).max(1.0))
                    .show(ui, |ui| {
                        for kind in lom_core::parallel::ACTION_TYPES {
                            if ui.button(self.label(kind)).clicked() {
                                let mut action = self.new_node(kind, String::new());
                                action.as_object_mut().unwrap().remove("id");
                                actions.push(action);
                                changed = true;
                                ui.close();
                            }
                        }
                    });
            });
        });
        if changed {
            node["actions"] = Value::Array(actions);
        }
        if let Err(error) = lom_core::parallel::validate_group(node) {
            ui.colored_label(Color32::from_rgb(179, 55, 49), error.to_string());
        }
        changed
    }
    fn character_group<'a>(&'a self, id: &'a str) -> &'a str {
        self.data["characters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == id)
            .and_then(|row| row["appearance_group"].as_str())
            .unwrap_or(id)
    }
    fn appearance_options(&self, node: &Value) -> Vec<(String, String)> {
        let root = self.character_group(node["character"].as_str().unwrap_or(""));
        let mut options: Vec<(String, String)> = Vec::new();
        for row in self.data["characters"].as_array().into_iter().flatten() {
            if row["appearance_group"] == root {
                let Some(id) = row["id"].as_str() else {
                    continue;
                };
                if id == "player" && node["type"] == "show" && node.get("id").is_some() {
                    options.extend([
                        ("player".into(), tr("跟随游戏")),
                        ("@original".into(), tr("普通")),
                        ("@beautified".into(), tr("美颜")),
                    ]);
                } else {
                    options.push((id.into(), tr(row["appearance_name"].as_str().unwrap_or(id))));
                }
            }
        }
        options.sort_by_key(|(id, _)| {
            if id == root || id.starts_with('@') {
                0
            } else {
                1
            }
        });
        number_duplicate_labels(&mut options);
        options
    }
    fn select_appearance(&self, node: &mut Value, selected: &str) {
        let (character, appearance) = match selected {
            "@original" => ("player", Some("original")),
            "@beautified" => ("player", Some("beautified")),
            "player" if node["type"] == "show" && node.get("id").is_some() => {
                ("player", Some("game"))
            }
            other => (other, None),
        };
        node["character"] = json!(character);
        if let Some(appearance) = appearance {
            node["appearance"] = json!(appearance);
        } else {
            node.as_object_mut().unwrap().remove("appearance");
        }
        if let Some(portrait) = node["portrait"].as_str().filter(|s| !s.is_empty()) {
            let options = self.options("portrait", node, &[], &[]);
            if !options.iter().any(|(id, _)| id == portrait) {
                let fallback = if options.iter().any(|(id, _)| id == "normal") {
                    "normal"
                } else {
                    options
                        .first()
                        .map(|(id, _)| id.as_str())
                        .unwrap_or("normal")
                };
                node["portrait"] = json!(fallback);
            }
        }
    }
    fn character_field(&self, ui: &mut Ui, node: &mut Value, label: &str, optional: bool) -> bool {
        let before = node.clone();
        let mut selection = node.clone();
        if let Some(id) = node["character"].as_str() {
            selection["character"] = json!(self.character_group(id));
        }
        if self.field(
            ui,
            &mut selection,
            "character",
            label,
            "character_group",
            optional,
            &[],
            &[],
        ) {
            if let Some(id) = selection["character"].as_str() {
                self.select_appearance(node, id);
            } else {
                node.as_object_mut().unwrap().remove("character");
            }
        }
        let options = self.appearance_options(node);
        if options.len() > 1 {
            let selected = if node["character"] == "player" {
                match node["appearance"].as_str() {
                    Some("original") => "@original",
                    Some("beautified") => "@beautified",
                    _ => "player",
                }
            } else {
                node["character"].as_str().unwrap_or("")
            };
            let mut selection = node.clone();
            selection["selected_appearance"] = json!(selected);
            if self.field(
                ui,
                &mut selection,
                "selected_appearance",
                "外观",
                "character_appearance",
                false,
                &[],
                &[],
            ) {
                if let Some(id) = selection["selected_appearance"].as_str() {
                    self.select_appearance(node, id);
                }
            }
        }
        *node != before
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
        if key == "character" && kind == "character" {
            return self.character_field(ui, node, label, optional);
        }
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
        let label = requirement_label(label, field_requirement(node, key, optional));
        let label = label.as_str();
        let options = self.options(kind, node, node_ids, story_ids);
        let mut changed = false;
        ui.push_id(key, |ui| {
            let width = ui.available_width();
            let stacked = width < 560.0;
            let label_width = if stacked {
                width
            } else {
                (width * 0.32).clamp(160.0, 220.0)
            };
            let value_width = if stacked {
                width
            } else {
                width - label_width - ui.spacing().item_spacing.x
            };
            let present = node.get(key).is_some();
            let selector = is_selector(kind);
            let mut reset = false;
            let layout = if stacked {
                egui::Layout::top_down(egui::Align::Min)
            } else {
                egui::Layout::left_to_right(egui::Align::Min)
            };
            ui.with_layout(layout, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(label_width);
                        ui.set_max_width(label_width);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(label);
                            if optional && !selector && kind != "bool" {
                                if present {
                                    reset = ui
                                        .small_button(tr("重置"))
                                        .on_hover_text(tr("恢复默认"))
                                        .clicked();
                                } else {
                                    ui.label(RichText::new(tr("默认")).small().weak());
                                }
                            }
                        });
                    },
                );
                ui.vertical(|ui| {
                    ui.set_width(value_width);
                    let mut value = node.get(key).cloned().unwrap_or_else(|| {
                        let template = &self.schema["_NODE_DEFAULTS"]
                            [node["type"].as_str().unwrap_or("")][key];
                        if template.is_null() || template.is_array() {
                            default_for(kind)
                        } else {
                            template.clone()
                        }
                    });
                    let field_changed = match kind {
                        "bool" if optional => {
                            let mut edited = false;
                            let title = if !present {
                                tr("默认")
                            } else if let Some(b) = value.as_bool() {
                                tr(if b { "启用" } else { "关闭" })
                            } else {
                                tr("自定义")
                            };
                            egui::ComboBox::from_id_salt("select")
                                .selected_text(title)
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(!present, tr("默认")).clicked() {
                                        reset = true;
                                        ui.close();
                                    }
                                    for (b, title) in [(true, "启用"), (false, "关闭")] {
                                        if ui
                                            .selectable_label(present && value == b, tr(title))
                                            .clicked()
                                        {
                                            value = json!(b);
                                            edited = true;
                                            ui.close();
                                        }
                                    }
                                });
                            edited
                        }
                        "bool" => {
                            let mut b = value.as_bool().unwrap_or(false);
                            let c = ui.checkbox(&mut b, tr("启用")).changed();
                            if c {
                                value = b.into();
                            }
                            c
                        }
                        "int" | "bool_int" | "discount_toggle" => {
                            let mut n = value.as_i64().unwrap_or(match value.as_str() {
                                Some("small") if key == "font_size" => 24,
                                Some("large") if key == "font_size" => 60,
                                _ => 0,
                            });
                            let response = if key == "font_size" {
                                ui.add(egui::DragValue::new(&mut n).speed(1).range(0..=200))
                                    .on_hover_text(tr("0 使用游戏默认字号，1～200 调节本句字号。"))
                            } else {
                                ui.add(egui::DragValue::new(&mut n).speed(1))
                            };
                            let mut c = response.changed();
                            if optional && !present {
                                response.context_menu(|ui| {
                                    if ui.button(tr("使用此值")).clicked() {
                                        c = true;
                                        ui.close();
                                    }
                                });
                            }
                            if c {
                                value = n.into();
                            }
                            c
                        }
                        "float" | "percent_scale" | "percent_cg_scale" | "percent_position"
                        | "percent_offset" | "percent_opacity" => {
                            let mut n = value.as_f64().unwrap_or(0.0);
                            let drag = egui::DragValue::new(&mut n).speed(if key == "wait_after" {
                                0.1
                            } else {
                                0.25
                            });
                            let response = if ["wait_after", "fade_out", "fade_in"].contains(&key) {
                                ui.add(drag.range(0.0..=f64::INFINITY))
                            } else {
                                ui.add(drag)
                            };
                            let mut c = response.changed();
                            if optional && !present {
                                response.context_menu(|ui| {
                                    if ui.button(tr("使用此值")).clicked() {
                                        c = true;
                                        ui.close();
                                    }
                                });
                            }
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
                                reset |= optional && s.is_empty();
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
                        | "custom_shop_items" => array_editor(self, ui, &mut value, kind, node_ids),
                        _ => {
                            let mut s = value.as_str().unwrap_or("").to_owned();
                            let mut c = false;
                            if !selector {
                                c = ui
                                    .add(
                                        egui::TextEdit::singleline(&mut s)
                                            .desired_width(f32::INFINITY)
                                            .hint_text(label),
                                    )
                                    .changed();
                                reset |= c && optional && s.is_empty();
                            } else if is_character_selector(kind) {
                                let absent = absent_label(node, key, kind);
                                if let Some(selection) = character_picker(
                                    ui,
                                    present.then_some(s.as_str()),
                                    &options,
                                    optional.then_some(absent.as_str()),
                                ) {
                                    if let Some(id) = selection {
                                        if optional && id.is_empty() {
                                            reset = true;
                                        } else {
                                            s = id;
                                            c = true;
                                        }
                                    } else {
                                        reset = true;
                                    }
                                }
                            } else {
                                let absent_label = absent_label(node, key, kind);
                                let display = if !present {
                                    absent_label.clone()
                                } else if let Some((_, name)) =
                                    options.iter().find(|(v, _)| v == &s)
                                {
                                    name.clone()
                                } else if s.is_empty() {
                                    tr("选择…")
                                } else {
                                    tr("自定义")
                                };
                                egui::ComboBox::from_id_salt("select")
                                    .truncate()
                                    .selected_text(display)
                                    .width(ui.available_width())
                                    .height(
                                        (ui.ctx().content_rect().height() * 0.55)
                                            .clamp(260.0, 560.0),
                                    )
                                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                    .show_ui(ui, |ui| {
                                        if optional {
                                            if ui
                                                .selectable_label(!present, &absent_label)
                                                .clicked()
                                            {
                                                reset = true;
                                                ui.close();
                                            }
                                            ui.separator();
                                        }
                                        let search_id = ui.id().with("search");
                                        let mut search = ui.data_mut(|d| {
                                            d.get_temp::<String>(search_id).unwrap_or_default()
                                        });
                                        if !options.is_empty() {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut search)
                                                    .hint_text(tr("搜索名称")),
                                            );
                                        }
                                        let lower = search.to_lowercase();
                                        for (id, name) in &options {
                                            if lower.is_empty()
                                                || id.to_lowercase().contains(&lower)
                                                || name.to_lowercase().contains(&lower)
                                            {
                                                if ui
                                                    .selectable_label(present && id == &s, name)
                                                    .clicked()
                                                {
                                                    s = id.clone();
                                                    c = true;
                                                    ui.close();
                                                }
                                            }
                                        }
                                        ui.collapsing(tr("手动输入"), |ui| {
                                            c |= ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut s)
                                                        .hint_text(tr("内部 ID / 自定义引用")),
                                                )
                                                .changed();
                                        });
                                        reset |= c && optional && s.is_empty();
                                        ui.data_mut(|d| d.insert_temp(search_id, search));
                                    });
                            }
                            if kind == "position" {
                                if let Some(position) = stage_position_picker(ui, &s) {
                                    s = position;
                                    c = true;
                                }
                            }
                            if c {
                                value = s.into();
                            }
                            c
                        }
                    };
                    if reset {
                        if let Some(object) = node.as_object_mut() {
                            changed |= object.remove(key).is_some();
                        }
                    } else if field_changed && node.get(key) != Some(&value) {
                        node[key] = value;
                        if key == "character" && node["type"] == "show" && node[key] != "player" {
                            node.as_object_mut().unwrap().remove("appearance");
                        }
                        changed = true;
                    }
                });
            });
        });
        changed
    }
}

fn is_character_selector(kind: &str) -> bool {
    matches!(
        kind,
        "character"
            | "character_group"
            | "affinity_character"
            | "affinity_optional"
            | "battle_character"
    )
}

fn stage_position_picker(ui: &mut Ui, selected: &str) -> Option<String> {
    let mut choice = None;
    egui::CollapsingHeader::new(tr("点击选择站位"))
        .id_salt("stage-position-picker").default_open(true).show(ui, |ui| {
            let positions: Value = serde_json::from_str(include_str!("../../../data/stage_positions.json")).expect("stage positions");
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(100.0), 86.0), egui::Sense::hover());
            let stage = egui::Rect::from_min_max(rect.min + egui::vec2(rect.width()*0.16, 0.0), rect.max - egui::vec2(rect.width()*0.16, 0.0));
            ui.painter().rect_filled(stage, 4.0, Color32::from_rgb(224,232,236));
            ui.painter().text(stage.center_top()+egui::vec2(0.0,9.0), egui::Align2::CENTER_TOP, tr("舞台"), egui::FontId::proportional(12.0), Color32::DARK_GRAY);
            for (id,label) in [("SL","左外"),("L1","左1"),("L2","左2"),("L3","左3"),("M","中"),("R1","右1"),("R2","右2"),("R3","右3"),("SR","右外")] {
                let x = positions["positions"][id]["x"].as_f64().unwrap_or(0.5) as f32;
                let normalized = if x < 0.0 {0.055} else if x > 1.0 {0.945} else {0.16+x*0.68};
                let point = egui::pos2(rect.left()+rect.width()*normalized, rect.center().y+6.0);
                let hit = egui::Rect::from_center_size(point, egui::vec2((rect.width()/11.0).clamp(12.0,28.0),40.0));
                let response = ui.interact(hit, ui.id().with(id),egui::Sense::click()).on_hover_text(format!("{id} · {} {:.0}%",tr("距画面左缘"),x*100.0));
                let active = selected == id;
                ui.painter().circle_filled(point,if active {6.0} else {4.0},if active {crate::shell::ACCENT} else {Color32::from_rgb(88,107,117)});
                ui.painter().text(point+egui::vec2(0.0,12.0),egui::Align2::CENTER_TOP,tr(label),egui::FontId::proportional(10.0),Color32::DARK_GRAY);
                if response.clicked() {choice=Some(id.to_owned());}
            }
            if let Some(position) = positions["positions"].get(selected) {
                ui.small(format!("{selected} · {} {:.0}% · {} {:.0}%", tr("横向"),position["x"].as_f64().unwrap_or(0.5)*100.0,tr("脚底距顶部"),position["feet"].as_f64().unwrap_or(1.0)*100.0));
            }
            ui.small(tr("SL / SR 在屏幕左 / 右外侧。完整列表可选择偏移站位，脚底超过 100% 表示低于画面底边。"));
        });
    choice
}

#[derive(Clone, Default)]
struct CharacterSearch {
    source: Option<String>,
    query: String,
    manual_id: String,
    open: bool,
}

/// Search is a draft; only choosing a result, clearing, or applying an ID commits.
fn character_picker(
    ui: &mut Ui,
    selected: Option<&str>,
    options: &[(String, String)],
    absent: Option<&str>,
) -> Option<Option<String>> {
    let state_id = ui.id().with("character-search");
    let mut state = ui
        .data_mut(|data| data.get_temp::<CharacterSearch>(state_id))
        .unwrap_or_default();
    if state.source.as_deref() != selected {
        state = CharacterSearch {
            source: selected.map(str::to_owned),
            manual_id: selected.unwrap_or("").to_owned(),
            ..Default::default()
        };
    }
    let selected_name = selected.and_then(|id| {
        options
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, name)| name.as_str())
    });
    let hint = selected_name
        .or(selected.filter(|id| !id.is_empty()))
        .or(absent)
        .map(str::to_owned)
        .unwrap_or_else(|| tr("搜索名称"));
    let search = ui
        .add(
            egui::TextEdit::singleline(&mut state.query)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        )
        .on_hover_text(tr("搜索名称"));
    if search.gained_focus() || search.changed() {
        state.open = true;
    }
    let escape = state.open
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let mut selection = None;
    let mut close = escape;
    if state.open && !escape {
        let mut result_focused = false;
        let results = egui::Frame::group(ui.style()).show(ui, |ui| {
            if let Some(label) = absent {
                let clear = ui.selectable_label(selected.is_none(), label);
                result_focused |= clear.has_focus();
                if clear.clicked() {
                    selection = Some(None);
                }
            }
            let query = state.query.to_lowercase();
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for (id, name) in options {
                        if query.is_empty()
                            || name.to_lowercase().contains(&query)
                            || id.to_lowercase().contains(&query)
                        {
                            let result = ui.selectable_label(selected == Some(id.as_str()), name);
                            result_focused |= result.has_focus();
                            if result.clicked() {
                                selection = Some(Some(id.clone()));
                            }
                        }
                    }
                });
            let manual = ui.collapsing(tr("手动输入"), |ui| {
                result_focused |= ui
                    .add(
                        egui::TextEdit::singleline(&mut state.manual_id)
                            .hint_text(tr("内部 ID / 自定义引用"))
                            .desired_width(f32::INFINITY),
                    )
                    .has_focus();
                let apply = ui.button(tr("使用此值"));
                result_focused |= apply.has_focus();
                if apply.clicked() {
                    selection = Some(if state.manual_id.is_empty() && absent.is_some() {
                        None
                    } else {
                        Some(state.manual_id.clone())
                    });
                }
            });
            result_focused |= manual.header_response.has_focus();
        });
        close |= (search.clicked_elsewhere() || (search.lost_focus() && !result_focused))
            && !results.response.contains_pointer();
    }
    if close || selection.is_some() {
        state.open = false;
        state.query.clear();
        state.manual_id = selected.unwrap_or("").to_owned();
        search.surrender_focus();
    }
    ui.data_mut(|data| data.insert_temp(state_id, state));
    selection
}

fn is_selector(kind: &str) -> bool {
    kind.starts_with("enum:")
        || matches!(kind, "character_group" | "character_appearance")
        || matches!(
            kind,
            "node_ref"
                | "official_picture"
                | "story_ref"
                | "facing"
                | "branch_source"
                | "portrait"
                | "mode"
                | "camera"
                | "flag_ref"
                | "battle_character"
                | "battle_faction"
                | "item"
                | "voice"
                | "character"
                | "affinity_character"
                | "affinity_optional"
                | "position"
                | "view"
                | "music"
                | "sound_name"
                | "stat"
                | "talent"
                | "combat_skill"
                | "game_flag"
                | "free_position"
                | "battle_skill"
                | "effect"
                | "menu_dialog"
                | "death_id"
                | "user_image"
                | "ending_image"
                | "intro_image"
        )
}
fn absent_label(node: &Value, key: &str, kind: &str) -> String {
    tr(match (node["type"].as_str().unwrap_or(""), key, kind) {
        ("say", "mode", _) => "对话（默认）",
        (_, _, "portrait") => "默认表情",
        (_, _, "voice") => "无语音",
        ("say", "character", _)
            if !matches!(node["mode"].as_str(), Some("narrative" | "center")) =>
        {
            "选择人物"
        }
        (_, _, "character" | "character_group") => "无",
        (_, _, "node_ref") => "继续下一步",
        (_, _, "story_ref" | "user_image" | "ending_image" | "intro_image") => "无",
        _ => "默认",
    })
}
/// Number distinct IDs sharing a visible label, without changing values or order.
/// Call on the complete list before filtering. Sorting IDs makes numbering
/// independent of search, selection, or the order in which entries were loaded.
pub fn number_duplicate_labels(options: &mut [(String, String)]) {
    use std::collections::{BTreeMap, BTreeSet};
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (id, name) in options.iter() {
        groups.entry(name.clone()).or_default().insert(id.clone());
    }
    let mut used: BTreeSet<String> = groups.keys().cloned().collect();
    let mut labels = BTreeMap::new();
    for (name, ids) in groups {
        if ids.len() < 2 {
            continue;
        }
        let mut number = 1;
        for id in ids {
            loop {
                let label = format!("{name} {number}");
                number += 1;
                if used.insert(label.clone()) {
                    labels.insert((id, name.clone()), label);
                    break;
                }
            }
        }
    }
    for (id, name) in options {
        if let Some(label) = labels.get(&(id.clone(), name.clone())) {
            *name = label.clone();
        }
    }
}

fn field_visible(kind: &str, node: &Value, key: &str) -> bool {
    if kind == "say" && key == "text_align" {
        return matches!(node["mode"].as_str(), Some("narrative" | "center"));
    }
    if kind == "cg" {
        return match key {
            "key" => node["action"] == "show",
            "key2" => {
                node["action"] == "show" && matches!(node["kind"].as_str(), Some("map" | "family"))
            }
            "n1" | "n2" => node["action"] == "show" && node["kind"] == "family",
            "color" => node["action"] == "show" && node["kind"] == "big",
            _ => true,
        };
    }
    if key == "appearance" {
        return false;
    } // rendered directly after the grouped character selector
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

#[test]
fn portrait_and_narration_controls_follow_the_selected_mode() {
    let node = json!({"type":"portrait"});
    assert_eq!(
        field_requirement(&node, "portrait", false),
        Requirement::Required
    );
    assert_eq!(
        field_requirement(&node, "character", false),
        Requirement::Required
    );
    for mode in ["narrative", "center", "character", "think"] {
        assert_eq!(
            field_visible("say", &json!({"mode":mode}), "text_align"),
            matches!(mode, "narrative" | "center")
        );
    }
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
fn array_editor(
    catalog: &Catalog,
    ui: &mut Ui,
    value: &mut Value,
    kind: &str,
    node_ids: &[String],
) -> bool {
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
                if typed_row(catalog, ui, &mut array[i], kind)
                    .map(|c| {
                        changed |= c;
                    })
                    .is_some()
                {
                    return;
                }
                if let Some(obj) = array[i].as_object_mut() {
                    for (key, val) in obj.iter_mut() {
                        ui.push_id(key, |ui| {
                            if matches!(
                                key.as_str(),
                                "goto" | "win" | "lose" | "success" | "failure"
                            ) {
                                ui.label(format!("{key} *"));
                                let mut s = val.as_str().unwrap_or("").to_owned();
                                let mut edited = false;
                                egui::ComboBox::from_id_salt("target")
                                    .truncate()
                                    .width(ui.available_width())
                                    .selected_text(&s)
                                    .show_ui(ui, |ui| {
                                        for n in node_ids {
                                            if ui.selectable_value(&mut s, n.clone(), n).changed() {
                                                edited = true;
                                            }
                                        }
                                        ui.collapsing(tr("手动输入"), |ui| {
                                            edited |= ui.text_edit_singleline(&mut s).changed();
                                        });
                                    });
                                if edited {
                                    *val = s.into();
                                    changed = true;
                                }
                            } else {
                                let required = matches!(
                                    (kind, key.as_str()),
                                    ("options", "text")
                                        | ("cases", "value")
                                        | ("dice_bands", "text" | "upper")
                                        | ("vars", "name" | "value")
                                );
                                let label = if required {
                                    format!("{key} *")
                                } else {
                                    key.clone()
                                };
                                changed |= value_editor(ui, &label, val, 1);
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
        let mut row = row_template(kind);
        if kind == "official_characters" {
            if let Some((id, _)) = catalog
                .options("battle_character", &Value::Null, &[], &[])
                .into_iter()
                .find(|(id, _)| !array.iter().any(|v| v == id))
            {
                row = json!(id);
            } else {
                return changed;
            }
        }
        if kind == "combat_talents" {
            row["key"] = catalog.data["combat_talents"][0]["id"].clone();
        }
        if kind == "battle_faction_list" {
            row["id"] = json!("000");
        }
        array.push(row);
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
    fn important_characters_group_appearances_and_filter_expressions_by_real_id() {
        let catalog = Catalog::new();
        let characters = catalog.options("character_group", &Value::Null, &[], &[]);
        assert_eq!(characters.iter().filter(|(id, _)| id == "girl5").count(), 1);
        assert!(!characters.iter().any(|(id, _)| id == "girl5_9"));
        let mut node = json!({"id":"n1","type":"show","character":"girl5","position":"M","portrait":"special5"});
        assert!(catalog
            .appearance_options(&node)
            .iter()
            .any(|(id, label)| id == "girl5_9" && label == "入魔"));
        catalog.select_appearance(&mut node, "girl5_9");
        assert_eq!(node["character"], "girl5_9");
        assert_eq!(node["portrait"], "normal");
        assert!(node.get("appearance").is_none());
        let portraits = catalog.options("portrait", &node, &[], &[]);
        assert!(portraits.iter().any(|(id, _)| id == "special1"));
        assert!(!portraits.iter().any(|(id, _)| id == "special5"));
        let story = json!({"id":"main","start":"n1","nodes":[node,{"id":"end","type":"end"}]});
        lom_core::validate::validate_story(&story).unwrap();
        let lua = lom_core::codegen::compile_story(&story, None, None, None).unwrap();
        assert!(lua.contains("girl5_9"));
    }
    #[test]
    fn appearance_selection_keeps_valid_expressions_and_legacy_documents_unchanged_on_draw() {
        let catalog = Catalog::new();
        let mut node = json!({"id":"n1","type":"say","character":"girl5_9","portrait":"special1","text":"..."});
        let original = node.clone();
        let context = egui::Context::default();
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        assert!(!changed);
        assert_eq!(node, original);
        catalog.select_appearance(&mut node, "girl5");
        assert_eq!(node["portrait"], "special1");
        catalog.select_appearance(&mut node, "girl5_9");
        assert_eq!(node["portrait"], "special1");
    }
    #[test]
    fn player_appearance_choices_keep_existing_host_contract_and_parallel_restrictions() {
        let catalog = Catalog::new();
        let mut node = json!({"id":"n1","type":"show","character":"player","position":"M","portrait":"normal"});
        assert!(catalog
            .appearance_options(&node)
            .iter()
            .any(|(id, _)| id == "@beautified"));
        catalog.select_appearance(&mut node, "@beautified");
        assert_eq!(node["character"], "player");
        assert_eq!(node["appearance"], "beautified");
        catalog.select_appearance(&mut node, "player");
        assert_eq!(node["appearance"], "game");
        node.as_object_mut().unwrap().remove("id");
        catalog.select_appearance(&mut node, "player");
        assert!(node.get("appearance").is_none());
        assert!(!catalog
            .appearance_options(&node)
            .iter()
            .any(|(id, _)| id.starts_with('@')));
    }
    #[test]
    fn clicking_appearance_then_expression_writes_the_selected_official_variant() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        let context = egui::Context::default();
        let mut node = json!({"id":"n1","type":"show","character":"girl5","position":"M","portrait":"special5"});
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        let events = click(&output, Role::ComboBox, &tr("普通"));
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        let events = click(&output, Role::Button, &tr("入魔"));
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(changed);
        assert_eq!(node["character"], "girl5_9");
        assert_eq!(node["portrait"], "normal");
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "portrait",
            "portrait",
            vec![],
        );
        let events = click(
            &output,
            Role::ComboBox,
            &crate::i18n::term("portraits", "normal", "normal"),
        );
        let _ = field_frame(
            &catalog, &context, &mut node, "portrait", "portrait", events,
        );
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "portrait",
            "portrait",
            vec![],
        );
        let special = catalog
            .options("portrait", &node, &[], &[])
            .into_iter()
            .find(|(id, _)| id == "special1")
            .unwrap()
            .1;
        let events = click(&output, Role::Button, &special);
        let (_, changed) = field_frame(
            &catalog, &context, &mut node, "portrait", "portrait", events,
        );
        assert!(changed);
        assert_eq!(node["portrait"], "special1");
    }
    #[test]
    fn numbered_selector_writes_original_id_and_keeps_number_after_selection() {
        use egui::accesskit::Role;
        let mut catalog = Catalog::new();
        catalog.data["characters"] = json!([
            {"id":"one", "name":"同名人物"},
            {"id":"two", "name":"同名人物"}
        ]);
        let context = egui::Context::default();
        let mut node = json!({"id":"n1", "type":"say", "character":"one"});
        let (output, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        assert!(!changed);
        let events = search_character(&output, "two");
        let (output, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(!changed);
        assert_eq!(node["character"], "one", "Searching must not commit an ID");
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(!nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Button && node.label() == Some("同名人物 1")));
        let events = click(&output, Role::Button, "同名人物 2");
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(changed);
        assert_eq!(node["character"], "two");
        let (output, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        assert!(!changed);
        assert!(!output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::ComboBox));
        assert_eq!(node, json!({"id":"n1", "type":"say", "character":"two"}));
    }
    fn field_frame(
        catalog: &Catalog,
        context: &egui::Context,
        node: &mut Value,
        key: &str,
        kind: &str,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, bool) {
        context.enable_accesskit();
        let mut changed = false;
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    changed |= catalog.field(ui, node, key, key, kind, true, &[], &[]);
                });
            },
        );
        (output, changed)
    }
    fn click(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        text: &str,
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
                (node.role() == role && (node.label() == Some(text) || node.value() == Some(text)))
                    .then_some(*id)
            })
            .unwrap_or_else(|| panic!("No {role:?} {text:?} in accessibility output: {nodes:?}"));
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target,
                data: None,
            },
        )]
    }
    fn search_character(output: &egui::FullOutput, query: &str) -> Vec<egui::Event> {
        let target = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::TextInput)
            .unwrap()
            .0;
        vec![
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Focus,
                target,
                data: None,
            }),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ..Default::default()
                },
            },
            egui::Event::Text(query.into()),
        ]
    }
    #[test]
    fn clicking_background_search_keeps_choices_open_until_selection() {
        use egui::accesskit::Role;
        fn pointer_click(
            output: &egui::FullOutput,
            role: Role,
            label: Option<&str>,
        ) -> Vec<egui::Event> {
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == role
                        && label.is_none_or(|text| {
                            node.label() == Some(text) || node.value() == Some(text)
                        })
                })
                .and_then(|(_, node)| node.bounds())
                .expect("click target is visible");
            let pos = egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            );
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        }
        let mut catalog = Catalog::new();
        catalog.data["views"] = json!([
            {"id":"fixture_day","name":"Daytime background"},
            {"id":"fixture_night","name":"Night background"}
        ]);
        let context = egui::Context::default();
        let mut node = json!({"id":"scene1","type":"scene","view":"fixture_day"});
        let original = node.clone();
        let (output, _) = field_frame(&catalog, &context, &mut node, "view", "view", vec![]);
        let events = pointer_click(&output, Role::ComboBox, None);
        let _ = field_frame(&catalog, &context, &mut node, "view", "view", events);
        let (output, _) = field_frame(&catalog, &context, &mut node, "view", "view", vec![]);
        let events = pointer_click(&output, Role::TextInput, None);
        let (_, changed) = field_frame(&catalog, &context, &mut node, "view", "view", events);
        assert!(!changed);
        let _ = field_frame(
            &catalog,
            &context,
            &mut node,
            "view",
            "view",
            vec![egui::Event::Text("Night".into())],
        );
        let (output, _) = field_frame(&catalog, &context, &mut node, "view", "view", vec![]);
        assert_eq!(node, original, "searching must not commit a background");
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(
            nodes
                .iter()
                .any(|(_, node)| node.role() == Role::Button
                    && node.label() == Some("Night background")),
            "search popup closed before choosing a background"
        );
        assert!(!nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Button
                && node.label() == Some("Daytime background")));
        let events = pointer_click(&output, Role::Button, Some("Night background"));
        let (_, changed) = field_frame(&catalog, &context, &mut node, "view", "view", events);
        assert!(changed);
        assert_eq!(node["view"], "fixture_night");
        let (output, _) = field_frame(&catalog, &context, &mut node, "view", "view", vec![]);
        assert!(!output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::TextInput));
    }

    #[test]
    fn content_name_and_unknown_text_fields_remain_directly_editable() {
        use egui::accesskit::Role;
        for kind in ["str", "line", "future_text_field"] {
            let catalog = Catalog::new();
            let context = egui::Context::default();
            let mut node = json!({"name":"原标题"});
            let (output, changed) =
                field_frame(&catalog, &context, &mut node, "name", kind, vec![]);
            assert!(!changed);
            let nodes = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes;
            assert!(!nodes.iter().any(|(_, node)| node.role() == Role::ComboBox));
            let text_input = nodes
                .iter()
                .find_map(|(id, node)| {
                    (node.role() == Role::TextInput && node.value() == Some("原标题"))
                        .then_some(*id)
                })
                .expect("Content names must expose a direct text input");
            let events = vec![
                egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Focus,
                    target: text_input,
                    data: None,
                }),
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Text("新标题".into()),
            ];
            let (_, changed) = field_frame(&catalog, &context, &mut node, "name", kind, events);
            assert!(changed);
            assert_eq!(node["name"], "新标题");
        }
    }
    #[test]
    fn character_queries_cancel_without_modifying_any_character_field() {
        let mut catalog = Catalog::new();
        catalog.data["characters"] = json!([
            {"id":"one", "name":"人物甲"}, {"id":"two", "name":"人物乙"}
        ]);
        catalog.data["affinity_characters"] = catalog.data["characters"].clone();
        catalog.schema["VERIFIED_BATTLE_CHARACTER_IDS"] = json!(["one", "two"]);
        for kind in [
            "character",
            "affinity_character",
            "affinity_optional",
            "battle_character",
        ] {
            let context = egui::Context::default();
            let mut node = json!({"id":"n1", "type":"say", "character":"one", "text":"原台词"});
            let before = node.clone();
            let (output, _) = field_frame(&catalog, &context, &mut node, "character", kind, vec![]);
            let events = search_character(&output, "人物乙");
            let (_, changed) =
                field_frame(&catalog, &context, &mut node, "character", kind, events);
            assert!(!changed);
            assert_eq!(node, before);
            let (output, changed) = field_frame(
                &catalog,
                &context,
                &mut node,
                "character",
                kind,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(!changed);
            assert_eq!(node, before);
            let events = search_character(&output, "two");
            let (_, changed) =
                field_frame(&catalog, &context, &mut node, "character", kind, events);
            assert!(!changed);
            let (_, changed) = field_frame(
                &catalog,
                &context,
                &mut node,
                "character",
                kind,
                vec![
                    egui::Event::PointerMoved(egui::pos2(690.0, 590.0)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(690.0, 590.0),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                    egui::Event::PointerButton {
                        pos: egui::pos2(690.0, 590.0),
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                ],
            );
            assert!(!changed);
            assert_eq!(node, before);
        }
    }
    #[test]
    fn character_search_selects_user_content_and_optional_clear() {
        use egui::accesskit::Role;
        let mut catalog = Catalog::new();
        catalog.data["characters"] = json!([
            {"id":"one", "name":"初始人物"},
            {"id":"user:demo.actor", "name":"用户林灯"}
        ]);
        let context = egui::Context::default();
        let mut node = json!({"id":"n1", "type":"say", "character":"one", "text":"原台词"});
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        let events = search_character(&output, "林灯");
        let (output, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(!changed);
        let events = click(&output, Role::Button, "用户林灯");
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(changed);
        assert_eq!(node["character"], "user:demo.actor");
        assert_eq!(node["text"], "原台词");
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        let events = search_character(&output, "");
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        let events = click(&output, Role::Button, "选择人物");
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(changed);
        assert!(node.get("character").is_none());
        assert_eq!(node["text"], "原台词");
    }
    #[test]
    fn character_manual_id_requires_explicit_apply() {
        use egui::accesskit::{Action, ActionRequest, Role};
        let catalog = Catalog::new();
        let context = egui::Context::default();
        let mut node = json!({"id":"n1", "type":"say", "character":"player", "text":"原台词"});
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        let events = search_character(&output, "unknown");
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        let events = click(&output, Role::Button, "手动输入");
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        let target = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::TextInput && node.value() == Some("player"))
            .unwrap()
            .0;
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![
                egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Focus,
                    target,
                    data: None,
                }),
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Text("user:custom.actor".into()),
            ],
        );
        assert!(!changed);
        assert_eq!(node["character"], "player");
        let (output, _) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            vec![],
        );
        let events = click(&output, Role::Button, "使用此值");
        let (_, changed) = field_frame(
            &catalog,
            &context,
            &mut node,
            "character",
            "character",
            events,
        );
        assert!(changed);
        assert_eq!(node["character"], "user:custom.actor");
        assert_eq!(node["text"], "原台词");
    }
    #[test]
    fn selector_clicks_distinguish_explicit_default_from_absent_value() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        let context = egui::Context::default();
        let mut node = json!({"id":"n1", "type":"say", "text":"test", "character":"player", "mode":"character"});
        let (output, changed) = field_frame(&catalog, &context, &mut node, "mode", "mode", vec![]);
        assert!(!changed);
        let events = click(&output, Role::ComboBox, "对话");
        let (output, changed) = field_frame(&catalog, &context, &mut node, "mode", "mode", events);
        assert!(!changed, "Opening the selector must not write a value");
        let events = click(&output, Role::Button, "对话（默认）");
        let (_, changed) = field_frame(&catalog, &context, &mut node, "mode", "mode", events);
        assert!(changed);
        assert!(node.get("mode").is_none(), "Default must remove the key");
        let (output, changed) = field_frame(&catalog, &context, &mut node, "mode", "mode", vec![]);
        assert!(!changed);
        let events = click(&output, Role::ComboBox, "对话（默认）");
        let (output, _) = field_frame(&catalog, &context, &mut node, "mode", "mode", events);
        let events = click(&output, Role::Button, "对话");
        let (_, changed) = field_frame(&catalog, &context, &mut node, "mode", "mode", events);
        assert!(
            changed,
            "Selecting the effective default must still store the explicit choice"
        );
        assert_eq!(node["mode"], "character");
    }
    #[test]
    fn optional_boolean_can_be_explicitly_disabled_and_restored_to_default() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        let context = egui::Context::default();
        let mut node = json!({"type":"stat", "key":"mental", "delta":1});
        let (output, changed) =
            field_frame(&catalog, &context, &mut node, "waitDisplay", "bool", vec![]);
        assert!(!changed);
        let events = click(&output, Role::ComboBox, "默认");
        let (output, _) = field_frame(&catalog, &context, &mut node, "waitDisplay", "bool", events);
        let events = click(&output, Role::Button, "关闭");
        let (_, changed) =
            field_frame(&catalog, &context, &mut node, "waitDisplay", "bool", events);
        assert!(changed);
        assert_eq!(node["waitDisplay"], false);
        let (output, _) = field_frame(&catalog, &context, &mut node, "waitDisplay", "bool", vec![]);
        let events = click(&output, Role::ComboBox, "关闭");
        let (output, _) = field_frame(&catalog, &context, &mut node, "waitDisplay", "bool", events);
        let events = click(&output, Role::Button, "默认");
        let (_, changed) =
            field_frame(&catalog, &context, &mut node, "waitDisplay", "bool", events);
        assert!(changed);
        assert!(node.get("waitDisplay").is_none());
    }
    #[test]
    fn empty_voice_catalog_preserves_unknown_voice_until_clear_and_keeps_valid_story() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        let context = egui::Context::default();
        let mut node = json!({"id":"n1", "type":"say", "text":"test", "character":"player", "voice":"user:old.voice"});
        let before = node.clone();
        let (output, changed) =
            field_frame(&catalog, &context, &mut node, "voice", "voice", vec![]);
        assert!(!changed);
        assert_eq!(node, before);
        let events = click(&output, Role::ComboBox, "自定义");
        let (output, changed) =
            field_frame(&catalog, &context, &mut node, "voice", "voice", events);
        assert!(!changed);
        assert_eq!(node, before);
        assert!(
            !output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.role() == Role::TextInput),
            "Internal IDs must remain collapsed"
        );
        let events = click(&output, Role::Button, "无语音");
        let (_, changed) = field_frame(&catalog, &context, &mut node, "voice", "voice", events);
        assert!(changed);
        assert!(node.get("voice").is_none());
        let (output, changed) =
            field_frame(&catalog, &context, &mut node, "voice", "voice", vec![]);
        assert!(!changed);
        let _ = click(&output, Role::ComboBox, "无语音");
        let story = json!({"story_schema":2, "id":"main", "start":"n1", "nodes":[node, {"id":"end", "type":"end"}]});
        lom_core::validate::validate_story(&story).unwrap();
    }
    #[test]
    fn optional_numbers_and_arrays_can_be_reset_without_reinstating_template_values() {
        use egui::accesskit::Role;
        for (node_type, key, kind, initial) in [
            ("background", "fade", "float", json!(0.5)),
            (
                "block",
                "vars",
                "vars",
                json!([{"name":"count", "value":"7"}]),
            ),
        ] {
            let catalog = Catalog::new();
            let context = egui::Context::default();
            let mut node = json!({"type":node_type, key:initial});
            let (output, changed) = field_frame(&catalog, &context, &mut node, key, kind, vec![]);
            assert!(!changed);
            let events = click(&output, Role::Button, "重置");
            let (_, changed) = field_frame(&catalog, &context, &mut node, key, kind, events);
            assert!(changed);
            assert!(node.get(key).is_none());
            let (_, changed) = field_frame(&catalog, &context, &mut node, key, kind, vec![]);
            assert!(!changed);
            assert!(node.get(key).is_none());
        }
    }
    #[test]
    fn changing_show_character_clears_only_inapplicable_player_appearance() {
        use egui::accesskit::Role;
        let mut catalog = Catalog::new();
        catalog.data["characters"] = json!([
            {"id":"player", "name":"赵活"},
            {"id":"sister1", "name":"小师妹"}
        ]);
        let context = egui::Context::default();
        let mut node = json!({"id":"n1", "type":"show", "character":"player", "position":"M", "appearance":"beautified"});
        assert!(!field_visible("show", &node, "appearance"));
        for target in ["sister1", "player"] {
            let names = catalog.options("character", &node, &[], &[]);
            let target_name = &names.iter().find(|(id, _)| id == target).unwrap().1;
            let (output, changed) = field_frame(
                &catalog,
                &context,
                &mut node,
                "character",
                "character",
                vec![],
            );
            assert!(!changed);
            let events = search_character(&output, target);
            let (output, _) = field_frame(
                &catalog,
                &context,
                &mut node,
                "character",
                "character",
                events,
            );
            let events = click(&output, Role::Button, target_name);
            let (_, changed) = field_frame(
                &catalog,
                &context,
                &mut node,
                "character",
                "character",
                events,
            );
            assert!(changed);
            assert_eq!(node["character"], target);
            if target == "player" {
                assert_eq!(node["appearance"], "game");
            } else {
                assert!(node.get("appearance").is_none());
            }
            assert!(!field_visible("show", &node, "appearance"));
            let story = json!({"id":"main", "start":"n1", "nodes":[node.clone(), {"id":"end", "type":"end"}]});
            lom_core::validate::validate_story(&story).unwrap();
        }
    }
    #[test]
    fn invalid_appearance_json_is_preserved_until_an_author_edits_it() {
        let catalog = Catalog::new();
        let context = egui::Context::default();
        for mut node in [
            json!({"id":"n1", "type":"show", "character":"sister1", "position":"M", "appearance":"beautified"}),
            json!({"id":"n1", "type":"say", "character":"player", "text":"test", "appearance":"beautified"}),
        ] {
            let before = node.clone();
            let _ = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!catalog.node_form(ui, &mut node, &[], &[]).changed);
                });
            });
            assert_eq!(node, before);
            assert!(!field_visible(
                node["type"].as_str().unwrap(),
                &node,
                "appearance"
            ));
            let story =
                json!({"id":"main", "start":"n1", "nodes":[node, {"id":"end", "type":"end"}]});
            assert!(lom_core::validate::validate_story(&story)
                .unwrap_err()
                .to_string()
                .contains("appearance"));
        }
    }
    #[test]
    fn choice_targets_use_one_selector_and_preserve_invalid_json() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut value = json!([{"text":"甲", "goto":"end1"}, {"text":"乙", "goto":7}]);
        let before = value.clone();
        let frame = |value: &mut Value, events| {
            let mut changed = false;
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed |= array_editor(
                            &catalog,
                            ui,
                            value,
                            "options",
                            &["end1".into(), "end2".into()],
                        );
                    });
                },
            );
            (output, changed)
        };
        let (output, changed) = frame(&mut value, vec![]);
        assert!(!changed);
        assert_eq!(value, before);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert_eq!(
            nodes
                .iter()
                .filter(|(_, node)| node.role() == Role::ComboBox)
                .count(),
            2
        );
        assert!(!nodes
            .iter()
            .any(|(_, node)| node.role() == Role::TextInput && node.value() == Some("end1")));
        let events = click(&output, Role::ComboBox, "end1");
        let (output, changed) = frame(&mut value, events);
        assert!(!changed);
        let events = click(&output, Role::Button, "end2");
        let (_, changed) = frame(&mut value, events);
        assert!(changed);
        assert_eq!(value[0]["goto"], "end2");
        assert_eq!(value[1], before[1]);
    }
    #[test]
    fn every_node_form_renders_without_modifying_untouched_document() {
        let catalog = Catalog::new();
        let context = egui::Context::default();
        for kind in catalog.schema["NODE_SCHEMAS"].as_object().unwrap().keys() {
            let original = catalog.new_node(kind, "n1".into());
            let mut defaults_omitted = original.clone();
            for field in catalog.schema["NODE_SCHEMAS"][kind]["fields"]
                .as_array()
                .unwrap()
            {
                if field[3] == true {
                    defaults_omitted
                        .as_object_mut()
                        .unwrap()
                        .remove(field[0].as_str().unwrap());
                }
            }
            for mut node in [original, defaults_omitted] {
                let before = node.clone();
                let _ = context.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(
                            !catalog
                                .node_form(
                                    ui,
                                    &mut node,
                                    &["n1".into(), "end1".into()],
                                    &["main".into()],
                                )
                                .changed
                        );
                    });
                });
                assert_eq!(node, before, "merely rendering {kind} mutated the project");
            }
        }
    }
}

#[cfg(test)]
mod conditional_tests {
    use super::*;
    #[test]
    fn required_markers_follow_compiler_conditions_and_not_form_defaults() {
        let fixtures: Vec<(Value, &[&str])> = vec![
            (
                json!({"type":"say","text":"test","character":"player"}),
                &["character"],
            ),
            (
                json!({"type":"intro","intro_source":"custom","name":"角色","text":"简介"}),
                &["name", "text"],
            ),
            (
                json!({"type":"intro","intro_source":"official","character":"player"}),
                &["character"],
            ),
            (
                json!({"type":"intro","intro_source":"character","character":"user:test.hero"}),
                &["character"],
            ),
            (
                json!({"type":"background","action":"set","image":"user:test.image"}),
                &["image"],
            ),
            (
                json!({"type":"custom_cg","action":"show","image":"user:test.image"}),
                &["image"],
            ),
            (
                json!({"type":"overlay","action":"show","slot":"main","image":"user:test.image"}),
                &["image"],
            ),
            (
                json!({"type":"cg","action":"show","kind":"family","key":"a","key2":"b","n1":1,"n2":1}),
                &["key", "key2", "n1", "n2"],
            ),
            (
                json!({"type":"time","op":"set","year":1,"month":1,"stage":1}),
                &["year", "month", "stage"],
            ),
            (
                json!({"type":"time","op":"mission","name":"a","year":1,"month":1,"stage":1}),
                &["name", "year", "month", "stage"],
            ),
            (json!({"type":"panel","panel":"cg","key":"a"}), &["key"]),
            (
                json!({"type":"enemy","op":"team","enemy":"a","value":1}),
                &["value"],
            ),
            (
                json!({"type":"battle_skill","op":"set","key":"a"}),
                &["key"],
            ),
            (
                json!({"type":"flag_check","source":"flag_value","flag":"a","op":">=","value":1,"success":"end","failure":"end"}),
                &["op", "value"],
            ),
            (
                json!({"type":"branch","source":"stat","stat":"morale","cases":[{"value":1,"goto":"end"}]}),
                &["stat"],
            ),
            (
                json!({"type":"dice","max":99,"header":"test","bands":[{"upper":49,"text":"失败","goto":"end"},{"text":"成功","goto":"end"}]}),
                &["max", "header", "bands"],
            ),
        ];
        for (mut node, required) in fixtures {
            node["id"] = json!("a");
            let story =
                json!({"id":"main","start":"a","nodes":[node.clone(),{"id":"end","type":"end"}]});
            lom_core::validate::validate_story(&story)
                .unwrap_or_else(|e| panic!("fixture {node}: {e}"));
            for key in required {
                assert_eq!(
                    field_requirement(&node, key, true),
                    Requirement::Required,
                    "{node}: {key}"
                );
                let mut missing = story.clone();
                missing["nodes"][0].as_object_mut().unwrap().remove(*key);
                assert!(
                    lom_core::validate::validate_story(&missing).is_err(),
                    "compiler allowed missing {key}: {node}"
                );
            }
        }
        for (node, keys) in [
            (
                json!({"type":"say","mode":"narrative"}),
                vec!["character", "portrait"],
            ),
            (json!({"type":"say","mode":"center"}), vec!["character"]),
            (
                json!({"type":"intro","intro_source":"character"}),
                vec!["name", "text", "image"],
            ),
            (json!({"type":"overlay","action":"hide"}), vec!["image"]),
            (
                json!({"type":"time","op":"round"}),
                vec!["year", "month", "stage", "name"],
            ),
            (
                json!({"type":"battle_skill","op":"reset"}),
                vec!["key", "level", "active"],
            ),
            (json!({"type":"panel","panel":"martial"}), vec!["key"]),
            (
                json!({"type":"character"}),
                vec!["title", "scale", "art_facing"],
            ),
        ] {
            for key in keys {
                assert_eq!(
                    field_requirement(&node, key, false),
                    Requirement::Optional,
                    "{node}: {key}"
                );
            }
        }
    }
    #[test]
    fn alternative_requirements_and_large_death_ids_are_labeled_as_groups() {
        for id in ["", "900000", "000900000", "999999999999999999999999"] {
            let node = json!({"type":"goto_scene","scene":"End","key":id});
            for key in ["title", "desc", "image"] {
                assert_eq!(field_requirement(&node, key, true), Requirement::OneOf);
            }
        }
        let node = json!({"type":"goto_scene","scene":"GameOver","key":"900000"});
        assert_eq!(field_requirement(&node, "title", true), Requirement::OneOf);
        assert_eq!(
            field_requirement(&node, "image", true),
            Requirement::Optional
        );
        for key in [
            "friend_factions",
            "friend_characters",
            "enemy_factions",
            "enemy_characters",
        ] {
            assert_eq!(
                field_requirement(&json!({"type":"battle"}), key, true),
                Requirement::OneOf
            );
        }
        assert_eq!(
            requirement_label("人物".into(), Requirement::Required),
            "人物 *"
        );
        assert_eq!(
            requirement_label("称号".into(), Requirement::Optional),
            "称号"
        );
    }
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

fn localized_rows(value: &Value, category: &str) -> Vec<(String, String)> {
    rows(value)
        .into_iter()
        .map(|(id, name)| {
            let name = crate::i18n::term(category, &id, &name);
            (id, name)
        })
        .collect()
}
fn pick(
    ui: &mut Ui,
    row: &mut Value,
    key: &str,
    label: &str,
    options: Vec<(String, String)>,
) -> bool {
    let mut selected = row[key].as_str().unwrap_or("").to_owned();
    let before = selected.clone();
    ui.label(format!("{} *", tr(label)));
    egui::ComboBox::from_id_salt(key)
        .truncate()
        .width(ui.available_width())
        .selected_text(
            options
                .iter()
                .find(|(k, _)| k == &selected)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| {
                    tr(if selected.is_empty() {
                        "选择…"
                    } else {
                        "自定义"
                    })
                }),
        )
        .show_ui(ui, |ui| {
            let sid = ui.id().with("filter");
            let mut q = ui.data_mut(|d| d.get_temp::<String>(sid).unwrap_or_default());
            ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("搜索名称")));
            for (id, name) in options {
                if q.is_empty()
                    || format!("{name} {id}")
                        .to_lowercase()
                        .contains(&q.to_lowercase())
                {
                    ui.selectable_value(&mut selected, id.clone(), name);
                }
            }
            ui.collapsing(tr("手动输入"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut selected).hint_text(tr("内部 ID / 自定义引用")),
                );
            });
            ui.data_mut(|d| d.insert_temp(sid, q));
        });
    if before != selected {
        row[key] = json!(selected);
        true
    } else {
        false
    }
}
fn number(ui: &mut Ui, row: &mut Value, key: &str, min: i64, max: i64) -> bool {
    let mut v = row[key].as_i64().unwrap_or(min).clamp(min, max);
    ui.label(requirement_label(
        tr(key),
        if matches!(key, "people" | "amount") {
            Requirement::Required
        } else {
            Requirement::Optional
        },
    ));
    if ui
        .add(egui::DragValue::new(&mut v).range(min..=max))
        .changed()
    {
        row[key] = json!(v);
        true
    } else {
        false
    }
}
fn typed_row(c: &Catalog, ui: &mut Ui, row: &mut Value, kind: &str) -> Option<bool> {
    let mut changed = false;
    if kind == "official_characters" {
        ui.label(format!("{} *", tr("官方角色")));
        if let Some(Some(id)) = character_picker(
            ui,
            row.as_str(),
            &c.options("battle_character", row, &[], &[]),
            None,
        ) {
            if row.as_str() != Some(id.as_str()) {
                *row = id.into();
                changed = true;
            }
        }
        return Some(changed);
    }
    if !row.is_object() {
        return None;
    }
    match kind {
        "combat_talents" => {
            let chosen = pick(
                ui,
                row,
                "key",
                "武学",
                c.options("combat_skill", row, &[], &[]),
            );
            changed |= chosen;
            let max = c.data["combat_talents"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| v["id"] == row["key"])
                .and_then(|v| v["max_level"].as_i64())
                .unwrap_or(1)
                .max(1);
            if chosen {
                row["level"] = json!(row["level"].as_i64().unwrap_or(1).clamp(1, max));
            }
            changed |= number(ui, row, "level", 1, max);
        }
        "battle_faction_list" => {
            changed |= pick(
                ui,
                row,
                "id",
                "阵营",
                c.options("battle_faction", row, &[], &[]),
            );
            changed |= number(ui, row, "people", 1, 10000);
        }
        "reward_entries" | "reward_entries_optional" | "custom_shop_items" => {
            if kind != "custom_shop_items" {
                changed |= pick(
                    ui,
                    row,
                    "kind",
                    "奖励类型",
                    c.options("enum:reward_kind", row, &[], &[]),
                );
            }
            let target = if kind == "custom_shop_items" {
                "item".to_owned()
            } else {
                row["kind"].as_str().unwrap_or("stat").to_owned()
            };
            if target == "item" {
                changed |= pick(
                    ui,
                    row,
                    "category",
                    "物品类型",
                    c.options("enum:item_kind", row, &[], &[]),
                );
            }
            let field = if kind == "custom_shop_items" {
                "item"
            } else {
                "key"
            };
            let typ = match target.as_str() {
                "affinity" => "affinity_character",
                "flag" => "flag_ref",
                s => s,
            };
            changed |= c.field(ui, row, field, "目标", typ, false, &[], &[]);
            if target != "flag" {
                changed |= number(
                    ui,
                    row,
                    if kind == "custom_shop_items" {
                        "count"
                    } else {
                        "amount"
                    },
                    -999999,
                    999999,
                );
            }
            ui.collapsing(tr("其他字段"), |ui| {
                changed |= value_editor(ui, "row", row, 1);
            });
        }
        _ => return None,
    }
    Some(changed)
}

#[cfg(test)]
mod catalog_parity_tests {
    use super::*;
    #[test]
    fn duplicate_labels_are_stable_and_leave_selection_values_unchanged() {
        let mut options: Vec<(String, String)> = vec![
            ("b".into(), "同名".into()),
            ("a".into(), "同名".into()),
            ("c".into(), "同名 1".into()),
            ("d".into(), "唯一".into()),
        ];
        let ids: Vec<_> = options.iter().map(|(id, _)| id.clone()).collect();
        let mut reversed = options.iter().rev().cloned().collect::<Vec<_>>();
        number_duplicate_labels(&mut options);
        number_duplicate_labels(&mut reversed);
        assert_eq!(
            options.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            ids
        );
        assert_eq!(options[0].1, "同名 3");
        assert_eq!(options[1].1, "同名 2");
        assert_eq!(options[2].1, "同名 1");
        assert_eq!(options[3].1, "唯一");
        reversed.reverse();
        assert_eq!(options, reversed);
        let before = options.clone();
        number_duplicate_labels(&mut options);
        assert_eq!(options, before);
        let mut aliases = vec![("a".into(), "同名".into()), ("a".into(), "同名".into())];
        number_duplicate_labels(&mut aliases);
        assert!(aliases.iter().all(|(_, name)| name == "同名"));
    }
    #[test]
    fn character_numbers_agree_across_selectors_and_preserve_user_names() {
        let mut catalog = Catalog::new();
        catalog.sync_assets(&std::collections::BTreeMap::from([
            (
                "assets/user/character/example.first/content.json".into(),
                serde_json::to_vec(
                    &json!({"id":"example.first","name":"My own name","type":"character"}),
                )
                .unwrap(),
            ),
            (
                "assets/user/character/example.second/content.json".into(),
                serde_json::to_vec(
                    &json!({"id":"example.second","name":"My own name","type":"character"}),
                )
                .unwrap(),
            ),
        ]));
        let full = catalog.options("character", &Value::Null, &[], &[]);
        assert_eq!(
            full.iter()
                .map(|(_, name)| name)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            full.len()
        );
        for kind in [
            "affinity_character",
            "affinity_optional",
            "battle_character",
        ] {
            for (id, name) in catalog.options(kind, &Value::Null, &[], &[]) {
                if let Some((_, full_name)) = full.iter().find(|(key, _)| key == &id) {
                    assert_eq!(&name, full_name, "{kind}/{id}");
                }
            }
        }
        assert_eq!(
            full.iter()
                .find(|(id, _)| id == "user:example.first")
                .unwrap()
                .1,
            "My own name 1"
        );
        assert_eq!(
            full.iter()
                .find(|(id, _)| id == "user:example.second")
                .unwrap()
                .1,
            "My own name 2"
        );
        assert_eq!(
            catalog.data["characters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "user:example.first")
                .unwrap()["name"],
            "My own name"
        );
    }
    #[test]
    fn battle_catalog_matches_existing_verified_contract() {
        let c = Catalog::new();
        let chars = c.options("battle_character", &Value::Null, &[], &[]);
        assert_eq!(
            chars.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec![
                "special4",
                "special102",
                "special103",
                "special401",
                "special811"
            ]
        );
        assert!(!c
            .options("battle_faction", &Value::Null, &[], &[])
            .iter()
            .any(|(id, _)| id == "400"));
        assert_eq!(c.options("combat_skill", &Value::Null, &[], &[]).len(), 115);
        assert_eq!(c.options("camera", &Value::Null, &[], &[]).len(), 4);
    }
    #[test]
    fn typed_rows_preserve_values_until_user_edits() {
        let c = Catalog::new();
        let ctx = egui::Context::default();
        for (kind, mut row) in [
            ("combat_talents", json!({"key":"future_skill","level":99})),
            (
                "battle_faction_list",
                json!({"id":"future_faction","people":20000}),
            ),
            ("official_characters", json!("future_character")),
            (
                "reward_entries",
                json!({"kind":"affinity","key":"brother4","amount":2}),
            ),
            (
                "custom_shop_items",
                json!({"category":"misc","item":"1","count":2,"cost":5}),
            ),
        ] {
            let before = row.clone();
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(typed_row(&c, ui, &mut row, kind), Some(false));
                });
            });
            assert_eq!(row, before);
        }
    }
}

#[cfg(test)]
mod background_catalog_tests {
    use super::*;

    #[test]
    fn every_official_preview_background_is_selectable_in_scene_forms() {
        let catalog = Catalog::new();
        let choices = catalog.options("view", &serde_json::json!({"type":"scene"}), &[], &[]);
        let previews: Value =
            serde_json::from_str(include_str!("../../../data/preview_map.json")).unwrap();
        let missing: Vec<_> = previews["views"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|id| !choices.iter().any(|(candidate, _)| candidate == *id))
            .collect();
        assert!(
            missing.is_empty(),
            "preview backgrounds missing from scene selector: {missing:?}"
        );
        for id in ["section_001_center_night", "section_001_center2_night"] {
            let (_, name) = choices
                .iter()
                .find(|(candidate, _)| candidate == id)
                .unwrap();
            assert_ne!(name, id);
            let story = serde_json::json!({"id":"background_check","story_schema":2,"start":"scene1","nodes":[
                {"id":"scene1","type":"scene","view":id}, {"id":"end1","type":"end"}
            ]});
            let lua = lom_core::codegen::compile_story(&story, None, None, None).unwrap();
            assert!(lua.contains(id));
            assert_eq!(
                catalog.data["views"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| row["id"] == id)
                    .count(),
                1
            );
        }
    }
}
