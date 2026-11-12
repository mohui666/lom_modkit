use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};
use std::path::Path;

fn story() -> Value {
    json!({"story_schema":2,"id":"main","start":"actor","nodes":[
        {"id":"actor","type":"show","character":"player","position":"M","portrait":"normal"},
        {"id":"picture","type":"cg","kind":"item","action":"show","key":"food_103"},
        {"id":"end","type":"end"}
    ]})
}

#[test]
fn scale_is_prepared_before_show_and_the_next_actor_show_resets_to_100() {
    let mut source = story();
    let old = codegen::compile_story(&source, None, None, None).unwrap();
    assert!(!old.contains("mod_character_scale("));
    assert!(!old.contains("mod_picture_scale_configure("));
    source["nodes"][0]["scale"] = json!(150);
    source["nodes"].as_array_mut().unwrap().insert(
        1,
        json!({"id":"again","type":"show","character":"player","position":"L"}),
    );
    let lua = codegen::compile_story(&source, None, None, None).unwrap();
    let scale = lua.find("mod_character_scale(\"player\", 150)").unwrap();
    let show = lua[scale..].find("stage.show{").unwrap() + scale;
    let reset = lua.find("mod_character_scale(\"player\", 100)").unwrap();
    assert!(scale < show && show < reset);
    for kind in ["picture", "item", "event", "big"] {
        source["nodes"][2] = json!({"id":"picture","type":"cg","kind":kind,"action":"show","key":"girlroom_work_002","scale":50});
        let lua = codegen::compile_story(&source, None, None, None).unwrap();
        assert!(lua.contains(&format!("mod_picture_scale_configure(\"{kind}\", 50)")));
    }
}

#[test]
fn scale_rejects_unrepresentable_sizes_and_inapplicable_picture_actions() {
    for invalid in [
        json!(0),
        json!(-1),
        json!(true),
        json!("150"),
        json!(f32::MAX as f64 * 2.0),
    ] {
        let mut source = story();
        source["nodes"][0]["scale"] = invalid.clone();
        assert!(validate::validate_story(&source).is_err(), "{invalid}");
        source = story();
        source["nodes"][1]["scale"] = invalid.clone();
        assert!(validate::validate_story(&source).is_err(), "{invalid}");
    }
    for (kind, action, background_only) in [
        ("item", "hide", false),
        ("map", "show", false),
        ("big", "show", true),
    ] {
        let mut source = story();
        source["nodes"][1] = json!({"id":"picture","type":"cg","kind":kind,"action":action,"key":"girlroom_work_002","scale":150,"background_only":background_only});
        let error = validate::validate_story(&source).unwrap_err().to_string();
        assert!(error.contains("图片大小"), "{error}");
        assert!(error.contains("picture"), "{error}");
    }
}

#[test]
fn released_1511_actor_can_gain_scale_save_export_reopen_without_changing_the_original() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.11-render-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    assert!(project.stories["main"]["nodes"][1].get("scale").is_none());
    project.stories.get_mut("main").unwrap()["nodes"][1]["scale"] = json!(150);
    project.stories.get_mut("main").unwrap()["nodes"].as_array_mut().unwrap().insert(2,
        json!({"id":"scaled_item","type":"cg","kind":"item","action":"show","key":"food_103","scale":50}));
    let folder = tempfile::tempdir().unwrap();
    let saved = folder.path().join("source");
    project.save_to(&saved).unwrap();
    let edited = Project::open(&saved).unwrap();
    let output = folder.path().join("scaled.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, project.stories);
    assert_eq!(reopened.assets, project.assets);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.15");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('substituted')\n");
    let tampered = folder.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
