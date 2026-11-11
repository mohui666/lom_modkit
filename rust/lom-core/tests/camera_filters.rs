use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};
use std::path::Path;

fn story(camera: Value) -> Value {
    let mut camera = camera;
    camera["id"] = json!("filter");
    camera["type"] = json!("camera");
    camera["name"] = json!("stage-memory");
    camera["active"] = json!(true);
    json!({"id":"main","start":"filter","nodes":[camera,{"id":"end","type":"end"}]})
}

#[test]
fn legacy_camera_output_stays_exact_and_new_controls_wait_for_native_fade() {
    let old = codegen::compile_story(&story(json!({})), None, None, None).unwrap();
    let zero = codegen::compile_story(&story(json!({"duration":0})), None, None, None).unwrap();
    assert_eq!(old, zero);
    assert!(old.contains("maincamera.ActiveVolume(\"stage-memory\", 1)"));
    let new = codegen::compile_story(
        &story(json!({"duration":1.25,"exclude_characters":["player","user:test.narrator"]})),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(new.contains("mod_camera_filter(\"stage-memory\", true, 1.25, {\"player\", \"user:test.narrator\"})\n\twait(1.25)"));
    assert!(new.contains("Host 1.5.10"));
}

#[test]
fn official_grayscale_uses_the_native_camera_api_and_shared_extensions() {
    let mut input = story(json!({}));
    input["nodes"][0]["name"] = json!("stage-gray-100");
    validate::validate_story(&input).unwrap();
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    assert!(lua.contains("maincamera.ActiveVolume(\"stage-gray-100\", 1)"));
    assert!(!lua.contains("mod_camera_filter("));
    input["nodes"][0]["duration"] = json!(1.2);
    input["nodes"][0]["exclude_characters"] = json!(["player"]);
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    assert!(
        lua.contains("mod_camera_filter(\"stage-gray-100\", true, 1.2, {\"player\"})\n\twait(1.2)")
    );
}

#[test]
fn filter_validation_rejects_invalid_times_and_character_ids() {
    for fields in [
        json!({"duration":-0.1}),
        json!({"duration":"1"}),
        json!({"duration":1e100}),
        json!({"exclude_characters":["player","player"]}),
        json!({"exclude_characters":[7]}),
        json!({"exclude_characters":["not_a_game_character"]}),
        json!({"exclude_characters":["user:bad"]}),
    ] {
        assert!(validate::validate_story(&story(fields)).is_err());
    }
    validate::validate_story(&story(json!({"duration":0,"exclude_characters":[]}))).unwrap();
}

#[test]
fn released_159_filter_package_opens_edits_saves_exports_reopens_and_rejects_lua_substitution() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/released-1.5.9-filters.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let old = package::read_package(&fixture).unwrap();
    assert!(String::from_utf8_lossy(&old["lua/main.lua"])
        .contains("maincamera.ActiveVolume(\"stage-memory\", 1)"));
    let mut project = Project::open(&fixture).unwrap();
    project.stories.get_mut("main").unwrap()["nodes"][0]["name"] = json!("stage-gray-100");
    project.stories.get_mut("main").unwrap()["nodes"][2]["name"] = json!("stage-gray-100");
    project.stories.get_mut("main").unwrap()["nodes"][0]["duration"] = json!(0.8);
    project.stories.get_mut("main").unwrap()["nodes"][0]["exclude_characters"] = json!(["player"]);
    project.stories.get_mut("main").unwrap()["nodes"][2]["duration"] = json!(0.5);
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("project");
    project.save_to(&folder).unwrap();
    let edited = Project::open(&folder).unwrap();
    let output = temp.path().join("updated.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut tampered = package::read_package(&output).unwrap();
    tampered.remove("package-content.sha256");
    tampered
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua')\n");
    let path = temp.path().join("tampered.lommod");
    package::write_package(tampered, &path).unwrap();
    assert!(Project::open(&path).is_err());
}
