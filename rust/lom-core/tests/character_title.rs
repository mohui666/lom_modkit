use lom_core::{codegen, localization, package, project::Project, validate};
use serde_json::json;

#[test]
fn title_node_uses_one_callback_for_player_and_other_characters() {
    for character in ["player", "brother1", "user:demo.actor"] {
        let mut story = json!({"id":"main","start":"title","nodes":[
            {"id":"title","type":"character_title","character":character,"action":"set","title":"唐门新秀","wait_after":0.5},
            {"id":"hide","type":"character_title","character":character,"action":"hide"},
            {"id":"reset","type":"character_title","character":character,"action":"reset"},
            {"id":"end","type":"end"}]});
        validate::validate_story(&story).unwrap();
        let lua = codegen::compile_story(&story, None, None, None).unwrap();
        assert!(lua.contains(&format!(
            "mod_character_title(\"{character}\", \"set\", \"唐门新秀\")"
        )));
        assert!(lua.contains("wait(0.5)"));
        assert_eq!(
            localization::iter_localizable_texts(&story)["title.title"],
            "唐门新秀"
        );
        for invalid in ["", "两行\n称号"] {
            story["nodes"][0]["title"] = json!(invalid);
            assert!(validate::validate_story(&story).is_err());
        }
    }
}

#[test]
fn released_153_title_stat_opens_edits_exports_and_keeps_lua_ownership() {
    // Captured from the released v1.5.3 compiler, before character_title existed.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.3-title-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let old_story = project.stories["main"].clone();
    let nodes = project.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap();
    nodes.insert(1, json!({"id":"new_title","type":"character_title","character":"player","action":"set","title":"唐门新秀"}));
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("edited");
    project.save_to(&saved).unwrap();
    let edited = Project::open(&saved).unwrap();
    let output = dir.path().join("edited.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.4");
    assert!(reopened.stories["main"]["nodes"]
        .as_array()
        .unwrap()
        .contains(&old_story["nodes"][1]));
    assert_eq!(std::fs::read(&fixture).unwrap(), original);

    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('substituted')\n");
    let tampered = dir.path().join("substituted.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
