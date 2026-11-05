use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};

fn story(character: &str, offset: Value) -> Value {
    json!({"id":"main","start":"show1","nodes":[
        {"id":"show1","type":"show","character":character,"position":"L1"},
        offset,
        {"id":"face1","type":"face","character":character,"facing":"left"},
        {"id":"move1","type":"move","character":character,"from":"L1","to":"M","duration":0},
        {"id":"show2","type":"show","character":character,"position":"R1"},
        {"id":"end1","type":"end"}]})
}

#[test]
fn layout_opt_in_keeps_legacy_lua_and_waits_for_layout_and_post_delay() {
    for character in ["player", "user:demo.actor"] {
        let mut offset = json!({"id":"layout","type":"offset","character":character,
            "x":12.5,"y":-4.25,"duration":0.25,"wait_after":0.5});
        let legacy =
            codegen::compile_story(&story(character, offset.clone()), None, None, None).unwrap();
        let old_call = if character == "player" {
            "runwait(characters.MoveOffsetCoroutine(\"player\", 12.5, -4.25, 0.25))"
        } else {
            "mod_char_offset(\"user:demo.actor\", 12.5, -4.25, 0.25)"
        };
        assert!(legacy.contains(old_call));
        assert!(!legacy.contains("mod_character_layout"));
        offset["absolute"] = json!(true);
        offset["scale"] = json!(125.5);
        let lua =
            codegen::compile_story(&story(character, offset.clone()), None, None, None).unwrap();
        assert!(lua.contains(&format!(
            "mod_character_layout(\"{character}\", 12.5, -4.25, 125.5, true, 0.25)\n\twait(0.25)\n\twait(0.5)"
        )), "{lua}");
        assert_eq!(
            lua.matches("mod_character_layout_reset(").count(),
            2,
            "Only explicit show resets body scale, face/move preserve it"
        );
        offset.as_object_mut().unwrap().remove("scale");
        let lua = codegen::compile_story(&story(character, offset), None, None, None).unwrap();
        assert!(lua.contains(&format!(
            "mod_character_layout(\"{character}\", 12.5, -4.25, -1, true, 0.25)"
        )));
    }
    let grouped = json!({"id":"main","start":"together","nodes":[
        {"id":"together","type":"parallel","actions":[
            {"type":"show","character":"player","position":"L1"},
            {"type":"offset","character":"user:demo.actor","x":0,"y":0,"duration":0,"scale":150}]},
        {"id":"end1","type":"end"}]});
    let lua = codegen::compile_story(&grouped, None, None, None).unwrap();
    assert_eq!(
        lua.matches("mod_character_layout_reset(\"player\")")
            .count(),
        1
    );
    assert_eq!(
        lua.matches("characters.LoadCharacterAsset(\"player\")")
            .count(),
        1
    );
    assert!(lua.contains("mod_character_layout(\"user:demo.actor\", 0, 0, 150, false, 0)"));
}

#[test]
fn layout_validation_preserves_positive_scales_and_bounds_overlay_position() {
    let offset = json!({"id":"layout","type":"offset","character":"player",
        "x":-1920.25,"y":1080.5,"duration":0,"absolute":true,"scale":2500.5});
    validate::validate_story(&story("player", offset.clone())).unwrap();
    for (field, value) in [
        ("scale", json!(0)),
        ("scale", json!(-1)),
        ("scale", json!(true)),
        ("scale", json!("100")),
        ("scale", json!(1e100)),
        ("absolute", json!("true")),
        ("duration", json!(-0.1)),
    ] {
        let mut invalid = offset.clone();
        invalid[field] = value;
        assert!(
            validate::validate_story(&story("player", invalid)).is_err(),
            "{field}"
        );
    }
    let overlay = json!({"id":"art","type":"overlay","action":"show","slot":"art",
        "image":"user:demo.image","position":"top_right","x":-100,"y":100});
    let wrap = |node| json!({"id":"main","start":"art","nodes":[node,{"id":"end1","type":"end"}]});
    validate::validate_story(&wrap(overlay.clone())).unwrap();
    let lua = codegen::compile_story(&wrap(overlay.clone()), None, None, None).unwrap();
    assert!(lua.contains("mod_overlay_show(\"art\", \"user:demo.image\", \"top_right\", 100, 100, \"front\", 0.25, -100, 100)"));
    for (field, value) in [("x", -100.01), ("y", 100.01)] {
        let mut invalid = overlay.clone();
        invalid[field] = json!(value);
        assert!(validate::validate_story(&wrap(invalid)).is_err());
    }
}

#[test]
fn new_layout_fields_require_host_1_4_4_without_changing_package_schemas() {
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("layout.lommod");
    let mut project = Project::new();
    project.stories.insert(
        "main".into(),
        story(
            "player",
            json!({
        "id":"layout","type":"offset","character":"player",
        "x":0,"y":0,"duration":0,"absolute":false}),
        ),
    );
    project.export(&path).unwrap();
    let entries = package::read_package(&path).unwrap();
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(manifest["min_host_version"], "1.4.4");
    assert_eq!(manifest["package_format"], 3);
    assert_eq!(manifest["story_schema"], 2);
    assert_eq!(lom_core::CONTENT_SCHEMA, 1);
    project.manifest["tested_host_version"] = json!("1.4.3");
    assert!(project.export(&path).is_err());
    project
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("tested_host_version");
    project.stories.insert(
        "main".into(),
        json!({"id":"main","start":"art","nodes":[
        {"id":"art","type":"overlay","action":"show","slot":"art",
            "image":"user:demo.image","y":5}, {"id":"end1","type":"end"}]}),
    );
    project.assets.insert("assets/user/image/demo.image/content.json".into(),
        serde_json::to_vec(&json!({"schema":1,"type":"image","id":"demo.image","name":"Art","files":{"main":"image.png"}})).unwrap());
    project.assets.insert(
        "assets/user/image/demo.image/image.png".into(),
        include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
        )
        .to_vec(),
    );
    project.export(&path).unwrap();
    let entries = package::read_package(&path).unwrap();
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(manifest["min_host_version"], "1.4.4");
    assert!(String::from_utf8_lossy(&entries["lua/main.lua"]).contains(
        "mod_overlay_show(\"art\", \"user:demo.image\", \"center\", 100, 100, \"front\", 0.25, 0, 5)"));
}
