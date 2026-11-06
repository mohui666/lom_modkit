use lom_core::{package, project::Project};
use serde_json::{json, Value};

#[test]
fn old_displayed_stat_package_imports_and_reexports_with_the_actual_write() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/legacy-displayed-stat.json")).unwrap();
    let mut project = Project::new();
    project.manifest = fixture["manifest"].clone();
    project
        .stories
        .insert("main".into(), fixture["story"].clone());
    let root = tempfile::tempdir().unwrap();
    let mut entries: package::Entries = fixture["entries"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, text)| (path.clone(), text.as_str().unwrap().as_bytes().to_vec()))
        .collect();
    entries.remove("package-content.sha256");
    let old = root.path().join("old.lommod");
    package::write_package(entries, &old).unwrap();
    let imported = Project::open(&old).unwrap();
    assert_eq!(imported.stories, project.stories);
    let fixed = root.path().join("fixed.lommod");
    imported.export(&fixed).unwrap();
    let entries = package::read_package(&fixed).unwrap();
    let lua = std::str::from_utf8(&entries["lua/main.lua"]).unwrap();
    assert!(lua.contains("statmodifymanager.SetPlayer(\"lover\", 1)"));
    assert_ne!(lua, fixture["lua"].as_str().unwrap());
}

#[test]
fn message_switches_raise_the_host_floor_for_new_item_and_quiet_callbacks() {
    let mut project = Project::new();
    project.manifest["min_host_version"] = json!("1.4.8");
    project
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("tested_host_version");
    let story: Value =
        serde_json::from_str(include_str!("fixtures/message-controls.json")).unwrap();
    project.stories.clear();
    project.stories.insert("message_controls".into(), story);
    project.manifest["entry"] = json!("message_controls");
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("messages.lommod");
    project.export(&path).unwrap();
    let entries = package::read_package(&path).unwrap();
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(manifest["min_host_version"], "1.4.9");
    assert!(package::inspect_package(&path).unwrap()["ok"]
        .as_bool()
        .unwrap());
}
