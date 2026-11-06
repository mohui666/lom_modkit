use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};

fn story() -> Value {
    json!({"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"player","position":"M"},
        {"id":"line","type":"say","character":"player","text":"本句气泡"},
        {"id":"next","type":"say","character":"player","text":"恢复原样"},
        {"id":"done","type":"end"}
    ]})
}

#[test]
fn dialogue_mood_wraps_only_its_say() {
    for mode in ["character", "think", "narrative", "center"] {
        let mut input = story();
        input["nodes"][1]["mode"] = json!(mode);
        let legacy = codegen::compile_story(&input, None, None, None).unwrap();
        assert!(!legacy.contains("mod_dialogue_mood"));
        for icon in ["auto", "hide", "shock", "laugh1"] {
            input["nodes"][1]["mood_icon"] = json!(icon);
            validate::validate_story(&input).unwrap();
            let lua = codegen::compile_story(&input, None, None, None).unwrap();
            let scope = format!("local __lom_restore_mood = mod_dialogue_mood(\"player\", \"{icon}\")\n\tsay(luamanager.GetStoryText(\"MOD_MOD_main_line\"))\n\t__lom_restore_mood()");
            assert!(lua.contains(&scope), "{lua}");
            assert_eq!(lua.matches("mod_dialogue_mood(").count(), 1);
        }
    }
    let mut input = story();
    input["nodes"][1]["mode"] = json!("narrative");
    input["nodes"][1]["mood_character"] = json!("artist1");
    input["nodes"][1]["mood_icon"] = json!("silent");
    assert!(codegen::compile_story(&input, None, None, None)
        .unwrap()
        .contains("mod_dialogue_mood(\"artist1\", \"silent\")"));
}

#[test]
fn inherited_dialogue_supports_a_scoped_mood_without_changing_native_settings() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/vanilla-import.json")).unwrap();
    let mut input = fixture["baseline"].clone();
    let node = input["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["type"] == "say")
        .unwrap();
    assert_eq!(node["inherit_dialog"], true);
    node["mood_icon"] = json!("shock");
    node["mood_character"] = json!("player");
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    assert!(
        lua.contains("mod_dialogue_mood(\"player\", \"shock\")\n\tsay("),
        "{lua}"
    );
    assert!(lua.contains("\n\t__lom_restore_mood()"));
}

#[test]
fn a_different_mood_target_is_packaged_and_checked_for_stage_presence() {
    let mut project = Project::new();
    let mut input = story();
    input["nodes"][1]["mood_icon"] = json!("shock");
    input["nodes"][1]["mood_character"] = json!("user:demo.actor");
    input["nodes"][1]["portrait"] = json!("shy");
    assert_eq!(
        lom_core::release::find_stage_issues(&input),
        vec![("line".into(), "user:demo.actor".into())]
    );
    let mut repaired = input.clone();
    assert_eq!(
        lom_core::release::ensure_stage(&mut repaired, "line").unwrap()["character"],
        "user:demo.actor"
    );
    assert!(lom_core::release::find_stage_issues(&repaired).is_empty());
    project.stories.clear();
    project.stories.insert("main".into(), input);
    project.manifest["entry"] = json!("main");
    let meta = lom_core::content::normalize_content_metadata(&json!({
        "schema":1,"id":"demo.actor","type":"character","name":"测试人物",
        "files":{"main":"normal.png"},"portraits":{"normal":"normal.png"}
    }))
    .unwrap();
    project.assets.insert(
        "assets/user/character/demo.actor/content.json".into(),
        serde_json::to_vec(&meta).unwrap(),
    );
    project.assets.insert(
        "assets/user/character/demo.actor/normal.png".into(),
        include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
        )
        .to_vec(),
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("target.lommod");
    project.export(&path).unwrap();
    let entries = package::read_package(&path).unwrap();
    assert!(entries.contains_key("assets/user/character/demo.actor/normal.png"));
    assert!(package::inspect_package(&path).unwrap()["ok"]
        .as_bool()
        .unwrap());
}

#[test]
fn dialogue_mood_requires_a_valid_target_and_icon() {
    let mut input = story();
    input["nodes"][1]["mood_icon"] = json!("shock");
    for target in [json!("missing.actor"), json!("user:../escape")] {
        input["nodes"][1]["mood_character"] = target;
        assert!(validate::validate_story(&input).is_err());
    }
    input["nodes"][1]["mood_character"] = json!("player");
    input["nodes"][1]["mood_icon"] = json!("unknown");
    assert!(validate::validate_story(&input).is_err());
    input["nodes"][1] =
        json!({"id":"line","type":"say","mode":"narrative","text":"旁白","mood_icon":"shock"});
    assert!(validate::validate_story(&input).is_err());
}

#[test]
fn dialogue_mood_packages_raise_only_lower_host_versions() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mood.lommod");
    let mut project = Project::new();
    project.stories.clear();
    let mut input = story();
    input["nodes"][1]["mood_icon"] = json!("shock");
    project.stories.insert("main".into(), input);
    project.manifest["entry"] = json!("main");
    for (minimum, expected) in [
        ("1.4.9", "1.4.10"),
        ("1.4.10-beta.1", "1.4.10"),
        ("2.0.0", "2.0.0"),
    ] {
        project.manifest["min_host_version"] = json!(minimum);
        project.export(&path).unwrap();
        let entries = package::read_package(&path).unwrap();
        let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
        assert_eq!(manifest["min_host_version"], expected);
        assert!(package::inspect_package(&path).unwrap()["ok"]
            .as_bool()
            .unwrap());
    }
}
