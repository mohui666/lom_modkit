use lom_core::{codegen, package, project::Project, story_api, validate};
use serde_json::{json, Value};

fn story(actions: Vec<Value>) -> Value {
    let mut nodes: Vec<Value> = actions
        .into_iter()
        .enumerate()
        .map(|(index, mut node)| {
            node["id"] = json!(format!("step{index}"));
            node
        })
        .collect();
    nodes.push(json!({"id":"done","type":"end"}));
    json!({"id":"main","start":"step0","nodes":nodes})
}

#[test]
fn pure_legacy_effects_keep_world_defaults_and_play_over_d_priority() {
    for (fields, suffix) in [
        (json!({}), "1"),
        (json!({"d":0}), "0"),
        (json!({"d":2}), "2"),
        (json!({"d":0,"play":true}), "1"),
        (json!({"d":1,"play":false}), "0"),
    ] {
        let mut effect = json!({"type":"effect","name":"Hit_001","a":-2.5,"b":0,"c":2});
        effect
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let lua = codegen::compile_story(&story(vec![effect]), None, None, None).unwrap();
        assert!(lua.contains(&format!(
            "effects.SetupEffect(\"Hit_001\", 0, 0, -2.5, 0, 2, {suffix})"
        )));
        assert!(!lua.contains("mod_effect("));
    }
}

#[test]
fn one_new_field_manages_the_whole_chapter_including_parallel_and_legacy_stop() {
    let input = story(vec![
        json!({"type":"effect","name":"Hit_001","x":-2.5,"c":2}),
        json!({"type":"parallel","actions":[
            {"type":"effect","name":"Smoke_001","space":"screen","a":-2,"b":0.5,
                "rotation":-45.5,"speed":0,"stop_after":3,"restart":true,"tag":"smoke","wait_after":0.75},
            {"type":"wait","seconds":0.25}]}),
        json!({"type":"effect","name":"Hit_001","d":0,"wait_after":0.5}),
    ]);
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    assert!(!lua.contains("effects.SetupEffect"));
    assert_eq!(lua.matches("mod_effect(").count(), 3);
    assert!(lua.contains(
        "mod_effect(\"Hit_001\", -2.5, 0, 1, 1, 2, true, \"world\", 0, 1, -1, false, false, \"\")"
    ));
    assert!(lua.contains("mod_effect(\"Smoke_001\", 50, 50, -2, 0.5, 1, true, \"screen\", -45.5, 0, 3, false, true, \"smoke\")\n\t\t\twait(0.75)"));
    assert!(lua.contains("mod_effect(\"Hit_001\", 0, 0, 1, 1, 1, false, \"world\", 0, 1, -1, false, false, \"\")\n\twait(0.5)"));
    assert!(
        !lua.contains("wait(3)"),
        "automatic stop must not block the chapter"
    );
}

#[test]
fn every_new_control_opts_in_and_values_keep_their_meaning() {
    let controls = json!({"space":"world","rotation":0,"speed":1,"stop_after":0,"clear":false,"restart":false,"tag":""});
    for (key, value) in controls.as_object().unwrap() {
        let mut effect = json!({"type":"effect","name":"Hit_001"});
        effect[key] = value.clone();
        let lua = codegen::compile_story(&story(vec![effect]), None, None, None).unwrap();
        assert!(lua.contains("mod_effect("), "{key}");
    }
    let input = story(vec![
        json!({"type":"effect","name":"Hit_001","space":"screen",
        "x":-200.5,"y":350.25,"a":0,"b":-5.5,"c":2,"play":false,"d":1,
        "rotation":720.5,"speed":1000.25,"clear":true,"tag":"branch\\\"tag"}),
    ]);
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    assert!(lua.contains(
        "-200.5, 350.25, 0, -5.5, 2, false, \"screen\", 720.5, 1000.25, -1, true, false"
    ));
    assert!(lua.contains("branch\\\\\\\"tag"));
}

#[test]
fn invalid_effect_controls_are_rejected_without_arbitrary_position_or_scale_caps() {
    for (key, value) in [
        ("space", json!("canvas")),
        ("rotation", json!("30")),
        ("rotation", json!(true)),
        ("rotation", json!(1e100)),
        ("x", json!(1e100)),
        ("a", json!(1e100)),
        ("speed", json!(-0.01)),
        ("speed", json!(null)),
        ("stop_after", json!(-1)),
        ("stop_after", json!("2")),
        ("clear", json!(1)),
        ("restart", json!(0)),
        ("tag", json!(12)),
    ] {
        let mut effect = json!({"type":"effect","name":"Hit_001"});
        effect[key] = value;
        assert!(
            validate::validate_story(&story(vec![effect])).is_err(),
            "{key}"
        );
    }
}

#[test]
fn only_new_controls_require_host_1_4_5_and_tested_versions_are_still_honored() {
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("effect.lommod");
    let mut project = Project::new();
    project.stories.insert(
        "main".into(),
        story(vec![json!({"type":"effect","name":"Hit_001"})]),
    );
    project.export(&path).unwrap();
    let loaded = Project::open(&path).unwrap();
    assert_ne!(loaded.manifest["min_host_version"], "1.4.5");
    project.stories.get_mut("main").unwrap()["nodes"][0]["rotation"] = json!(0);
    project.export(&path).unwrap();
    package::inspect_package(&path).unwrap();
    assert_eq!(
        Project::open(&path).unwrap().manifest["min_host_version"],
        "1.4.5"
    );
    project.manifest["tested_host_version"] = json!("1.4.4");
    assert!(project.export(&path).is_err());
    project
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("tested_host_version");
    project.manifest["min_host_version"] = json!("1.5.0");
    project.export(&path).unwrap();
    assert_eq!(
        Project::open(&path).unwrap().manifest["min_host_version"],
        "1.5.0"
    );
}

#[test]
fn new_effects_start_centered_in_screen_coordinates() {
    let mut node = story_api::new_node("effect", "effect1").unwrap();
    assert_eq!(node["space"], "screen");
    assert_eq!(node["x"], 50);
    assert_eq!(node["y"], 50);
    assert!(validate::validate_story(&story(vec![node.clone()])).is_err());
    node["name"] = json!("Hit_001");
    let lua = codegen::compile_story(&story(vec![node]), None, None, None).unwrap();
    assert!(lua.contains(
        "mod_effect(\"Hit_001\", 50, 50, 1, 1, 1, true, \"screen\", 0, 1, -1, false, false, \"\")"
    ));
}

#[test]
fn empty_effect_names_fail_before_runtime_but_tagged_stops_are_valid() {
    for fields in [
        json!({}),
        json!({"play":false}),
        json!({"play":true,"tag":"hit"}),
        json!({"d":0}),
        json!({"play":true,"d":0,"tag":"hit"}),
    ] {
        for name in ["", "  "] {
            let mut effect = json!({"type":"effect","name":name});
            effect
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let input = story(vec![effect]);
            assert!(validate::validate_story(&input)
                .unwrap_err()
                .to_string()
                .contains("特效名称不能为空"));
            assert!(codegen::compile_story(&input, None, None, None).is_err());
        }
    }
    for fields in [
        json!({"play":false}),
        json!({"d":0}),
        json!({"play":false,"d":1}),
    ] {
        let mut effect = json!({"type":"effect","name":"","tag":"hit"});
        effect
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let lua = codegen::compile_story(&story(vec![effect]), None, None, None).unwrap();
        assert!(lua.contains("mod_effect(\"\", 0, 0, 1, 1, 1, false,"));
    }
}

#[test]
fn parallel_effect_tags_allow_independent_plays_and_preserve_stop_conflicts() {
    let effect =
        |name: &str, tag: &str, play| json!({"type":"effect","name":name,"tag":tag,"play":play});
    let wrap = |actions| story(vec![json!({"type":"parallel","actions":actions})]);
    let independent = wrap(json!([
        effect("Hit_001", "left", true),
        effect("Hit_001", "right", true)
    ]));
    let lua = codegen::compile_story(&independent, None, None, None).unwrap();
    assert_eq!(lua.matches("mod_effect(").count(), 2);
    for tag in ["", "shared"] {
        let stacked = wrap(json!([
            {"type":"effect","name":"Hit_001","c":0,"space":"screen","x":30,"tag":tag},
            {"type":"effect","name":"Hit_001","c":0,"space":"screen","x":70,"tag":tag}]));
        assert_eq!(
            codegen::compile_story(&stacked, None, None, None)
                .unwrap()
                .matches("mod_effect(")
                .count(),
            2
        );
        let stopped = wrap(json!([
            {"type":"effect","name":"Hit_001","c":0,"tag":tag},
            {"type":"effect","name":"Hit_001","play":false,"tag":tag}]));
        assert!(validate::validate_story(&stopped).is_err());
    }
    for actions in [
        json!([
            effect("Hit_001", "same", true),
            effect("Hit_001", "same", true)
        ]),
        json!([
            effect("Hit_001", "same", false),
            effect("Smoke_001", "same", true)
        ]),
        json!([
            effect("Hit_001", "", false),
            effect("Hit_001", "tagged", true)
        ]),
        json!([{"type":"effect","name":"Hit_001"},{"type":"effect","name":"Hit_001"}]),
    ] {
        assert!(validate::validate_story(&wrap(actions)).is_err());
    }
    // Explicit play still wins over d=0; this is a play, not a name-wide stop.
    validate::validate_story(&wrap(json!([
        {"type":"effect","name":"Hit_001","play":true,"d":0},effect("Hit_001", "tagged", true)
    ])))
    .unwrap();
}

#[test]
fn parallel_effect_identity_uses_the_same_tag_namespace_as_name_fallback() {
    let implicit = json!({"type":"effect","name":"Hit_001","c":1});
    for explicit_name in ["Hit_001", "BackFireSpark_001"] {
        let conflicting = story(vec![json!({"type":"parallel","actions":[
            implicit.clone(),
            {"type":"effect","name":explicit_name,"c":1,"tag":"Hit_001"}
        ]})]);
        let error = validate::validate_story(&conflicting)
            .unwrap_err()
            .to_string();
        assert!(error.contains("特效实例 Hit_001"), "{error}");
    }
    let independent = story(vec![json!({"type":"parallel","actions":[
        implicit,
        {"type":"effect","name":"Hit_001","c":1,"tag":"Hit_001_other"}
    ]})]);
    validate::validate_story(&independent).unwrap();
}
