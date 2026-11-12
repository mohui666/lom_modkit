use lom_core::{codegen, package, project::Project, render_priority as rp, validate};
use serde_json::json;

#[test]
fn render_priority_matches_tags_resets_and_validates_integer_orders() {
    let mut rules = json!([]);
    for node in [
        json!({"target":"effect","name":"Hit_001","order":5}),
        json!({"target":"effect","tag":"left","order":20}),
        json!({"target":"character","character":"player","order":-3}),
    ] {
        rp::validate(&node).unwrap();
        rp::apply(&mut rules, &node);
    }
    assert_eq!(rp::priority(&rules, "effect", "Hit_001", "right"), 5);
    assert_eq!(rp::priority(&rules, "effect", "Hit_001", "left"), 20);
    assert_eq!(rp::priority(&rules, "effect", "Rain_001", "left"), 20);
    assert_eq!(rp::priority(&rules, "character", "player", ""), -3);
    rp::apply(
        &mut rules,
        &json!({"target":"effect","name":"Hit_001","tag":"left","enabled":false}),
    );
    assert_eq!(rp::priority(&rules, "effect", "Hit_001", "left"), 0);
    assert_eq!(rp::priority(&rules, "effect", "Hit_001", "right"), 5);
    assert_eq!(rules.as_array().unwrap().len(), 3);
    for node in [
        json!({"target":"character"}),
        json!({"target":"overlay","slot":" "}),
        json!({"target":"effect"}),
        json!({"target":"background","order":0.5}),
        json!({"target":"background","order":10001}),
        json!({"target":"unknown"}),
    ] {
        assert!(rp::validate(&node).is_err(), "{node}");
    }
}

#[test]
fn render_priority_upgrades_released_source_without_loosening_lua_ownership() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.11-render-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let old_entries = package::read_package(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let legacy = project.stories["main"].clone();
    let dir = tempfile::tempdir().unwrap();
    let unchanged = dir.path().join("unchanged.lommod");
    project.export(&unchanged).unwrap();
    assert_eq!(
        package::read_package(&unchanged).unwrap()["lua/main.lua"],
        old_entries["lua/main.lua"]
    );
    let nodes = project.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap();
    nodes.insert(nodes.len()-1,json!({"id":"priority","type":"render_priority","target":"effect","tag":"Hit_001","order":12}));
    validate::validate_story_with_catalog(
        &project.stories["main"],
        &validate::catalog_with_assets(&project.assets).unwrap(),
    )
    .unwrap();
    let lua = codegen::compile_story(&project.stories["main"], None, None, None).unwrap();
    assert!(
        lua.contains("mod_render_priority(\"effect\", \"\", \"Hit_001\", 12, true)"),
        "{lua}"
    );
    assert!(lua.contains("mod_effect("));
    assert!(!lua.contains("effects.SetupEffect("));
    project.save_to(&dir.path().join("edited")).unwrap();
    let edited = Project::open(&dir.path().join("edited")).unwrap();
    let output = dir.path().join("edited.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.assets, edited.assets);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.12");
    assert_eq!(
        legacy["nodes"].as_array().unwrap().len() + 1,
        reopened.stories["main"]["nodes"].as_array().unwrap().len()
    );
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua')\n");
    let tampered = dir.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
