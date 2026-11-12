use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};
use std::path::Path;

fn event_story() -> Value {
    json!({"story_schema":2,"id":"main","start":"book","nodes":[
        {"id":"book","type":"cg","kind":"event","action":"show","key":"girlroom_work_002"},
        {"id":"close","type":"cg","kind":"event","action":"hide"},
        {"id":"end","type":"end"}
    ]})
}

#[test]
fn event_fade_out_keeps_legacy_lua_for_absent_and_zero_then_waits_before_the_next_step() {
    let mut story = event_story();
    let legacy = codegen::compile_story(&story, None, None, None).unwrap();
    assert!(legacy.contains("\tmod_event_picture_hide()"));
    assert!(!legacy.contains("mod_event_picture_hide_fade"));
    story["nodes"][1]["fade_out"] = json!(0);
    assert_eq!(
        codegen::compile_story(&story, None, None, None).unwrap(),
        legacy
    );

    story["nodes"][1]["fade_out"] = json!(0.8);
    story["nodes"][1]["wait_after"] = json!(0.3);
    let lua = codegen::compile_story(&story, None, None, None).unwrap();
    let guard = lua
        .find("if mod_event_picture_hide_fade == nil then error(")
        .unwrap();
    let hide = lua.find("\tmod_event_picture_hide_fade(0.8)").unwrap();
    let wait = lua[hide..].find("\twait(0.8)").unwrap() + hide;
    let hold = lua[wait..].find("\twait(0.3)").unwrap() + wait;
    let next = lua[hold..].find("\treturn node_end()").unwrap() + hold;
    assert!(guard < hide && hide < wait && wait < hold && hold < next);
    assert!(lua.contains("Host 1.5.15"));
}

#[test]
fn event_fade_out_validates_the_finite_host_range_at_the_affected_step() {
    let mut story = event_story();
    for valid in [json!(0), json!(0.7), json!(f32::MAX as f64)] {
        story["nodes"][1]["fade_out"] = valid;
        validate::validate_story(&story).unwrap();
    }
    for invalid in [
        json!(-0.1),
        json!("soon"),
        Value::Null,
        json!(f32::MAX as f64 * 2.0),
    ] {
        story["nodes"][1]["fade_out"] = invalid;
        let error = validate::validate_story(&story).unwrap_err().to_string();
        assert!(error.contains("fade_out"), "{error}");
        assert!(error.contains("close"), "{error}");
    }
}

#[test]
fn only_positive_event_hide_fade_requires_host_1515_and_roundtrips() {
    let folder = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.manifest["min_host_version"] = json!("1.4.2");
    project.stories.insert("main".into(), event_story());
    for (seconds, required) in [(0.0, "1.4.2"), (0.8, "1.5.15")] {
        project.stories.get_mut("main").unwrap()["nodes"][1]["fade_out"] = json!(seconds);
        let path = folder.path().join(format!("{seconds}.lommod"));
        project.export(&path).unwrap();
        let reopened = Project::open(&path).unwrap();
        assert_eq!(reopened.manifest["min_host_version"], required);
        assert_eq!(reopened.stories, project.stories);
    }
    // An inactive field on a show node does not request the hide bridge.
    project.stories.get_mut("main").unwrap()["nodes"][1]
        .as_object_mut()
        .unwrap()
        .remove("fade_out");
    project.stories.get_mut("main").unwrap()["nodes"][0]["fade_out"] = json!(0.8);
    let path = folder.path().join("show.lommod");
    project.export(&path).unwrap();
    assert_eq!(
        Project::open(&path).unwrap().manifest["min_host_version"],
        "1.4.2"
    );
}

#[test]
fn released_158_event_book_opens_edits_saves_exports_reopens_and_still_rejects_substituted_lua() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.8-big-picture.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let old_entries = package::read_package(&fixture).unwrap();
    let old_lua = String::from_utf8_lossy(&old_entries["lua/main.lua"]);
    assert!(old_lua.contains("\tmod_event_picture_hide()"));
    assert!(!old_lua.contains("mod_event_picture_hide_fade"));
    let mut project = Project::open(&fixture).unwrap();
    assert_eq!(project.stories["main"]["nodes"][1]["id"], "hide_event");
    assert!(project.stories["main"]["nodes"][1]
        .get("fade_out")
        .is_none());
    project.stories.get_mut("main").unwrap()["nodes"][1]["fade_out"] = json!(0.75);
    // This established historical fixture also has an editable old negative
    // big-picture fade. Correct it before re-exporting through today's checks.
    assert_eq!(project.stories["main"]["nodes"][2]["fade_in"], -0.5);
    project.stories.get_mut("main").unwrap()["nodes"][2]["fade_in"] = json!(0.5);
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("project");
    project.save_to(&source).unwrap();
    let edited = Project::open(&source).unwrap();
    assert_eq!(edited.stories, project.stories);
    let output = folder.path().join("edited.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.assets, edited.assets);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.15");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    assert!(String::from_utf8_lossy(&entries["lua/main.lua"])
        .contains("\tmod_event_picture_hide_fade(0.75)\n\twait(0.75)"));
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('substituted')\n");
    let tampered = folder.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
