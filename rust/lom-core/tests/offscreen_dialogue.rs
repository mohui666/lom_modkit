use serde_json::json;

#[test]
fn offscreen_dialogue_does_not_load_focus_or_reuse_an_unknown_character() {
    for name in [None, Some("远处的声音")] {
        let mut node = json!({"id":"voice","type":"say","mode":"offscreen","text":"救命！"});
        if let Some(name) = name {
            node["speaker_name"] = json!(name);
        }
        let story = json!({"story_schema":2,"id":"main","start":"voice","nodes":[node,{"id":"end","type":"end"}]});
        lom_core::validate::validate_story(&story).unwrap();
        let lua = lom_core::codegen::compile_story(&story, None, None, None).unwrap();
        assert!(lua.contains("setsaydialog(saydialogs.character)"));
        assert!(lua.contains("setcharacter(narrative)"));
        assert!(lua.contains(&format!(
            "mod_dialogue_name(\"{}\")",
            name.unwrap_or("？？？")
        )));
        assert!(lua.contains("__lom_restore_name()"));
        for forbidden in [
            "LoadCharacterAsset",
            "LoadCharacterPortrait",
            "showPortrait",
            "characters.Focus",
            "characters.Get(\"unknown\")",
        ] {
            assert!(!lua.contains(forbidden), "{forbidden}");
        }
    }
}

#[test]
fn offscreen_dialogue_roundtrips_and_sets_the_host_requirement() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = lom_core::project::Project::new();
    project.stories.insert(
        "main".into(),
        json!({"story_schema":2,"id":"main","start":"voice","nodes":[
        {"id":"voice","type":"say","mode":"offscreen","speaker_name":"陌生人","text":"有人吗？"},
        {"id":"end","type":"end"}]}),
    );
    let package = dir.path().join("voice.lommod");
    project.export(&package).unwrap();
    let reopened = lom_core::project::Project::open(&package).unwrap();
    assert_eq!(reopened.manifest["min_host_version"], "1.5.1");
    assert_eq!(reopened.stories, project.stories);
    assert_eq!(
        lom_core::localization::iter_localizable_texts(&reopened.stories["main"])
            ["voice.speaker_name"],
        "陌生人"
    );
}
