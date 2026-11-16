use lom_core::{codegen, package, project::Project, validate};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

const BACKGROUND: &str = "combat:back_combat_backmountain_1";
fn duel(background: &str) -> serde_json::Value {
    json!({"story_schema":2,"id":"main","start":"duel","nodes":[
        {"id":"duel","type":"combat","character":"artist1","background":background,"win":"end","lose":"end"},
        {"id":"end","type":"end"}
    ]})
}

#[test]
fn dedicated_combat_backgrounds_preserve_legacy_view_ids_and_validate_their_own_catalog() {
    for id in ["center", "backmountain", "backmountain2", BACKGROUND] {
        validate::validate_story(&duel(id)).unwrap();
        assert!(codegen::compile_story(&duel(id), None, None, None)
            .unwrap()
            .contains(id));
    }
    for id in [
        "combat:",
        "combat:../back_combat_backmountain_1",
        "combat:unknown",
        "back_combat_backmountain_1",
    ] {
        assert!(validate::validate_story(&duel(id)).is_err(), "{id}");
    }
    let data = validate::editor_data();
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../../../data/ref/combat_backgrounds.json")).unwrap();
    assert_eq!(data["combat_backgrounds"], metadata["backgrounds"]);
    assert_eq!(data["combat_backgrounds"].as_array().unwrap().len(), 28);
    assert!(data["combat_backgrounds"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| {
            row["id"].as_str().unwrap() == format!("combat:{}", row["sprite"].as_str().unwrap())
                && !data["views"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|view| view["id"] == row["id"])
        }));
}

#[test]
fn only_dedicated_combat_backgrounds_require_host_162_and_round_trip_without_game_assets() {
    for (background, expected_min) in [("backmountain", "1.0.0"), (BACKGROUND, "1.6.2")] {
        let mut project = Project::new();
        project.manifest["min_host_version"] = json!("1.0.0");
        project.manifest["tested_host_version"] = json!("1.6.2");
        project.stories.insert("main".into(), duel(background));
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source");
        project.save_to(&source).unwrap();
        let reopened = Project::open(&source).unwrap();
        let output = folder.path().join("duel.lommod");
        reopened.export(&output).unwrap();
        let exported = Project::open(&output).unwrap();
        assert_eq!(exported.manifest["min_host_version"], expected_min);
        assert_eq!(exported.stories, reopened.stories);
        assert!(exported.assets.is_empty());
        let entries = package::read_package(&output).unwrap();
        assert!(!entries.keys().any(|key| key.contains("back_combat")));
    }
}

#[test]
fn released_159_and_1518_projects_accept_background_edits_and_keep_lua_ownership() {
    for name in [
        "released-1.5.9-filters.lommod",
        "released-1.5.18-picture-fades.lommod",
    ] {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let original = std::fs::read(&fixture).unwrap();
        let mut project = Project::open(&fixture).unwrap();
        let old = project.stories.clone();
        project.stories.insert("combat_background_test".into(), {
            let mut story = duel(BACKGROUND);
            story["id"] = json!("combat_background_test");
            story
        });
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("edited");
        project.save_to(&source).unwrap();
        let edited = Project::open(&source).unwrap();
        let output = folder.path().join("export.lommod");
        edited.export(&output).unwrap();
        let reopened = Project::open(&output).unwrap();
        assert_eq!(reopened.stories, edited.stories);
        assert_eq!(reopened.manifest["min_host_version"], "1.6.2");
        for (id, story) in &old {
            assert_eq!(&reopened.stories[id], story);
        }
        let mut substituted = package::read_package(&output).unwrap();
        substituted.remove("package-content.sha256");
        substituted
            .get_mut("lua/combat_background_test.lua")
            .unwrap()
            .extend_from_slice(b"\nerror('unrelated Lua')\n");
        let record = String::from_utf8(substituted["story-lua.sha256"].clone()).unwrap();
        let mut refreshed = "algorithm=lom-story-lua-sha256-v1\n".to_owned();
        for line in record.lines().skip(1).filter(|line| !line.is_empty()) {
            let fields: Vec<_> = line.split('\t').collect();
            refreshed.push_str(&format!(
                "{}\t{:X}\t{}\t{:X}\n",
                fields[0],
                Sha256::digest(&substituted[fields[0]]),
                fields[2],
                Sha256::digest(&substituted[fields[2]])
            ));
        }
        substituted.insert("story-lua.sha256".into(), refreshed.into_bytes());
        let bad = folder.path().join("substitution.lommod");
        package::write_package(substituted, &bad).unwrap();
        let inspected = package::inspect_package(&bad).unwrap();
        assert_eq!(inspected["integrity_verified"], true);
        assert_eq!(inspected["source_lua_match"], false);
        assert!(Project::open(&bad).is_err());
        assert_eq!(std::fs::read(&fixture).unwrap(), original);
    }
}
