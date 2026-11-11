use lom_core::{package, project::Project, scenario, scenario_edit};
use serde_json::json;
use std::path::Path;

fn released() -> Project {
    Project::open(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/released-1.5.9-scenario-modes.lommod"),
    )
    .unwrap()
}

#[test]
fn repairing_opening_updates_both_references_without_touching_event_rules() {
    let mut project = released();
    let original = project.clone();
    project.manifest["entry"] = json!("side");
    assert!(scenario::validate(
        project.scenario.as_ref().unwrap(),
        &project.manifest,
        &project.stories
    )
    .unwrap_err()
    .to_string()
    .contains("entry_event"));
    scenario_edit::set_entry(&mut project, "opening", "main").unwrap();
    assert_eq!(project, original);
    scenario_edit::set_entry(&mut project, "opening", "side").unwrap();
    assert_eq!(project.manifest["entry"], "side");
    assert_eq!(
        project.scenario.as_ref().unwrap()["events"][0]["script"],
        "side"
    );
    assert_eq!(project.stories, original.stories);
    assert_eq!(project.assets, original.assets);
    assert_eq!(
        project.scenario.as_ref().unwrap()["events"][1],
        original.scenario.as_ref().unwrap()["events"][1]
    );
    scenario::validate(
        project.scenario.as_ref().unwrap(),
        &project.manifest,
        &project.stories,
    )
    .unwrap();
}

#[test]
fn rejected_conversion_or_binding_is_atomic() {
    let original = released();
    for (trigger, position) in [
        ("", None),
        ("position", None),
        ("position", Some("not_a_position")),
    ] {
        let mut project = original.clone();
        assert!(scenario_edit::to_addon(&mut project, trigger, position).is_err());
        assert_eq!(project, original);
    }
    let mut project = original.clone();
    assert!(scenario_edit::set_entry(&mut project, "visit", "main").is_err());
    assert_eq!(project, original);
    assert!(scenario_edit::set_entry(&mut project, "opening", "missing").is_err());
    assert_eq!(project, original);
    project.scenario.as_mut().unwrap()["events"][1]["chance"] = json!(-1);
    let invalid = project.clone();
    assert!(scenario_edit::to_addon(&mut project, "auto", None).is_err());
    assert_eq!(
        project, invalid,
        "conversion must not discard invalid business rules to pass validation"
    );
}

#[test]
fn missing_opening_can_be_created_without_replacing_an_ordinary_event() {
    let mut project = released();
    scenario_edit::to_addon(&mut project, "auto", None).unwrap();
    project.manifest["play_mode"] = json!("sandbox");
    project.manifest["campaign"]["new_game"] = json!(true);
    let before = project.scenario.clone().unwrap();
    let id = scenario_edit::create_entry(&mut project, "side").unwrap();
    assert_eq!(id, "opening2");
    assert_eq!(project.manifest["entry"], "side");
    for (i, event) in before["events"].as_array().unwrap().iter().enumerate() {
        assert_eq!(project.scenario.as_ref().unwrap()["events"][i], *event);
    }
    scenario::validate(
        project.scenario.as_ref().unwrap(),
        &project.manifest,
        &project.stories,
    )
    .unwrap();
}

#[test]
fn released_159_open_repair_convert_save_export_reopen_preserves_identity_and_rejects_lua_substitution(
) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.9-scenario-modes.lommod");
    let original_bytes = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let original = project.clone();
    let root = tempfile::tempdir().unwrap();
    // Reproduce the reported stale association in an editable old project.
    project.manifest["entry"] = json!("side");
    let broken = root.path().join("broken");
    project.save_to(&broken).unwrap();
    let mut project = Project::open(&broken).unwrap();
    scenario_edit::set_entry(&mut project, "opening", "side").unwrap();
    let repaired = root.path().join("repaired.lommod");
    project.export(&repaired).unwrap();
    let mut project = Project::open(&repaired).unwrap();
    scenario_edit::to_addon(&mut project, "position", Some("Center")).unwrap();
    assert_eq!(project.manifest["id"], original.manifest["id"]);
    assert_eq!(
        project.manifest["campaign_id"],
        original.manifest["campaign_id"]
    );
    assert_eq!(project.stories, original.stories);
    assert_eq!(project.assets, original.assets);
    let config = project.scenario.as_ref().unwrap();
    assert!(config.get("entry_event").is_none());
    assert_eq!(config["events"][0]["id"], "opening");
    assert_eq!(config["events"][0]["trigger"], "position");
    assert_eq!(config["events"][0]["position"], "Center");
    assert_eq!(config["events"][0]["script"], "side");
    assert_eq!(
        config["events"][0]["outcomes"],
        original.scenario.as_ref().unwrap()["events"][0]["outcomes"]
    );
    assert_eq!(
        config["events"][1],
        original.scenario.as_ref().unwrap()["events"][1]
    );
    assert_eq!(
        config["tasks"],
        original.scenario.as_ref().unwrap()["tasks"]
    );
    let addon_folder = root.path().join("addon");
    project.save_to(&addon_folder).unwrap();
    let project = Project::open(&addon_folder).unwrap();
    let addon_package = root.path().join("addon.lommod");
    project.export(&addon_package).unwrap();
    let mut project = Project::open(&addon_package).unwrap();
    assert_eq!(project.manifest["play_mode"], "addon");
    let ordinary_events = project.scenario.as_ref().unwrap()["events"].clone();
    scenario_edit::to_sandbox(&mut project, "main").unwrap();
    assert_eq!(
        project.manifest["campaign_id"],
        original.manifest["campaign_id"]
    );
    assert_eq!(
        project.scenario.as_ref().unwrap()["entry_event"],
        "opening2"
    );
    for (i, event) in ordinary_events.as_array().unwrap().iter().enumerate() {
        assert_eq!(project.scenario.as_ref().unwrap()["events"][i], *event);
    }
    let final_folder = root.path().join("sandbox");
    project.save_to(&final_folder).unwrap();
    let project = Project::open(&final_folder).unwrap();
    let final_package = root.path().join("sandbox.lommod");
    project.export(&final_package).unwrap();
    let reopened = Project::open(&final_package).unwrap();
    assert_eq!(reopened.stories, original.stories);
    assert_eq!(reopened.assets, original.assets);
    assert_eq!(reopened.manifest["id"], original.manifest["id"]);
    assert_eq!(reopened.manifest["play_mode"], "sandbox");
    let mut entries = package::read_package(&final_package).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated substitution')\n");
    let tampered = root.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
    assert_eq!(std::fs::read(&fixture).unwrap(), original_bytes);
}
