use lom_core::{content, content_edit, package, project::Project};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

// Captured with the released lomc 1.5.10 binary, not the current emitter.
#[test]
fn released_martial_package_can_gain_talents_save_export_reopen_and_still_reject_lua_substitution()
{
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.10-martial-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let key = "assets/user/martial/fixture.manual/content.json";
    let mut meta: Value = serde_json::from_slice(&project.assets[key]).unwrap();
    assert!(meta["levels"][0].get("talents").is_none());
    meta["levels"][0]["talents"] = json!([{"talent":"user:fixture.awakening","level":1}]);
    let talent_key = "assets/user/talent/fixture.awakening/content.json";
    project.assets.insert(talent_key.into(), lom_core::stable_json(&json!({"schema":1,"id":"fixture.awakening","type":"talent","name":"觉醒","description":"功法升级获得","files":{"main":""}})).unwrap());
    content_edit::update(&mut project, key, &meta, &BTreeMap::new()).unwrap();
    assert!(content_edit::remove(&mut project, talent_key).is_err());
    let dir = tempfile::tempdir().unwrap();
    project.save_to(&dir.path().join("project")).unwrap();
    let reopened = Project::open(&dir.path().join("project")).unwrap();
    let output = dir.path().join("edited.lommod");
    reopened.export(&output).unwrap();
    let loaded = Project::open(&output).unwrap();
    assert_eq!(loaded.manifest["min_host_version"], "1.5.11");
    assert!(loaded.assets.contains_key(talent_key));
    let meta: Value = serde_json::from_slice(&loaded.assets[key]).unwrap();
    assert_eq!(
        meta["levels"][0]["talents"][0]["talent"],
        "user:fixture.awakening"
    );
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated substitution')\n");
    let tampered = dir.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}

#[test]
fn invalid_upgrade_talents_and_missing_custom_dependencies_are_actionable() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.10-martial-base.lommod");
    let mut project = Project::open(&fixture).unwrap();
    let key = "assets/user/martial/fixture.manual/content.json";
    let original: Value = serde_json::from_slice(&project.assets[key]).unwrap();
    for grants in [
        json!([{"talent":"invented","level":1}]),
        json!([{"talent":"user:fixture.awakening","level":0}]),
        json!([{"talent":"user:fixture.awakening","level":1},{"talent":"user:fixture.awakening","level":1}]),
    ] {
        let mut meta = original.clone();
        meta["levels"][0]["talents"] = grants;
        assert!(content::normalize_content_metadata(&meta).is_err());
    }
    let mut meta = original;
    meta["levels"][0]["talents"] = json!([{"talent":"user:fixture.awakening","level":1}]);
    content_edit::update(&mut project, key, &meta, &BTreeMap::new()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let error = project
        .export(&dir.path().join("missing.lommod"))
        .unwrap_err();
    assert!(format!("{error:#}").contains("第 1 级升级天赋"));
}
