use crate::preview_layout::{Change as LayoutChange, Target as LayoutTarget};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, TextureHandle, Vec2};
use lom_core::project::Project;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
};

#[path = "preview_effects.rs"]
pub(crate) mod effects;
pub(crate) use effects::Library as EffectLibrary;
#[path = "preview_filters.rs"]
mod filters;
#[path = "flow_graph.rs"]
mod flow_graph;
#[path = "preview_handles.rs"]
mod handles;
use handles::{opaque_bounds, LayoutClipboard, LayoutCommand, LayoutDrag, LayoutObject};

pub fn edges(story: &Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (index, node) in story["nodes"].as_array().into_iter().flatten().enumerate() {
        let id = text(node, "id");
        for target in lom_core::analysis::successors(story, index) {
            let mut labels = Vec::new();
            for (key, label) in [
                ("goto", "跳转"),
                ("win", "胜利"),
                ("lose", "失败"),
                ("success", "成功"),
                ("failure", "失败"),
            ] {
                if node[key].as_str() == Some(&target) {
                    labels.push(if node["type"] == "native_branch" {
                        match key {
                            "success" => "条件成立",
                            "failure" => "条件不成立",
                            _ => label,
                        }
                        .to_owned()
                    } else {
                        label.to_owned()
                    });
                }
            }
            for key in ["options", "cases", "bands"] {
                for (i, row) in node[key].as_array().into_iter().flatten().enumerate() {
                    for jump in ["goto", "goto_大成功", "goto_成功", "goto_失败"] {
                        if row[jump].as_str() == Some(&target) {
                            let mut label = match key {
                                "cases" => {
                                    let source = node["source"].as_str().unwrap_or("mod");
                                    match (source, row["value"].as_i64()) {
                                        ("mod" | "condition", Some(1)) => "条件成立（1）".into(),
                                        ("mod" | "condition", Some(2)) => "条件不成立（2）".into(),
                                        _ => format!(
                                            "条件 {} {}",
                                            if source == "game" {
                                                "="
                                            } else {
                                                row["op"].as_str().unwrap_or(">=")
                                            },
                                            row["value"]
                                        ),
                                    }
                                }
                                _ => row["text"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| {
                                        format!(
                                            "{} {}",
                                            if key == "bands" { "区间" } else { "选项" },
                                            i + 1
                                        )
                                    }),
                            };
                            if let Some(outcome) = jump.strip_prefix("goto_") {
                                label.push_str(&format!(" · {outcome}"));
                            }
                            if !labels.contains(&label) {
                                labels.push(label);
                            }
                        }
                    }
                }
            }
            out.push((
                id.to_owned(),
                target,
                if labels.is_empty() {
                    "下一步".into()
                } else {
                    labels.join(" / ")
                },
            ));
        }
    }
    out
}
pub fn route(story: &Value, target: &str) -> Option<Vec<String>> {
    let ns = story["nodes"].as_array()?;
    let start = story["start"]
        .as_str()
        .or_else(|| ns.first()?.get("id")?.as_str())?;
    let mut links: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (index, node) in ns.iter().enumerate() {
        links
            .entry(text(node, "id"))
            .or_default()
            .extend(lom_core::analysis::successors(story, index));
    }
    let mut queue = VecDeque::from([start.to_owned()]);
    let mut prev: BTreeMap<String, Option<String>> = BTreeMap::from([(start.into(), None)]);
    while let Some(id) = queue.pop_front() {
        if id == target {
            let mut out = vec![id.clone()];
            let mut cur = id;
            while let Some(Some(p)) = prev.get(&cur) {
                out.push(p.clone());
                cur = p.clone();
            }
            out.reverse();
            return Some(out);
        }
        for to in links.get(id.as_str()).into_iter().flatten() {
            if !prev.contains_key(to) {
                prev.insert(to.clone(), Some(id.clone()));
                queue.push_back(to.clone());
            }
        }
    }
    None
}
pub fn simulate(story: &Value, target: &str) -> Value {
    simulate_until(story, target, true)
}
fn portrait_preview_node(story: &Value, selected: usize) -> Value {
    let mut node = story["nodes"][selected].clone();
    let cid = text(&node, "character").to_owned();
    if !cid.is_empty() && node["portrait"].as_str().is_none_or(str::is_empty) {
        let state = simulate(story, text(&node, "id"));
        if let Some(actor) = state["actors"].get(&cid) {
            node["portrait"] = actor["portrait"].clone();
            node["appearance"] = actor["appearance"].clone();
        }
    }
    node
}

// Editing focus changes only the frame being painted, never the simulated stage
// used by export or the prelude for starting a game playtest at this step.
fn highlight_editing_character(
    state: &mut Value,
    node: &Value,
    action: Option<usize>,
    layout: Option<&LayoutTarget>,
) {
    let character = match layout {
        Some(LayoutTarget::Character(id) | LayoutTarget::Mood(id)) => Some(id.as_str()),
        _ => {
            let actions = lom_core::parallel::actions(node);
            let target = action
                .and_then(|index| actions.get(index))
                .or_else(|| actions.iter().find(|a| !text(a, "character").is_empty()));
            target
                .filter(|a| {
                    matches!(
                        text(a, "type"),
                        "show"
                            | "portrait"
                            | "move"
                            | "face"
                            | "rotate"
                            | "offset"
                            | "shock"
                            | "focus"
                            | "mood"
                    )
                })
                .map(|a| text(a, "character"))
        }
    };
    if let Some(actor) = character
        .filter(|id| !id.is_empty())
        .and_then(|id| state["actors"].get_mut(id))
    {
        actor["dimmed"] = json!(false);
    }
}
fn simulate_until(story: &Value, target: &str, include_target: bool) -> Value {
    let mut state = json!({"actors":{},"dimming":{},"overlays":{},"official_pictures":{},"background":null,"view":null,"custom_cg":null,"dialog":null,"choice":null,"blackout":{"amount":0.0,"dir":"lr"},"reached":false,"steps":0});
    state["hidden_actors"] = json!({});
    state["dimming_modes"] = json!({});
    state["speaker"] = Value::Null;
    state["mood_enabled"] = json!(story["mood"].as_bool().unwrap_or(false));
    state["mood_overrides"] = json!({});
    state["camera_filters"] = json!({});
    state["camera_filter_exclusions"] = json!({});
    state["render_priorities"] = json!([]);
    let Some(ns) = story["nodes"].as_array() else {
        return state;
    };
    let path = route(story, target);
    state["reached"] = path.is_some().into();
    let walk = path.unwrap_or_else(|| vec![target.into()]);
    let mut nodes_by_id = BTreeMap::new();
    for node in ns {
        if let Some(id) = node["id"].as_str() {
            nodes_by_id.entry(id).or_insert(node);
        }
    }
    for id in &walk {
        if !include_target && id == target {
            break;
        }
        if let Some(n) = nodes_by_id.get(id.as_str()) {
            apply(&mut state, n);
        }
    }
    state["steps"] = walk.len().into();
    state["effects"] = effects::simulate(story, target, 0.0, include_target, None);
    state
}
pub fn build_playtest_prelude(story: &Value, target: &str) -> Vec<Value> {
    let state = simulate_until(story, target, false);
    let mut nodes = Vec::new();
    nodes.extend(
        state["render_priorities"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned()
            .map(|mut node| {
                node.as_object_mut().unwrap().remove("wait_after");
                node
            }),
    );
    for (character, title) in state["character_titles"].as_object().into_iter().flatten() {
        nodes.push(json!({"type":"character_title","character":character,
            "action":if title == "" { "hide" } else { "set" },"title":title}));
    }
    for (character, name) in state["character_names"].as_object().into_iter().flatten() {
        nodes.push(json!({"type":"character_title","character":character,"action":"keep","name_action":"set","name":name}));
    }
    for (name, active) in state["camera_filters"].as_object().into_iter().flatten() {
        let mut node = json!({"type":"camera","name":name,"active":active});
        if let Some(ids) = state["camera_filter_exclusions"].get(name) {
            node["exclude_characters"] = ids.clone();
        }
        nodes.push(node);
    }
    if lom_core::parallel::story_actions(story).any(|(_, n)| n["type"] == "mood") {
        nodes.push(json!({"type":"mood","enabled":state["mood_enabled"]}));
    }
    if number(&state["blackout"], "amount", 0.0) >= 1.0 {
        nodes.push(json!({"type":"transition","phase":"in","dir":state["blackout"]["dir"]}));
    }
    if let Some(view) = state["view"]
        .as_str()
        .filter(|v| !v.is_empty() && *v != "out")
    {
        nodes.push(json!({"type":"scene","view":view}));
    }
    if let Some(background) = state["background"].as_str().filter(|v| !v.is_empty()) {
        nodes.push(json!({"type":"background","action":"set","image":background}));
    }
    for (slot, overlay) in state["overlays"].as_object().into_iter().flatten() {
        if !text(overlay, "image").is_empty() {
            let mut node = json!({"type":"overlay","action":"show","slot":slot,"image":overlay["image"],"position":overlay.get("position").cloned().unwrap_or(json!("center")),"scale":number(overlay,"scale",100.0),"opacity":number(overlay,"opacity",100.0),"layer":overlay.get("layer").cloned().unwrap_or(json!("front")),"fade":0});
            for field in ["x", "y"] {
                if let Some(value) = overlay.get(field) {
                    node[field] = value.clone();
                }
            }
            nodes.push(node);
        }
    }
    let positions: Value = serde_json::from_str(include_str!("../../../data/stage_positions.json"))
        .expect("embedded stage positions");
    let mut actors: Vec<_> = state["actors"].as_object().into_iter().flatten().collect();
    actors.sort_by(|(a, x), (b, y)| {
        let pos = |v: &Value| number(&positions["positions"][text(v, "position")], "x", 0.5);
        pos(x).total_cmp(&pos(y)).then(a.cmp(b))
    });
    for (id, actor) in actors {
        let mut node = json!({"type":"show","character":id,"position":actor["position"],"portrait":actor["portrait"],"facing":actor["facing"]});
        if let Some(appearance) = actor["appearance"].as_str().filter(|s| !s.is_empty()) {
            node["appearance"] = appearance.into();
        }
        nodes.push(node);
        if actor["layout_active"] == true {
            nodes.push(json!({"type":"offset","character":id,"x":number(actor,"offset_x",0.0),"y":number(actor,"offset_y",0.0),"absolute":true,"scale":number(actor,"layout_scale",100.0),"duration":0}));
            if number(actor, "rotation", 0.0) != 0.0 {
                nodes.push(
                    json!({"type":"rotate","character":id,"angle":actor["rotation"],"duration":0}),
                );
            }
        }
        if state["dimming_modes"][id]
            .as_str()
            .is_some_and(|mode| mode != "manual")
        {
            nodes.push(json!({"type":"dim","character":id,"dimmed":actor["dimmed"],"mode":"until_dialogue"}));
        } else if let Some(dimmed) = state["dimming"].get(id) {
            nodes.push(json!({"type":"dim","character":id,"dimmed":dimmed}));
        } else if actor["dimmed"] == true {
            nodes.push(json!({"type":"dim","character":id,"dimmed":true,"mode":"until_dialogue"}));
        }
    }
    for (id, setting) in state["mood_overrides"].as_object().into_iter().flatten() {
        if state["actors"].get(id).is_some() {
            let mut setting = setting.clone();
            setting.as_object_mut().unwrap().remove("id");
            setting.as_object_mut().unwrap().remove("goto");
            nodes.push(setting);
        }
    }
    let cg = &state["custom_cg"];
    if !text(cg, "image").is_empty() {
        nodes.push(json!({"type":"custom_cg","action":"show","image":cg["image"],"fade":0,"scale":number(cg,"scale",100.0),"x":number(cg,"x",0.0),"y":number(cg,"y",0.0)}));
    }
    for picture in state["official_pictures"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
    {
        let mut picture = picture.clone();
        picture.as_object_mut().unwrap().remove("id");
        picture.as_object_mut().unwrap().remove("goto");
        picture.as_object_mut().unwrap().remove("event_text");
        if picture["kind"] == "event" {
            picture.as_object_mut().unwrap().remove("fade_in");
        } else if picture["kind"] == "big" {
            picture["fade_in"] = json!(0);
        }
        nodes.push(picture);
    }
    let mut used: std::collections::BTreeSet<String> = story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect();
    let mut seq = 0;
    for node in &mut nodes {
        while used.contains(&format!("zz_playtest_{seq}")) {
            seq += 1;
        }
        let id = format!("zz_playtest_{seq}");
        used.insert(id.clone());
        node["id"] = id.into();
        seq += 1;
    }
    for i in 0..nodes.len() {
        let next = nodes
            .get(i + 1)
            .map(|v| v["id"].clone())
            .unwrap_or(json!(target));
        nodes[i]["goto"] = next;
    }
    nodes
}
fn apply(s: &mut Value, n: &Value) {
    s["dialog"] = Value::Null;
    s["choice"] = Value::Null;
    s["dialogue_mood"] = Value::Null;
    let kind = text(n, "type");
    let cid = if kind == "say" && n["mode"] == "offscreen" {
        ""
    } else {
        text(n, "character")
    };
    match kind {
        "render_priority" => lom_core::render_priority::apply(&mut s["render_priorities"], n),
        "character_title" => {
            if n["action"] == "reset" {
                if let Some(titles) = s["character_titles"].as_object_mut() {
                    titles.remove(cid);
                }
            } else if n["action"] != "keep" {
                s["character_titles"][cid] = if n["action"] == "hide" {
                    json!("")
                } else {
                    n["title"].clone()
                };
            }
            if n["name_action"] == "reset" {
                if let Some(names) = s["character_names"].as_object_mut() {
                    names.remove(cid);
                }
            } else if n["name_action"] == "set" {
                s["character_names"][cid] = n["name"].clone();
            }
        }
        "camera" => {
            if let Some(name) = n["name"].as_str().filter(|name| !name.is_empty()) {
                // Preserve independent enable/disable state, including parallel actions.
                let settings = s["camera_filters"].as_object_mut().unwrap();
                settings.shift_remove(name);
                settings.insert(name.to_owned(), json!(n["active"] == true));
                if n["active"] == true {
                    if let Some(ids) = n.get("exclude_characters") {
                        s["camera_filter_exclusions"][name] = ids.clone();
                    }
                } else {
                    s["camera_filter_exclusions"]
                        .as_object_mut()
                        .unwrap()
                        .remove(name);
                }
            }
        }
        "raw" => {
            let (actions, report) = lom_core::lua_import::preview_nodes(text(n, "code"));
            for action in &actions {
                apply(s, action);
            }
            s["raw_preview"] = json!(report);
        }
        "parallel" => {
            for action in lom_core::parallel::actions(n) {
                apply(s, action);
            }
        }
        "transition" => {
            s["blackout"] = json!({"amount":if n["phase"] == "in" {1.0} else {0.0},
                "dir":n["dir"].as_str().unwrap_or("lr")});
        }
        "scene" => {
            s["view"] = n["view"].clone();
            s["background"] = Value::Null;
        }
        "combat" => {
            s["view"] = n.get("background").cloned().unwrap_or(json!("center"));
            s["background"] = Value::Null;
        }
        "background" => {
            s["background"] = if matches!(text(n, "action"), "clear" | "fadeout") {
                Value::Null
            } else {
                n["image"].clone()
            };
        }
        "custom_cg" => {
            s["custom_cg"] = if text(n, "action") == "hide" {
                Value::Null
            } else {
                n.clone()
            };
        }
        "cg" => {
            let kind = text(n, "kind");
            if n["action"] == "hide" {
                if let Some(pictures) = s["official_pictures"].as_object_mut() {
                    pictures.remove(kind);
                }
            } else {
                s["official_pictures"][kind] = n.clone();
                if kind == "event" && !text(n, "event_text").is_empty() {
                    s["dialog"] = json!({"mode":"narrative", "text":n["event_text"]});
                }
            }
        }
        "overlay" => {
            let slot = n["slot"].as_str().unwrap_or("main");
            if text(n, "action") == "hide" {
                if let Some(o) = s["overlays"].as_object_mut() {
                    o.remove(slot);
                }
            } else {
                s["overlays"][slot] = n.clone();
            }
        }
        "mood" => {
            if !cid.is_empty() {
                if n["enabled"] == true
                    && n["icon"].as_str().unwrap_or("auto") == "auto"
                    && number(n, "offset_x", 0.0) == 0.0
                    && number(n, "offset_y", 0.0) == 0.0
                {
                    s["mood_overrides"].as_object_mut().unwrap().remove(cid);
                } else {
                    s["mood_overrides"][cid] = n.clone();
                }
            } else {
                s["mood_enabled"] = n["enabled"].clone();
                if n["enabled"] == false {
                    s["mood_overrides"] = json!({});
                }
            }
        }
        "portrait" => {
            if s["actors"].get(cid).is_some() {
                s["actors"][cid]["portrait"] = n["portrait"].clone();
            }
        }
        "show" => {
            if !cid.is_empty() {
                let dimmed = s["dimming"].get(cid).cloned().unwrap_or(json!(false));
                s["actors"][cid] = json!({"position":n.get("position").cloned().unwrap_or(json!("M")),"portrait":n.get("portrait").cloned().unwrap_or(json!("normal")),"facing":n.get("facing").cloned().unwrap_or(json!("right")),"appearance":n["appearance"],"offset_x":0.0,"offset_y":0.0,"rotation":0.0,"dimmed":dimmed});
            }
        }
        "hide_all" => {
            s["render_priorities"] = json!([]);
            s["actors"] = json!({});
            s["mood_overrides"] = json!({});
            s["dimming"] = json!({});
            s["dimming_modes"] = json!({});
            s["hidden_actors"] = json!({});
        }
        "hide" => {
            if let Some(actor) = s["actors"].get(cid).cloned() {
                s["hidden_actors"][cid] = actor;
                if n["to"].is_string() {
                    s["hidden_actors"][cid]["position"] = n["to"].clone();
                }
            }
            if let Some(o) = s["actors"].as_object_mut() {
                o.remove(cid);
            }
            s["mood_overrides"].as_object_mut().unwrap().remove(cid);
        }
        "move" | "face" | "rotate" | "dim" | "offset" => {
            if s["actors"].get(cid).is_some() {
                if kind == "dim" {
                    let mode = n["mode"].as_str().unwrap_or("manual");
                    s["dimming_modes"][cid] = json!(mode);
                    if mode == "auto" {
                        s["dimming"].as_object_mut().unwrap().remove(cid);
                    } else {
                        s["dimming"][cid] = n["dimmed"].clone();
                    }
                }
                let automatic_dim =
                    !s["speaker"].as_str().unwrap_or("").is_empty() && s["speaker"] != cid;
                let a = &mut s["actors"][cid];
                match kind {
                    "move" => {
                        a["position"] = n["to"].clone();
                        // Named stage movement starts from its explicit slot,
                        // just like the runtime stage.show call.
                        a["offset_x"] = json!(0.0);
                        a["offset_y"] = json!(0.0);
                    }
                    "face" => a["facing"] = n["facing"].clone(),
                    "rotate" => a["rotation"] = n["angle"].clone(),
                    "dim" => {
                        a["dimmed"] = if n["mode"] == "auto" {
                            json!(automatic_dim)
                        } else {
                            n["dimmed"].clone()
                        }
                    }
                    "offset" => {
                        if n.get("absolute").is_some() || n.get("scale").is_some() {
                            a["layout_active"] = true.into();
                        }
                        let relative = n["absolute"] != true;
                        a["offset_x"] = json!(
                            if relative {
                                number(a, "offset_x", 0.0)
                            } else {
                                0.0
                            } + number(n, "x", 0.0)
                        );
                        a["offset_y"] = json!(
                            if relative {
                                number(a, "offset_y", 0.0)
                            } else {
                                0.0
                            } + number(n, "y", 0.0)
                        );
                        if n["scale"].is_number() {
                            a["layout_scale"] = n["scale"].clone();
                        }
                    }
                    _ => {}
                }
            }
        }
        "say" => {
            if !cid.is_empty()
                && matches!(
                    n["mode"].as_str().unwrap_or("character"),
                    "character" | "think"
                )
                && s["actors"].get(cid).is_none()
            {
                let actor = s["hidden_actors"].get(cid).cloned().unwrap_or_else(|| json!({
                    "position":"M","portrait":"normal","facing":"right","offset_x":0.0,"offset_y":0.0,"rotation":0.0,"dimmed":false
                }));
                s["actors"][cid] = actor;
            }
            s["speaker"] = json!(cid);
            let ids: Vec<String> = s["dimming_modes"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            for id in ids {
                if matches!(
                    s["dimming_modes"][&id].as_str(),
                    Some("auto" | "until_dialogue")
                ) {
                    s["dimming_modes"][&id] = json!("auto");
                    s["dimming"].as_object_mut().unwrap().remove(&id);
                    if s["actors"].get(&id).is_some() {
                        s["actors"][&id]["dimmed"] = json!(id != cid);
                    }
                }
            }
            let official: Vec<String> = s["actors"]
                .as_object()
                .unwrap()
                .keys()
                .filter(|id| !id.starts_with("user:"))
                .cloned()
                .collect();
            for id in official {
                if !cid.starts_with("user:")
                    && n["inherit_dialog"] != true
                    && s["dimming"].get(&id).is_none()
                {
                    s["actors"][&id]["dimmed"] = json!(id != cid);
                }
            }
            if n["mood_icon"].is_string() {
                let target = n["mood_character"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .unwrap_or(cid);
                s["dialogue_mood"] = json!({"character":target, "icon":n["mood_icon"]});
            }
            if s["actors"].get(cid).is_some() {
                s["actors"][cid]["portrait"] =
                    n.get("portrait").cloned().unwrap_or(json!("normal"));
            }
            s["dialog"] = n.clone();
        }
        "choice" => s["choice"] = n["options"].clone(),
        _ => {}
    }
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn number(v: &Value, key: &str, default: f64) -> f64 {
    v[key].as_f64().unwrap_or(default)
}

// The game's transition duration is serialized on its scene component. This is
// a readable editor preview interval, not a claim about that installed value.
const PREVIEW_TRANSITION_SECONDS: f64 = 1.0;

/// Editor timing only: game loading and audio are not simulated here.
pub fn playback_duration(node: &Value) -> f64 {
    fn duration(action: &Value) -> f64 {
        match text(action, "type") {
            "portrait" => number(action, "fade_out", 0.5) + number(action, "fade_in", 1.5),
            "show" => number(action, "fadeDuration", 0.0).max(number(action, "moveDuration", 0.0)),
            "hide" | "hide_all" => number(action, "fadeDuration", 0.0),
            "move" => number(action, "duration", 1.0),
            "offset" | "rotate" | "camera" => number(action, "duration", 0.0),
            "shock" => number(action, "duration", 0.5),
            "wait" => number(action, "seconds", 0.0),
            "transition" => PREVIEW_TRANSITION_SECONDS,
            "background" | "custom_cg" | "overlay" => number(action, "fade", 0.0),
            "music" | "sound" if action["op"] == "fadeout" => number(action, "seconds", 2.0),
            "effect" => 0.0,
            "cg" if action["kind"] == "event" && action["action"] == "show" => {
                number(action, "fade_in", 0.0)
            }
            "cg" if action["kind"] == "big" => {
                if action["action"] == "hide" {
                    1.0
                } else {
                    number(action, "fade_in", 1.0)
                        * if action["background_only"] == true {
                            1.0
                        } else {
                            2.0
                        }
                }
            }
            _ => 0.0,
        }
        .max(0.0)
            + number(action, "wait_after", 0.0).max(0.0)
    }
    if node["type"] == "parallel" {
        lom_core::parallel::actions(node)
            .iter()
            .map(duration)
            .fold(0.0, f64::max)
            .max(0.1)
    } else {
        match text(node, "type") {
            "cg" if node["kind"] == "big" => duration(node).max(0.1),
            "cg" if node["kind"] == "event" && node["action"] == "show" => {
                duration(node).max(0.0) + 1.8
            }
            "show" | "portrait" | "hide" | "hide_all" | "move" | "offset" | "rotate" | "shock"
            | "wait" | "transition" | "mood" | "effect" | "character_title" | "camera" => {
                duration(node).max(0.1)
            }
            _ => 1.8,
        }
    }
}

fn position(positions: &Value, name: &str) -> (f64, f64) {
    let key = if name.eq_ignore_ascii_case("c") {
        "M".to_owned()
    } else {
        name.to_uppercase()
    };
    let entry = &positions["positions"][key];
    (number(entry, "x", 0.5), number(entry, "feet", 1.003))
}

/// All children read the same elapsed time. Each retains its own duration;
/// a short movement reaches its endpoint while its sibling keeps moving.
#[cfg(test)]
fn simulate_frame(story: &Value, target: &str, elapsed: f64, positions: &Value) -> Value {
    let mut state = simulate(story, target);
    state["effects"] = effects::simulate(story, target, elapsed, true, None);
    let before = simulate_until(story, target, false);
    let Some(node) = story["nodes"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|node| node["id"] == target))
    else {
        return state;
    };
    animate_frame(&mut state, &before, node, elapsed, positions);
    state
}

fn animate_frame(state: &mut Value, before: &Value, node: &Value, elapsed: f64, positions: &Value) {
    for action in lom_core::parallel::actions(node) {
        if action["type"] == "cg" && action["kind"] == "big" {
            let ease = |t: f64| {
                let t = t.clamp(0.0, 1.0);
                1.0 - (1.0 - t) * (1.0 - t)
            };
            if action["action"] == "hide" {
                if elapsed < 1.0 {
                    state["official_pictures"]["big"] = before["official_pictures"]["big"].clone();
                    state["official_pictures"]["big"]["preview_opacity"] =
                        json!(1.0 - ease(elapsed));
                }
            } else {
                let fade = number(action, "fade_in", 1.0);
                state["official_pictures"]["big"]["preview_opacity"] = json!(if fade > 0.0 {
                    ease(elapsed / fade)
                } else {
                    1.0
                });
                state["official_pictures"]["big"]["preview_image_opacity"] = json!(if fade > 0.0 {
                    ease((elapsed - fade) / fade)
                } else {
                    1.0
                });
            }
            continue;
        }
        if action["type"] == "hide_all" {
            let fade = number(action, "fadeDuration", 0.0);
            if fade > elapsed && fade > 0.0 {
                for (cid, actor) in before["actors"].as_object().into_iter().flatten() {
                    state["actors"][cid] = actor.clone();
                    state["actors"][cid]["preview_opacity"] =
                        (1.0 - elapsed / fade).clamp(0.0, 1.0).into();
                    state["actors"][cid]["preview_hide_mood"] = true.into();
                }
            }
            continue;
        }
        if action["type"] == "transition" {
            let from = number(&before["blackout"], "amount", 0.0);
            let to = if action["phase"] == "in" { 1.0 } else { 0.0 };
            let progress = (elapsed / PREVIEW_TRANSITION_SECONDS).clamp(0.0, 1.0);
            state["blackout"]["amount"] = (from + (to - from) * progress).into();
            continue;
        }
        if action["type"] == "cg" && action["kind"] == "event" && action["action"] == "show" {
            let fade = number(action, "fade_in", 0.0);
            state["official_pictures"]["event"]["preview_opacity"] = if fade > 0.0 {
                (elapsed / fade).clamp(0.0, 1.0)
            } else {
                1.0
            }
            .into();
            if elapsed < fade {
                state["dialog"] = before["dialog"].clone();
            }
            continue;
        }
        if action["type"] == "camera" {
            let name = text(action, "name");
            let duration = number(action, "duration", 0.0);
            let progress = if duration <= 0.0 {
                1.0
            } else {
                (elapsed / duration).clamp(0.0, 1.0)
            };
            let progress = 1.0 - (1.0 - progress).powi(2);
            let from = if before["camera_filters"][name] == true {
                1.0
            } else {
                0.0
            };
            let to = if action["active"] == true { 1.0 } else { 0.0 };
            let weight = from + (to - from) * progress;
            state["camera_filter_weights"][name] = json!(weight);
            state["camera_filters"][name] = json!(weight > 0.0);
            if action["active"] == false && progress < 1.0 {
                if let Some(ids) = before["camera_filter_exclusions"].get(name) {
                    state["camera_filter_exclusions"][name] = ids.clone();
                }
            }
            continue;
        }
        let cid = text(action, "character");
        if cid.is_empty() {
            continue;
        }
        let old = &before["actors"][cid];
        let progress = |duration: f64| {
            if duration <= 0.0 {
                1.0
            } else {
                (elapsed / duration).clamp(0.0, 1.0)
            }
        };
        match text(action, "type") {
            "hide" if old.is_object() => {
                let alpha = 1.0 - progress(number(action, "fadeDuration", 0.0));
                if alpha > 0.0 {
                    state["actors"][cid] = old.clone();
                    state["actors"][cid]["preview_opacity"] = alpha.into();
                    state["actors"][cid]["preview_hide_mood"] = true.into();
                }
            }
            "portrait" if old.is_object() => {
                let fade_out = number(action, "fade_out", 0.5);
                let fade_in = number(action, "fade_in", 1.5);
                let alpha = if elapsed < fade_out {
                    state["actors"][cid] = old.clone();
                    1.0 - progress(fade_out)
                } else if fade_in > 0.0 {
                    ((elapsed - fade_out) / fade_in).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                state["actors"][cid]["preview_opacity"] = alpha.into();
                state["actors"][cid]["preview_hide_mood"] = (elapsed < fade_out + fade_in).into();
            }
            "show" => {
                if state["actors"][cid].is_object() {
                    state["actors"][cid]["preview_opacity"] =
                        progress(number(action, "fadeDuration", 0.0)).into();
                }
            }
            "move" if old.is_object() => {
                let from = if cid.starts_with("user:") {
                    // CustomCharacterRuntime.Move starts a visible actor at
                    // its current location, including an earlier offset.
                    let base = position(positions, text(old, "position"));
                    (
                        base.0 + number(old, "offset_x", 0.0) / 1920.0,
                        base.1 - number(old, "offset_y", 0.0) / 1080.0,
                    )
                } else {
                    position(
                        positions,
                        action["from"].as_str().unwrap_or(text(old, "position")),
                    )
                };
                let to = position(positions, text(action, "to"));
                let p = progress(number(action, "duration", 1.0));
                state["actors"][cid]["preview_x"] = (from.0 + (to.0 - from.0) * p).into();
                state["actors"][cid]["preview_feet"] = (from.1 + (to.1 - from.1) * p).into();
            }
            "shock" if old.is_object() => {
                let duration = number(action, "duration", 0.5);
                if elapsed > 0.0 && elapsed < duration {
                    // A diminishing preview, without persisting motion into the layout.
                    // The official flowchart uses iTween; the custom actor uses this
                    // sine/cosine envelope. This is an editor illustration, not its RNG.
                    let strength = 12.0 * (1.0 - elapsed / duration);
                    let phase = elapsed * 70.0;
                    state["actors"][cid]["preview_shock_x"] = (phase.sin() * strength).into();
                    state["actors"][cid]["preview_shock_y"] =
                        ((phase * 0.73).cos() * strength * 0.45).into();
                }
            }
            "offset" | "rotate" if old.is_object() => {
                let p = progress(number(action, "duration", 0.0));
                let fields: &[&str] = if action["type"] == "rotate" {
                    &["rotation"]
                } else {
                    &["offset_x", "offset_y"]
                };
                for field in fields {
                    let start = number(old, field, 0.0);
                    let end = number(&state["actors"][cid], field, start);
                    state["actors"][cid][*field] = (start + (end - start) * p).into();
                }
            }
            _ => {}
        }
    }
}

fn blackout_rect(rect: Rect, amount: f64, direction: &str) -> Rect {
    let width = rect.width() * amount.clamp(0.0, 1.0) as f32;
    // StoryTransitionBlack.SetFillOrigin: rl = Left, lr/default = Right.
    let left = if direction == "rl" {
        rect.left()
    } else {
        rect.right() - width
    };
    Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(width, rect.height()))
}

fn raw_preview_notice(report: &Value, locale: &str) -> String {
    let notes = report["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|note| {
            let line = report["preview_stopped_at"].as_u64();
            for (suffix, name) in [
                (
                    " 行起保留原生 Lua，静态预览在此停止。",
                    "editor.preview.raw_stop",
                ),
                (
                    " 行的参数不符合当前节点契约，保留原生 Lua。",
                    "editor.preview.raw_invalid",
                ),
            ] {
                if let Some(number) = note
                    .strip_prefix("第 ")
                    .and_then(|text| text.strip_suffix(suffix))
                {
                    if number
                        .parse::<u64>()
                        .ok()
                        .is_some_and(|number| Some(number) == line)
                    {
                        return crate::i18n::message_for_locale(
                            name,
                            &[("line", number.into())],
                            locale,
                        );
                    }
                }
            }
            if note == "含原版默认补间或异步移动，原生语句保留；预览仅示意动作完成时的画面。"
            {
                return crate::i18n::message_for_locale("editor.preview.raw_async", &[], locale);
            }
            note.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ");
    crate::i18n::message_for_locale("editor.preview.raw_notice", &[("notes", notes)], locale)
}

fn preview_character_title(project: &Project, state: &Value, id: &str) -> String {
    if let Some(title) = state["character_titles"].get(id).and_then(Value::as_str) {
        return title.into();
    }
    if let Some(content_id) = id.strip_prefix("user:") {
        for (path, bytes) in &project.assets {
            if !path.ends_with("/content.json") {
                continue;
            }
            if let Ok(meta) = serde_json::from_slice::<Value>(bytes) {
                if meta["type"] == "character" && meta["id"] == content_id {
                    return text(&meta, "title").into();
                }
            }
        }
    }
    String::new()
}

fn character_display_name(project: &Project, id: &str) -> String {
    if let Some(content_id) = id.strip_prefix("user:") {
        for (path, bytes) in &project.assets {
            if !path.ends_with("/content.json") {
                continue;
            }
            let Ok(meta) = serde_json::from_slice::<Value>(bytes) else {
                continue;
            };
            if matches!(meta["type"].as_str(), Some("character" | "image"))
                && meta["id"] == content_id
            {
                return meta["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id)
                    .to_owned();
            }
        }
        return id.to_owned();
    }
    let name = lom_core::validate::editor_data()["characters"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["id"] == id)
        .and_then(|entry| entry["name"].as_str())
        .unwrap_or(id);
    crate::i18n::term("characters", id, name)
}

struct Cached {
    texture: TextureHandle,
    filter_image: Option<filters::FilterImage>,
    opaque: Rect,
    bytes: usize,
    touched: u64,
}
#[derive(Debug, PartialEq)]
enum AssetSource<'a> {
    Project(&'a [u8]),
    Library(PathBuf),
}
struct Simulation {
    story: Value,
    target: String,
    before: Value,
    after: Value,
    has_effects: bool,
    effects: effects::Simulation,
}
pub struct MoodEdit {
    pub action: Option<usize>,
    pub x: f64,
    pub y: f64,
}
struct MoodDrag {
    character: String,
    action: Option<usize>,
    origin: Pos2,
    x: f64,
    y: f64,
}
fn editable_mood<'a>(node: &'a Value, character: &str) -> Option<(Option<usize>, &'a Value)> {
    let matches = |action: &&Value| {
        action["type"] == "mood"
            && action["enabled"] == true
            && text(action, "character") == character
    };
    if node["type"] == "parallel" {
        node["actions"]
            .as_array()?
            .iter()
            .enumerate()
            .find(|(_, action)| matches(action))
            .map(|(index, action)| (Some(index), action))
    } else {
        Some(node).filter(matches).map(|action| (None, action))
    }
}
pub struct Preview {
    pub mapping: Value,
    pub library: PathBuf,
    positions: Value,
    simulation: Option<Simulation>,
    cache: BTreeMap<String, Cached>,
    tick: u64,
    mood_clocks: BTreeMap<String, (String, f64)>,
    pub extract_requested: bool,
    pub mood_edit: Option<MoodEdit>,
    mood_drag: Option<MoodDrag>,
    pub layout_mode: bool,
    pub legacy_character_rotation: bool,
    pub layout_edit: Option<LayoutChange>,
    layout_selected: Option<LayoutTarget>,
    layout_context: (String, String),
    layout_drag: Option<LayoutDrag>,
    layout_command: Option<LayoutCommand>,
    layout_clipboard: Option<LayoutClipboard>,
    layout_snap: bool,
    effect_clock: effects::Clock,
    effect_library: Option<std::sync::Arc<effects::Library>>,
    effect_library_error: Option<String>,
    pub effect_extract_requested: Option<Vec<String>>,
    camera_filter: filters::Settings,
}
impl Preview {
    fn character_rotation(&self, degrees: f32) -> f32 {
        // egui's Y axis points down; Unity's positive Z rotates counterclockwise.
        if self.legacy_character_rotation {
            degrees
        } else {
            -degrees
        }
    }
    pub fn new(_repo: &Path) -> Self {
        // Embedded mappings keep a standalone app independent of its build machine.
        // Never touch the compilation-time Documents checkout during app startup.
        let library = if let Some(root) = std::env::var_os("LOM_MODKIT_ROOT") {
            PathBuf::from(root).join("data")
        } else if let Some(appdata) = std::env::var_os("APPDATA") {
            PathBuf::from(appdata).join("lom_modkit/game-previews")
        } else {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            if cfg!(target_os = "macos") {
                home.join("Library/Application Support/lom_modkit/game-previews")
            } else {
                home.join(".local/share/lom_modkit/game-previews")
            }
        };
        let mapping: Value = serde_json::from_str(include_str!("../../../data/preview_map.json"))
            .expect("embedded preview mapping");
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json"))
                .unwrap_or(json!({}));
        let (effect_library, effect_library_error) = match effects::Library::current() {
            Ok(library) => (library.map(std::sync::Arc::new), None),
            Err(error) => (None, Some(crate::error_ui::capture(error))),
        };
        Self {
            mapping,
            library,
            positions,
            simulation: None,
            cache: BTreeMap::new(),
            tick: 0,
            mood_clocks: BTreeMap::new(),
            extract_requested: false,
            mood_edit: None,
            mood_drag: None,
            layout_mode: false,
            legacy_character_rotation: false,
            layout_edit: None,
            layout_selected: None,
            layout_context: (String::new(), String::new()),
            layout_drag: None,
            layout_command: None,
            layout_clipboard: None,
            layout_snap: true,
            effect_clock: effects::Clock::default(),
            effect_library,
            effect_library_error,
            effect_extract_requested: None,
            camera_filter: filters::Settings::default(),
        }
    }
    pub fn set_effect_library(&mut self, path: &Path) -> anyhow::Result<()> {
        let loaded = (|| -> anyhow::Result<_> {
            let next = effects::Library::load(path)?;
            if let Some(current) = &self.effect_library {
                let mut merged = (**current).clone();
                merged.merge(next)?;
                Ok(merged)
            } else {
                Ok(next)
            }
        })();
        match loaded {
            Ok(library) => {
                self.effect_library = Some(std::sync::Arc::new(library));
                self.effect_library_error = None;
                self.simulation = None;
                Ok(())
            }
            Err(error) => {
                self.effect_library_error = Some(crate::error_ui::capture(&error));
                Err(error)
            }
        }
    }

    pub fn state(&mut self, story: &Value, target: &str) -> &Value {
        if self
            .simulation
            .as_ref()
            .is_none_or(|cached| cached.target != target || cached.story != *story)
        {
            self.simulation = Some(Simulation {
                story: story.clone(),
                target: target.to_owned(),
                before: simulate_until(story, target, false),
                after: simulate(story, target),
                has_effects: lom_core::parallel::story_actions(story)
                    .any(|(_, node)| node["type"] == "effect"),
                effects: effects::prepare(story, target, true, self.effect_library.as_deref()),
            });
        }
        &self.simulation.as_ref().unwrap().after
    }

    pub fn state_before(&mut self, story: &Value, target: &str) -> &Value {
        self.state(story, target);
        &self.simulation.as_ref().unwrap().before
    }

    fn trim_texture_cache(&mut self, additional: usize) {
        while !self.cache.is_empty()
            && self
                .cache
                .values()
                .map(|cached| cached.bytes)
                .sum::<usize>()
                + additional
                > 256 * 1024 * 1024
        {
            let oldest = self
                .cache
                .iter()
                .min_by_key(|(_, cached)| cached.touched)
                .map(|(key, _)| key.clone())
                .unwrap();
            self.cache.remove(&oldest);
        }
    }

    fn cached_texture(&mut self, key: &str) -> Option<(TextureHandle, Rect)> {
        let cached = self.cache.get_mut(key)?;
        cached.touched = self.tick;
        if let Some(image) = &mut cached.filter_image {
            if image.update(&self.camera_filter) {
                cached.texture.set(
                    egui::ImageData::Color(image.pixels()),
                    egui::TextureOptions::LINEAR,
                );
                cached.bytes = cached.texture.size().iter().product::<usize>() * 4 + image.bytes();
            }
        }
        let result = (cached.texture.clone(), cached.opaque);
        self.trim_texture_cache(0);
        Some(result)
    }

    fn cache_texture(
        &mut self,
        ctx: &egui::Context,
        key: String,
        pixels: egui::ColorImage,
        opaque: Rect,
    ) -> TextureHandle {
        let cost = pixels.pixels.len() * 4;
        let (filter_image, pixels) = if self.camera_filter == filters::Settings::default() {
            (None, egui::ImageData::from(pixels))
        } else {
            let mut image = filters::FilterImage::new(pixels);
            image.update(&self.camera_filter);
            let pixels = egui::ImageData::Color(image.pixels());
            (Some(image), pixels)
        };
        let cost = cost + filter_image.as_ref().map_or(0, filters::FilterImage::bytes);
        self.trim_texture_cache(cost);
        let texture = ctx.load_texture(&key, pixels, egui::TextureOptions::LINEAR);
        self.cache.insert(
            key,
            Cached {
                texture: texture.clone(),
                filter_image,
                opaque,
                bytes: cost,
                touched: self.tick,
            },
        );
        texture
    }

    fn effect_texture(
        &mut self,
        ctx: &egui::Context,
        frame: &effects::Frame,
        expected: [usize; 2],
    ) -> Option<(TextureHandle, Rect)> {
        let key = self
            .camera_filter
            .cache_key(&format!("effect-frame:{}", frame.file.display()));
        self.tick += 1;
        if let Some(cached) = self.cached_texture(&key) {
            return Some(cached);
        }
        let loaded = (|| -> anyhow::Result<_> {
            let bytes = std::fs::read(&frame.file)?;
            let image = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()?
                .decode()?
                .into_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            anyhow::ensure!(
                size == expected,
                crate::i18n::key("editor.preview.frame_size_mismatch")
            );
            let limit = ctx.input(|input| input.max_texture_side);
            anyhow::ensure!(
                size.iter().all(|side| *side <= limit),
                crate::i18n::key("editor.preview.frame_too_large")
            );
            let opaque = effects::visible_bounds(&image);
            // RGB at alpha=0 carries additive light. Never premultiply again.
            let pixels = egui::ColorImage::from_rgba_premultiplied(size, image.as_raw());
            let texture = self.cache_texture(ctx, key, pixels, opaque);
            Ok((texture, opaque))
        })();
        match loaded {
            Ok(texture) => Some(texture),
            Err(error) => {
                self.effect_library_error = Some(format!("{}: {error:#}", frame.file.display()));
                None
            }
        }
    }
    pub fn set_library(&mut self, path: &Path) -> anyhow::Result<()> {
        let root = if path.join("preview_map.json").is_file() {
            path.to_path_buf()
        } else {
            path.join("data")
        };
        anyhow::ensure!(
            root.join("preview_map.json").is_file(),
            crate::i18n::key("editor.preview.library_missing")
        );
        let mapping = lom_core::load_json(root.join("preview_map.json")).map_err(|error| {
            anyhow::anyhow!(crate::i18n::message(
                "editor.preview.library_damaged",
                &[("error", crate::error_ui::capture(error))]
            ))
        })?;
        crate::asset_extraction::validate_mapping(&root, &mapping).map_err(|error| {
            anyhow::anyhow!(crate::i18n::message(
                "editor.preview.library_incomplete",
                &[("error", crate::error_ui::capture(error))]
            ))
        })?;
        self.mapping = mapping;
        self.library = root;
        self.cache.clear();
        self.mood_clocks.clear();
        Ok(())
    }
    fn asset<'a>(
        &self,
        p: &'a Project,
        name: &str,
        character: bool,
        portrait: &str,
    ) -> Option<(String, AssetSource<'a>, Value)> {
        if name.is_empty() {
            return None;
        }
        if let Some(id) = name.strip_prefix("user:") {
            for (key, bytes) in &p.assets {
                if key.ends_with("/content.json") {
                    if let Ok(meta) = serde_json::from_slice::<Value>(bytes) {
                        if meta["id"].as_str() == Some(id) {
                            let file = if character {
                                meta["portraits"][portrait]
                                    .as_str()
                                    .or_else(|| meta["portraits"]["normal"].as_str())
                                    .or_else(|| meta["files"]["main"].as_str())
                            } else {
                                meta["files"]["main"].as_str()
                            }?;
                            let path = Path::new(key)
                                .parent()?
                                .join(file)
                                .to_string_lossy()
                                .replace('\\', "/");
                            return p
                                .assets
                                .get(&path)
                                .map(|b| (path, AssetSource::Project(b), meta));
                        }
                    }
                }
            }
            return None;
        }
        if character {
            for (key, bytes) in &p.assets {
                if !key.starts_with("assets/user/character/") || !key.ends_with("/content.json") {
                    continue;
                }
                let Ok(meta) = serde_json::from_slice::<Value>(bytes) else {
                    continue;
                };
                let target = if name == "player_beautified" {
                    "player"
                } else {
                    name
                };
                if meta["extends"] != target {
                    continue;
                }
                if let Some(file) = meta["portraits"][portrait].as_str() {
                    let path = format!("{}{file}", key.strip_suffix("content.json")?);
                    return p
                        .assets
                        .get(&path)
                        .map(|b| (path, AssetSource::Project(b), meta));
                }
            }
        }
        if let Some(bytes) = p.assets.get(name) {
            return Some((
                format!("project:{name}"),
                AssetSource::Project(bytes),
                json!({}),
            ));
        }
        let rel = if let Some(key) = name.strip_prefix("@mood:") {
            key
        } else if let Some(key) = name.strip_prefix("@picture:") {
            self.mapping["pictures"][key].as_str()?
        } else if character {
            self.mapping["characters"][name]["portraits"][portrait]
                .as_str()
                .or_else(|| self.mapping["characters"][name]["portraits"]["normal"].as_str())?
        } else {
            self.mapping["views"][name]
                .as_str()
                .or_else(|| self.mapping["pictures"][name].as_str())
                .unwrap_or(name)
        };
        let rel = Path::new(rel);
        if rel.is_absolute()
            || rel.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::Prefix(_)
                )
            })
        {
            return None;
        }
        let canon = self.library.join(rel).canonicalize().ok()?;
        let root = self.library.canonicalize().ok()?;
        if !canon.starts_with(root) {
            return None;
        }
        let mut meta = json!({});
        if character && self.mapping["portrait_canvas"] != "sprite_rect" {
            static GEOMETRY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            let geometry = GEOMETRY.get_or_init(|| {
                serde_json::from_str(include_str!("../../../data/portrait_geometry.json"))
                    .expect("portrait geometry")
            });
            let portrait = if self.mapping["characters"][name]["portraits"][portrait].is_string() {
                portrait
            } else {
                "normal"
            };
            if geometry[name][portrait].is_object() {
                meta["_preview_geometry"] = geometry[name][portrait].clone();
            }
        }
        Some((
            canon.to_string_lossy().into(),
            AssetSource::Library(canon),
            meta,
        ))
    }
    fn texture(
        &mut self,
        ctx: &egui::Context,
        p: &Project,
        name: &str,
        character: bool,
        portrait: &str,
    ) -> Option<(TextureHandle, Value)> {
        self.tick += 1;
        let mood_key = name
            .strip_prefix("@mood:")
            .map(|_| format!("mood-frame:{}:{name}", self.library.display()));
        let mood_cached_key = mood_key
            .as_ref()
            .map(|key| self.camera_filter.cache_key(key));
        if let Some(cached) = mood_cached_key
            .as_ref()
            .and_then(|key| self.cached_texture(key))
        {
            return Some((cached.0, json!({})));
        }
        let (key, source, meta) = self.asset(p, name, character, portrait)?;
        use std::hash::{Hash, Hasher};
        let key = if let Some(key) = mood_key {
            key
        } else {
            match &source {
                AssetSource::Project(bytes) => {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    bytes.hash(&mut hash);
                    format!("{key}:{}", hash.finish())
                }
                AssetSource::Library(path) => {
                    let metadata = std::fs::metadata(path).ok()?;
                    if metadata.len() > 32 * 1024 * 1024 {
                        return None;
                    }
                    format!("{key}:{}:{:?}", metadata.len(), metadata.modified().ok()?)
                }
            }
        };
        let key = self.camera_filter.cache_key(&key);
        if let Some(cached) = self.cached_texture(&key) {
            return Some((cached.0, meta));
        }
        let bytes = match source {
            AssetSource::Project(bytes) => std::borrow::Cow::Borrowed(bytes),
            AssetSource::Library(path) => std::borrow::Cow::Owned(std::fs::read(path).ok()?),
        };
        if bytes.len() > 32 * 1024 * 1024 {
            return None;
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let image = reader.decode().ok()?;
        let image = if image.width() > 1600 || image.height() > 1600 {
            image.thumbnail(1600, 1600)
        } else {
            image
        }
        .into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let opaque = opaque_bounds(&image);
        let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = self.cache_texture(ctx, key, pixels, opaque);
        Some((texture, meta))
    }
    pub fn image_browser(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        options: &[(String, String)],
        selected: &str,
        official: bool,
    ) -> Option<String> {
        let mut chosen = None;
        egui::CollapsingHeader::new(crate::i18n::key("editor.preview.browse_images"))
            .id_salt("image-browser")
            .show(ui, |ui| {
                let id = ui.id().with("image-search");
                let mut query = ui
                    .data_mut(|data| data.get_temp::<String>(id))
                    .unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text(crate::i18n::key("editor.preview.image_search")),
                );
                if official
                    && self.mapping["pictures"]
                        .as_object()
                        .is_none_or(|pictures| pictures.is_empty())
                {
                    ui.small(crate::i18n::key("editor.preview.image_extract"));
                    if ui.button(crate::i18n::tr("从游戏提取预览素材…")).clicked() {
                        self.extract_requested = true;
                    }
                }
                if options.is_empty() {
                    ui.label(crate::i18n::key("editor.preview.image_import"));
                }
                let query_lower = query.to_lowercase();
                let category_id = ui.id().with("image-category");
                let mut category = ui
                    .data_mut(|data| data.get_temp::<String>(category_id))
                    .unwrap_or_default();
                let catalog = crate::forms::picture_catalog();
                if official {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(
                            &mut category,
                            String::new(),
                            crate::i18n::key("editor.preview.images"),
                        );
                        for kind in ["BigPicture", "Picture", "Items", "Book", "Develop", "UI"] {
                            ui.selectable_value(
                                &mut category,
                                kind.to_owned(),
                                crate::i18n::tr(crate::forms::picture_category(kind)),
                            );
                        }
                    });
                    ui.data_mut(|data| data.insert_temp(category_id, category.clone()));
                }
                let columns = ((ui.available_width() / 136.0).floor() as usize).clamp(1, 4);
                let width = ((ui.available_width()
                    - ui.spacing().item_spacing.x * (columns - 1) as f32)
                    / columns as f32)
                    .max(80.0);
                egui::ScrollArea::vertical()
                    .id_salt("image-browser-scroll")
                    .max_height((ui.ctx().content_rect().height() * 0.45).clamp(180.0, 420.0))
                    .show(ui, |ui| {
                        egui::Grid::new("image-browser-grid")
                            .num_columns(columns)
                            .show(ui, |ui| {
                                let mut shown = 0;
                                for (key, name) in options.iter().filter(|(key, name)| {
                                    (!official
                                        || category.is_empty()
                                        || catalog["pictures"].as_array().unwrap().iter().any(
                                            |row| row["key"] == *key && row["category"] == category,
                                        ))
                                        && (query_lower.is_empty()
                                            || key.to_lowercase().contains(&query_lower)
                                            || name.to_lowercase().contains(&query_lower))
                                }) {
                                    ui.push_id(key, |ui| {
                                        ui.vertical(|ui| {
                                            ui.set_width(width);
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(width, 85.0),
                                                egui::Sense::click(),
                                            );
                                            if ui.is_rect_visible(rect) {
                                                ui.painter().rect_filled(
                                                    rect,
                                                    3.0,
                                                    Color32::from_rgb(38, 44, 49),
                                                );
                                                let lookup =
                                                    if official && !key.starts_with("user:") {
                                                        format!("@picture:{key}")
                                                    } else {
                                                        key.clone()
                                                    };
                                                if let Some((texture, _)) = self.texture(
                                                    ui.ctx(),
                                                    project,
                                                    &lookup,
                                                    false,
                                                    "",
                                                ) {
                                                    fit_image(
                                                        ui.painter(),
                                                        &texture,
                                                        rect.shrink(3.0),
                                                    );
                                                } else {
                                                    ui.painter().text(
                                                        rect.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        crate::i18n::key("editor.preview.none"),
                                                        FontId::proportional(12.0),
                                                        Color32::GRAY,
                                                    );
                                                }
                                            }
                                            let label = ui
                                                .add_sized(
                                                    [width, 22.0],
                                                    egui::Button::selectable(selected == key, name)
                                                        .truncate(),
                                                )
                                                .on_hover_text(format!("{name}\n{key}"));
                                            if response.clicked() || label.clicked() {
                                                chosen = Some(key.clone());
                                            }
                                        });
                                    });
                                    shown += 1;
                                    if shown % columns == 0 {
                                        ui.end_row();
                                    }
                                }
                                if shown == 0 && !options.is_empty() {
                                    ui.label(crate::i18n::key("editor.preview.image_empty"));
                                }
                            });
                    });
                ui.data_mut(|data| data.insert_temp(id, query));
            });
        chosen
    }
    fn mood_texture(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        cid: &str,
        icon: &str,
        flipped: bool,
    ) -> Option<(TextureHandle, Value)> {
        let animation = &self.mapping["moods"][icon][if flipped { "right" } else { "left" }];
        let Some(frames) = animation["frames"].as_array() else {
            self.mood_clocks.remove(cid);
            return None;
        };
        let now = ctx.input(|i| i.time);
        let key = format!("{icon}:{flipped}");
        let clock = self
            .mood_clocks
            .entry(cid.into())
            .or_insert((key.clone(), now));
        if clock.0 != key {
            *clock = (key, now);
        }
        let elapsed = now - clock.1;
        let duration = number(animation, "duration", 1.0);
        let loop_start = animation["loop_start"]
            .as_f64()
            .filter(|start| *start >= 0.0 && duration > *start);
        let time = match loop_start {
            Some(start) if elapsed >= duration => start + (elapsed - start) % (duration - start),
            _ => elapsed.min(duration),
        };
        let frame = frames
            .iter()
            .rev()
            .find(|frame| number(frame, "time", 0.0) <= time)?;
        let next_frame = frames
            .iter()
            .find(|frame| number(frame, "time", 0.0) > time)
            .map(|frame| number(frame, "time", 0.0));
        let next_time = next_frame.or_else(|| loop_start.map(|_| duration));
        if let Some(next_time) = next_time.filter(|_| elapsed < duration || loop_start.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                (next_time - time).max(0.001),
            ));
        }
        let file = format!("@mood:{}", text(frame, "file"));
        let layout = json!({"size": animation["size"], "offset": animation["offset"]});
        let (texture, _) = self.texture(ctx, project, &file, false, "")?;
        Some((texture, layout))
    }
    fn anchor(&self, pos: &str) -> (f32, f32) {
        let (x, feet) = position(&self.positions, pos);
        (x as f32, feet as f32)
    }
    fn portrait_canvas_notice(&mut self, ui: &mut egui::Ui) {
        if self.mapping["portrait_canvas"] == "sprite_rect"
            || !self.library.join("preview_map.json").is_file()
        {
            return;
        }
        ui.small(crate::i18n::tr("当前使用旧版预览素材。"));
        if ui.button(crate::i18n::tr("从游戏提取预览素材…")).clicked() {
            self.extract_requested = true;
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        story: &Value,
        index: usize,
        elapsed: Option<f64>,
    ) -> Option<String> {
        let ctx = ui.ctx().clone();
        let node = &story["nodes"][index];
        let target = text(node, "id");
        let context = (text(story, "id").to_owned(), target.to_owned());
        if self.layout_context != context {
            self.cancel_layout_drag();
            self.mood_drag = None;
            self.mood_edit = None;
            self.mood_clocks.clear();
            self.layout_context = context;
        }
        let mut state = self.state(story, target).clone();
        if state["camera_filters"]
            .as_object()
            .is_some_and(|items| items.values().any(|active| active == true))
        {
            ui.small(crate::i18n::key("editor.preview.camera_approximation"));
        }
        if let Some(elapsed) = elapsed.filter(|_| !self.layout_mode) {
            animate_frame(
                &mut state,
                &self.simulation.as_ref().unwrap().before,
                node,
                elapsed,
                &self.positions,
            );
        }
        let now = ui.input(|input| input.time);
        self.camera_filter = filters::Settings::from_state(&state);
        let has_effects = self.simulation.as_ref().unwrap().has_effects;
        if has_effects {
            self.effect_clock.sync(p, story, target, now);
        }
        let effect_library = self.effect_library.clone();
        let effect_cycle = effects::cycle(node, effect_library.as_deref());
        let effect_time = self
            .effect_clock
            .time(now, elapsed, effect_cycle, self.layout_mode);
        if has_effects {
            state["effects"] = self
                .simulation
                .as_ref()
                .unwrap()
                .effects
                .sample(effect_time, effect_library.as_deref());
        }
        let captured = effect_cycle.is_some()
            || state["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|item| matches!(text(item, "cache_status"), "play" | "tail"));
        if captured
            && self
                .effect_clock
                .ui(ui, effect_cycle.is_some(), self.layout_mode)
        {
            let time = self
                .effect_clock
                .time(now, elapsed, effect_cycle, self.layout_mode);
            state["effects"] = self
                .simulation
                .as_ref()
                .unwrap()
                .effects
                .sample(time, effect_library.as_deref());
        }
        if has_effects {
            let missing = effect_library.is_none()
                || state["effects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|item| matches!(text(item, "cache_status"), "missing" | "missing_tail"));
            let truncated = state["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|item| matches!(text(item, "cache_status"), "truncated" | "tail_truncated"));
            if missing || self.effect_library_error.is_some() || truncated {
                ui.label(crate::i18n::tr(if missing {
                    "缺少原版特效预览"
                } else if truncated {
                    "原版特效采样已到末尾，未记录自然结束。"
                } else {
                    "原版特效预览缓存无效"
                }));
                if let Some(error) = &self.effect_library_error {
                    crate::error_ui::show_with_actions(
                        ui,
                        &format!("原版特效预览缓存无效：{error}"),
                    );
                }
                let names = effects::extraction_names(
                    &state,
                    truncated || self.effect_library_error.is_some(),
                );
                if ui
                    .add_enabled(
                        !names.is_empty(),
                        egui::Button::new(crate::i18n::tr("提取当前特效预览")),
                    )
                    .clicked()
                {
                    self.effect_extract_requested = Some(names);
                }
            }
            if let Some(tail) = state["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|item| item["cache_status"] == "tail")
            {
                ui.small(crate::i18n::tr("正在播放原版采样尾段"))
                    .on_hover_text(crate::i18n::message(
                        "editor.preview.effect_tail",
                        &[(
                            "seconds",
                            format!("{:.3}", number(tail, "tail_sample_time", 0.0)),
                        )],
                    ));
            }
            if captured {
                ui.small(crate::i18n::tr("原版帧已按舞台视口裁切"))
                    .on_hover_text(crate::i18n::tr(
                        "移动、缩放或旋转不能补回采样时位于屏幕外的粒子。",
                    ));
            }
        }
        self.layout_controls(ui, p, &state);
        if has_effects && self.layout_mode {
            state["effects"] = self
                .simulation
                .as_ref()
                .unwrap()
                .effects
                .sample(0.0, effect_library.as_deref());
        }
        self.apply_layout_draft(&mut state);
        if elapsed.is_none() || self.layout_mode {
            let action = ui
                .ctx()
                .data(|data| {
                    data.get_temp::<(String, usize)>(egui::Id::new("preview-character-action"))
                })
                .filter(|(id, _)| id == target)
                .map(|(_, index)| index);
            highlight_editing_character(
                &mut state,
                node,
                action,
                self.layout_selected.as_ref().filter(|_| self.layout_mode),
            );
        }
        let mood_target = ((node["type"] == "mood"
            || (node["type"] == "parallel"
                && lom_core::parallel::actions(node)
                    .iter()
                    .any(|action| action["type"] == "mood")))
            && !self.layout_mode)
            .then(|| {
                if node["type"] == "parallel" {
                    return crate::i18n::tr("表情气泡");
                }
                if text(node, "character").is_empty() {
                    crate::i18n::tr("全场自动气泡")
                } else {
                    format!(
                        "{} · {}",
                        character_display_name(p, text(node, "character")),
                        crate::i18n::tr("表情气泡")
                    )
                }
            });
        if let Some(label) = &mood_target {
            ui.label(
                egui::RichText::new(crate::i18n::message(
                    "editor.preview.adjusting",
                    &[("target", label.clone())],
                ))
                .strong()
                .color(Color32::from_rgb(25, 90, 145)),
            );
        }
        if state["actors"].as_object().is_some_and(|actors| {
            actors
                .keys()
                .any(|id| !id.starts_with("user:") && self.mapping["characters"][id].is_object())
        }) {
            self.portrait_canvas_notice(ui);
        }
        if node["type"] == "use_item" {
            ui.small(crate::i18n::tr(
                "消耗品使用效果仅在游戏内执行；此预览不改变背包或属性。",
            ));
        }
        if node["type"] == "mood" {
            if !text(node, "character").is_empty() {
                ui.small(crate::i18n::key("editor.preview.individual_bubble"));
            } else {
                ui.small(crate::i18n::tr(if state["mood_enabled"] == true {
                    "表情气泡已开启，图标由游戏中的人物表情决定。"
                } else {
                    "表情气泡已关闭。"
                }));
            }
        }
        if (state["mood_enabled"] == true
            || state["mood_overrides"]
                .as_object()
                .is_some_and(|v| !v.is_empty()))
            && !self.mapping["moods"].is_object()
        {
            ui.small(crate::i18n::key("editor.preview.bubble_extract"));
            if ui.button(crate::i18n::tr("从游戏提取预览素材…")).clicked() {
                self.extract_requested = true;
            }
        }
        self.mood_clocks
            .retain(|cid, _| state["actors"][cid].is_object());
        if state["raw_preview"].is_object() {
            ui.small(raw_preview_notice(
                &state["raw_preview"],
                crate::i18n::locale(),
            ));
        }
        let size = Vec2::new(
            ui.available_width().max(160.0),
            ui.available_height().max(120.0),
        );
        let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_filled(outer, 8.0, Color32::from_black_alpha(4));
        let width = size.x.min(size.y * 16.0 / 9.0);
        let rect = Rect::from_center_size(outer.center(), Vec2::new(width, width * 9.0 / 16.0));
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 6.0, Color32::from_rgb(29, 35, 42));
        let priorities = &state["render_priorities"];
        let sorting = lom_core::render_priority::active(priorities);
        let mut scene_order = ShapeOrder::new(sorting);
        let mut unfiltered_order = ShapeOrder::new(sorting);
        let bg_start = scene_order.begin(&painter);
        let bg = state["background"]
            .as_str()
            .or_else(|| state["view"].as_str())
            .unwrap_or("");
        if let Some((tex, _)) = self.texture(&ctx, p, bg, false, "") {
            painter.image(tex.id(), rect, uv(), Color32::WHITE);
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if bg.is_empty() { "" } else { bg },
                FontId::proportional(13.0),
                Color32::from_rgb(109, 118, 113),
            );
        }
        scene_order.capture(
            &painter,
            bg_start,
            lom_core::render_priority::priority(priorities, "background", "", ""),
        );
        let mut layout_objects = Vec::new();
        let scene_filter = self.camera_filter.clone();
        for layer in ["back", "actors", "front", "pictures", "unfiltered_actors"] {
            self.camera_filter = scene_filter.clone();
            if layer == "pictures" {
                if state["custom_cg"].is_object() {
                    let start = scene_order.begin(&painter);
                    if let Some(object) = self.overlay(
                        &ctx,
                        &painter,
                        p,
                        &state["custom_cg"],
                        rect,
                        LayoutTarget::Cg,
                    ) {
                        layout_objects.push(object);
                    }
                    scene_order.capture(
                        &painter,
                        start,
                        lom_core::render_priority::priority(priorities, "custom_cg", "", ""),
                    );
                }
                if sorting {
                    for item in state["effects"].as_array().into_iter().flatten() {
                        let start = scene_order.begin(&painter);
                        layout_objects.extend(effects::paint(
                            &painter,
                            rect,
                            &json!([item]),
                            self.layout_mode,
                            effect_library.as_deref(),
                            |frame, size| self.effect_texture(&ctx, frame, size),
                        ));
                        let tag = item["tag"]
                            .as_str()
                            .filter(|tag| !tag.is_empty())
                            .unwrap_or(text(item, "name"));
                        scene_order.capture(
                            &painter,
                            start,
                            lom_core::render_priority::priority(
                                priorities,
                                "effect",
                                text(item, "name"),
                                tag,
                            ),
                        );
                    }
                } else {
                    layout_objects.extend(effects::paint(
                        &painter,
                        rect,
                        &state["effects"],
                        self.layout_mode,
                        effect_library.as_deref(),
                        |frame, size| self.effect_texture(&ctx, frame, size),
                    ));
                }
                for picture in state["official_pictures"]
                    .as_object()
                    .into_iter()
                    .flat_map(|o| o.values())
                {
                    let kind = text(picture, "kind");
                    if kind == "event" {
                        let mut painter = painter.clone();
                        painter.multiply_opacity(number(picture, "preview_opacity", 1.0) as f32);
                        // Scene layout from StoryMainUI._picture, in the native 1920×1080 canvas.
                        let scale = rect.height() / 1080.0;
                        let book = Rect::from_center_size(
                            rect.center() - egui::vec2(0.0, 65.0 * scale),
                            egui::vec2(1000.0, 1000.0) * scale,
                        );
                        let frame = self.mapping["event_book"].as_str().unwrap_or("").to_owned();
                        if let Some((texture, _)) = self.texture(&ctx, p, &frame, false, "") {
                            painter.image(
                                texture.id(),
                                book,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        } else {
                            painter.rect_filled(book, 3.0, Color32::from_rgb(220, 207, 176));
                            painter.text(
                                book.center_top() + egui::vec2(0.0, 100.0 * scale),
                                egui::Align2::CENTER_TOP,
                                crate::i18n::key("editor.preview.book_extract"),
                                FontId::proportional(12.0),
                                Color32::DARK_GRAY,
                            );
                        }
                        let image_rect = Rect::from_min_size(
                            book.min + egui::vec2(87.0, 202.0) * scale,
                            egui::vec2(400.0, 588.0) * scale,
                        );
                        let key = text(picture, "key");
                        let lookup = if key.starts_with("user:") {
                            key.to_owned()
                        } else {
                            format!("@picture:{key}")
                        };
                        if let Some((texture, _)) = self.texture(&ctx, p, &lookup, false, "") {
                            fit_image(&painter, &texture, image_rect);
                        } else {
                            painter.text(
                                image_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                crate::i18n::key("editor.preview.no_image"),
                                FontId::proportional(12.0),
                                Color32::DARK_GRAY,
                            );
                        }
                        let default_title = crate::i18n::key("editor.preview.event");
                        let title = picture["event_title"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(&default_title);
                        let caption = title
                            .chars()
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let font_size =
                            (240.0 / title.chars().count().max(1) as f32).min(64.0) * scale;
                        painter.text(
                            book.min + egui::vec2(786.4, 625.6) * scale,
                            egui::Align2::CENTER_CENTER,
                            caption,
                            FontId::proportional(font_size),
                            Color32::from_rgb(48, 44, 41),
                        );
                    } else if matches!(kind, "picture" | "item" | "big") {
                        let mut painter = painter.clone();
                        painter.multiply_opacity(number(picture, "preview_opacity", 1.0) as f32);
                        if kind == "big" && picture["color"] != 2 {
                            painter.rect_filled(
                                rect,
                                0.0,
                                if picture["color"] == 1 {
                                    Color32::BLACK
                                } else {
                                    Color32::WHITE
                                },
                            );
                        }
                        if kind == "big" && picture["background_only"] == true {
                            continue;
                        }
                        if kind == "big" {
                            painter.multiply_opacity(
                                number(picture, "preview_image_opacity", 1.0) as f32
                            );
                        }
                        let key = text(picture, "key");
                        let lookup = if key.starts_with("user:") {
                            key.to_owned()
                        } else {
                            format!("@picture:{key}")
                        };
                        if let Some((texture, _)) = self.texture(&ctx, p, &lookup, false, "") {
                            let target = if kind == "big" {
                                rect
                            } else {
                                rect.shrink2(rect.size() * 0.18)
                            };
                            fit_image(&painter, &texture, target);
                        } else {
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!(
                                    "{}: {}\n{}",
                                    crate::i18n::key("editor.preview.official_images"),
                                    text(picture, "key"),
                                    crate::i18n::key("editor.preview.extract_images")
                                ),
                                FontId::proportional(13.0),
                                Color32::GRAY,
                            );
                        }
                    } else if kind == "title" {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            text(picture, "key"),
                            FontId::proportional(rect.height() * 0.09),
                            Color32::WHITE,
                        );
                    }
                }
                self.camera_filter.paint_vignette(&painter, rect);
                continue;
            }
            if layer == "actors" || layer == "unfiltered_actors" {
                let mut actors: Vec<(&String, &Value)> = state["actors"]
                    .as_object()
                    .map(|o| o.iter().collect())
                    .unwrap_or_default();
                actors.sort_by(|a, b| {
                    self.anchor(text(a.1, "position"))
                        .0
                        .total_cmp(&self.anchor(text(b.1, "position")).0)
                });
                for (cid, a) in actors {
                    let excluded = filters::excludes(&state, cid);
                    if excluded != (layer == "unfiltered_actors") {
                        continue;
                    }
                    let order = if excluded {
                        &mut unfiltered_order
                    } else {
                        &mut scene_order
                    };
                    let start = order.begin(&painter);
                    self.camera_filter = if excluded {
                        filters::Settings::default()
                    } else {
                        scene_filter.clone()
                    };
                    let lookup = if cid == "player" && a["appearance"] == "beautified" {
                        "player_beautified"
                    } else {
                        cid
                    };
                    let (x, feet) = self.anchor(text(a, "position"));
                    let x = number(a, "preview_x", x as f64) as f32;
                    let feet = number(a, "preview_feet", feet as f64) as f32;
                    let opacity = number(a, "preview_opacity", 1.0).clamp(0.0, 1.0) as f32;
                    let base = Pos2::new(
                        rect.left()
                            + rect.width() * x
                            + (number(a, "offset_x", 0.0) + number(a, "preview_shock_x", 0.0))
                                as f32
                                * rect.width()
                                / 1920.0,
                        rect.top() + rect.height() * feet
                            - (number(a, "offset_y", 0.0) + number(a, "preview_shock_y", 0.0))
                                as f32
                                * rect.height()
                                / 1080.0,
                    );
                    if let Some((tex, meta)) =
                        self.texture(&ctx, p, lookup, true, text(a, "portrait"))
                    {
                        let instance_scale = number(a, "layout_scale", 100.0) as f32 / 100.0;
                        let body_scale =
                            number(&meta, "scale", 100.0) as f32 / 100.0 * instance_scale;
                        let size = tex.size_vec2();
                        let calibration =
                            lom_core::portrait_alignment::layout(&meta, text(a, "portrait"));
                        let art_left = meta["art_facing"].as_str().unwrap_or("left") != "right";
                        let flipped = (text(a, "facing") == "left") != art_left;
                        let sign = if flipped { -1.0 } else { 1.0 };
                        let dest =
                            portrait_rect(rect, base, size, body_scale, calibration, sign, &meta);
                        let brightness = if a["dimmed"].as_bool().unwrap_or(false) {
                            128
                        } else {
                            255
                        };
                        let color = Color32::from_rgba_unmultiplied(
                            brightness,
                            brightness,
                            brightness,
                            (255.0 * opacity).round() as u8,
                        );
                        sprite(
                            &painter,
                            tex.id(),
                            dest,
                            base,
                            self.character_rotation(number(a, "rotation", 0.0) as f32),
                            flipped,
                            color,
                        );
                        if self.layout_mode {
                            layout_objects.push(LayoutObject {
                                target: LayoutTarget::Character(cid.clone()),
                                bounds: self.opaque_rect(&tex, dest, flipped),
                                pivot: base,
                                angle: self.character_rotation(number(a, "rotation", 0.0) as f32),
                                value: LayoutChange {
                                    target: LayoutTarget::Character(cid.clone()),
                                    x: number(a, "offset_x", 0.0),
                                    y: number(a, "offset_y", 0.0),
                                    scale: number(a, "layout_scale", 100.0),
                                    effect_scale: None,
                                },
                                units: Vec2::new(rect.width() / 1920.0, rect.height() / 1080.0),
                            });
                        }
                        let mood_action = editable_mood(node, cid);
                        let editing_mood = mood_action.is_some() && !self.layout_mode;
                        let icon = visible_mood(&state, cid, a);
                        if icon.is_some() || editing_mood {
                            let unit = rect.height() / 1080.0
                                * if cid.starts_with("user:") {
                                    body_scale
                                } else {
                                    instance_scale
                                };
                            let sign = if flipped { -1.0 } else { 1.0 };
                            let mut mood_rect = dest;
                            if cid.starts_with("user:") {
                                if let Some(reference) = meta["portrait_reference"].as_str() {
                                    if let Some((texture, _)) =
                                        self.texture(&ctx, p, lookup, true, reference)
                                    {
                                        let size = texture.size_vec2();
                                        let value =
                                            lom_core::portrait_alignment::layout(&meta, reference);
                                        let scale = value[2] as f32 / 100.0;
                                        let fit = (rect.width() / size.x)
                                            .min(rect.height() / size.y)
                                            * body_scale
                                            * scale;
                                        mood_rect = Rect::from_min_size(
                                            Pos2::new(
                                                base.x - size.x * fit / 2.0
                                                    + sign * value[0] as f32 / 1920.0
                                                        * rect.width()
                                                        * body_scale,
                                                base.y
                                                    - rect.height() * body_scale * scale / 2.0
                                                    - size.y * fit / 2.0
                                                    - value[1] as f32 / 1080.0
                                                        * rect.height()
                                                        * body_scale,
                                            ),
                                            size * fit,
                                        );
                                    }
                                }
                            }
                            let anchor = if cid.starts_with("user:") {
                                Pos2::new(
                                    base.x
                                        + sign
                                            * (number(&meta, "mood_x", 50.0) as f32 / 100.0 - 0.5)
                                            * mood_rect.width()
                                        + mood_rect.center().x
                                        - base.x,
                                    mood_rect.bottom()
                                        - number(&meta, "mood_y", 90.0) as f32 / 100.0
                                            * mood_rect.height(),
                                )
                            } else {
                                let position = &self.mapping["characters"][lookup]["mood_position"];
                                Pos2::new(
                                    base.x
                                        + sign * position[0].as_f64().unwrap_or(0.0) as f32 * unit,
                                    base.y
                                        - rect.height() * instance_scale * 0.5
                                        - position[1].as_f64().unwrap_or(0.0) as f32 * unit,
                                )
                            };
                            let setting = &state["mood_overrides"][cid];
                            let anchor = anchor
                                + Vec2::new(
                                    sign * number(setting, "offset_x", 0.0) as f32 * unit,
                                    -number(setting, "offset_y", 0.0) as f32 * unit,
                                );
                            let rotation =
                                self.character_rotation(number(a, "rotation", 0.0) as f32);
                            if let Some(icon) = icon {
                                if let Some((texture, animation)) =
                                    self.mood_texture(&ctx, p, cid, icon, flipped)
                                {
                                    let size = Vec2::new(
                                        animation["size"][0].as_f64().unwrap_or(100.0) as f32,
                                        animation["size"][1].as_f64().unwrap_or(100.0) as f32,
                                    ) * unit;
                                    let center = anchor
                                        + Vec2::new(
                                            sign * animation["offset"][0].as_f64().unwrap_or(0.0)
                                                as f32
                                                * unit,
                                            -animation["offset"][1].as_f64().unwrap_or(0.0) as f32
                                                * unit,
                                        );
                                    sprite(
                                        &painter,
                                        texture.id(),
                                        Rect::from_center_size(center, size),
                                        base,
                                        rotation,
                                        flipped,
                                        Color32::WHITE,
                                    );
                                    if self.layout_mode && setting["enabled"] == true {
                                        layout_objects.push(LayoutObject {
                                            target: LayoutTarget::Mood(cid.clone()),
                                            bounds: self.opaque_rect(
                                                &texture,
                                                Rect::from_center_size(center, size),
                                                flipped,
                                            ),
                                            pivot: base,
                                            angle: rotation,
                                            value: LayoutChange {
                                                target: LayoutTarget::Mood(cid.clone()),
                                                x: number(setting, "offset_x", 0.0),
                                                y: number(setting, "offset_y", 0.0),
                                                scale: 100.0,
                                                effect_scale: None,
                                            },
                                            units: Vec2::new(sign * unit, unit),
                                        });
                                    }
                                }
                            }
                            if editing_mood
                                && state["blackout"]["amount"].as_f64().unwrap_or(0.0) < 1.0
                            {
                                let transform =
                                    egui::emath::Rot2::from_angle(rotation.to_radians());
                                let center = base + transform * (anchor - base);
                                let handle = Rect::from_center_size(
                                    center,
                                    Vec2::splat((140.0 * unit).max(28.0)),
                                );
                                painter.rect_stroke(
                                    handle,
                                    3.0,
                                    Stroke::new(1.5_f32, Color32::from_rgb(45, 145, 220)),
                                    egui::StrokeKind::Outside,
                                );
                                if let Some(label) = &mood_target {
                                    handles::adjustment_badge(
                                        &painter,
                                        rect,
                                        handle.left_top(),
                                        label,
                                    );
                                }
                                ui.interact(
                                    handle.intersect(rect),
                                    ui.id().with(("mood-drag", cid)),
                                    egui::Sense::drag(),
                                )
                                .on_hover_text(crate::i18n::key("editor.preview.bubble_drag"))
                                .on_hover_cursor(egui::CursorIcon::Grab);
                                // A quick drag can start and finish in one native
                                // frame, after PointerState has cleared press_origin.
                                let press = ui.input(|input| {
                                    input.events.iter().find_map(|event| match event {
                                        egui::Event::PointerButton {
                                            pos,
                                            button: egui::PointerButton::Primary,
                                            pressed: true,
                                            ..
                                        } if handle.intersect(rect).contains(*pos) => Some(*pos),
                                        _ => None,
                                    })
                                });
                                if let Some(origin) = press {
                                    if ui.is_enabled()
                                        && ctx.layer_id_at(origin) == Some(ui.layer_id())
                                    {
                                        let (action, mood) = mood_action.unwrap();
                                        self.mood_drag = Some(MoodDrag {
                                            character: cid.clone(),
                                            action,
                                            origin,
                                            x: number(mood, "offset_x", 0.0),
                                            y: number(mood, "offset_y", 0.0),
                                        });
                                    }
                                }
                                if let Some(drag) = self
                                    .mood_drag
                                    .as_ref()
                                    .filter(|drag| drag.character == *cid)
                                {
                                    let position = ui
                                        .input(|input| input.pointer.latest_pos())
                                        .unwrap_or(drag.origin);
                                    let delta = transform.inverse() * (position - drag.origin);
                                    let next = (
                                        (drag.x + (delta.x / unit / sign) as f64)
                                            .clamp(-1920.0, 1920.0),
                                        (drag.y - (delta.y / unit) as f64).clamp(-1080.0, 1080.0),
                                    );
                                    let (_, mood) = mood_action.unwrap();
                                    if next
                                        != (
                                            number(mood, "offset_x", 0.0),
                                            number(mood, "offset_y", 0.0),
                                        )
                                    {
                                        self.mood_edit = Some(MoodEdit {
                                            action: drag.action,
                                            x: next.0,
                                            y: next.1,
                                        });
                                    }
                                }
                                if !ui.input(|input| input.pointer.primary_down())
                                    && self
                                        .mood_drag
                                        .as_ref()
                                        .is_some_and(|drag| drag.character == *cid)
                                {
                                    self.mood_drag = None;
                                }
                            }
                        } else {
                            self.mood_clocks.remove(cid);
                        }
                    } else {
                        let h = rect.height() * 0.70;
                        let actor = Rect::from_min_size(
                            Pos2::new(base.x - h * 0.19, base.y - h),
                            Vec2::new(h * 0.38, h),
                        );
                        painter.rect_filled(
                            actor,
                            20.0,
                            Color32::from_rgb(78, 88, 83).gamma_multiply(opacity),
                        );
                        painter.text(
                            actor.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{}\n{}", cid, text(a, "portrait")),
                            FontId::proportional(12.0),
                            Color32::LIGHT_GRAY.gamma_multiply(opacity),
                        );
                    }
                    order.capture(
                        &painter,
                        start,
                        lom_core::render_priority::priority(priorities, "character", cid, ""),
                    );
                }
            } else if let Some(overlays) = state["overlays"].as_object() {
                for (slot, item) in overlays
                    .iter()
                    .filter(|(_, n)| n["layer"].as_str().unwrap_or("front") == layer)
                {
                    let start = scene_order.begin(&painter);
                    if let Some(object) = self.overlay(
                        &ctx,
                        &painter,
                        p,
                        item,
                        rect,
                        LayoutTarget::Overlay(slot.clone()),
                    ) {
                        layout_objects.push(object);
                    }
                    scene_order.capture(
                        &painter,
                        start,
                        lom_core::render_priority::priority(priorities, "overlay", slot, ""),
                    );
                }
            }
        }
        ShapeOrder::finish(&painter, [scene_order, unfiltered_order]);
        if sorting {
            layout_objects.sort_by_key(|object| {
                let (kind, key, tag, lane) = match &object.target {
                    LayoutTarget::Character(id) | LayoutTarget::Mood(id) => {
                        ("character", id.as_str(), "", filters::excludes(&state, id))
                    }
                    LayoutTarget::Overlay(slot) => ("overlay", slot.as_str(), "", false),
                    LayoutTarget::Cg => ("custom_cg", "", "", false),
                    LayoutTarget::Effect { .. } => {
                        let item = state["effects"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|item| effects::target(item).as_ref() == Some(&object.target));
                        item.map_or(("effect", "", "", false), |item| {
                            (
                                "effect",
                                text(item, "name"),
                                item["tag"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or(text(item, "name")),
                                false,
                            )
                        })
                    }
                };
                (
                    lane,
                    lom_core::render_priority::priority(priorities, kind, key, tag),
                )
            });
        }
        if let Some(dialog) = state["dialog"].as_object() {
            let txt = dialog.get("text").and_then(Value::as_str).unwrap_or("");
            let mode = dialog
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("character");
            let dr = if mode == "center" {
                rect.shrink2(Vec2::new(rect.width() * 0.08, rect.height() * 0.25))
            } else {
                Rect::from_min_max(
                    Pos2::new(rect.left() + 12.0, rect.bottom() - rect.height() * 0.28),
                    Pos2::new(rect.right() - 12.0, rect.bottom() - 9.0),
                )
            };
            painter.rect_filled(dr, 5.0, Color32::from_black_alpha(195));
            let character = dialog
                .get("character")
                .and_then(Value::as_str)
                .unwrap_or("");
            if mode == "character" || mode == "think" || mode == "offscreen" {
                if mode != "offscreen" {
                    let title = preview_character_title(p, &state, character);
                    if !title.is_empty() {
                        painter.text(
                            dr.left_top() + Vec2::new(12.0, -8.0),
                            egui::Align2::LEFT_BOTTOM,
                            title,
                            FontId::proportional(11.0),
                            Color32::from_rgb(246, 241, 221),
                        );
                    }
                }
                painter.text(
                    dr.left_top() + Vec2::new(12.0, 9.0),
                    egui::Align2::LEFT_TOP,
                    if mode == "offscreen" {
                        dialog
                            .get("speaker_name")
                            .and_then(Value::as_str)
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or("？？？")
                            .to_owned()
                    } else {
                        state["character_names"]
                            .get(character)
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(|| character_display_name(p, character))
                    },
                    FontId::proportional(13.0),
                    Color32::from_rgb(221, 168, 110),
                );
            }
            let font_size = dialog
                .get("font_size")
                .and_then(Value::as_i64)
                .unwrap_or(match dialog.get("font_size").and_then(Value::as_str) {
                    Some("small") => 24,
                    Some("large") => 60,
                    _ => 0,
                });
            let font_scale = if font_size > 0 {
                font_size as f32 / 40.0
            } else {
                1.0
            };
            let centered = mode == "center"
                && dialog.get("text_align").and_then(Value::as_str) != Some("left");
            let left_margin = if matches!(mode, "narrative" | "center")
                && dialog.get("text_align").and_then(Value::as_str) == Some("left")
            {
                dr.width() * 0.05
            } else {
                12.0
            };
            let mut job = egui::text::LayoutJob::simple(
                txt.to_owned(),
                FontId::proportional((rect.width() / 38.0).clamp(12.0, 19.0) * font_scale),
                Color32::from_rgb(246, 241, 221),
                dr.width() - left_margin * 2.0,
            );
            job.halign = if centered {
                egui::Align::Center
            } else {
                egui::Align::Min
            };
            let galley = painter.layout_job(job);
            painter.galley(
                dr.left_top()
                    + Vec2::new(
                        if centered {
                            dr.width() / 2.0
                        } else {
                            left_margin
                        },
                        if mode == "character" || mode == "think" || mode == "offscreen" {
                            30.0
                        } else {
                            12.0
                        },
                    ),
                galley,
                Color32::WHITE,
            );
        }
        if let Some(card) = card_data(p, node) {
            if card.kind == "ending" {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(9, 12, 12));
                let band = relative(rect, 0.0, 0.22, 1.0, 0.57);
                painter.rect_filled(band, 0.0, Color32::from_rgb(229, 213, 177));
                let book = relative(rect, 0.15, 0.10, 0.38, 0.78);
                painter.rect_filled(book, 0.0, Color32::from_rgb(200, 166, 107));
                let cover = book.shrink2(Vec2::new(book.width() * 0.08, book.height() * 0.04));
                painter.rect_filled(cover, 0.0, Color32::from_rgb(224, 204, 163));
                painter.rect_stroke(
                    book,
                    0.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(111, 77, 42)),
                    egui::StrokeKind::Inside,
                );
                let art = cover.shrink2(Vec2::new(cover.width() * 0.08, cover.height() * 0.10));
                if let Some((tex, _)) = self.texture(&ctx, p, &card.image, false, "") {
                    fit_image(&painter, &tex, art);
                } else {
                    paint_text(
                        &painter,
                        art,
                        &crate::i18n::key("editor.preview.ending_image"),
                        rect.height() * 0.035,
                        Color32::from_rgb(80, 64, 45),
                    );
                }
                paint_text(
                    &painter,
                    relative(rect, 0.56, 0.31, 0.37, 0.14),
                    &card.title,
                    rect.height() * 0.075,
                    Color32::from_rgb(151, 48, 38),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.56, 0.48, 0.37, 0.24),
                    &card.body,
                    rect.height() * 0.036,
                    Color32::from_rgb(37, 31, 24),
                );
            } else if card.kind == "intro" {
                painter.rect_filled(rect, 0.0, Color32::from_black_alpha(125));
                let panel = relative(rect, 0.35, 0.33, 0.50, 0.49);
                painter.rect_filled(panel, 4.0, Color32::from_rgba_unmultiplied(25, 25, 29, 239));
                painter.rect_stroke(
                    panel,
                    4.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(164, 137, 91)),
                    egui::StrokeKind::Inside,
                );
                let scale = number(node, "image_scale", 100.0) as f32 / 100.0;
                let x = number(node, "image_x", 0.0).clamp(-30.0, 30.0) as f32 / 100.0;
                let y = number(node, "image_y", 0.0).clamp(-30.0, 30.0) as f32 / 100.0;
                let avatar = Rect::from_center_size(
                    Pos2::new(
                        rect.left() + rect.width() * (0.31 + x),
                        rect.top() + rect.height() * (0.50 - y),
                    ),
                    Vec2::new(rect.width() * 0.30 * scale, rect.height() * 0.62 * scale),
                );
                if let Some((tex, _)) = self.texture(&ctx, p, &card.image, false, "") {
                    fit_image(&painter, &tex, avatar);
                } else {
                    painter.rect_filled(
                        avatar,
                        2.0,
                        Color32::from_rgba_unmultiplied(24, 27, 30, 170),
                    );
                    paint_text(
                        &painter,
                        avatar.shrink(12.0),
                        &card.placeholder,
                        rect.height() * 0.035,
                        Color32::from_rgb(220, 210, 187),
                    );
                }
                paint_text(
                    &painter,
                    relative(rect, 0.52, 0.40, 0.29, 0.075),
                    &card.title,
                    rect.height() * 0.033,
                    Color32::from_rgb(213, 184, 122),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.52, 0.48, 0.29, 0.10),
                    &card.name,
                    rect.height() * 0.070,
                    Color32::from_rgb(235, 78, 56),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.52, 0.61, 0.29, 0.17),
                    &card.body,
                    rect.height() * 0.031,
                    Color32::from_rgb(235, 229, 216),
                );
            } else {
                painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(15, 12, 14, 247));
                paint_text(
                    &painter,
                    relative(rect, 0.15, 0.22, 0.70, 0.18),
                    &card.title,
                    rect.height() * 0.10,
                    Color32::from_rgb(197, 78, 67),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.20, 0.48, 0.60, 0.28),
                    &card.body,
                    rect.height() * 0.045,
                    Color32::from_rgb(231, 221, 202),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.28, 0.84, 0.6, 0.10),
                    &crate::i18n::message(
                        "editor.preview.death_next",
                        &[
                            ("id", text(node, "death_id").into()),
                            ("next", text(node, "next").into()),
                        ],
                    ),
                    rect.height() * 0.028,
                    Color32::GRAY,
                );
            }
        }
        let mut choice = None;
        let covered = number(&state["blackout"], "amount", 0.0);
        if let Some(options) = state["choice"]
            .as_array()
            .filter(|_| covered == 0.0 && !self.layout_mode)
        {
            for (i, item) in options.iter().enumerate() {
                let button = Rect::from_min_size(
                    Pos2::new(
                        rect.left() + rect.width() * 0.25,
                        rect.top() + rect.height() * 0.30 + i as f32 * 35.0,
                    ),
                    Vec2::new(rect.width() * 0.5, 30.0),
                );
                if ui
                    .put(button, egui::Button::new(text(item, "text")))
                    .clicked()
                {
                    choice = Some(text(item, "goto").to_owned());
                }
            }
        }
        if covered > 0.0 {
            painter.rect_filled(
                blackout_rect(rect, covered, text(&state["blackout"], "dir")),
                0.0,
                Color32::BLACK,
            );
        }
        if covered == 0.0
            && !state["official_pictures"]
                .as_object()
                .is_some_and(|images| !images.is_empty())
            && !matches!(text(node, "type"), "intro" | "endgame" | "death")
        {
            self.layout_handles(ui, rect, &layout_objects, p, &state);
        } else {
            self.layout_drag = None;
        }
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, Color32::from_rgb(173, 161, 134)),
            egui::StrokeKind::Inside,
        );
        if !state["reached"].as_bool().unwrap_or(false) {
            ui.painter().text(
                outer.left_bottom() + Vec2::new(8.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                crate::i18n::key("editor.preview.unreachable"),
                FontId::proportional(11.0),
                Color32::LIGHT_RED,
            );
        }
        self.camera_filter = filters::Settings::default();
        choice
    }
    pub fn show_portrait(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        story: &Value,
        selected: usize,
    ) {
        let resolved = portrait_preview_node(story, selected);
        let node = &resolved;
        let cid = text(node, "character");
        if cid.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(crate::i18n::key("portrait.choose_character"));
            });
            return;
        }
        let portrait = node["portrait"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("normal");
        let lookup = if cid == "player" && node["appearance"] == "beautified" {
            "player_beautified"
        } else {
            cid
        };
        ui.label(egui::RichText::new(character_display_name(project, cid)).strong())
            .on_hover_text(cid);
        let portrait_name = if cid.starts_with("user:") {
            portrait.to_owned()
        } else {
            crate::i18n::term("portraits", portrait, portrait)
        };
        ui.label(
            egui::RichText::new(portrait_name)
                .small()
                .color(Color32::GRAY),
        )
        .on_hover_text(portrait);
        let texture = self.texture(ui.ctx(), project, lookup, true, portrait);
        if texture.is_none() {
            ui.add_space(12.0);
            ui.label(crate::i18n::tr(
                "尚未载入这个立绘的预览图片。试玩仍可直接使用游戏中的素材。",
            ));
            if ui.button(crate::i18n::tr("从游戏提取预览素材…")).clicked() {
                self.extract_requested = true;
                ui.ctx().request_repaint();
            }
            ui.small(crate::i18n::tr(
                "已有素材库：文件 → 选择预览素材库…，选择包含 preview_map.json 的文件夹。",
            ));
            return;
        }
        if !cid.starts_with("user:") {
            self.portrait_canvas_notice(ui);
        }
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(18, 21, 31, 225));
        if let Some((texture, _)) = texture {
            fit_image(&painter, &texture, rect.shrink(20.0));
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::key("portrait.no_assets"),
                FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
    }
    fn overlay(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        p: &Project,
        n: &Value,
        rect: Rect,
        target: LayoutTarget,
    ) -> Option<LayoutObject> {
        if let Some((tex, _)) = self.texture(ctx, p, text(n, "image"), false, "") {
            let scale = number(n, "scale", 100.0) as f32 / 100.0;
            let aspect = tex.size_vec2();
            let fit = (rect.width() / aspect.x).min(rect.height() / aspect.y) * scale;
            let size = aspect * fit;
            let mut center = rect.center()
                + Vec2::new(
                    number(n, "x", 0.0) as f32 / 100.0 * rect.width(),
                    -number(n, "y", 0.0) as f32 / 100.0 * rect.height(),
                );
            let position = text(n, "position");
            if position.contains("left") {
                center.x -= rect.width() * 0.32;
            }
            if position.contains("right") {
                center.x += rect.width() * 0.32;
            }
            if position.contains("top") {
                center.y -= rect.height() * 0.32;
            }
            if position.contains("bottom") {
                center.y += rect.height() * 0.32;
            }
            painter.image(
                tex.id(),
                Rect::from_center_size(center, size),
                uv(),
                Color32::from_white_alpha(
                    (number(n, "opacity", 100.0) * 2.55).clamp(0.0, 255.0) as u8
                ),
            );
            return self.layout_mode.then(|| LayoutObject {
                bounds: self.opaque_rect(&tex, Rect::from_center_size(center, size), false),
                pivot: center,
                angle: 0.0,
                value: LayoutChange {
                    target: target.clone(),
                    x: number(n, "x", 0.0),
                    y: number(n, "y", 0.0),
                    scale: number(n, "scale", 100.0),
                    effect_scale: None,
                },
                target,
                units: Vec2::new(rect.width() / 100.0, rect.height() / 100.0),
            });
        }
        None
    }
}
// Capture only stage objects, then insert their stable priority order at the
// first object in each camera lane. Keep each shape's original clipping and
// leave dialogue, official panels and blackout outside these ranges.
#[derive(Default)]
struct ShapeOrder {
    active: bool,
    ranges: Vec<(usize, usize, i64)>,
}
impl ShapeOrder {
    fn new(active: bool) -> Self {
        Self {
            active,
            ranges: Vec::new(),
        }
    }
    fn begin(&self, painter: &egui::Painter) -> Option<usize> {
        self.active.then(|| painter.add(egui::Shape::Noop).0)
    }
    fn capture(&mut self, painter: &egui::Painter, start: Option<usize>, priority: i64) {
        if let Some(start) = start {
            self.ranges
                .push((start, painter.add(egui::Shape::Noop).0, priority));
        }
    }
    fn finish(painter: &egui::Painter, mut lanes: [Self; 2]) {
        let mut spans: Vec<_> = lanes
            .iter()
            .flat_map(|lane| lane.ranges.iter().map(|&(a, b, _)| (a, b)))
            .collect();
        spans.sort_unstable();
        let (Some(&(first, _)), Some(&(_, last))) = (spans.first(), spans.last()) else {
            return;
        };
        painter.ctx().graphics_mut(|graphics| {
            let list = graphics.entry(painter.layer_id());
            let shapes: Vec<_> = list.all_entries().cloned().collect();
            let mut inserts = Vec::new();
            for lane in &mut lanes {
                if let Some(&(start, _, _)) = lane.ranges.first() {
                    lane.ranges.sort_by_key(|&(_, _, priority)| priority);
                    let ordered: Vec<_> = lane
                        .ranges
                        .iter()
                        .flat_map(|&(a, b, _)| shapes[a + 1..b].iter().cloned())
                        .collect();
                    inserts.push((start, ordered));
                }
            }
            let mut result = Vec::new();
            let mut span = 0;
            let mut index = first;
            while index <= last {
                if let Some((_, items)) = inserts.iter_mut().find(|(start, _)| *start == index) {
                    result.append(items);
                }
                if span < spans.len() && spans[span].0 == index {
                    index = spans[span].1 + 1;
                    span += 1;
                } else {
                    result.push(shapes[index].clone());
                    index += 1;
                }
            }
            result.resize_with(last - first + 1, || egui::epaint::ClippedShape {
                clip_rect: painter.clip_rect(),
                shape: egui::Shape::Noop,
            });
            for (offset, shape) in result.into_iter().enumerate() {
                list.set(
                    egui::layers::ShapeIdx(first + offset),
                    shape.clip_rect,
                    shape.shape,
                );
            }
        });
    }
}

fn uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}

#[cfg(test)]
mod render_priority_tests {
    use super::*;

    #[test]
    fn render_priority_f5_preserves_future_targets_and_hide_all_clears_rules() {
        let mut story = json!({"id":"main","story_schema":2,"start":"priority","nodes":[
            {"id":"priority","type":"render_priority","target":"character","character":"player","order":12,"wait_after":12},
            {"id":"show","type":"show","character":"player","position":"M","portrait":"normal"},
            {"id":"hide","type":"hide","character":"player"},
            {"id":"future","type":"wait","seconds":1},
            {"id":"clear","type":"hide_all"},
            {"id":"done","type":"end"}]});
        let prelude = build_playtest_prelude(&story, "future");
        assert_eq!(prelude[0]["type"], "render_priority");
        assert_eq!(prelude[0]["order"], 12);
        assert!(prelude[0].get("wait_after").is_none());
        assert!(!prelude.iter().any(|node| node["type"] == "show"));
        assert_eq!(
            simulate_until(&story, "done", false)["render_priorities"],
            json!([])
        );
        story["nodes"][1] = json!({"id":"show","type":"effect","name":"Hit_001","c":0});
        assert!(effects::managed(&story));
    }

    #[test]
    fn render_priority_paint_reorders_stage_without_crossing_ui_or_losing_clips() {
        let ctx = egui::Context::default();
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let clipped = stage.shrink(10.0);
        let output = ctx.run(Default::default(), |ctx| {
            let painter = ctx
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(stage);
            let mut scene = ShapeOrder::new(true);
            let mut unfiltered = ShapeOrder::new(true);
            for (color, priority) in [(Color32::BLUE, 0), (Color32::GREEN, 30)] {
                let start = scene.begin(&painter);
                painter.rect_filled(stage, 0.0, color);
                scene.capture(&painter, start, priority);
            }
            painter.rect_filled(stage, 0.0, Color32::YELLOW);
            let start = scene.begin(&painter);
            painter
                .with_clip_rect(clipped)
                .rect_filled(stage, 0.0, Color32::RED);
            scene.capture(&painter, start, 10);
            let start = unfiltered.begin(&painter);
            painter.rect_filled(stage, 0.0, Color32::WHITE);
            unfiltered.capture(&painter, start, -100);
            painter.rect_filled(stage, 0.0, Color32::BLACK);
            ShapeOrder::finish(&painter, [scene, unfiltered]);
        });
        let rectangles: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) => Some((rect.fill, shape.clip_rect)),
                _ => None,
            })
            .collect();
        assert_eq!(
            rectangles
                .iter()
                .map(|&(color, _)| color)
                .collect::<Vec<_>>(),
            [
                Color32::BLUE,
                Color32::RED,
                Color32::GREEN,
                Color32::YELLOW,
                Color32::WHITE,
                Color32::BLACK
            ]
        );
        assert_eq!(rectangles[1].1, clipped);
    }
}
fn portrait_rect(
    stage: Rect,
    base: Pos2,
    size: Vec2,
    body_scale: f32,
    calibration: [f64; 3],
    sign: f32,
    meta: &Value,
) -> Rect {
    let geometry = &meta["_preview_geometry"];
    let canvas = Vec2::new(
        geometry["canvas"][0].as_f64().unwrap_or(0.0) as f32,
        geometry["canvas"][1].as_f64().unwrap_or(0.0) as f32,
    );
    let image = Rect::from_min_size(
        Pos2::new(
            geometry["image"][0].as_f64().unwrap_or(0.0) as f32,
            geometry["image"][1].as_f64().unwrap_or(0.0) as f32,
        ),
        Vec2::new(
            geometry["image"][2].as_f64().unwrap_or(0.0) as f32,
            geometry["image"][3].as_f64().unwrap_or(0.0) as f32,
        ),
    );
    // Legacy PNGs contain the trimmed image. Match its aspect before applying
    // geometry so an already restored canvas is never padded a second time.
    let legacy = canvas.min_elem() > 0.0
        && image.is_positive()
        && ((size.x / size.y - image.width() / image.height()) * size.y).abs() <= 2.0;
    let (canvas, image) = if legacy {
        (canvas, image)
    } else {
        (size, Rect::from_min_size(Pos2::ZERO, size))
    };
    let scale = calibration[2] as f32 / 100.0;
    let factor = (stage.width() / canvas.x).min(stage.height() / canvas.y) * body_scale * scale;
    let top_left = Pos2::new(
        base.x - canvas.x * factor / 2.0
            + sign * calibration[0] as f32 / 1920.0 * stage.width() * body_scale,
        base.y
            - stage.height() * body_scale * scale / 2.0
            - canvas.y * factor / 2.0
            - calibration[1] as f32 / 1080.0 * stage.height() * body_scale,
    );
    let x = if sign < 0.0 {
        canvas.x - image.right()
    } else {
        image.left()
    };
    Rect::from_min_size(
        top_left + Vec2::new(x, image.top()) * factor,
        image.size() * factor,
    )
}
fn visible_mood<'a>(state: &'a Value, cid: &str, actor: &'a Value) -> Option<&'a str> {
    if actor["dimmed"] == true
        || actor["preview_hide_mood"] == true
        || number(actor, "preview_opacity", 1.0) < 0.999
    {
        return None;
    }
    let dialogue = &state["dialogue_mood"];
    if dialogue["character"] == cid {
        return match dialogue["icon"].as_str()? {
            "hide" => None,
            "auto" => actor["portrait"].as_str(),
            icon => Some(icon),
        };
    }
    let setting = &state["mood_overrides"][cid];
    if setting.is_object() {
        if setting["enabled"] != true {
            return None;
        }
        if setting["icon"].as_str().unwrap_or("auto") != "auto" {
            return setting["icon"].as_str();
        }
    }
    (state["mood_enabled"] == true)
        .then(|| actor["portrait"].as_str())
        .flatten()
}

fn sprite(
    p: &egui::Painter,
    id: egui::TextureId,
    rect: Rect,
    pivot: Pos2,
    degrees: f32,
    flip: bool,
    color: Color32,
) {
    let rotation = egui::emath::Rot2::from_angle(degrees.to_radians());
    let mut mesh = egui::Mesh::with_texture(id);
    let positions = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let uvs = if flip {
        [
            Pos2::new(1.0, 0.0),
            Pos2::new(0.0, 0.0),
            Pos2::new(0.0, 1.0),
            Pos2::new(1.0, 1.0),
        ]
    } else {
        [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ]
    };
    for i in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: pivot + rotation * (positions[i] - pivot),
            uv: uvs[i],
            color,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod rotation_direction_tests {
    use super::*;
    #[test]
    fn positive_character_angle_is_counterclockwise_with_an_opt_in_legacy_preview() {
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        for legacy in [false, true] {
            preview.legacy_character_rotation = legacy;
            for flipped in [false, true] {
                let ctx = egui::Context::default();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(300.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            sprite(
                                ui.painter(),
                                egui::TextureId::User(44),
                                Rect::from_min_max(
                                    Pos2::new(100.0, 100.0),
                                    Pos2::new(110.0, 120.0),
                                ),
                                Pos2::new(100.0, 100.0),
                                preview.character_rotation(90.0),
                                flipped,
                                Color32::WHITE,
                            );
                        });
                    },
                );
                let point = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if mesh.texture_id == egui::TextureId::User(44) => {
                            Some(mesh.vertices[1].pos)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!((point.x - 100.0).abs() < 0.001);
                assert!((point.y - if legacy { 110.0 } else { 90.0 }).abs() < 0.001);
            }
        }
    }
}
#[derive(Debug)]
struct CardData {
    kind: &'static str,
    title: String,
    name: String,
    body: String,
    image: String,
    placeholder: String,
}
fn card_data(project: &Project, node: &Value) -> Option<CardData> {
    let kind = text(node, "type");
    let mut card = CardData {
        kind: "",
        title: String::new(),
        name: String::new(),
        body: String::new(),
        image: String::new(),
        placeholder: String::new(),
    };
    let official = lom_core::validate::editor_data();
    if kind == "goto_scene" && text(node, "scene") == "End" {
        card.kind = "ending";
        card.title = text(node, "title").into();
        if card.title.is_empty() {
            card.title = crate::i18n::key("editor.preview.ending_title");
        }
        card.body = text(node, "desc").into();
        card.image = text(node, "image").into();
    } else if kind == "death" {
        card.kind = "death";
        card.title = text(node, "title").into();
        if card.title.is_empty() {
            card.title = official["death_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| v["id"] == node["death_id"])
                .and_then(|v| v["name"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| crate::i18n::key("editor.preview.death_title"));
        }
        card.body = text(node, "text").into();
    } else if kind == "intro" {
        card.kind = "intro";
        match text(node, "intro_source") {
            "custom" => {
                card.title = text(node, "title").into();
                card.name = text(node, "name").into();
                card.body = text(node, "text").into();
                card.image = text(node, "image").into();
                card.placeholder = crate::i18n::key("editor.preview.custom_image_missing");
            }
            "character" => {
                let id = text(node, "character").trim_start_matches("user:");
                card.name = text(node, "character").into();
                card.body = crate::i18n::key("editor.preview.custom_intro_missing");
                card.placeholder = crate::i18n::key("editor.preview.custom_intro_image_missing");
                for (path, bytes) in &project.assets {
                    if !path.ends_with("/content.json") {
                        continue;
                    }
                    let Ok(meta) = serde_json::from_slice::<Value>(bytes) else {
                        continue;
                    };
                    if meta["id"] != id {
                        continue;
                    }
                    let intro = &meta["intro"];
                    card.title = intro["title"]
                        .as_str()
                        .or_else(|| meta["title"].as_str())
                        .unwrap_or("")
                        .into();
                    card.name = intro["name"]
                        .as_str()
                        .or_else(|| meta["name"].as_str())
                        .unwrap_or(id)
                        .into();
                    if let Some(body) = intro["text"].as_str() {
                        card.body = body.into();
                    }
                    if let Some(image) = intro["image"].as_str() {
                        if let Some(parent) = Path::new(path).parent() {
                            card.image = parent.join(image).to_string_lossy().replace('\\', "/");
                        }
                    }
                    break;
                }
            }
            _ => {
                let id = text(node, "character");
                let entry = official["characters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|v| v["id"] == id);
                card.title = entry
                    .and_then(|v| v["title"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| crate::i18n::key("editor.preview.official_profile"));
                card.name = character_display_name(project, id);
                card.body = entry
                    .and_then(|v| v["intro"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| crate::i18n::key("editor.preview.official_intro_missing"));
                card.placeholder = crate::i18n::key("editor.preview.official_portrait");
            }
        }
    } else {
        return None;
    }
    Some(card)
}
fn relative(rect: Rect, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        ),
        Vec2::new(rect.width() * w, rect.height() * h),
    )
}
fn paint_text(painter: &egui::Painter, rect: Rect, text: &str, size: f32, color: Color32) {
    let galley = painter.layout(
        text.into(),
        FontId::proportional(size.max(10.0)),
        color,
        rect.width(),
    );
    painter
        .with_clip_rect(rect)
        .galley(rect.left_top(), galley, color);
}
fn fit_image(painter: &egui::Painter, texture: &TextureHandle, rect: Rect) {
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    painter.image(
        texture.id(),
        Rect::from_center_size(rect.center(), size * scale),
        uv(),
        Color32::WHITE,
    );
}

pub fn graph(
    ui: &mut egui::Ui,
    project: &Project,
    story: &Value,
    selected: usize,
) -> Option<usize> {
    flow_graph::show(ui, project, story, selected)
}
#[cfg(test)]
mod tests {
    #[test]
    fn big_picture_preview_reveals_background_before_image_and_restores_without_replay() {
        let story = json!({"id":"main","start":"big","nodes":[
            {"id":"big","type":"cg","kind":"big","action":"show","key":"user:demo.big","color":0,"fade_in":2},
            {"id":"hold","type":"wait","seconds":1},
            {"id":"blank","type":"cg","kind":"big","action":"show","background_only":true,"color":0,"fade_in":2},
            {"id":"hide","type":"cg","kind":"big","action":"hide"}, {"id":"end","type":"end"}]});
        let first = simulate_frame(&story, "big", 1.0, &json!({}));
        assert_eq!(first["official_pictures"]["big"]["preview_opacity"], 0.75);
        assert_eq!(
            first["official_pictures"]["big"]["preview_image_opacity"],
            0.0
        );
        let image = simulate_frame(&story, "big", 3.0, &json!({}));
        assert_eq!(image["official_pictures"]["big"]["preview_opacity"], 1.0);
        assert_eq!(
            image["official_pictures"]["big"]["preview_image_opacity"],
            0.75
        );
        let prelude = build_playtest_prelude(&story, "hold");
        assert!(prelude
            .iter()
            .any(|n| n["kind"] == "big" && n["key"] == "user:demo.big" && n["fade_in"] == 0));
        let blank = simulate_frame(&story, "blank", 1.0, &json!({}));
        assert_eq!(blank["official_pictures"]["big"]["background_only"], true);
        let end = simulate(&story, "end");
        assert!(end["official_pictures"].get("big").is_none());
    }

    #[test]
    fn names_and_titles_change_independently_and_both_restore_in_playtest() {
        let story = json!({"id":"main","start":"title","nodes":[
            {"id":"title","type":"character_title","character":"player","action":"set","title":"唐门新秀"},
            {"id":"name","type":"character_title","character":"player","action":"keep","name_action":"set","name":"叶云舟"},
            {"id":"say","type":"say","character":"player","text":"好"},
            {"id":"reset","type":"character_title","character":"player","action":"keep","name_action":"reset"},
            {"id":"end","type":"end"}]});
        let state = simulate_until(&story, "say", true);
        assert_eq!(state["character_names"]["player"], "叶云舟");
        assert_eq!(state["character_titles"]["player"], "唐门新秀");
        let prelude = build_playtest_prelude(&story, "say");
        assert!(prelude
            .iter()
            .any(|n| n["name"] == "叶云舟" && n["action"] == "keep"));
        assert!(prelude.iter().any(|n| n["title"] == "唐门新秀"));
        let reset = simulate_until(&story, "end", true);
        assert!(reset["character_names"].get("player").is_none());
        assert_eq!(reset["character_titles"]["player"], "唐门新秀");
    }
    #[test]
    fn titles_restore_for_playtest_and_reset_to_character_defaults() {
        let story = json!({"id":"main","start":"title","nodes":[
            {"id":"title","type":"character_title","character":"player","action":"set","title":"唐门新秀"},
            {"id":"hide","type":"character_title","character":"brother1","action":"hide"},
            {"id":"say","type":"say","mode":"character","character":"player","text":"好。"},
            {"id":"reset","type":"character_title","character":"player","action":"reset"},
            {"id":"end","type":"end"}]});
        let state = simulate_until(&story, "say", true);
        assert_eq!(state["character_titles"]["player"], "唐门新秀");
        assert_eq!(state["character_titles"]["brother1"], "");
        let prelude = build_playtest_prelude(&story, "say");
        assert!(prelude
            .iter()
            .any(|n| n["character"] == "player" && n["action"] == "set"));
        assert!(prelude
            .iter()
            .any(|n| n["character"] == "brother1" && n["action"] == "hide"));
        let reset = simulate_until(&story, "end", true);
        assert!(reset["character_titles"].get("player").is_none());
        assert_eq!(
            playback_duration(&json!({"type":"character_title","wait_after":0.5})),
            0.5
        );
    }
    #[test]
    fn event_book_fades_as_a_whole_and_playtest_restoration_does_not_replay_the_fade() {
        let positions = json!({});
        let story = json!({"id":"main","start":"book","nodes":[
            {"id":"book","type":"cg","kind":"event","action":"show","key":"girlroom_work_002","event_text":"淡入后读信","fade_in":0.8},
            {"id":"later","type":"wait","seconds":1}, {"id":"end","type":"end"}
        ]});
        let half = simulate_frame(&story, "book", 0.4, &positions);
        assert_eq!(half["official_pictures"]["event"]["preview_opacity"], 0.5);
        assert!(half["dialog"].is_null());
        let done = simulate_frame(&story, "book", 0.9, &positions);
        assert_eq!(done["official_pictures"]["event"]["preview_opacity"], 1.0);
        assert_eq!(done["dialog"]["text"], "淡入后读信");
        let prelude = build_playtest_prelude(&story, "later");
        let restored = prelude.iter().find(|node| node["kind"] == "event").unwrap();
        assert!(restored.get("fade_in").is_none() && restored.get("event_text").is_none());
        let mut old = story;
        old["nodes"][0].as_object_mut().unwrap().remove("fade_in");
        assert_eq!(
            simulate_frame(&old, "book", 0.0, &positions)["official_pictures"]["event"]
                ["preview_opacity"],
            1.0
        );
    }
    #[test]
    fn movement_portrait_tab_inherits_the_selected_actors_current_expression() {
        for character in ["player", "user:demo.actor"] {
            let story = json!({"start":"enter","nodes":[
                {"id":"enter","type":"show","character":character,"portrait":"shy","appearance":"beautified"},
                {"id":"other","type":"show","character":"other_actor","portrait":"normal"},
                {"id":"move","type":"move","character":character,"from":"L1","to":"M"},
                {"id":"change","type":"portrait","character":character,"portrait":"angry1"},
                {"id":"offset","type":"offset","character":character,"x":30,"y":0}
            ]});
            let before = story.clone();
            assert_eq!(portrait_preview_node(&story, 2)["portrait"], "shy");
            assert_eq!(portrait_preview_node(&story, 2)["appearance"], "beautified");
            assert_eq!(portrait_preview_node(&story, 3)["portrait"], "angry1");
            assert_eq!(portrait_preview_node(&story, 4)["portrait"], "angry1");
            assert_eq!(
                simulate(&story, "offset")["actors"][character]["portrait"],
                "angry1"
            );
            assert_eq!(story, before);
        }
    }
    #[test]
    fn parallel_bubble_and_shock_are_visible_together_and_restored_for_playtest() {
        use super::*;
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json")).unwrap();
        for character in ["player", "user:demo.actor"] {
            let story = json!({"id":"main","start":"enter","nodes":[
                {"id":"enter","type":"show","character":character,"position":"M"},
                {"id":"together","type":"parallel","actions":[
                    {"type":"shock","character":character,"duration":0.5},
                    {"type":"mood","character":character,"enabled":true,"icon":"shock","wait_after":0.5}
                ]},
                {"id":"after","type":"say","text":"气泡仍在"}
            ]});
            let frame = simulate_frame(&story, "together", 0.1, &positions);
            assert!(number(&frame["actors"][character], "preview_shock_x", 0.0).abs() > 0.01);
            assert_eq!(frame["mood_overrides"][character]["icon"], "shock");
            let after = simulate(&story, "after");
            assert!(after["actors"][character]["preview_shock_x"].is_null());
            assert_eq!(after["mood_overrides"][character]["icon"], "shock");
            let prelude = build_playtest_prelude(&story, "after");
            assert!(prelude.iter().any(|node| node["type"] == "mood"
                && node["character"] == character
                && node["icon"] == "shock"));
        }
    }
    #[test]
    fn shock_preview_moves_temporarily_and_restores_the_layout() {
        use super::*;
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json")).unwrap();
        for actor in ["player", "user:demo.actor"] {
            for parallel in [false, true] {
                let action = json!({"type":"shock","character":actor});
                let mut shock = if parallel {
                    json!({"id":"shake","type":"parallel","actions":[action,{"type":"wait","seconds":1.0}]})
                } else {
                    let mut node = action;
                    node["id"] = json!("shake");
                    node
                };
                let expected_duration = if parallel { 1.0 } else { 0.5 };
                assert_eq!(playback_duration(&shock), expected_duration);
                let story = json!({"id":"main","start":"enter","nodes":[
                    {"id":"enter","type":"show","character":actor,"position":"M"},
                    {"id":"offset","type":"offset","character":actor,"x":123,"y":45},
                    shock.clone(), {"id":"after","type":"say","text":"结束"}]});
                let original = story.clone();
                let first = simulate_frame(&story, "shake", 0.05, &positions);
                let second = simulate_frame(&story, "shake", 0.1, &positions);
                assert_ne!(
                    first["actors"][actor]["preview_shock_x"],
                    second["actors"][actor]["preview_shock_x"]
                );
                assert!(number(&first["actors"][actor], "preview_shock_x", 0.0).abs() > 0.01);
                for elapsed in [0.0, 0.5, 0.75, 1.0] {
                    let frame = simulate_frame(&story, "shake", elapsed, &positions);
                    assert_eq!(number(&frame["actors"][actor], "preview_shock_x", 0.0), 0.0);
                    assert_eq!(number(&frame["actors"][actor], "preview_shock_y", 0.0), 0.0);
                    assert_eq!(frame["actors"][actor]["offset_x"], 123.0);
                    assert_eq!(frame["actors"][actor]["offset_y"], 45.0);
                }
                assert!(simulate(&story, "after")["actors"][actor]["preview_shock_x"].is_null());
                assert_eq!(story, original);
                if parallel {
                    shock["actions"][0]["duration"] = json!(0);
                } else {
                    shock["duration"] = json!(0);
                }
                let stopped = json!({"id":"main","start":"enter","nodes":[
                    {"id":"enter","type":"show","character":actor,"position":"M"},shock]});
                assert!(
                    simulate_frame(&stopped, "shake", 0.1, &positions)["actors"][actor]
                        ["preview_shock_x"]
                        .is_null()
                );
            }
        }
    }

    #[test]
    fn original_effect_frames_preserve_additive_pixels_and_stay_behind_dialogue_ui() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        image::RgbaImage::from_pixel(640, 360, image::Rgba([90, 40, 10, 0]))
            .save(temp.path().join("frame.png"))
            .unwrap();
        std::fs::write(
            temp.path().join("effect_map.json"),
            serde_json::to_vec(&json!({
                "format":1,"source":"game-render","width":640,"height":360,
                "world_width":19.2,"world_height":10.8,"alpha_mode":"premultiplied",
                "effects":{"Glow_001":{"frames":[{"file":"frame.png","time":0}],
                    "duration":1,"preview_time":0,"loop_start":0,"looping":true,"complete":false}}
            }))
            .unwrap(),
        )
        .unwrap();
        let story = json!({"id":"main","start":"effect1","nodes":[
            {"id":"effect1","type":"effect","name":"Glow_001"},
            {"id":"say1","type":"say","mode":"narrative","text":"特效后对白"}
        ]});
        let context = egui::Context::default();
        let project = Project::new();
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        preview.set_effect_library(temp.path()).unwrap();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    preview.show(ui, &project, &story, 1, Some(0.2));
                });
            },
        );
        let texture = preview
            .cache
            .iter()
            .find(|(key, _)| key.starts_with("effect-frame:"))
            .map(|(_, cached)| cached.texture.id())
            .expect("original frame was loaded");
        let uploaded = output
            .textures_delta
            .set
            .iter()
            .find(|(id, _)| *id == texture)
            .unwrap();
        let egui::ImageData::Color(image) = &uploaded.1.image;
        assert_eq!(
            image.pixels[0].to_array(),
            [90, 40, 10, 0],
            "additive RGB at alpha=0 must survive loading"
        );
        let effect = output
            .shapes
            .iter()
            .position(
                |shape| matches!(&shape.shape,egui::Shape::Mesh(mesh) if mesh.texture_id==texture),
            )
            .expect("original frame is painted");
        let dialogue=output.shapes.iter().position(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.fill==Color32::from_black_alpha(195))).expect("dialogue background is painted");
        assert!(
            effect < dialogue,
            "original particle frames belong behind dialogue UI"
        );
    }

    #[test]
    fn invalid_original_effect_cache_exposes_error() {
        use super::*;
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        let temp = tempfile::tempdir().unwrap();
        assert!(preview.set_effect_library(temp.path()).is_err());
        assert!(preview.effect_library_error.is_some());
        assert!(preview.effect_extract_requested.is_none());
    }

    #[test]
    fn additional_effect_batch_preserves_current_layout_selection_and_playback_clock() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        preview.effect_library = None;
        for (index, name) in ["Rain_001", "Glow_001"].iter().enumerate() {
            let directory = temp.path().join(index.to_string());
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("frame.png"), []).unwrap();
            std::fs::write(directory.join("effect_map.json"),serde_json::to_vec(&json!({
                "format":1,"source":"game-render","width":640,"height":360,
                "world_width":19.2,"world_height":10.8,"alpha_mode":"premultiplied",
                "effects":{(*name):{"frames":[{"file":"frame.png","time":0}],
                    "duration":1,"preview_time":0,"loop_start":0,"looping":true,"complete":false}}
            })).unwrap()).unwrap();
            if index == 0 {
                preview.set_effect_library(&directory).unwrap();
                preview.layout_mode = true;
                preview.layout_selected = Some(LayoutTarget::Effect {
                    node: "rain".into(),
                    action: None,
                });
                preview
                    .effect_clock
                    .sync(&Project::new(), &json!({"id":"main"}), "rain", 5.0);
            } else {
                preview.set_effect_library(&directory).unwrap();
            }
        }
        let library = preview.effect_library.as_ref().unwrap();
        assert!(library.contains("Rain_001") && library.contains("Glow_001"));
        assert!(preview.layout_mode);
        assert_eq!(
            preview.layout_selected,
            Some(LayoutTarget::Effect {
                node: "rain".into(),
                action: None
            })
        );
        assert_eq!(preview.effect_clock.time(7.0, None, None, false), 2.0);
    }

    #[test]
    fn effect_selection_uses_offset_additive_pixels_without_cropping_or_double_rotation() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let mut image = image::RgbaImage::new(640, 360);
        image.put_pixel(400, 60, image::Rgba([0, 0, 0, 5]));
        image.put_pixel(480, 90, image::Rgba([90, 40, 10, 0]));
        image.save(temp.path().join("frame.png")).unwrap();
        image::RgbaImage::new(640, 360)
            .save(temp.path().join("empty.png"))
            .unwrap();
        let entry = |file: &str| {
            json!({"frames":[{"file":file,"time":0}],"duration":1,"preview_time":0,
            "loop_start":0,"looping":true,"complete":false})
        };
        std::fs::write(
            temp.path().join("effect_map.json"),
            serde_json::to_vec(&json!({
                "format":1,"source":"game-render","width":640,"height":360,
                "world_width":19.2,"world_height":10.8,"alpha_mode":"premultiplied",
                "effects":{"Glow_001":entry("frame.png"),"empty":entry("empty.png")}
            }))
            .unwrap(),
        )
        .unwrap();
        let library = effects::Library::load(temp.path()).unwrap();
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        let context = egui::Context::default();
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0));
        let mut objects = Vec::new();
        let output = context.run(egui::RawInput::default(), |ctx| {
            objects = effects::paint(
                &ctx.layer_painter(egui::LayerId::background())
                    .with_clip_rect(stage),
                stage,
                &json!([
                    {"name":"Glow_001","a":-2,"b":0.5,"rotation":90,"source_node":"fx"},
                    {"name":"empty","source_node":"empty"}
                ]),
                true,
                Some(&library),
                |frame, size| preview.effect_texture(ctx, frame, size),
            );
        });
        let cached = preview
            .cache
            .values()
            .find(|cached| cached.opaque.is_positive())
            .unwrap();
        assert!((cached.opaque.min - Pos2::new(400.0 / 640.0, 60.0 / 360.0)).length() < 0.00001);
        assert!((cached.opaque.max - Pos2::new(481.0 / 640.0, 91.0 / 360.0)).length() < 0.00001);
        assert_eq!(objects.len(), 2);
        let object = &objects[0];
        assert!((object.bounds.min - Pos2::new(-2.0, 120.0)).length() < 0.001);
        assert!((object.bounds.max - Pos2::new(160.0, 135.5)).length() < 0.001);
        assert_eq!(object.pivot, stage.center());
        assert_eq!(object.angle, -90.0);
        assert_eq!(
            objects[1].bounds,
            Rect::from_center_size(stage.center(), Vec2::splat(12.0))
        );
        let mesh = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(mesh.vertices[0].uv, Pos2::ZERO);
        assert_eq!(mesh.vertices[2].uv, Pos2::new(1.0, 1.0));
        assert!((mesh.vertices[0].pos - Pos2::new(230.0, -460.0)).length() < 0.001);
        assert!((mesh.vertices[2].pos - Pos2::new(410.0, 820.0)).length() < 0.001);
    }

    #[test]
    fn effect_layout_uses_recorded_representative_frame_while_playback_and_tail_keep_their_time() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        image::RgbaImage::new(4, 4)
            .save(temp.path().join("empty.png"))
            .unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([90, 40, 10, 0]))
            .save(temp.path().join("visible.png"))
            .unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 90]))
            .save(temp.path().join("tail.png"))
            .unwrap();
        let mut map = json!({"format":1,"source":"game-render","width":4,"height":4,
            "world_width":19.2,"world_height":10.8,"alpha_mode":"premultiplied",
            "effects":{"EventBubble_001":{"frames":[{"file":"empty.png","time":0},{"file":"visible.png","time":0.1}],
                "duration":0.2,"preview_time":0.1,"loop_start":0,"looping":false,"complete":true,
                "tail_frames":[{"file":"tail.png","time":0}],"tail_duration":0.3,"tail_sample_time":0.1,"tail_complete":true}}});
        let save = |map: &Value| {
            std::fs::write(
                temp.path().join("effect_map.json"),
                serde_json::to_vec(map).unwrap(),
            )
            .unwrap()
        };
        save(&map);
        let library = effects::Library::load(temp.path()).unwrap();
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        let context = egui::Context::default();
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0));
        for (layout, stopped, expected) in [
            (false, None, "empty.png"),
            (true, None, "visible.png"),
            (true, Some(0.0), "tail.png"),
        ] {
            let mut loaded = PathBuf::new();
            let _=context.run(egui::RawInput::default(),|ctx| {
                let objects=effects::paint(&ctx.layer_painter(egui::LayerId::background()),stage,
                    &json!([{"name":"EventBubble_001","source_node":"fx","age":0,"stopped_age":stopped}]),
                    layout,Some(&library),|frame,size| {
                        loaded=frame.file.clone();
                        preview.effect_texture(ctx,frame,size)
                    });
                if layout {assert_eq!(objects[0].bounds,stage);}
            });
            assert_eq!(loaded.file_name().unwrap(), expected);
        }
        map["effects"]["EventBubble_001"]["preview_time"] = json!(0.05);
        save(&map);
        assert!(
            effects::Library::load(temp.path()).is_err(),
            "representative time must be an actual recorded frame"
        );
    }

    #[test]
    fn mood_sequence_uses_authored_holds_for_shows_and_hides() {
        use super::*;
        for character in ["player", "user:fixture.actor"] {
            let story = json!({"start":"show","nodes":[
                {"id":"show","type":"show","character":character},
                {"id":"shock","type":"mood","character":character,"enabled":true,"icon":"shock","wait_after":0.8},
                {"id":"sweat","type":"mood","character":character,"enabled":true,"icon":"nervous1","wait_after":1.2},
                {"id":"hide","type":"mood","character":character,"enabled":false,"wait_after":0.25}
            ]});
            let before = story.clone();
            for (index, seconds) in [(1, 0.8), (2, 1.2), (3, 0.25)] {
                let node = &story["nodes"][index];
                assert_eq!(playback_duration(node), seconds);
                let state = simulate(&story, text(node, "id"));
                assert_eq!(
                    state["mood_overrides"][character]["enabled"],
                    node["enabled"]
                );
                assert_eq!(
                    visible_mood(&state, character, &state["actors"][character]),
                    if node["enabled"] == true {
                        node["icon"].as_str()
                    } else {
                        None
                    }
                );
            }
            assert_eq!(story, before);
        }
        assert_eq!(
            playback_duration(&json!({"type":"mood","enabled":true})),
            0.1
        );
        assert_eq!(
            playback_duration(&json!({"type":"mood","enabled":false,"wait_after":0})),
            0.1
        );
    }
    #[test]
    fn hide_all_preview_fades_every_actor_before_clearing() {
        let story = json!({"start":"a","nodes":[
            {"id":"a","type":"show","character":"player","position":"L1"},
            {"id":"b","type":"show","character":"user:demo.actor","position":"R1"},
            {"id":"clear","type":"hide_all","fadeDuration":2,"wait_after":0.5},
            {"id":"end","type":"end"}]});
        let positions = json!({});
        let frame = simulate_frame(&story, "clear", 1.0, &positions);
        for cid in ["player", "user:demo.actor"] {
            assert_eq!(frame["actors"][cid]["preview_opacity"], 0.5);
            assert_eq!(frame["actors"][cid]["preview_hide_mood"], true);
        }
        assert!(simulate_frame(&story, "clear", 2.0, &positions)["actors"]
            .as_object()
            .unwrap()
            .is_empty());
        assert_eq!(playback_duration(&story["nodes"][2]), 2.5);
    }
    #[test]
    fn camera_resume_preserves_independent_switches_and_excludes_target() {
        let story = json!({"start":"memory","nodes":[
            {"id":"memory","type":"camera","name":"stage-memory","active":true},
            {"id":"dream","type":"camera","name":"stage-dream","active":true},
            {"id":"clear","type":"camera","name":"stage-memory","active":false},
            {"id":"target","type":"camera","name":"stage-dream","active":false},
            {"id":"end","type":"end"}]});
        let prelude = build_playtest_prelude(&story, "target");
        assert_eq!(prelude.len(), 2);
        assert!(prelude
            .iter()
            .any(|n| n["name"] == "stage-memory" && n["active"] == false));
        assert!(prelude
            .iter()
            .any(|n| n["name"] == "stage-dream" && n["active"] == true));
        assert_eq!(prelude.last().unwrap()["goto"], "target");
        assert_eq!(
            simulate(&story, "end")["camera_filters"],
            json!({"stage-memory":false,"stage-dream":false})
        );
    }
    #[test]
    fn hide_all_preview_clears_actors_and_bubbles_and_preserves_images() {
        let story = json!({"start":"background","nodes":[
            {"id":"background","type":"background","action":"show","image":"user:demo.bg"},
            {"id":"cg","type":"custom_cg","action":"show","image":"user:demo.cg"},
            {"id":"overlay","type":"overlay","action":"show","slot":"main","image":"user:demo.overlay"},
            {"id":"official","type":"show","character":"player","position":"L1"},
            {"id":"custom","type":"show","character":"user:demo.actor","position":"R1"},
            {"id":"bubble","type":"mood","character":"player","enabled":true,"icon":"shock"},
            {"id":"clear","type":"hide_all"},
            {"id":"end","type":"end"}]});
        let before = simulate(&story, "bubble");
        assert_eq!(before["actors"].as_object().unwrap().len(), 2);
        let state = simulate(&story, "end");
        assert!(state["actors"].as_object().unwrap().is_empty());
        assert!(state["mood_overrides"].as_object().unwrap().is_empty());
        for key in ["background", "custom_cg", "overlays"] {
            assert_eq!(state[key], before[key]);
        }
        assert!(build_playtest_prelude(&story, "end")
            .iter()
            .all(|n| n["type"] != "show"));
    }

    #[test]
    fn camera_fades_preserve_exclusions_until_fade_out_finishes_and_resume_them() {
        let story = json!({"id":"main","start":"memory","nodes":[
            {"id":"memory","type":"camera","name":"stage-memory","active":true,"duration":1,"exclude_characters":["player"]},
            {"id":"say","type":"say","mode":"narrative","text":"回忆"},
            {"id":"clear","type":"camera","name":"stage-memory","active":false,"duration":2},
            {"id":"end","type":"end"}]});
        let mut on = simulate(&story, "memory");
        animate_frame(
            &mut on,
            &simulate_until(&story, "memory", false),
            &story["nodes"][0],
            0.5,
            &json!({}),
        );
        assert_eq!(on["camera_filter_weights"]["stage-memory"], 0.75);
        assert!(filters::excludes(&on, "player"));
        assert!(!filters::excludes(&on, "brother1"));
        let prelude = build_playtest_prelude(&story, "say");
        assert!(prelude.iter().any(
            |node| node["type"] == "camera" && node["exclude_characters"] == json!(["player"])
        ));
        let before = simulate_until(&story, "clear", false);
        let mut off = simulate(&story, "clear");
        animate_frame(&mut off, &before, &story["nodes"][2], 1.0, &json!({}));
        assert_eq!(off["camera_filter_weights"]["stage-memory"], 0.25);
        assert!(filters::excludes(&off, "player"));
        let mut off = simulate(&story, "clear");
        animate_frame(&mut off, &before, &story["nodes"][2], 2.0, &json!({}));
        assert!(!filters::excludes(&off, "player"));
        assert_eq!(playback_duration(&story["nodes"][2]), 2.0);
    }

    #[test]
    fn stage_filter_state_is_before_target_and_follows_the_preview_route() {
        let story = json!({"start":"branch","nodes":[
            {"id":"branch","type":"choice","options":[{"goto":"a"},{"goto":"b"}]},
            {"id":"a","type":"show","character":"first","goto":"hide"},
            {"id":"b","type":"show","character":"other","goto":"end"},
            {"id":"hide","type":"hide","character":"first","goto":"parallel"},
            {"id":"parallel","type":"parallel","actions":[
                {"type":"show","character":"user:hero"},
                {"type":"show","character":"variant"}
            ],"goto":"clear"},
            {"id":"clear","type":"hide_all","goto":"end"},
            {"id":"end","type":"end"}
        ]});
        let mut preview = Preview::new(Path::new("."));
        assert!(preview.state_before(&story, "a")["actors"]
            .as_object()
            .unwrap()
            .is_empty());
        assert!(preview.state_before(&story, "hide")["actors"]
            .get("first")
            .is_some());
        assert!(preview.state_before(&story, "parallel")["actors"]
            .as_object()
            .unwrap()
            .is_empty());
        let actors = &preview.state_before(&story, "clear")["actors"];
        assert!(actors.get("user:hero").is_some());
        assert!(actors.get("variant").is_some());
        assert!(actors.get("other").is_none());
        // Inspect the continuation of this route, rather than the other branch's shorter path.
        let mut linear = story.clone();
        linear["nodes"][0]["options"] = json!([{"goto":"a"}]);
        assert!(preview.state_before(&linear, "end")["actors"]
            .as_object()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn character_bubble_resume_follows_hide_restore_and_global_clear() {
        use super::*;
        let mut story = json!({"start":"enter","nodes":[
            {"id":"enter","type":"show","character":"artist1","position":"M","portrait":"normal"},
            {"id":"bubble","type":"mood","character":"artist1","enabled":true,"icon":"laugh1"},
            {"id":"say","type":"say","mode":"narrative","text":"预览"},
            {"id":"clear","type":"mood","enabled":false},
            {"id":"end","type":"end"}]});
        let state = simulate(&story, "say");
        assert_eq!(state["mood_enabled"], false);
        assert_eq!(state["actors"]["artist1"]["portrait"], "normal");
        assert_eq!(state["mood_overrides"]["artist1"]["icon"], "laugh1");
        let resume = build_playtest_prelude(&story, "say");
        assert!(
            resume.iter().position(|n| n["type"] == "show").unwrap()
                < resume.iter().position(|n| n["icon"] == "laugh1").unwrap()
        );
        for clearing in [
            json!({"id":"clear","type":"mood","enabled":false}),
            json!({"id":"clear","type":"mood","character":"artist1","enabled":true,"icon":"auto"}),
            json!({"id":"clear","type":"hide","character":"artist1"}),
        ] {
            story["nodes"][3] = clearing;
            assert!(simulate(&story, "end")["mood_overrides"]
                .as_object()
                .unwrap()
                .is_empty());
        }
    }
    #[test]
    fn mood_switch_preview_and_playtest_resume_keep_the_latest_switch() {
        let story = json!({"mood":false,"start":"on","nodes":[
            {"id":"on","type":"mood","enabled":true},
            {"id":"say","type":"say","mode":"narrative","text":"气泡开启"},
            {"id":"off","type":"mood","enabled":false},
            {"id":"end","type":"end"}]});
        assert_eq!(simulate(&story, "say")["mood_enabled"], true);
        assert_eq!(simulate(&story, "end")["mood_enabled"], false);
        let enabled = build_playtest_prelude(&story, "say");
        assert_eq!(enabled[0]["type"], "mood");
        assert_eq!(enabled[0]["enabled"], true);
        let disabled = build_playtest_prelude(&story, "end");
        assert_eq!(disabled[0]["type"], "mood");
        assert_eq!(disabled[0]["enabled"], false);
    }
    #[test]
    fn portrait_transition_keeps_transform_and_fades_old_then_new() {
        use super::*;
        for actor in ["player", "user:demo.actor"] {
            let story = json!({"start":"enter","nodes":[
                {"id":"enter","type":"show","character":actor,"position":"R1","facing":"left"},
                {"id":"offset","type":"offset","character":actor,"x":17,"y":-6,"duration":0},
                {"id":"rotate","type":"rotate","character":actor,"angle":12,"duration":0},
                {"id":"dim","type":"dim","character":actor,"dimmed":true},
                {"id":"change","type":"portrait","character":actor,"portrait":"angry1"}]});
            let before = simulate(&story, "dim");
            let positions = json!({});
            let fading_out = simulate_frame(&story, "change", 0.25, &positions);
            assert_eq!(fading_out["actors"][actor]["portrait"], "normal");
            assert_eq!(fading_out["actors"][actor]["preview_opacity"], 0.5);
            let fading_in = simulate_frame(&story, "change", 1.25, &positions);
            assert_eq!(fading_in["actors"][actor]["portrait"], "angry1");
            assert_eq!(fading_in["actors"][actor]["preview_opacity"], 0.5);
            for key in [
                "position", "facing", "offset_x", "offset_y", "rotation", "dimmed",
            ] {
                assert_eq!(
                    fading_in["actors"][actor][key],
                    before["actors"][actor][key]
                );
            }
            assert_eq!(playback_duration(&story["nodes"][4]), 2.0);
        }
    }
    use super::*;
    #[test]
    fn official_pictures_persist_until_hidden_and_are_restored_for_playtest() {
        let story = json!({"id":"main","start":"picture","nodes":[
            {"id":"picture","type":"cg","action":"show","kind":"big","key":"one","color":2},
            {"id":"speech","type":"say","mode":"narrative","text":"hello"},
            {"id":"hide","type":"cg","action":"hide","kind":"big"}]});
        assert_eq!(
            simulate(&story, "speech")["official_pictures"]["big"]["key"],
            "one"
        );
        assert!(simulate(&story, "hide")["official_pictures"]
            .as_object()
            .unwrap()
            .is_empty());
        let prelude = build_playtest_prelude(&story, "speech");
        assert!(prelude
            .iter()
            .any(|n| n["type"] == "cg" && n["key"] == "one" && n["color"] == 2));
        assert_eq!(
            playback_duration(&json!({"type":"parallel","actions":[
            {"type":"move","duration":1,"wait_after":2},{"type":"wait","seconds":2}]})),
            3.0
        );
    }
    #[test]
    fn raw_lua_previews_known_scene_and_portrait_prefix_without_guessing_branches() {
        let code = r#"
getvar(flowcharts.view, "ViewName").value = "center"
runblock(flowcharts.view, "view")
stage.show{character=characters.Get("player"),portrait=characters.GetPortrait("player", "angry1"),toPosition="L1",facing="right"}
if ReadGameState() then
runblock(flowcharts.view, "out")
end
"#;
        let story =
            json!({"id":"main","start":"old","nodes":[{"id":"old","type":"raw","code":code}]});
        let state = simulate(&story, "old");
        assert_eq!(state["view"], "center");
        assert_eq!(state["actors"]["player"]["portrait"], "angry1");
        assert_eq!(state["actors"]["player"]["position"], "L1");
        assert_eq!(state["raw_preview"]["preview_stopped_at"], 5);
        assert!(raw_preview_notice(&state["raw_preview"], "chs").contains("第 5 行"));
    }
    #[test]
    fn cached_raw_preview_notes_follow_locale_without_changing_report() {
        let report = json!({"preview_stopped_at":5, "notes":[
            "第 5 行起保留原生 Lua，静态预览在此停止。", "author {notes} / 条件成立"]});
        let before = report.clone();
        for locale in crate::i18n::LOCALES {
            let notice = raw_preview_notice(&report, locale);
            assert!(notice.contains("author {notes} / 条件成立"));
            assert!(notice.contains('5'));
            if matches!(locale, "ja" | "ko") {
                assert!(!notice.contains("静态预览在此停止"));
            }
        }
        assert_eq!(report, before);
    }
    #[test]
    fn library_picker_explains_wrong_folders_and_preserves_the_active_library() {
        let root = tempfile::tempdir().unwrap();
        let mut preview = Preview::new(root.path());
        let original = preview.library.clone();
        let error = preview.set_library(root.path()).unwrap_err().to_string();
        assert!(error.contains("preview_map.json"));
        assert!(error.contains("从游戏提取预览素材"));
        assert_eq!(preview.library, original);
        std::fs::write(root.path().join("preview_map.json"), b"{ not-json").unwrap();
        let error = preview.set_library(root.path()).unwrap_err();
        let visible = crate::error_ui::text(&format!("{error:#}"));
        assert!(visible.contains("请"), "{visible}");
        assert!(!visible.contains("key must be a string"), "{visible}");
        assert!(format!("{error:#}").contains("key must be a string at line 1 column 3"));
        assert_eq!(preview.library, original);
        let mapping = json!({"characters":{"girl5_9":{"first":"normal","portraits":{"normal":"assets/normal.png"}}},"views":{}});
        std::fs::write(root.path().join("preview_map.json"), mapping.to_string()).unwrap();
        assert!(preview
            .set_library(root.path())
            .unwrap_err()
            .to_string()
            .contains("不完整"));
        assert_eq!(preview.library, original);
        std::fs::create_dir(root.path().join("assets")).unwrap();
        std::fs::write(root.path().join("assets/normal.png"), b"fixture").unwrap();
        preview.set_library(root.path()).unwrap();
        assert_eq!(preview.library, root.path());
    }
    #[test]
    fn blackout_survives_background_change_and_parallel_placement_until_reveal() {
        let story = json!({"id":"main","start":"cover","nodes":[
            {"id":"cover","type":"transition","phase":"in","dir":"lr"},
            {"id":"background","type":"background","action":"show","image":"new.png"},
            {"id":"place","type":"parallel","actions":[
                {"type":"show","character":"a","position":"L1","fadeDuration":0},
                {"type":"show","character":"b","position":"R3","fadeDuration":0}]},
            {"id":"reveal","type":"transition","phase":"out","dir":"rl"},
            {"id":"talk","type":"say","text":"人物本就在此"}
        ]});
        let original = story.clone();
        for target in ["cover", "background", "place"] {
            assert_eq!(simulate(&story, target)["blackout"]["amount"], 1.0);
        }
        let staged = simulate_frame(&story, "place", 0.0, &json!({}));
        assert_eq!(staged["background"], "new.png");
        assert_eq!(staged["actors"].as_object().unwrap().len(), 2);
        assert_eq!(staged["blackout"]["amount"], 1.0);
        let opening = simulate_frame(
            &story,
            "reveal",
            PREVIEW_TRANSITION_SECONDS / 2.0,
            &json!({}),
        );
        assert_eq!(opening["blackout"]["amount"], 0.5);
        assert_eq!(opening["actors"].as_object().unwrap().len(), 2);
        assert_eq!(opening["background"], "new.png");
        assert!(opening["dialog"].is_null());
        assert_eq!(simulate(&story, "talk")["blackout"]["amount"], 0.0);
        let prelude = build_playtest_prelude(&story, "reveal");
        assert_eq!(prelude[0]["type"], "transition");
        assert_eq!(prelude[0]["phase"], "in");
        assert_eq!(prelude.iter().filter(|n| n["type"] == "show").count(), 2);
        assert_eq!(story, original);
    }
    #[test]
    fn parallel_transition_and_actor_motion_share_the_same_clock() {
        let story = json!({"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"a","position":"L1"},
            {"id":"cover","type":"parallel","actions":[
                {"type":"transition","phase":"in","dir":"lr"},
                {"type":"move","character":"a","from":"L1","to":"M","duration":2}]},
            {"id":"done","type":"end"}
        ]});
        let positions = json!({"positions":{"L1":{"x":0.0},"M":{"x":1.0}}});
        let frame = simulate_frame(&story, "cover", 0.5, &positions);
        assert_eq!(frame["blackout"]["amount"], 0.5);
        assert_eq!(frame["actors"]["a"]["preview_x"], 0.25);
        assert_eq!(playback_duration(&story["nodes"][1]), 2.0);
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 100.0));
        assert_eq!(blackout_rect(rect, 1.0, "lr"), rect);
        assert_eq!(blackout_rect(rect, 0.5, "rl").left(), rect.left());
        assert_eq!(blackout_rect(rect, 0.5, "lr").right(), rect.right());
        for direction in ["lr", "rl", "tb", "bt"] {
            assert_eq!(blackout_rect(rect, 0.0, direction).width(), 0.0);
        }
    }

    #[test]
    fn editing_focus_brightens_only_the_painted_frame_and_respects_selected_parallel_action() {
        let story = json!({"id":"main","start":"left","nodes":[
            {"id":"left","type":"show","character":"player"},
            {"id":"right","type":"show","character":"other"},
            {"id":"dim","type":"dim","character":"player","dimmed":true},
            {"id":"edit","type":"parallel","actions":[
                {"type":"portrait","character":"player","portrait":"normal"},
                {"type":"face","character":"other","facing":"left"}]},
            {"id":"end","type":"end"}]});
        let actual = simulate(&story, "edit");
        let mut frame = actual.clone();
        highlight_editing_character(&mut frame, &story["nodes"][3], Some(0), None);
        assert_eq!(frame["actors"]["player"]["dimmed"], false);
        assert_eq!(simulate(&story, "edit"), actual);
        let mut frame = actual.clone();
        highlight_editing_character(&mut frame, &story["nodes"][3], Some(1), None);
        assert_eq!(frame["actors"]["player"]["dimmed"], true);
        assert!(build_playtest_prelude(&story, "end")
            .iter()
            .any(|n| n["type"] == "dim" && n["character"] == "player" && n["dimmed"] == true));
        highlight_editing_character(
            &mut frame,
            &json!({"type":"dim","character":"player","dimmed":true}),
            None,
            None,
        );
        assert_eq!(frame["actors"]["player"]["dimmed"], true);
    }

    #[test]
    fn temporary_dimming_expires_at_dialogue_and_can_restore_automatic_after_manual_steps() {
        for actor in ["player", "user:demo.speaker"] {
            let story = json!({"id":"main","start":"enter","nodes":[
                {"id":"enter","type":"show","character":actor,"position":"M"},
                {"id":"other","type":"show","character":"brother1","position":"L1"},
                {"id":"highlight","type":"parallel","actions":[
                    {"type":"dim","character":actor,"dimmed":false,"mode":"until_dialogue"},
                    {"type":"mood","character":actor,"enabled":true,"icon":"nervous1"}]},
                {"id":"pause","type":"wait","seconds":1},
                {"id":"first","type":"say","character":"brother1","text":"别人说话"},
                {"id":"second","type":"say","character":actor,"text":"自己说话"},
                {"id":"manual","type":"dim","character":actor,"dimmed":true},
                {"id":"still_dark","type":"say","character":actor,"text":"旧行为仍是持续压暗"},
                {"id":"restore","type":"dim","character":actor,"dimmed":true,"mode":"auto"},
                {"id":"again","type":"say","character":"brother1","text":"恢复自动后别人说话"},
                {"id":"own","type":"say","character":actor,"text":"恢复自动后自己说话"},
                {"id":"end","type":"end"}]});
            let paused = simulate(&story, "pause");
            assert_eq!(paused["actors"][actor]["dimmed"], false);
            assert_eq!(
                visible_mood(&paused, actor, &paused["actors"][actor]),
                Some("nervous1")
            );
            for (id, dark) in [
                ("first", true),
                ("second", false),
                ("still_dark", true),
                ("restore", false),
                ("again", true),
                ("own", false),
            ] {
                assert_eq!(
                    simulate(&story, id)["actors"][actor]["dimmed"],
                    dark,
                    "{actor}/{id}"
                );
            }
            let prelude = build_playtest_prelude(&story, "pause");
            assert!(prelude.iter().any(|n| n["character"] == actor
                && n["type"] == "dim"
                && n["mode"] == "until_dialogue"));
        }
    }

    #[test]
    fn speech_can_introduce_a_new_appearance_and_offscreen_speech_keeps_the_stage_cast() {
        let story = json!({"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"girl8","position":"R1"},
            {"id":"variant","type":"say","character":"girl8_2","portrait":"normal","text":"散发外观"},
            {"id":"hide","type":"hide","character":"girl8_2"},
            {"id":"return","type":"say","character":"girl8_2","portrait":"sad2","text":"再次发言"},
            {"id":"voice","type":"say","mode":"offscreen","character":"special2_1","text":"无立绘呼救"},
            {"id":"end","type":"end"}]});
        let variant = simulate(&story, "variant");
        assert_eq!(variant["actors"]["girl8_2"]["position"], "M");
        assert_eq!(variant["actors"]["girl8"]["position"], "R1");
        assert!(simulate(&story, "hide")["actors"].get("girl8_2").is_none());
        let returned = simulate(&story, "return");
        assert_eq!(returned["actors"]["girl8_2"]["portrait"], "sad2");
        assert_eq!(returned["actors"]["girl8_2"]["position"], "M");
        let voice = simulate(&story, "voice");
        assert_eq!(voice["actors"].as_object().unwrap().len(), 2);
        assert!(voice["actors"].get("special2_1").is_none());
        assert!(voice["actors"].get("unknown").is_none());
    }

    #[test]
    fn offscreen_dialogue_renders_its_own_name_without_a_portrait() {
        for name in [None, Some("远处的声音")] {
            let project = Project::new();
            let mut node = json!({"id":"voice","type":"say","mode":"offscreen","text":"救命！"});
            if let Some(name) = name {
                node["speaker_name"] = json!(name);
            }
            let story = json!({"start":"voice","nodes":[node]});
            assert!(simulate(&story, "voice")["actors"]
                .as_object()
                .unwrap()
                .is_empty());
            let context = egui::Context::default();
            let mut preview = Preview::new(Path::new("/not-a-repository"));
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        preview.show(ui, &project, &story, 0, None);
                    });
                },
            );
            let labels: Vec<_> = output
                .shapes
                .into_iter()
                .filter_map(|shape| match shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                    _ => None,
                })
                .collect();
            assert!(
                labels.iter().any(|text| text == name.unwrap_or("？？？")),
                "{labels:?}"
            );
        }
    }

    #[test]
    fn explicit_dimming_survives_speech_portrait_changes_and_playtest_prelude() {
        for actor in ["player", "user:demo.speaker"] {
            let story = json!({"id":"main","start":"enter","nodes":[
                {"id":"enter","type":"show","character":actor,"position":"M"},
                {"id":"dark","type":"parallel","actions":[
                    {"type":"dim","character":actor,"dimmed":true},
                    {"type":"wait","seconds":0.2}]},
                {"id":"speech","type":"say","character":actor,"portrait":"normal","text":"保持压暗"},
                {"id":"portrait","type":"show","character":actor,"position":"R1","portrait":"angry"},
                {"id":"hide","type":"hide","character":actor},
                {"id":"return","type":"show","character":actor,"position":"M"},
                {"id":"bright","type":"dim","character":actor,"dimmed":false},
                {"id":"done","type":"say","character":actor,"text":"恢复明亮"}]});
            for target in ["speech", "portrait", "return"] {
                assert_eq!(simulate(&story, target)["actors"][actor]["dimmed"], true);
            }
            let prelude = build_playtest_prelude(&story, "portrait");
            assert!(prelude.iter().any(|node| node["type"] == "dim"
                && node["character"] == actor
                && node["dimmed"] == true));
            assert_eq!(simulate(&story, "done")["actors"][actor]["dimmed"], false);
        }
    }
    #[test]
    fn offscreen_offsets_survive_dialogue_and_reset_on_reentry_or_named_movement() {
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json")).unwrap();
        for actor in ["player", "user:demo.actor"] {
            let story = json!({"start":"enter","nodes":[
                {"id":"enter","type":"show","character":actor,"position":"L1"},
                {"id":"exit","type":"offset","character":actor,"x":-2400,"y":-400,"duration":1},
                {"id":"speech","type":"say","character":actor,"portrait":"angry","text":"屏外说话"},
                {"id":"return","type":"show","character":actor,"position":"R1"},
                {"id":"adjust","type":"offset","character":actor,"x":170,"y":60},
                {"id":"move","type":"move","character":actor,"from":"R1","to":"M","duration":1},
                {"id":"done","type":"say","character":actor,"text":"已到中间"}
            ]});
            for elapsed in [0.0, 0.5, 1.0, 1.1] {
                let state = simulate_frame(&story, "exit", elapsed, &positions);
                assert_eq!(
                    state["actors"][actor]["offset_x"],
                    -2400.0 * elapsed.min(1.0)
                );
                assert_eq!(
                    state["actors"][actor]["offset_y"],
                    -400.0 * elapsed.min(1.0)
                );
            }
            let speech = simulate(&story, "speech");
            assert_eq!(speech["actors"][actor]["offset_x"], -2400.0);
            assert_eq!(speech["actors"][actor]["offset_y"], -400.0);
            for target in ["return", "move", "done"] {
                let state = simulate(&story, target);
                assert_eq!(state["actors"][actor]["offset_x"], 0.0, "{actor}/{target}");
                assert_eq!(state["actors"][actor]["offset_y"], 0.0, "{actor}/{target}");
            }
            let halfway = simulate_frame(&story, "move", 0.5, &positions);
            let mut from = position(&positions, "R1");
            if actor.starts_with("user:") {
                from.0 += 170.0 / 1920.0;
                from.1 -= 60.0 / 1080.0;
            }
            let to = position(&positions, "M");
            assert_eq!(halfway["actors"][actor]["preview_x"], (from.0 + to.0) / 2.0);
            assert_eq!(
                halfway["actors"][actor]["preview_feet"],
                (from.1 + to.1) / 2.0
            );
            assert_eq!(halfway["actors"][actor]["offset_x"], 0.0);
        }
    }
    #[test]
    fn parallel_playback_uses_each_duration_and_waits_for_the_slowest_action() {
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json")).unwrap();
        let story = json!({"id":"main","start":"enter","nodes":[
            {"id":"enter","type":"parallel","actions":[
                {"type":"show","character":"a","position":"L1"},
                {"type":"show","character":"b","position":"R3"}]},
            {"id":"move","type":"parallel","actions":[
                {"type":"move","character":"a","from":"L1","to":"L3","duration":1},
                {"type":"move","character":"b","from":"R3","to":"R1","duration":2}]},
            {"id":"say","type":"say","text":"两人都已到位"}
        ]});
        let original = story.clone();
        let halfway = simulate_frame(&story, "move", 0.5, &positions);
        let (a_start, _) = position(&positions, "L1");
        let (a_end, _) = position(&positions, "L3");
        let (b_start, _) = position(&positions, "R3");
        let (b_end, _) = position(&positions, "R1");
        assert!(
            (number(&halfway["actors"]["a"], "preview_x", 0.0) - (a_start + a_end) / 2.0).abs()
                < 1e-6
        );
        assert!(
            (number(&halfway["actors"]["b"], "preview_x", 0.0)
                - (b_start + (b_end - b_start) * 0.25))
                .abs()
                < 1e-6
        );
        let waiting = simulate_frame(&story, "move", 1.5, &positions);
        assert_eq!(waiting["actors"]["a"]["preview_x"], a_end);
        assert!(number(&waiting["actors"]["b"], "preview_x", 0.0) > b_end);
        assert!(waiting["dialog"].is_null());
        assert_eq!(playback_duration(&story["nodes"][1]), 2.0);
        assert_eq!(simulate(&story, "say")["dialog"]["text"], "两人都已到位");
        assert_eq!(story, original);
    }
    #[test]
    fn parallel_fades_keep_both_actors_until_their_own_fade_finishes() {
        let positions = json!({});
        let story = json!({"id":"main","start":"enter","nodes":[
            {"id":"enter","type":"parallel","actions":[
                {"type":"show","character":"a","position":"L1","fadeDuration":1},
                {"type":"show","character":"b","position":"R3","fadeDuration":1}]},
            {"id":"leave","type":"parallel","actions":[
                {"type":"hide","character":"a","fadeDuration":0.5},
                {"type":"hide","character":"b","fadeDuration":1}]}
        ]});
        let entering = simulate_frame(&story, "enter", 0.25, &positions);
        assert_eq!(entering["actors"]["a"]["preview_opacity"], 0.25);
        assert_eq!(entering["actors"]["b"]["preview_opacity"], 0.25);
        let leaving = simulate_frame(&story, "leave", 0.25, &positions);
        assert_eq!(leaving["actors"]["a"]["preview_opacity"], 0.5);
        assert_eq!(leaving["actors"]["b"]["preview_opacity"], 0.75);
        let later = simulate_frame(&story, "leave", 0.5, &positions);
        assert!(later["actors"].get("a").is_none());
        assert_eq!(later["actors"]["b"]["preview_opacity"], 0.5);
        assert!(simulate_frame(&story, "leave", 1.0, &positions)["actors"]
            .as_object()
            .unwrap()
            .is_empty());
        assert_eq!(playback_duration(&story["nodes"][1]), 1.0);
        assert_eq!(
            playback_duration(&json!({"type":"parallel","actions":[
            {"type":"sound"},{"type":"wait","seconds":3.5}]})),
            3.5
        );
    }
    #[test]
    fn parallel_preview_applies_all_actions_and_preserves_the_group_source() {
        let story = json!({"id":"main","start":"enter","nodes":[
            {"id":"enter","type":"parallel","actions":[
                {"type":"show","character":"player","position":"L"},
                {"type":"show","character":"brother1","position":"R"}]},
            {"id":"leave","type":"parallel","actions":[
                {"type":"hide","character":"player","fadeDuration":0.4},
                {"type":"hide","character":"brother1","fadeDuration":0.8}]},
            {"id":"done","type":"end"}
        ]});
        let before = story.clone();
        let entered = simulate(&story, "enter");
        assert_eq!(entered["actors"]["player"]["position"], "L");
        assert_eq!(entered["actors"]["brother1"]["position"], "R");
        let left = simulate(&story, "leave");
        assert!(left["actors"].as_object().unwrap().is_empty());
        assert_eq!(story, before);
    }
    #[test]
    fn branch_preview_follows_reachable_path() {
        let story = json!({"start":"s","nodes":[{"id":"s","type":"show","character":"player","position":"LM1"},{"id":"c","type":"choice","options":[{"text":"甲","goto":"a"},{"text":"乙","goto":"b"}]},{"id":"a","type":"end"},{"id":"b","type":"say","character":"player","text":"分支"}]});
        let state = simulate(&story, "b");
        assert_eq!(state["actors"]["player"]["position"], "LM1");
        assert_eq!(state["dialog"]["text"], "分支");
        assert_eq!(state["reached"], true);
    }
    #[test]
    fn combat_has_no_implicit_fallthrough() {
        let story = json!({"nodes":[{"id":"a","type":"combat","win":"w","lose":"l"},{"id":"b","type":"say"}]});
        let edges = edges(&story);
        assert_eq!(edges.len(), 2);
        assert!(!edges.iter().any(|(_, to, _)| to == "b"));
    }
}

#[cfg(test)]
mod content_tests {
    use super::*;
    #[test]
    fn legacy_cropped_portraits_match_the_original_canvas_in_both_directions() {
        let geometry: Value =
            serde_json::from_str(include_str!("../../../data/portrait_geometry.json")).unwrap();
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
        let base = Pos2::new(960.0, 1083.0);
        for character in ["player", "girl2"] {
            let data = &geometry[character]["normal"];
            let canvas = Vec2::new(
                data["canvas"][0].as_f64().unwrap() as f32,
                data["canvas"][1].as_f64().unwrap() as f32,
            );
            let crop = Rect::from_min_size(
                Pos2::new(
                    data["image"][0].as_f64().unwrap() as f32,
                    data["image"][1].as_f64().unwrap() as f32,
                ),
                Vec2::new(
                    data["image"][2].as_f64().unwrap() as f32,
                    data["image"][3].as_f64().unwrap() as f32,
                ),
            );
            for sign in [-1.0, 1.0] {
                for scale in [0.5, 1.0, 1.5] {
                    let calibration = [15.0, 20.0, 115.0];
                    let full =
                        portrait_rect(stage, base, canvas, scale, calibration, sign, &json!({}));
                    let meta = json!({"_preview_geometry": data});
                    let legacy = portrait_rect(
                        stage,
                        base,
                        crop.size() * 0.8,
                        scale,
                        calibration,
                        sign,
                        &meta,
                    );
                    let factor = full.height() / canvas.y;
                    let x = if sign < 0.0 {
                        canvas.x - crop.right()
                    } else {
                        crop.left()
                    };
                    let expected = Rect::from_min_size(
                        full.min + Vec2::new(x, crop.top()) * factor,
                        crop.size() * factor,
                    );
                    assert!(legacy.min.distance(expected.min) < 0.01);
                    assert!(legacy.max.distance(expected.max) < 0.01);
                    assert_eq!(
                        portrait_rect(stage, base, canvas, scale, calibration, sign, &meta),
                        full
                    );
                }
            }
        }
    }
    #[test]
    fn dialogue_mood_restores_the_previous_setting_and_never_enters_the_prelude() {
        for character in ["player", "user:fixture.actor"] {
            let mut story = json!({"start":"show","mood":false,"nodes":[
                {"id":"show","type":"show","character":character,"portrait":"shy"},
                {"id":"persistent","type":"mood","character":character,"enabled":true,"icon":"silent","offset_x":12},
                {"id":"line","type":"say","character":character,"portrait":"shy","mood_icon":"shock","text":"本句"},
                {"id":"next","type":"say","character":character,"portrait":"shy","text":"恢复"}
            ]});
            for (icon, expected) in [
                ("shock", Some("shock")),
                ("hide", None),
                ("auto", Some("shy")),
            ] {
                story["nodes"][2]["mood_icon"] = json!(icon);
                let state = simulate(&story, "line");
                assert_eq!(
                    visible_mood(&state, character, &state["actors"][character]),
                    expected
                );
                assert_eq!(state["mood_overrides"][character]["offset_x"], 12);
                let next = simulate(&story, "next");
                assert_eq!(
                    visible_mood(&next, character, &next["actors"][character]),
                    Some("silent")
                );
                let prelude = build_playtest_prelude(&story, "next");
                assert!(prelude
                    .iter()
                    .any(|node| node["type"] == "mood" && node["icon"] == "silent"));
                assert!(!prelude
                    .iter()
                    .any(|node| node["icon"] == icon && icon != "silent"));
            }
        }
    }

    #[test]
    fn animation_frames_stay_cached_beyond_sixty_images_and_reload_with_the_library() {
        let directory = tempfile::tempdir().unwrap();
        let mut preview = Preview::new(Path::new("/unused"));
        preview.library = directory.path().to_path_buf();
        let mut frames = Vec::new();
        let ctx = egui::Context::default();
        let project = Project::new();
        for index in 0..90 {
            let name = format!("frame{index}.png");
            image::RgbaImage::from_pixel(8, 8, image::Rgba([index, 0, 0, 255]))
                .save(directory.path().join(&name))
                .unwrap();
            let key = format!("@mood:{name}");
            frames.push((
                key.clone(),
                preview
                    .texture(&ctx, &project, &key, false, "")
                    .unwrap()
                    .0
                    .id(),
            ));
        }
        for (key, id) in &frames {
            assert_eq!(
                preview
                    .texture(&ctx, &project, key, false, "")
                    .unwrap()
                    .0
                    .id(),
                *id
            );
        }
        assert_eq!(preview.cache.len(), 90);
        std::fs::write(
            directory.path().join("preview_map.json"),
            b"{\"characters\":{},\"views\":{}}",
        )
        .unwrap();
        preview.set_library(directory.path()).unwrap();
        assert_ne!(
            preview
                .texture(&ctx, &project, &frames[0].0, false, "")
                .unwrap()
                .0
                .id(),
            frames[0].1
        );
    }

    #[test]
    fn cached_stage_updates_after_story_edits_selection_changes_and_undo() {
        let mut preview = Preview::new(Path::new("/unused"));
        let mut story = json!({"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"player","position":"L1"},
            {"id":"say","type":"say","text":"原文"},
            {"id":"done","type":"end"}
        ]});
        let before = story.clone();
        let pointer = preview.state(&story, "say")["dialog"]["text"]
            .as_str()
            .unwrap()
            .as_ptr();
        assert_eq!(
            preview.state(&story, "say")["dialog"]["text"]
                .as_str()
                .unwrap()
                .as_ptr(),
            pointer
        );
        story["nodes"][0]["position"] = json!("R1");
        story["nodes"][1]["text"] = json!("新文");
        assert_eq!(preview.state(&story, "say")["dialog"]["text"], "新文");
        assert_eq!(
            preview.simulation.as_ref().unwrap().before["actors"]["player"]["position"],
            "R1"
        );
        assert!(preview.state(&story, "show")["dialog"].is_null());
        assert_eq!(preview.state(&before, "say")["dialog"]["text"], "原文");
        assert_eq!(
            preview.state(&before, "say")["actors"]["player"]["position"],
            "L1"
        );
    }
    #[test]
    fn cached_images_detect_library_and_same_path_project_replacements() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let mut preview = Preview::new(Path::new("/unused"));
        preview.library = directory.path().to_path_buf();
        let mut project = Project::new();
        let ctx = egui::Context::default();
        let first = preview
            .texture(&ctx, &project, "image.png", false, "")
            .unwrap()
            .0;
        assert_eq!(
            preview
                .texture(&ctx, &project, "image.png", false, "")
                .unwrap()
                .0
                .id(),
            first.id()
        );
        image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 255, 0, 255]))
            .save(&path)
            .unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            )
            .unwrap();
        assert_ne!(
            preview
                .texture(&ctx, &project, "image.png", false, "")
                .unwrap()
                .0
                .id(),
            first.id()
        );
        project
            .assets
            .insert("picture.png".into(), std::fs::read(&path).unwrap());
        let first = preview
            .texture(&ctx, &project, "picture.png", false, "")
            .unwrap()
            .0;
        image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 255, 255]))
            .save(&path)
            .unwrap();
        project
            .assets
            .insert("picture.png".into(), std::fs::read(&path).unwrap());
        assert_ne!(
            preview
                .texture(&ctx, &project, "picture.png", false, "")
                .unwrap()
                .0
                .id(),
            first.id()
        );
    }
    #[test]
    fn old_portrait_libraries_offer_reextraction_without_changing_the_project() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("assets")).unwrap();
        image::RgbaImage::from_pixel(80, 100, image::Rgba([255, 0, 0, 255]))
            .save(directory.path().join("assets/normal.png"))
            .unwrap();
        let mapping = json!({"characters":{"fixture":{"first":"normal","portraits":{"normal":"assets/normal.png"}}},"views":{}});
        std::fs::write(
            directory.path().join("preview_map.json"),
            serde_json::to_vec(&mapping).unwrap(),
        )
        .unwrap();
        let project = Project::new();
        let before = project.stories.clone();
        let story = json!({"start":"show","nodes":[{"id":"show","type":"show","character":"fixture","portrait":"normal"}]});
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        preview.set_library(directory.path()).unwrap();
        let context = egui::Context::default();
        context.enable_accesskit();
        let draw = |preview: &mut Preview, events| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        preview.show(ui, &project, &story, 0, None);
                    });
                },
            )
        };
        let output = draw(&mut preview, vec![]);
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        assert!(nodes.iter().any(|(_, node)| node
            .label()
            .or_else(|| node.value())
            .is_some_and(|label| label.contains("旧版预览素材"))));
        let button = nodes
            .into_iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(crate::i18n::tr("从游戏提取预览素材…").as_str()))
                .then_some(id)
            })
            .unwrap();
        draw(
            &mut preview,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: button,
                    data: None,
                },
            )],
        );
        assert!(preview.extract_requested);
        assert_eq!(project.stories, before);
        preview.mapping["portrait_canvas"] = json!("sprite_rect");
        let output = draw(&mut preview, vec![]);
        assert!(!output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node
                .label()
                .or_else(|| node.value())
                .is_some_and(|label| label.contains("旧版预览素材"))));
    }
    #[test]
    fn portraits_reach_the_stage_bottom_and_keep_explicit_vertical_offsets() {
        let mut project = Project::new();
        let root = "assets/user/character/demo.actor";
        project.assets.insert(
            format!("{root}/content.json"),
            serde_json::to_vec(&json!({"id":"demo.actor", "type":"character", "portraits":{"normal":"normal.png"}})).unwrap(),
        );
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(80, 100, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        project
            .assets
            .insert(format!("{root}/normal.png"), png.into_inner());

        for size in [Vec2::new(800.0, 500.0), Vec2::new(500.0, 800.0)] {
            for (position, offset_y) in [("L1", 0), ("R1", 0), ("BC1", 0), ("L1", 54)] {
                let story = json!({"start":"show", "nodes":[
                    {"id":"show", "type":"show", "character":"user:demo.actor", "portrait":"normal", "position":position},
                    {"id":"offset", "type":"offset", "character":"user:demo.actor", "y":offset_y}
                ]});
                let context = egui::Context::default();
                let mut preview = Preview::new(Path::new("/not-a-repository"));
                let output = context.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            preview.show(ui, &project, &story, 1, None);
                        });
                    },
                );
                let (stage, bottom) = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Mesh(mesh) = &shape.shape {
                            Some((
                                shape.clip_rect,
                                mesh.vertices
                                    .iter()
                                    .map(|v| v.pos.y)
                                    .fold(f32::NEG_INFINITY, f32::max),
                            ))
                        } else {
                            None
                        }
                    })
                    .expect("the portrait must render as a textured mesh");
                if offset_y == 0 {
                    assert!(
                        bottom >= stage.bottom(),
                        "{position}: portrait leaves a gap below its feet at {size:?}"
                    );
                } else {
                    assert!(
                        bottom < stage.bottom() - stage.height() * 0.04,
                        "an author's upward offset must still lift the portrait"
                    );
                }
            }
        }
    }

    #[test]
    fn omitted_dialogue_mode_renders_character_name_and_matching_text_position() {
        let mut project = Project::new();
        project.assets.insert(
            "assets/user/character/demo.actor/content.json".into(),
            serde_json::to_vec(&json!({"id":"demo.actor", "type":"character", "name":"林灯"}))
                .unwrap(),
        );
        let render = |mode: Option<&str>| {
            let mut node = json!({"id":"say", "type":"say", "character":"user:demo.actor", "text":"默认对白正文"});
            if let Some(mode) = mode {
                node["mode"] = mode.into();
            }
            let story = json!({"start":"say", "nodes":[node]});
            let context = egui::Context::default();
            let mut preview = Preview::new(Path::new("/not-a-repository"));
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        preview.show(ui, &project, &story, 0, None);
                    });
                },
            );
            output
                .shapes
                .into_iter()
                .filter_map(|shape| match shape.shape {
                    egui::Shape::Text(text)
                        if ["林灯", "默认对白正文"].contains(&text.galley.job.text.as_str()) =>
                    {
                        Some((text.galley.job.text.clone(), text.pos))
                    }
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>()
        };
        let explicit = render(Some("character"));
        assert!(explicit.contains_key("林灯"));
        assert!(explicit["林灯"].y < explicit["默认对白正文"].y);
        assert_eq!(render(None), explicit);
        for mode in ["narrative", "center"] {
            let rendered = render(Some(mode));
            assert!(
                !rendered.contains_key("林灯"),
                "{mode} must omit the speaker name"
            );
            assert!(rendered.contains_key("默认对白正文"));
        }
    }
    #[test]
    fn preview_names_use_character_metadata_without_selector_numbers() {
        let mut project = Project::new();
        for id in ["demo.first", "demo.second"] {
            project.assets.insert(
                format!("assets/user/character/{id}/content.json"),
                serde_json::to_vec(&json!({"id":id, "type":"character", "name":"林灯"})).unwrap(),
            );
            assert_eq!(
                character_display_name(&project, &format!("user:{id}")),
                "林灯"
            );
        }
        let original_assets = project.assets.clone();
        assert_eq!(
            character_display_name(&project, "player"),
            crate::i18n::term("characters", "player", "赵活")
        );
        for unknown in ["future_character", "user:future.actor"] {
            assert_eq!(character_display_name(&project, unknown), unknown);
        }
        assert_eq!(project.assets, original_assets);
    }
    #[test]
    fn user_image_and_character_follow_files_main_contract() {
        let mut project = Project::new();
        let image_meta = include_bytes!(
            "../../../samples/showcase3/assets/user/image/showcase.courier_station/content.json"
        );
        project.assets.insert(
            "assets/user/image/showcase.courier_station/content.json".into(),
            image_meta.to_vec(),
        );
        project.assets.insert(
            "assets/user/image/showcase.courier_station/courier_station.png".into(),
            vec![1, 2, 3],
        );
        let character_meta = include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/content.json"
        );
        project.assets.insert(
            "assets/user/character/showcase.lin_deng/content.json".into(),
            character_meta.to_vec(),
        );
        project.assets.insert(
            "assets/user/character/showcase.lin_deng/normal.png".into(),
            vec![4, 5],
        );
        project.assets.insert(
            "assets/user/character/showcase.lin_deng/happy.png".into(),
            vec![6, 7],
        );
        let preview = Preview::new(Path::new("/not-a-repository"));
        assert_eq!(
            preview
                .asset(&project, "user:showcase.courier_station", false, "")
                .unwrap()
                .1,
            AssetSource::Project(&[1, 2, 3])
        );
        assert_eq!(
            preview
                .asset(&project, "user:showcase.lin_deng", true, "happy")
                .unwrap()
                .1,
            AssetSource::Project(&[6, 7])
        );
        assert_eq!(
            preview
                .asset(&project, "user:showcase.lin_deng", true, "missing")
                .unwrap()
                .1,
            AssetSource::Project(&[4, 5])
        );
        assert!(preview.mapping["characters"].is_object());
    }
    #[test]
    fn legacy_dice_keeps_all_three_outcome_edges() {
        let story = json!({"nodes":[{"id":"d","type":"dice","options":[{"goto_大成功":"a","goto_成功":"b","goto_失败":"c"}]}]});
        let targets: std::collections::BTreeSet<_> =
            edges(&story).into_iter().map(|(_, to, _)| to).collect();
        assert_eq!(
            targets,
            std::collections::BTreeSet::from(["a".into(), "b".into(), "c".into()])
        );
    }
}

#[cfg(test)]
mod card_tests {
    use super::*;
    #[test]
    fn ending_uses_description_and_project_image() {
        let p = Project::new();
        let c=card_data(&p,&json!({"type":"goto_scene","scene":"End","title":"终章","desc":"真正结局正文","text":"错误字段","image":"assets/end.png"})).unwrap();
        assert_eq!(c.body, "真正结局正文");
        assert_eq!(c.image, "assets/end.png");
    }
    #[test]
    fn character_intro_uses_own_metadata_and_embedded_official_text() {
        let mut p = Project::new();
        p.assets.insert("assets/characters/demo.hero/content.json".into(),serde_json::to_vec(&json!({"id":"demo.hero","name":"角色名","title":"默认称号","intro":{"name":"介绍名","title":"介绍称号","text":"自定义正文","image":"intro.png"}})).unwrap());
        let c = card_data(
            &p,
            &json!({"type":"intro","intro_source":"character","character":"user:demo.hero"}),
        )
        .unwrap();
        assert_eq!(
            (c.title.as_str(), c.name.as_str(), c.body.as_str()),
            ("介绍称号", "介绍名", "自定义正文")
        );
        assert_eq!(c.image, "assets/characters/demo.hero/intro.png");
        let data = lom_core::validate::editor_data();
        if let Some(official) = data["characters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["intro"].as_str().is_some_and(|s| !s.is_empty()))
        {
            let c = card_data(
                &p,
                &json!({"type":"intro","intro_source":"official","character":official["id"]}),
            )
            .unwrap();
            assert_eq!(c.body, official["intro"].as_str().unwrap());
        }
    }
}

#[cfg(test)]
mod filter_performance_tests {
    use super::*;

    #[test]
    fn filter_fade_reuses_asset_and_effect_textures_without_changing_original_layers() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("own.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([80, 40, 10, 73]))
            .save(&file)
            .unwrap();
        let mut project = Project::new();
        project
            .assets
            .insert("assets/own.png".into(), std::fs::read(&file).unwrap());
        let frame = effects::Frame { file, time: 0.0 };
        let context = egui::Context::default();
        let mut preview = Preview::new(Path::new("/not-a-repository"));
        let mut ids = None;
        for weight in [0.1, 0.5, 1.0, 0.5, 0.1, 0.1] {
            let previous = ids;
            let output = context.run(egui::RawInput::default(), |ctx| {
                preview.camera_filter = filters::Settings::from_state(&json!({
                    "camera_filters":{"stage-memory":true},"camera_filter_weights":{"stage-memory":weight}
                }));
                let filtered = preview.texture(ctx, &project, "assets/own.png", false, "").unwrap().0;
                let effect = preview.effect_texture(ctx, &frame, [8, 8]).unwrap().0;
                preview.camera_filter = filters::Settings::default();
                let original = preview.texture(ctx, &project, "assets/own.png", false, "").unwrap().0;
                assert_ne!(filtered.id(), original.id(), "original-color actors have a separate texture");
                ids = Some((filtered.id(), effect.id(), original.id()));
                if let Some(previous) = previous { assert_eq!(ids.unwrap(), previous); }
            });
            if weight == 0.1 && previous.is_some() {
                assert!(
                    output
                        .textures_delta
                        .set
                        .iter()
                        .all(|(id, _)| *id != ids.unwrap().2),
                    "a filtered frame must not change the original texture"
                );
            }
            assert_eq!(
                preview.cache.len(),
                3,
                "weights must not create more cache entries"
            );
        }
        let output = context.run(egui::RawInput::default(), |ctx| {
            preview.camera_filter = filters::Settings::from_state(&json!({
                "camera_filters":{"stage-memory":true},"camera_filter_weights":{"stage-memory":0.1}
            }));
            let _ = preview
                .texture(ctx, &project, "assets/own.png", false, "")
                .unwrap();
            let _ = preview.effect_texture(ctx, &frame, [8, 8]).unwrap();
        });
        assert!(
            output.textures_delta.set.is_empty(),
            "a static frame needs no image upload"
        );
    }

    // Run manually in release mode: uses only our own showcase art, never game assets.
    #[test]
    #[ignore]
    fn measure_filter_fade_texture_cost() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let background = "assets/user/image/showcase.courier_station/courier_station.png";
        let portrait = "assets/user/character/showcase.lin_deng/normal.png";
        let mut project = Project::new();
        for file in [background, portrait] {
            project.assets.insert(
                file.into(),
                std::fs::read(root.join("samples/showcase3").join(file)).unwrap(),
            );
        }
        let frame = effects::Frame {
            file: root.join("samples/showcase3").join(background),
            time: 0.0,
        };
        for preset in ["stage-memory", "stage-gray-100", "stage-dream"] {
            for frames in [10, 60] {
                let context = egui::Context::default();
                let mut preview = Preview::new(Path::new("/not-a-repository"));
                let mut uploads = 0;
                let mut upload_bytes = 0;
                let mut peak_cache = 0;
                let mut unique_textures = std::collections::HashSet::new();
                let mut frame_times = Vec::new();
                let started = std::time::Instant::now();
                for index in 0..frames {
                    let frame_started = std::time::Instant::now();
                    let weight = (index + 1) as f64 / frames as f64;
                    preview.camera_filter = filters::Settings::from_state(&json!({
                        "camera_filters":{preset:true},
                        "camera_filter_weights":{preset:weight}
                    }));
                    let output = context.run(egui::RawInput::default(), |ctx| {
                        let _ = preview
                            .texture(ctx, &project, background, false, "")
                            .unwrap();
                        let _ = preview.texture(ctx, &project, portrait, false, "").unwrap();
                        let _ = preview.effect_texture(ctx, &frame, [1672, 941]).unwrap();
                    });
                    unique_textures.extend(output.textures_delta.set.iter().map(|(id, _)| *id));
                    uploads += output.textures_delta.set.len();
                    upload_bytes += output
                        .textures_delta
                        .set
                        .iter()
                        .map(|(_, delta)| delta.image.size().iter().product::<usize>() * 4)
                        .sum::<usize>();
                    peak_cache =
                        peak_cache.max(preview.cache.values().map(|c| c.bytes).sum::<usize>());
                    frame_times.push(frame_started.elapsed().as_secs_f64() * 1000.0);
                }
                println!("FILTER_PERF preset={preset} frames={frames} ms={:.3} cold_ms={:.3} warm_ms={:.3} uploads={uploads} textures={} upload_mib={:.3} cache_entries={} peak_cache_mib={:.3}",
                    started.elapsed().as_secs_f64()*1000.0,
                    frame_times[0], frame_times[1..].iter().sum::<f64>() / (frames - 1) as f64,
                    unique_textures.len(),
                    upload_bytes as f64/1048576.0,preview.cache.len(),peak_cache as f64/1048576.0);
            }
        }
    }
}

#[cfg(test)]
mod prelude_tests {
    use super::*;
    #[test]
    fn playtest_prelude_restores_only_visual_state_before_target_and_avoids_id_collisions() {
        let mut story = json!({"id":"main","start":"scene","nodes":[{"id":"scene","type":"scene","view":"black"},{"id":"zz_playtest_0","type":"show","character":"player","position":"L","portrait":"normal","facing":"right","appearance":"beautified"},{"id":"target","type":"hide","character":"player"},{"id":"end","type":"end"}]});
        let nodes = build_playtest_prelude(&story, "target");
        assert_eq!(nodes.len(), 2);
        assert_ne!(nodes[0]["id"], "zz_playtest_0");
        assert_eq!(nodes[1]["character"], "player");
        assert_eq!(nodes[1]["appearance"], "beautified");
        assert_eq!(nodes[1]["goto"], "target");
        story["start"] = nodes[0]["id"].clone();
        story["nodes"].as_array_mut().unwrap().extend(nodes);
        lom_core::validate::validate_story(&story).unwrap();
        assert!(build_playtest_prelude(&story, "scene").is_empty());
    }
}
