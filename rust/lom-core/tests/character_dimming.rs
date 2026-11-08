use serde_json::json;

#[test]
fn restoring_dialogue_dimming_survives_project_and_package_roundtrips_and_requires_the_new_host() {
    let dir = tempfile::tempdir().unwrap();
    for mode in ["until_dialogue", "auto"] {
        let mut project = lom_core::project::Project::new();
        project.stories.insert(
            "main".into(),
            json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"player","position":"M"},
            {"id":"dim","type":"dim","character":"player","dimmed":false,"mode":mode},
            {"id":"say","type":"say","character":"player","text":"自动亮暗"},
            {"id":"end","type":"end"}]}),
        );
        let folder = dir.path().join(mode);
        project.save_to(&folder).unwrap();
        let reopened = lom_core::project::Project::open(&folder).unwrap();
        let package = dir.path().join(format!("{mode}.lommod"));
        reopened.export(&package).unwrap();
        let imported = lom_core::project::Project::open(&package).unwrap();
        assert_eq!(imported.manifest["min_host_version"], "1.5.1");
        assert_eq!(imported.stories, reopened.stories);
    }
}

#[test]
fn temporary_and_automatic_dimming_use_the_new_bridge_without_changing_legacy_lua() {
    for actor in ["player", "user:demo.speaker"] {
        for mode in ["until_dialogue", "auto"] {
            let story = json!({"story_schema":2,"id":"main","start":"dim","nodes":[
                {"id":"dim","type":"dim","character":actor,"dimmed":false,"mode":mode},
                {"id":"say","type":"say","character":actor,"text":"恢复自动"},
                {"id":"end","type":"end"}]});
            let lua = lom_core::codegen::compile_story(&story, None, None, None).unwrap();
            assert!(lua.contains(&format!(
                "mod_char_dim_mode(\"{actor}\", false, \"{mode}\")"
            )));
        }
    }
}

#[test]
fn explicit_dimming_uses_the_persistent_bridge_before_and_after_speech() {
    for actor in ["player", "user:demo.speaker"] {
        for parallel in [false, true] {
            let dark = json!({"id":"dark","type":"dim","character":actor,"dimmed":true});
            let dark = if parallel {
                json!({"id":"dark","type":"parallel","actions":[
                    {"type":"dim","character":actor,"dimmed":true},
                    {"type":"wait","seconds":0.2}]})
            } else {
                dark
            };
            let story = json!({"story_schema":2,"id":"main","title":"压暗持续","start":"enter","nodes":[
                {"id":"enter","type":"show","character":actor,"position":"M"}, dark,
                {"id":"speech","type":"say","character":actor,"text":"说话后仍保持压暗"},
                {"id":"bright","type":"dim","character":actor,"dimmed":false},
                {"id":"end","type":"end"}]});
            let lua = lom_core::codegen::compile_story(&story, None, None, None).unwrap();
            let before = lua
                .find(&format!("mod_char_dim(\"{actor}\", true)"))
                .unwrap();
            let speech = lua.find("GetStoryText").unwrap();
            let after = lua
                .find(&format!("mod_char_dim(\"{actor}\", false)"))
                .unwrap();
            assert!(before < speech && speech < after);
            assert!(!lua.contains("stage.SetDimmed"));
        }
    }
}
