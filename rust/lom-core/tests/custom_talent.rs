use lom_core::{codegen, content, content_edit, package, project::Project, release};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

fn definition() -> Value {
    json!({"content_schema":1,"id":"demo.awakening","type":"talent","name":"万木逢春","description":"独立天赋。\n当前没有效果。","files":{"main":""}})
}

#[test]
fn independent_talent_needs_no_media_and_rejects_effects() {
    let meta = content::normalize_content_metadata(&definition()).unwrap();
    assert!(content::listed_content_files(&meta).is_empty());
    assert_eq!(
        content::content_metadata_payload(&meta)["description"],
        definition()["description"]
    );
    for key in ["effects", "bonuses", "combat_skill", "battle_skill"] {
        let mut invalid = definition();
        invalid[key] = json!([]);
        assert!(
            content::normalize_content_metadata(&invalid).is_err(),
            "{key}"
        );
    }
}

#[test]
fn released_package_accepts_independent_talent_edit_export_reopen_and_rejects_substitution() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.4-name-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let old_entries = package::read_package(&fixture).unwrap();
    let root = tempfile::tempdir().unwrap();
    let unchanged = root.path().join("unchanged.lommod");
    project.export(&unchanged).unwrap();
    assert_eq!(
        old_entries["lua/main.lua"],
        package::read_package(&unchanged).unwrap()["lua/main.lua"]
    );
    let key = "assets/user/talent/demo.awakening/content.json";
    project
        .assets
        .insert(key.into(), lom_core::stable_json(&definition()).unwrap());
    let nodes = project.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap();
    nodes.insert(
        1,
        json!({"id":"awaken","type":"talent","talent":"user:demo.awakening","level":1}),
    );
    nodes.insert(2, json!({"id":"check_awaken","type":"talent_check","talent":"user:demo.awakening","op":">=","value":1,"success":"remove_awaken","failure":"remove_awaken"}));
    nodes.insert(3, json!({"id":"remove_awaken","type":"talent","talent":"user:demo.awakening","level":-1,"show_message":false}));
    content_edit::update(&mut project, key, &definition(), &BTreeMap::new()).unwrap();
    assert!(content_edit::remove(&mut project, key).is_err());
    let source = root.path().join("project");
    project.save_to(&source).unwrap();
    let edited = Project::open(&source).unwrap();
    let output = root.path().join("edited.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.8");
    assert_eq!(reopened.assets[key], edited.assets[key]);
    assert_eq!(original, std::fs::read(&fixture).unwrap());
    let lua = codegen::compile_story(&edited.stories["main"], None, None, None).unwrap();
    assert!(lua.contains("mod_talent(\"user:demo.awakening\", 1)"));
    assert!(lua.contains("mod_talent_level(\"user:demo.awakening\")"));
    assert!(release::referenced_asset_paths(
        &edited.stories,
        &edited.assets.keys().cloned().collect::<Vec<_>>()
    )
    .contains(&key.to_string()));
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('substituted')\n");
    let tampered = root.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}

#[test]
fn tea_catalog_includes_the_three_confirmed_game_ids() {
    let rows = lom_core::validate::editor_data()["items_misc"]
        .as_array()
        .unwrap();
    for (id, name) in [
        ("3001", "纳溪梅岭"),
        ("3002", "蒙顶露芽"),
        ("3003", "峨嵋雪芽"),
    ] {
        assert!(rows
            .iter()
            .any(|row| row["id"] == id && row["name"] == name));
    }
}
