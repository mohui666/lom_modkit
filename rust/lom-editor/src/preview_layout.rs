//! Transactional edits made by dragging the stage preview.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Character(String),
    Mood(String),
    Overlay(String),
    Cg,
    Effect { node: String, action: Option<usize> },
}

#[derive(Clone, Debug)]
pub struct Change {
    pub target: Target,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub effect_scale: Option<[f64; 2]>,
}

#[derive(Clone, Copy)]
enum Source {
    Action(usize, Option<usize>),
    Raw,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

fn affects(action: &Value, target: &Target) -> bool {
    match target {
        Target::Character(character) => {
            text(action, "character") == character
                && matches!(text(action, "type"), "show" | "hide" | "move" | "offset")
        }
        Target::Mood(character) => {
            action["type"] == "mood" && text(action, "character") == character
        }
        Target::Overlay(slot) => {
            action["type"] == "overlay" && action["slot"].as_str().unwrap_or("main") == slot
        }
        Target::Cg => action["type"] == "custom_cg",
        Target::Effect { .. } => false,
    }
}

fn observe(action: &Value, target: &Target, location: Source, source: &mut Option<Source>) {
    if action["type"] == "raw" {
        let (actions, _) = lom_core::lua_import::preview_nodes(text(action, "code"));
        for action in actions {
            observe(&action, target, Source::Raw, source);
        }
    } else if affects(action, target) {
        *source = if action["type"] == "hide" || action["action"] == "hide" {
            None
        } else {
            Some(location)
        };
    }
}

fn source_on_route(story: &Value, current: &str, target: &Target) -> Result<Source> {
    let path = crate::preview::route(story, current).context("当前步骤无法从剧情开头到达")?;
    let nodes = story["nodes"].as_array().context("剧情缺少步骤列表")?;
    let mut source = None;
    for id in path {
        let (index, parent) = nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node["id"] == id)
            .context("预览路径中的步骤不存在")?;
        if parent["type"] == "parallel" {
            for (action, node) in lom_core::parallel::actions(parent).iter().enumerate() {
                observe(
                    node,
                    target,
                    Source::Action(index, Some(action)),
                    &mut source,
                );
            }
        } else {
            observe(parent, target, Source::Action(index, None), &mut source);
        }
    }
    source.context("当前预览中没有可调整的目标")
}

fn write_offset(node: &mut Value, change: &Change) {
    node["x"] = json!(change.x);
    node["y"] = json!(change.y);
    node["scale"] = json!(change.scale);
    node["absolute"] = json!(true);
}

fn apply_inner(story: &mut Value, selected: usize, change: &Change) -> Result<usize> {
    ensure!(
        change.x.is_finite() && change.y.is_finite(),
        "位置必须是有限数值"
    );
    ensure!(change.scale.is_finite(), "缩放必须是有限数值");
    if matches!(change.target, Target::Mood(_)) {
        ensure!(
            change.x.abs() <= 1920.0 && change.y.abs() <= 1080.0,
            "气泡偏移须在横向 ±1920、纵向 ±1080 范围内"
        );
    }
    lom_core::content::positive_scale(&json!(change.scale), "scale")?;
    if matches!(&change.target, Target::Overlay(_) | Target::Cg) {
        ensure!(
            (-100.0..=100.0).contains(&change.x) && (-100.0..=100.0).contains(&change.y),
            "图片位置必须在 -100～100% 之间"
        );
    }
    let nodes = story["nodes"].as_array().context("剧情缺少步骤列表")?;
    let current = nodes.get(selected).context("所选步骤不存在")?;
    ensure!(current.is_object(), "所选步骤必须是对象");
    let current_id = current["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .context("所选步骤缺少 ID")?
        .to_owned();
    if let Target::Effect { node, action } = &change.target {
        let path =
            crate::preview::route(story, &current_id).context("当前步骤无法从剧情开头到达")?;
        ensure!(
            path.iter().any(|id| id == node),
            "特效来源不在当前预览路径中"
        );
        let parent = nodes
            .iter()
            .position(|entry| entry["id"] == *node)
            .context("特效来源步骤不存在")?;
        let source = if let Some(action) = action {
            ensure!(
                nodes[parent]["type"] == "parallel",
                "特效来源已不属于同时执行组"
            );
            nodes[parent]["actions"]
                .as_array()
                .and_then(|actions| actions.get(*action))
                .context("特效来源动作不存在")?
        } else {
            &nodes[parent]
        };
        ensure!(
            source["type"] == "effect" && crate::preview::effects::plays(source),
            "仅可调整可编辑的特效播放动作"
        );
        let ratio = change.scale / 100.0;
        let [a, b] = change.effect_scale.unwrap_or([
            source["a"].as_f64().unwrap_or(1.0) * ratio,
            source["b"].as_f64().unwrap_or(1.0) * ratio,
        ]);
        ensure!(a.is_finite() && b.is_finite(), "特效缩放结果超出可表示范围");
        let source = if let Some(action) = action {
            &mut story["nodes"][parent]["actions"][*action]
        } else {
            &mut story["nodes"][parent]
        };
        source["x"] = json!(change.x);
        source["y"] = json!(change.y);
        source["a"] = json!(a);
        source["b"] = json!(b);
        return Ok(selected);
    }
    let state = crate::preview::simulate(story, &current_id);
    let visible = match &change.target {
        Target::Character(character) => {
            !character.is_empty() && state["actors"][character].is_object()
        }
        Target::Mood(character) => {
            state["actors"][character].is_object()
                && state["mood_overrides"][character]["enabled"] == true
        }
        Target::Overlay(slot) => state["overlays"][slot].is_object(),
        Target::Cg => state["custom_cg"].is_object(),
        Target::Effect { .. } => unreachable!(),
    };
    ensure!(visible, "目标尚未显示或已经隐藏，无法调整布局");
    let source = source_on_route(story, &current_id, &change.target)?;
    ensure!(
        !matches!(source, Source::Raw),
        "当前目标来自高级 Lua 静态预览，请先改为可编辑的演出步骤"
    );
    let Source::Action(parent, child) = source else {
        unreachable!()
    };

    if matches!(change.target, Target::Mood(_)) {
        let action = if let Some(child) = child {
            &mut story["nodes"][parent]["actions"][child]
        } else {
            &mut story["nodes"][parent]
        };
        action["offset_x"] = json!(change.x);
        action["offset_y"] = json!(change.y);
        return Ok(selected);
    }

    if let Target::Character(character) = &change.target {
        if current["type"] == "offset" && text(current, "character") == character {
            write_offset(&mut story["nodes"][selected], change);
            return Ok(selected);
        }
        let action = child.map_or(&nodes[parent], |child| &nodes[parent]["actions"][child]);
        if action["type"] == "offset" && action["absolute"] == true {
            let action = if let Some(child) = child {
                &mut story["nodes"][parent]["actions"][child]
            } else {
                &mut story["nodes"][parent]
            };
            write_offset(action, change);
            return Ok(selected);
        }
        let offset_id = (1..)
            .map(|number| format!("offset{number}"))
            .find(|id| !nodes.iter().any(|node| node["id"] == *id))
            .unwrap();
        let mut offset = json!({"id":offset_id,"type":"offset","character":character,"duration":0});
        write_offset(&mut offset, change);
        let successor = nodes[parent]["goto"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        if let Some(successor) = successor {
            offset["goto"] = json!(successor);
            // Explicit branches must pass through the new layout step.
            story["nodes"][parent]["goto"] = json!(offset_id);
        }
        // Keep the layout on the branch which actually showed or moved the
        // character. Sequential steps keep their implicit continuation, so
        // adding and deleting a layout does not introduce a permanent jump.
        story["nodes"]
            .as_array_mut()
            .unwrap()
            .insert(parent + 1, offset);
        Ok(if parent <= selected {
            selected + 1
        } else {
            selected
        })
    } else {
        let action = if let Some(child) = child {
            &mut story["nodes"][parent]["actions"][child]
        } else {
            &mut story["nodes"][parent]
        };
        ensure!(
            action.is_object() && action["action"] == "show",
            "当前图片没有可编辑的显示步骤"
        );
        action["x"] = json!(change.x);
        action["y"] = json!(change.y);
        action["scale"] = json!(change.scale);
        Ok(selected)
    }
}

pub fn apply(story: &mut Value, selected: usize, change: &Change) -> Result<usize> {
    let mut next = story.clone();
    let selected = apply_inner(&mut next, selected, change)?;
    *story = next;
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_alignment_edits_its_own_source_without_changing_the_character_or_timing() {
        let mut source = story(json!([
            {"id":"show","type":"show","character":"player"},
            {"id":"bubble","type":"mood","character":"player","enabled":true,"icon":"laugh1","wait_after":0.4},
            {"id":"line","type":"say","mode":"narrative","text":"test"}
        ]));
        let before = source.clone();
        let change = Change {
            target: Target::Mood("player".into()),
            x: 20.5,
            y: -30.5,
            scale: 100.0,
            effect_scale: None,
        };
        apply(&mut source, 2, &change).unwrap();
        assert_eq!(source["nodes"][0], before["nodes"][0]);
        assert_eq!(source["nodes"][1]["offset_x"], 20.5);
        assert_eq!(source["nodes"][1]["wait_after"], 0.4);
        assert_eq!(source["nodes"][1]["icon"], "laugh1");
        assert_eq!(
            crate::preview::simulate(&source, "line")["mood_overrides"]["player"]["offset_y"],
            -30.5
        );
    }

    #[test]
    fn pasted_effect_layout_preserves_both_signed_scale_axes() {
        let mut source = story(
            json!([{ "id":"fx","type":"effect","name":"Glow","a":0.5,"b":-2.0},{"id":"line","type":"say","mode":"narrative","text":"test"}]),
        );
        apply(
            &mut source,
            1,
            &Change {
                target: Target::Effect {
                    node: "fx".into(),
                    action: None,
                },
                x: 25.0,
                y: 75.0,
                scale: 100.0,
                effect_scale: Some([-3.0, 0.75]),
            },
        )
        .unwrap();
        assert_eq!(source["nodes"][0]["a"], -3.0);
        assert_eq!(source["nodes"][0]["b"], 0.75);
    }

    fn story(nodes: Value) -> Value {
        json!({"story_schema":2,"id":"main","start":nodes[0]["id"],"nodes":nodes})
    }
    fn character(x: f64) -> Change {
        Change {
            target: Target::Character("player".into()),
            x,
            y: 60.0,
            scale: 175.5,
            effect_scale: None,
        }
    }

    #[test]
    fn branch_merge_edits_only_the_showing_branch_source() {
        let mut source = story(json!([
            {"id":"choice1","type":"choice","options":[{"goto":"a_show","text":"人物登场"},{"goto":"b_wait","text":"无人物"}]},
            {"id":"a_show","type":"show","character":"player","goto":"current"},
            {"id":"b_wait","type":"wait","seconds":1,"goto":"current"},
            {"id":"current","type":"say","mode":"narrative","text":"汇合对白","goto":"current"},
            {"id":"end","type":"end"}
        ]));
        let original = source.clone();
        let selected = apply(&mut source, 3, &character(120.0)).unwrap();
        assert_eq!(selected, 4);
        assert_eq!(source["nodes"][selected], original["nodes"][3]);
        assert_eq!(source["nodes"][0], original["nodes"][0]);
        assert_eq!(source["start"], "choice1");
        assert_eq!(source["nodes"][1]["goto"], "offset1");
        assert_eq!(source["nodes"][2]["goto"], "current");
        assert_eq!(source["nodes"][3], original["nodes"][2]);
        assert_eq!(lom_core::analysis::successors(&source, 3), vec!["current"]);
        assert_eq!(source["nodes"][selected]["goto"], "current");
    }

    #[test]
    fn implicit_source_insertion_is_reused_on_the_next_drag() {
        let mut source = story(json!([
            {"id":"show1","type":"show","character":"player"},
            {"id":"say1","type":"say","character":"player","text":"对白"},
            {"id":"end","type":"end"}
        ]));
        let selected = apply(&mut source, 1, &character(120.0)).unwrap();
        assert_eq!(selected, 2);
        assert_eq!(lom_core::analysis::successors(&source, 0), vec!["offset1"]);
        assert_eq!(lom_core::analysis::successors(&source, 1), vec!["say1"]);
        assert!(source["nodes"][0].get("goto").is_none());
        assert!(source["nodes"][1].get("goto").is_none());
        assert_eq!(
            apply(&mut source, selected, &character(180.0)).unwrap(),
            selected
        );
        assert_eq!(source["nodes"].as_array().unwrap().len(), 4);
        assert_eq!(source["nodes"][1]["x"], 180.0);
        assert_eq!(source["nodes"][1]["absolute"], true);
        assert_eq!(source["nodes"][1]["duration"], 0);
    }

    #[test]
    fn parallel_show_inserts_after_and_updates_the_selected_offset() {
        let mut source = story(json!([
            {"id":"parallel1","type":"parallel","goto":"end","actions":[
                {"type":"show","character":"player","position":"L1"},
                {"type":"show","character":"girl1","position":"R1"}]},
            {"id":"skipped","type":"say","text":"跳过"},
            {"id":"end","type":"end"}
        ]));
        let selected = apply(&mut source, 0, &character(120.0)).unwrap();
        assert_eq!(selected, 1);
        assert_eq!(source["nodes"][0]["goto"], "offset1");
        assert_eq!(source["nodes"][1]["goto"], "end");
        source["nodes"][1]["wait_after"] = json!(0.5);
        source["nodes"][1]["duration"] = json!(1.25);
        source["nodes"][1]["absolute"] = json!(false);
        assert_eq!(
            apply(&mut source, selected, &character(250.0)).unwrap(),
            selected
        );
        assert_eq!(source["nodes"].as_array().unwrap().len(), 4);
        assert_eq!(source["nodes"][1]["wait_after"], 0.5);
        assert_eq!(source["nodes"][1]["duration"], 1.25);
        assert_eq!(source["nodes"][1]["absolute"], true);
        assert_eq!(source["nodes"][1]["x"], 250.0);
        assert_eq!(source["nodes"][0]["actions"][1]["character"], "girl1");
    }

    #[test]
    fn existing_parallel_absolute_offset_retains_author_timing() {
        let mut source = story(json!([
            {"id":"show1","type":"show","character":"player"},
            {"id":"layout","type":"parallel","actions":[
                {"type":"offset","character":"player","absolute":true,"x":10,"y":20,"scale":100,"duration":2.5,"wait_after":0.4},
                {"type":"show","character":"girl1","position":"R1"}]},
            {"id":"say1","type":"say","mode":"narrative","text":"对白"}
        ]));
        let original = source.clone();
        assert_eq!(apply(&mut source, 2, &character(80.0)).unwrap(), 2);
        assert_eq!(source["nodes"].as_array().unwrap().len(), 3);
        let action = &source["nodes"][1]["actions"][0];
        assert_eq!(action["x"], 80.0);
        assert_eq!(action["duration"], 2.5);
        assert_eq!(action["wait_after"], 0.4);
        assert_eq!(
            source["nodes"][1]["actions"][1],
            original["nodes"][1]["actions"][1]
        );

        source["nodes"][1]["actions"][0]["absolute"] = json!(false);
        let selected = apply(&mut source, 2, &character(140.0)).unwrap();
        assert_eq!(source["nodes"][selected]["id"], "say1");
        assert_eq!(source["nodes"][2]["type"], "offset");
        assert_eq!(source["nodes"][2]["duration"], 0);
        assert_eq!(source["nodes"][1]["actions"][0]["duration"], 2.5);
    }

    #[test]
    fn images_edit_the_parallel_source_on_the_actual_route_and_keep_other_fields() {
        let mut source = story(json!([
            {"id":"pictures","type":"parallel","goto":"current","actions":[
                {"type":"overlay","action":"show","slot":"seal","image":"user:demo.seal","position":"left","opacity":45,"fade":0.2,"layer":"back"},
                {"type":"custom_cg","action":"show","image":"user:demo.cg","fade":0.7}]},
            {"id":"off_path","type":"overlay","action":"show","slot":"seal","image":"user:demo.other","position":"right"},
            {"id":"current","type":"say","mode":"narrative","text":"对白"}
        ]));
        let original = source.clone();
        let change = Change {
            target: Target::Overlay("seal".into()),
            x: 12.5,
            y: -20.0,
            scale: 240.5,
            effect_scale: None,
        };
        assert_eq!(apply(&mut source, 2, &change).unwrap(), 2);
        let action = &source["nodes"][0]["actions"][0];
        assert_eq!(action["x"], 12.5);
        for field in ["position", "opacity", "fade", "layer", "image"] {
            assert_eq!(action[field], original["nodes"][0]["actions"][0][field]);
        }
        assert_eq!(source["nodes"][1], original["nodes"][1]);
        let cg = Change {
            target: Target::Cg,
            ..change
        };
        apply(&mut source, 2, &cg).unwrap();
        assert_eq!(source["nodes"][0]["actions"][1]["scale"], 240.5);
        assert_eq!(source["nodes"][0]["actions"][1]["fade"], 0.7);
        let before = source.clone();
        assert!(apply(&mut source, 2, &Change { x: 101.0, ..cg }).is_err());
        assert_eq!(source, before);
    }

    #[test]
    fn raw_layout_and_invalid_numbers_leave_the_story_unchanged() {
        let mut source = story(json!([
            {"id":"raw1","type":"raw","code":"stage.show{character=characters.Get(\"player\"),portrait=characters.GetPortrait(\"player\",\"normal\"),toPosition=\"L1\",facing=\"right\"}"},
            {"id":"say1","type":"say","character":"player","text":"对白"}
        ]));
        let original = source.clone();
        assert!(apply(&mut source, 1, &character(0.0))
            .unwrap_err()
            .to_string()
            .contains("高级 Lua"));
        assert_eq!(source, original);
        assert!(apply(
            &mut source,
            1,
            &Change {
                x: f64::NAN,
                ..character(0.0)
            }
        )
        .is_err());
        assert_eq!(source, original);
    }

    #[test]
    fn effect_layout_changes_only_the_exact_parallel_source_and_keeps_signed_aspect() {
        let mut source = story(json!([
            {"id":"effects","type":"parallel","actions":[
                {"type":"wait","seconds":0.2},
                {"type":"effect","name":"Rain_001","space":"screen","x":50,"y":50,"a":-2,"b":0.5,"rotation":32,"speed":0.7,"tag":"weather","c":0,"wait_after":0.4}]},
            {"id":"later","type":"say","mode":"narrative","text":"对白"}
        ]));
        let original = source.clone();
        let change = Change {
            target: Target::Effect {
                node: "effects".into(),
                action: Some(1),
            },
            x: -225.0,
            y: 360.5,
            scale: 150.0,
            effect_scale: None,
        };
        assert_eq!(apply(&mut source, 1, &change).unwrap(), 1);
        assert_eq!(source["nodes"].as_array().unwrap().len(), 2);
        let effect = &source["nodes"][0]["actions"][1];
        assert_eq!(effect["x"], -225.0);
        assert_eq!(effect["y"], 360.5);
        assert_eq!(effect["a"], -3.0);
        assert_eq!(effect["b"], 0.75);
        for field in [
            "name",
            "space",
            "rotation",
            "speed",
            "tag",
            "c",
            "wait_after",
        ] {
            assert_eq!(effect[field], original["nodes"][0]["actions"][1][field]);
        }
        assert_eq!(
            source["nodes"][0]["actions"][0],
            original["nodes"][0]["actions"][0]
        );
        source["nodes"][0]["actions"][1]["b"] = json!(0);
        apply(&mut source, 1, &change).unwrap();
        assert_eq!(source["nodes"][0]["actions"][1]["b"], 0.0);
    }

    #[test]
    fn effect_layout_rejects_other_branches_and_stop_actions_transactionally() {
        let mut source = story(json!([
            {"id":"choice","type":"choice","options":[{"goto":"a"},{"goto":"z"}]},
            {"id":"a","type":"effect","name":"Rain_001","goto":"current"},
            {"id":"z","type":"effect","name":"Snow_001","goto":"current"},
            {"id":"current","type":"say","text":"对白"}
        ]));
        let original = source.clone();
        let change = Change {
            target: Target::Effect {
                node: "z".into(),
                action: None,
            },
            x: 12.0,
            y: -4.0,
            scale: 200.0,
            effect_scale: None,
        };
        assert!(apply(&mut source, 3, &change).is_err());
        assert_eq!(source, original);
        source["nodes"][1]["play"] = json!(false);
        let before = source.clone();
        assert!(apply(
            &mut source,
            3,
            &Change {
                target: Target::Effect {
                    node: "a".into(),
                    action: None
                },
                ..change
            }
        )
        .is_err());
        assert_eq!(source, before);
    }
}
