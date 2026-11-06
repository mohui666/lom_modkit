use lom_core::{native_upgrade, package, project::Project, vanilla_import};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/vanilla-import.json")).unwrap()
}

#[test]
fn upgrades_legacy_visual_edits_and_linear_insertions_without_changing_official_source() {
    let fixture = fixture();
    let story = fixture["story"].clone();
    let before = story.clone();
    let upgraded = native_upgrade::upgrade(&story).unwrap();
    assert_eq!(story, before);
    assert_eq!(upgraded["vanilla_import"]["schema"], 2);
    assert_eq!(
        vanilla_import::parse(&upgraded).unwrap().unwrap().source(),
        fixture["source"]
    );
    let nodes = upgraded["nodes"].as_array().unwrap();
    assert!(nodes.iter().all(|node| node["type"] != "raw"));
    assert!(nodes.iter().any(|node| node["type"] == "native_branch"));
    assert_eq!(
        nodes
            .iter()
            .find(|node| node["id"] == "original_2")
            .unwrap()["text"],
        "改写甲"
    );
    assert_eq!(
        nodes.iter().find(|node| node["id"] == "inserted").unwrap()["text"],
        "新增对白"
    );
    let tail = nodes
        .iter()
        .find(|node| node["id"] == "original_6")
        .unwrap();
    assert_eq!(
        nodes.iter().find(|node| node["id"] == "inserted").unwrap()["goto"],
        tail["id"]
    );
    for old in story["nodes"].as_array().unwrap() {
        assert!(nodes.iter().any(|node| node["id"] == old["id"]));
    }

    let mut project = Project::new_vanilla_patch("DEMO").unwrap();
    project.stories.clear();
    project
        .stories
        .insert("native_import".into(), upgraded.clone());
    project.manifest["entry"] = json!("native_import");
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("upgraded.lommod");
    project.export(&output).unwrap();
    assert_eq!(
        package::inspect_package(&output).unwrap()["source_lua_match"],
        true
    );
    assert_eq!(
        Project::open(&output).unwrap().stories["native_import"],
        upgraded
    );
}

#[test]
fn upgrades_an_insertion_at_the_end_of_a_true_arm_to_its_join_target() {
    let mut story = fixture()["baseline"].clone();
    story["nodes"].as_array_mut().unwrap().insert(
        2,
        json!({
            "id":"after_true","type":"say","text":"真分支新增","inherit_dialog":true
        }),
    );
    let upgraded = native_upgrade::upgrade(&story).unwrap();
    let nodes = upgraded["nodes"].as_array().unwrap();
    let original = nodes
        .iter()
        .find(|node| node["id"] == "original_2")
        .unwrap();
    let inserted = nodes
        .iter()
        .find(|node| node["id"] == "after_true")
        .unwrap();
    assert_eq!(original["goto"], inserted["id"]);
    assert_ne!(inserted["goto"], "original_3");
    let next = nodes
        .iter()
        .find(|node| node["id"] == inserted["goto"])
        .unwrap();
    assert_eq!(next["type"], "native_action");
    assert!(next["operations"].as_array().unwrap().is_empty());
}

#[test]
fn refuses_edited_raw_deleted_origins_and_custom_control_flow_without_mutating_input() {
    let mut raw = fixture()["baseline"].clone();
    raw["nodes"][0]["code"] = json!(format!(
        "{}\nuser_change()\n",
        raw["nodes"][0]["code"].as_str().unwrap()
    ));
    let before = raw.clone();
    let error = native_upgrade::upgrade(&raw).unwrap_err().to_string();
    assert!(error.contains("original_1") && error.contains("高级 Lua 已修改"));
    assert_eq!(raw, before);

    let mut deleted = fixture()["baseline"].clone();
    deleted["nodes"].as_array_mut().unwrap().remove(1);
    let before = deleted.clone();
    assert!(native_upgrade::upgrade(&deleted)
        .unwrap_err()
        .to_string()
        .contains("删除或重排"));
    assert_eq!(deleted, before);

    let mut branching = fixture()["baseline"].clone();
    branching["nodes"].as_array_mut().unwrap().insert(
        5,
        json!({
            "id":"extra_check","type":"flag_check","source":"condition","flag":"EXTRA",
            "success":"original_6","failure":"original_6"
        }),
    );
    let before = branching.clone();
    let error = native_upgrade::upgrade(&branching).unwrap_err().to_string();
    assert!(error.contains("extra_check") && error.contains("多出口流程"));
    assert_eq!(branching, before);
}
