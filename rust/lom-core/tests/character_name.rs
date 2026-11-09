use lom_core::{codegen, localization, package, project::Project, validate};
use serde_json::json;

#[test]
fn name_only_and_title_only_operations_remain_independent() {
    let mut story = json!({"id":"main","start":"name","nodes":[
        {"id":"name","type":"character_title","character":"trainee_girl1","action":"keep","name_action":"set","name":"小圆,甲","wait_after":0.5},
        {"id":"reset","type":"character_title","character":"player","action":"keep","name_action":"reset"},
        {"id":"end","type":"end"}]});
    validate::validate_story(&story).unwrap();
    let lua = codegen::compile_story(&story, None, None, None).unwrap();
    assert!(lua.contains("mod_character_name(\"trainee_girl1\", \"set\", \"小圆,甲\")"));
    assert!(!lua.contains("mod_character_title("));
    assert!(lua.contains("wait(0.5)"));
    assert_eq!(
        localization::iter_localizable_texts(&story)["name.name"],
        "小圆,甲"
    );
    for invalid in ["", "\n小圆", " "] {
        story["nodes"][0]["name"] = json!(invalid);
        assert!(validate::validate_story(&story).is_err());
    }
}

#[test]
fn released_154_title_node_retains_output_then_accepts_name_edit_and_roundtrip() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.4-name-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let old_entries = package::read_package(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let unchanged = dir.path().join("unchanged.lommod");
    project.export(&unchanged).unwrap();
    let unchanged_entries = package::read_package(&unchanged).unwrap();
    assert_eq!(
        old_entries["lua/main.lua"],
        unchanged_entries["lua/main.lua"]
    );
    assert_eq!(
        Project::open(&unchanged).unwrap().manifest["min_host_version"],
        "1.5.4"
    );
    project.stories.get_mut("main").unwrap()["nodes"].as_array_mut().unwrap().insert(1,
        json!({"id":"name","type":"character_title","character":"player","action":"keep","name_action":"set","name":"叶云舟"}));
    let saved = dir.path().join("edited");
    project.save_to(&saved).unwrap();
    let edited = Project::open(&saved).unwrap();
    let output = dir.path().join("new.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.5");
    assert_eq!(original, std::fs::read(&fixture).unwrap());
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('substituted')\n");
    let tampered = dir.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
