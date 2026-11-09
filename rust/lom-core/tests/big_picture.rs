use lom_core::{codegen, content_edit, package, project::Project, release, validate};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

#[test]
fn blank_big_picture_needs_no_image_and_still_requires_a_show_before_hide() {
    let mut story = json!({"id":"main","start":"white","nodes":[
        {"id":"white","type":"cg","kind":"big","action":"show","background_only":true,"color":0,"fade_in":0.7},
        {"id":"close","type":"cg","kind":"big","action":"hide"}, {"id":"end","type":"end"}]});
    validate::validate_story(&story).unwrap();
    let lua = codegen::compile_story(&story, None, None, None).unwrap();
    assert!(lua.contains("mod_big_picture_configure(0.7)"));
    assert!(lua.contains("runwait(mainui.ShowBigPicture(\"\", 0))"));
    assert!(lua.contains("runwait(mainui.HideBigPicture())"));
    story["nodes"][0]["key"] = json!("user:missing.image");
    let mut project = Project::new();
    project.stories.insert("main".into(), story.clone());
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("blank.lommod");
    project.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert!(reopened.assets.is_empty());
    assert_eq!(reopened.manifest["min_host_version"], "1.5.9");
    assert!(release::referenced_asset_paths(
        &project.stories,
        &["assets/user/image/missing.image/content.json".into()]
    )
    .is_empty());
    story["start"] = json!("close");
    assert!(validate::validate_story(&story).is_err());
}

#[test]
fn released_158_big_picture_opens_then_edits_saves_exports_and_reopens() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.8-big-picture.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let old = package::read_package(&fixture).unwrap();
    assert!(String::from_utf8_lossy(&old["lua/main.lua"])
        .contains("mainui.ShowBigPicture(\"user:test.image\", 0)"));
    assert!(!String::from_utf8_lossy(&old["lua/main.lua"]).contains("mod_big_picture_configure"));
    let mut project = Project::open(&fixture).unwrap();
    assert_eq!(project.stories["main"]["nodes"][2]["fade_in"], -0.5);
    assert!(validate::validate_story(&project.stories["main"])
        .unwrap_err()
        .to_string()
        .contains("fade_in"));
    project.stories.get_mut("main").unwrap()["nodes"][2]["fade_in"] = json!(0.5);
    let key = "assets/user/image/test.image/content.json";
    let mut meta: serde_json::Value = serde_json::from_slice(&project.assets[key]).unwrap();
    meta["name"] = json!("自定义大图");
    content_edit::update(&mut project, key, &meta, &BTreeMap::new()).unwrap();
    // Isolate the custom big picture reference: it must package/protect the image
    // independently of the old event node that was needed by the 1.5.8 fixture.
    project.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap()
        .drain(..2);
    project.stories.get_mut("main").unwrap()["start"] = json!("big");
    assert!(content_edit::remove(&mut project, key).is_err());
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("project");
    project.save_to(&source).unwrap();
    let edited = Project::open(&source).unwrap();
    let output = dir.path().join("new.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.assets[key], edited.assets[key]);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.9");
    assert!(
        !release::run_preflight(&edited, release::Profile::Editing, "1.5.9")
            .iter()
            .any(|issue| issue.severity == "error")
    );
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    assert!(entries.contains_key("assets/user/image/test.image/image.png"));
    let lua = String::from_utf8_lossy(&entries["lua/main.lua"]);
    assert!(lua.contains("mod_big_picture_configure(0.5)"));
    assert!(lua.contains("mainui.ShowBigPicture(\"big_girl_event_004\", 2)"));
    assert!(!lua.contains("mod_big_picture_configure(1)"));
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('substituted')\n");
    let tampered = dir.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
