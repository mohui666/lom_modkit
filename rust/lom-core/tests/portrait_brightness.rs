use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};
use std::path::Path;

#[test]
fn portrait_brightness_is_optional_and_applies_between_the_two_fades() {
    for character in ["player", "user:demo.actor"] {
        for grouped in [false, true] {
            let mut change = json!({"type":"portrait","character":character,"portrait":"normal"});
            let story = |change: Value| {
                json!({"id":"main","start":"enter","nodes":[
                {"id":"enter","type":"show","character":character,"position":"M"},
                if grouped {json!({"id":"change","type":"parallel","actions":[change,{"type":"wait","seconds":0}]})}
                else {let mut change=change;change["id"]=json!("change");change},
                {"id":"end","type":"end"}]})
            };
            let original =
                codegen::compile_story(&story(change.clone()), None, None, None).unwrap();
            change["brighten"] = json!(false);
            assert_eq!(
                codegen::compile_story(&story(change.clone()), None, None, None).unwrap(),
                original
            );
            change["brighten"] = json!(true);
            let lua = codegen::compile_story(&story(change.clone()), None, None, None).unwrap();
            let brighten = format!("mod_char_dim_mode(\"{character}\", false, \"until_dialogue\")");
            let at = lua.find(&brighten).unwrap();
            let fade_out = lua
                .find(&format!(
                    "mod_char_portrait_transition(\"{character}\", \"normal\", 0.5, false)"
                ))
                .unwrap();
            let fade_in = lua
                .find(&format!(
                    "mod_char_portrait_transition(\"{character}\", \"normal\", 1.5, true)"
                ))
                .unwrap();
            assert!(fade_out < at && at < fade_in);
            change["brighten"] = json!("true");
            assert!(validate::validate_story(&story(change)).is_err());
        }
    }
}

#[test]
fn old_released_package_can_add_brightening_and_reopen_without_accepting_unrelated_lua() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.11-render-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let old = project.stories["main"].clone();
    let nodes = project.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap();
    nodes.insert(0,json!({"id":"new_enter","type":"show","character":"player","position":"L1","goto":"new_bright"}));
    nodes.insert(1,json!({"id":"new_bright","type":"portrait","character":"player","portrait":"angry1","brighten":true,"goto":old["start"]}));
    project.stories.get_mut("main").unwrap()["start"] = json!("new_enter");
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("edited");
    project.save_to(&folder).unwrap();
    let saved = Project::open(&folder).unwrap();
    let output = root.path().join("edited.lommod");
    saved.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, saved.stories);
    assert_eq!(reopened.assets, saved.assets);
    assert_eq!(
        &reopened.stories["main"]["nodes"].as_array().unwrap()[2..],
        old["nodes"].as_array().unwrap()
    );
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated replacement')\n");
    let tampered = root.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}

#[test]
fn brightening_uses_the_existing_dimming_host_contract() {
    let mut project = Project::new();
    project.manifest["min_host_version"] = json!("1.4.0");
    project.stories.insert("main".into(),json!({"id":"main","start":"enter","nodes":[
        {"id":"enter","type":"show","character":"player","position":"M"},
        {"id":"bright","type":"parallel","actions":[{"type":"portrait","character":"player","portrait":"normal","brighten":true},{"type":"wait","seconds":0}]},
        {"id":"end","type":"end"}]}));
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bright.lommod");
    project.export(&path).unwrap();
    assert_eq!(
        Project::open(&path).unwrap().manifest["min_host_version"],
        "1.5.1"
    );
}
