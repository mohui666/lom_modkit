use lom_core::{project::Project, scenario};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/scenario-contract.json")).unwrap()
}
fn patched(base: &Value, patches: &Value) -> Value {
    let mut c = base.clone();
    for p in patches.as_array().unwrap() {
        let path = p["path"].as_str().unwrap();
        if p["remove"] == true {
            let (parent, key) = path.rsplit_once('/').unwrap();
            let parent = c.pointer_mut(parent).unwrap();
            if let Some(a) = parent.as_array_mut() {
                a.remove(key.parse::<usize>().unwrap());
            } else {
                parent.as_object_mut().unwrap().remove(key);
            }
        } else {
            let (parent, key) = path.rsplit_once('/').unwrap();
            let parent = c.pointer_mut(parent).unwrap();
            if let Some(a) = parent.as_array_mut() {
                a[key.parse::<usize>().unwrap()] = p["value"].clone();
            } else {
                parent[key] = p["value"].clone();
            }
        }
    }
    c
}
#[test]
fn shared_authoring_contract() {
    let f = fixture();
    let stories = serde_json::from_value(f["stories"].clone()).unwrap();
    for case in f["cases"].as_array().unwrap() {
        let mut m = f["manifest"].clone();
        if let Some(mode) = case.get("mode") {
            m["play_mode"] = mode.clone();
        }
        let c = patched(&f["config"], &case["patches"]);
        let result = scenario::validate(&c, &m, &stories);
        assert_eq!(
            result.is_ok(),
            case["valid"] == true,
            "{}: {result:?}",
            case["name"]
        );
    }
}
#[test]
fn shared_presentation_node_boundary_covers_the_editor_schema() {
    let fixture = fixture();
    let forbidden = fixture["presentation_forbidden"].as_array().unwrap();
    for mode in ["addon", "sandbox"] {
        let project = Project::new_scenario(mode);
        for kind in lom_core::validate::node_schema()["nodes"]
            .as_object()
            .unwrap()
            .keys()
        {
            let expected = !forbidden.contains(&json!(kind));
            assert_eq!(scenario::performance_node(kind), expected, "{mode}: {kind}");
            let mut stories = project.stories.clone();
            stories.get_mut("main").unwrap()["nodes"][0]["type"] = json!(kind);
            assert_eq!(
                scenario::validate(
                    project.scenario.as_ref().unwrap(),
                    &project.manifest,
                    &stories
                )
                .is_ok(),
                expected,
                "{mode}: {kind}"
            );
        }
    }
}

#[test]
fn project_and_package_roundtrip_preserve_separate_configuration() {
    for mode in ["addon", "sandbox"] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let mut p = Project::new_scenario(mode);
        p.save_to(&source).unwrap();
        assert!(source.join("scenario.json").is_file());
        let loaded = Project::open(&source).unwrap();
        assert_eq!(loaded.scenario, p.scenario);
        let package = root.path().join("sample.lommod");
        loaded.export(&package).unwrap();
        let packed = Project::open(&package).unwrap();
        assert_eq!(packed.scenario, p.scenario);
        assert_eq!(packed.manifest["play_mode"], mode);
        assert!(p.save_to(&root.path().join("single.json")).is_err());
    }
}
#[test]
fn new_modes_protect_legacy_and_presentation_boundary() {
    let p = Project::new_scenario("sandbox");
    lom_core::validate::validate_manifest(&p.manifest).unwrap();
    let mut m = p.manifest.clone();
    m["min_host_version"] = json!("1.1.2");
    assert!(lom_core::validate::validate_manifest(&m).is_err());
    m = p.manifest.clone();
    m["campaign"]["triggers"] = json!([{"type":"position","position":"Center","script":"main"}]);
    assert!(lom_core::validate::validate_manifest(&m).is_err());
    for kind in [
        "free_trigger",
        "mod_quest",
        "persistent_var",
        "result_screen",
        "raw",
    ] {
        let mut stories = p.stories.clone();
        stories.get_mut("main").unwrap()["nodes"][0]["type"] = json!(kind);
        assert!(scenario::validate(p.scenario.as_ref().unwrap(), &p.manifest, &stories).is_err());
    }
    assert!(Project::new().scenario.is_none());
}

#[test]
fn new_modes_keep_in_story_attributes_items_flags_and_time_available() {
    for mode in ["addon", "sandbox"] {
        let p = Project::new_scenario(mode);
        for kind in [
            "stat",
            "stat_set",
            "affinity",
            "talent",
            "item",
            "flag",
            "game_flag",
            "mission",
            "time",
        ] {
            assert!(
                scenario::performance_node(kind),
                "{mode}: {kind} disappeared from the palette"
            );
            let mut stories = p.stories.clone();
            stories.get_mut("main").unwrap()["nodes"][0]["type"] = json!(kind);
            scenario::validate(p.scenario.as_ref().unwrap(), &p.manifest, &stories).unwrap();
        }
    }
}
#[test]
fn result_node_compiles_and_cannot_continue_to_another_chapter() {
    let p = Project::new_scenario("sandbox");
    let mut story = p.stories["main"].clone();
    story["nodes"][1]["event_result"] = json!("accepted");
    let lua = lom_core::codegen::compile_story(&story, Some(&p.manifest), None, None).unwrap();
    assert!(lua.contains("mod_event_result(\"accepted\")"));
    story["nodes"][1]["next_script"] = json!("main");
    assert!(lom_core::validate::validate_story(&story).is_err());
}
