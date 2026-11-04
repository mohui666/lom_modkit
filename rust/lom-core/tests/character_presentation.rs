use lom_core::{codegen, release, validate};
use serde_json::{json, Value};

fn story(node: Value) -> Value {
    json!({"id":"main","start":"enter","nodes":[
        {"id":"enter","type":"show","character":"player","position":"L1"},
        node,{"id":"end","type":"end"}]})
}

#[test]
fn portrait_changes_wait_for_both_phases_without_dialogue_or_position_reset() {
    for character in ["player", "user:demo.actor"] {
        let node =
            json!({"id":"change","type":"portrait","character":character,"portrait":"normal"});
        let lua = codegen::compile_story(&story(node.clone()), None, None, None).unwrap();
        let change = lua
            .split("-- [change] portrait")
            .nth(1)
            .unwrap()
            .split("-- [end]")
            .next()
            .unwrap();
        assert!(change.contains(&format!(
            "mod_char_portrait_transition(\"{character}\", \"normal\", 0.5, false)\n\twait(0.5)"
        )));
        assert!(change.contains(&format!(
            "mod_char_portrait_transition(\"{character}\", \"normal\", 1.5, true)\n\twait(1.5)"
        )));
        for unwanted in [
            "stage.show",
            "stage.hide",
            "mod_char_show",
            "setcharacter",
            "\tsay(",
        ] {
            assert!(!change.contains(unwanted), "{change}");
        }
        let mut grouped = node;
        grouped.as_object_mut().unwrap().remove("id");
        let lua = codegen::compile_story(&story(json!({"id":"change","type":"parallel","actions":[grouped,{"type":"wait","seconds":0}]})), None, None, None).unwrap();
        assert_eq!(lua.matches("mod_char_portrait_transition(").count(), 2);
    }
}

#[test]
fn portrait_validation_and_preflight_require_an_existing_actor() {
    let change = json!({"id":"change","type":"portrait","character":"player","portrait":"normal"});
    assert!(validate::validate_story(&story(change.clone())).is_ok());
    for (field, value) in [
        ("fade_out", json!(-0.1)),
        ("fade_in", json!("1")),
        ("portrait", json!("unknown_portrait")),
        ("action", json!("invalid")),
    ] {
        let mut invalid = change.clone();
        invalid[field] = value;
        assert!(validate::validate_story(&story(invalid)).is_err());
    }
    let mut missing = story(change);
    missing["nodes"].as_array_mut().unwrap().remove(0);
    missing["start"] = json!("change");
    assert_eq!(
        release::find_stage_issues(&missing),
        vec![("change".into(), "player".into())]
    );
}

#[test]
fn left_narration_is_scoped_to_its_line_and_default_preserves_legacy_lua() {
    for mode in ["narrative", "center"] {
        let mut node = json!({"id":"line","type":"say","text":"旁白","mode":mode});
        let legacy = codegen::compile_story(&story(node.clone()), None, None, None).unwrap();
        node["text_align"] = json!("default");
        assert_eq!(
            legacy,
            codegen::compile_story(&story(node.clone()), None, None, None).unwrap()
        );
        node["text_align"] = json!("left");
        let lua = codegen::compile_story(&story(node.clone()), None, None, None).unwrap();
        assert!(lua.contains(
            "mod_narrative_left(true)\n\tsay(luamanager.GetStoryText(\"MOD_MOD_main_line\"))"
        ));
        let reset = lua.find("mod_narrative_left(false)").unwrap();
        let left = lua.find("mod_narrative_left(true)").unwrap();
        assert!(reset < left);
        assert!(!lua[left..].contains("mod_narrative_left(false)"));
        node["text_align"] = json!("invalid");
        assert!(validate::validate_story(&story(node)).is_err());
    }
}

#[test]
fn mood_switches_follow_execution_order_and_override_chapter_defaults() {
    for default in [true, false] {
        let mut input = story(json!({"id":"on","type":"mood","enabled":true}));
        input["mood"] = json!(default);
        let nodes = input["nodes"].as_array_mut().unwrap();
        nodes.insert(
            2,
            json!({"id":"talk","type":"say","character":"player","text":"气泡应开启"}),
        );
        nodes.insert(
            3,
            json!({"id":"off","type":"mood","enabled":false,"goto":"on"}),
        );
        let lua = codegen::compile_story(&input, None, None, None).unwrap();
        assert!(lua.contains("mod_set_mood(true)\n\tmod_refresh_mood()"));
        assert!(lua.contains("mod_set_mood(false)\n\tmod_hide_mood()\n\treturn node_on()"));
        let talk = lua
            .split("-- [talk] say")
            .nth(1)
            .unwrap()
            .split("-- [off]")
            .next()
            .unwrap();
        assert!(!talk.contains("mod_hide_mood"));
        input["nodes"][1]["enabled"] = json!(1);
        assert!(validate::validate_story(&input).is_err());
    }
}

#[test]
fn a_normal_only_actor_can_choose_bubbles_without_loading_other_portraits() {
    for (enabled, icon, expected) in [
        (true, "shock", "shock"),
        (true, "laugh1", "laugh1"),
        (false, "shock", "hide"),
        (true, "auto", "auto"),
    ] {
        let input = json!({"id":"main","start":"enter","nodes":[
            {"id":"enter","type":"show","character":"artist1","portrait":"normal","position":"M"},
            {"id":"bubble","type":"mood","character":"artist1","enabled":enabled,"icon":icon},
            {"id":"talk","type":"say","character":"artist1","portrait":"normal","text":"同一立绘"},
            {"id":"end","type":"end"}]});
        validate::validate_story(&input).unwrap();
        assert!(release::find_stage_issues(&input).is_empty());
        let lua = codegen::compile_story(&input, None, None, None).unwrap();
        assert!(lua.contains(&format!("mod_character_mood(\"artist1\", \"{expected}\")")));
        assert!(!lua.contains("mod_hide_mood()"));
        assert!(!lua.contains(&format!("GetPortrait(\"artist1\", \"{expected}\")")));
        let mut missing = input.clone();
        missing["nodes"].as_array_mut().unwrap().remove(0);
        missing["start"] = json!("bubble");
        assert!(release::find_stage_issues(&missing).contains(&("bubble".into(), "artist1".into())));
    }
    for fields in [
        json!({"character":"artist1","icon":"question"}),
        json!({"character":"user:demo.actor","icon":"shock"}),
        json!({"character":"unknown_actor","icon":"shock"}),
        json!({"character":"","icon":"shock"}),
        json!({"icon":"shock"}),
    ] {
        let mut node = json!({"id":"bubble","type":"mood","enabled":true});
        for (key, value) in fields.as_object().unwrap() {
            node[key] = value.clone();
        }
        assert!(validate::validate_story(&story(node)).is_err());
    }
}

#[test]
fn character_bubble_packages_require_a_host_with_the_new_entry_point() {
    let mut project = lom_core::project::Project::new();
    project.manifest["min_host_version"] = json!("1.4.0");
    project.stories.insert(
        "main".into(),
        story(
            json!({"id":"bubble","type":"mood","character":"player","enabled":true,"icon":"shock"}),
        ),
    );
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("character-bubble.lommod");
    project.export(&output).unwrap();
    let entries = lom_core::package::read_package(&output).unwrap();
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(manifest["min_host_version"], "1.4.1");
    assert_eq!(project.manifest["min_host_version"], "1.4.0");
}

#[test]
fn export_requires_the_new_host_only_when_using_the_new_controls() {
    for node in [
        json!({"id":"change","type":"portrait","character":"player","portrait":"normal"}),
        json!({"id":"switch","type":"mood","enabled":true}),
        json!({"id":"line","type":"say","mode":"narrative","text":"旁白","text_align":"left"}),
    ] {
        let mut project = lom_core::project::Project::new();
        project.manifest["min_host_version"] = json!("1.1.3");
        project.stories.insert("main".into(), story(node));
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("new-controls.lommod");
        project.export(&output).unwrap();
        let entries = lom_core::package::read_package(&output).unwrap();
        let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
        assert_eq!(manifest["min_host_version"], "1.3.6");
        assert_eq!(project.manifest["min_host_version"], "1.1.3");
    }
}
