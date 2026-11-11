use lom_core::{project::Project, scenario};
use serde_json::{json, Value};

fn project() -> Project {
    let mut project = Project::new_scenario("addon");
    project.manifest["min_host_version"] = json!("1.1.3");
    project
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("tested_host_version");
    let event = &mut project.scenario.as_mut().unwrap()["events"][0];
    event["trigger"] = json!("story_entry");
    event.as_object_mut().unwrap().remove("position");
    event["target"] = json!("section_01_free_03");
    event["from"] = json!("section_01_free_start_02");
    project
}
fn valid(project: &Project) -> bool {
    scenario::validate(
        project.scenario.as_ref().unwrap(),
        &project.manifest,
        &project.stories,
    )
    .is_ok()
}

#[test]
fn original_entry_requires_safe_target_and_optional_safe_source() {
    let base = project();
    assert!(valid(&base));
    let mut any_source = base.clone();
    any_source.scenario.as_mut().unwrap()["events"][0]
        .as_object_mut()
        .unwrap()
        .remove("from");
    assert!(valid(&any_source));
    for field in ["target", "from"] {
        for value in [
            json!(""),
            json!("../chapter"),
            json!("MOD_other_main"),
            json!("chapter.lua"),
            json!("x".repeat(65)),
            json!(null),
            json!(7),
        ] {
            let mut invalid = base.clone();
            invalid.scenario.as_mut().unwrap()["events"][0][field] = value;
            assert!(!valid(&invalid), "accepted invalid {field}");
        }
    }
    for trigger in ["auto", "position", "start"] {
        let mut invalid = base.clone();
        let event = &mut invalid.scenario.as_mut().unwrap()["events"][0];
        event["trigger"] = json!(trigger);
        if trigger == "position" {
            event["position"] = json!("Center");
        }
        assert!(
            !valid(&invalid),
            "old trigger accepted story-entry-only fields"
        );
    }
    let mut mixed = base.clone();
    mixed.scenario.as_mut().unwrap()["events"][0]["position"] = json!("Center");
    assert!(!valid(&mixed));
}

#[test]
fn pooled_original_entries_must_share_target_and_source() {
    let mut project = project();
    let events = project.scenario.as_mut().unwrap()["events"]
        .as_array_mut()
        .unwrap();
    events[0]["pool"] = json!("visits");
    let mut other = events[0].clone();
    other["id"] = json!("second");
    events.push(other);
    assert!(valid(&project));
    for field in ["target", "from"] {
        let mut invalid = project.clone();
        invalid.scenario.as_mut().unwrap()["events"][1][field] = json!("another_original_chapter");
        assert!(!valid(&invalid), "mixed {field} was accepted in one pool");
    }
}

#[test]
fn export_requires_new_host_only_for_original_entry_events() {
    let directory = tempfile::tempdir().unwrap();
    let project = project();
    let output = directory.path().join("story-entry.lommod");
    project.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
    assert_eq!(reopened.scenario, project.scenario);
    let mut legacy = project.clone();
    let event = &mut legacy.scenario.as_mut().unwrap()["events"][0];
    event["trigger"] = json!("position");
    event["position"] = json!("Center");
    for key in ["target", "from"] {
        event.as_object_mut().unwrap().remove(key);
    }
    let output = directory.path().join("legacy.lommod");
    legacy.export(&output).unwrap();
    assert_eq!(
        Project::open(&output).unwrap().manifest["min_host_version"],
        "1.1.3"
    );
}

#[test]
fn entry_ids_use_the_existing_original_chapter_contract() {
    for id in ["chapter", "chapter_1", "chapter-2"] {
        assert!(lom_core::vanilla_patch::valid_target(id));
    }
    let mut project = project();
    let event = &mut project.scenario.as_mut().unwrap()["events"][0];
    event["target"] = Value::String("a".repeat(64));
    assert!(valid(&project));
}
