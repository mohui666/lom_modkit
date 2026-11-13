use lom_core::{codegen, package, project::Project, validate};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

fn story() -> Value {
    json!({"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"player","position":"M"},
        {"id":"line","type":"say","character":"player","text":"本句气泡"},
        {"id":"next","type":"say","character":"player","text":"恢复原样"},
        {"id":"done","type":"end"}
    ]})
}

#[test]
fn dialogue_mood_empty_values_keep_the_omitted_field_behavior() {
    let baseline = codegen::compile_story(&story(), None, None, None).unwrap();
    let directory = tempfile::tempdir().unwrap();
    for value in [json!(""), Value::Null] {
        let mut input = story();
        input["nodes"][1]["mood_icon"] = value;
        input["nodes"][1]["mode"] = json!("narrative");
        input["nodes"][1]["mood_character"] = json!("user:unused.actor");
        validate::validate_story(&input).unwrap();
        assert!(lom_core::release::find_stage_issues(&input).is_empty());
        // Compare with the same ordinary narrative line, without a mood override.
        let mut plain = input.clone();
        plain["nodes"][1]
            .as_object_mut()
            .unwrap()
            .remove("mood_icon");
        assert_eq!(
            codegen::compile_story(&input, None, None, None).unwrap(),
            codegen::compile_story(&plain, None, None, None).unwrap()
        );
        let mut project = Project::new();
        project.stories.clear();
        input["story_schema"] = json!(2);
        project.stories.insert("main".into(), input);
        project.manifest["entry"] = json!("main");
        project.manifest["min_host_version"] = json!("1.4.9");
        let exported = directory.path().join("empty.lommod");
        project.export(&exported).unwrap();
        let restored = Project::open(&exported).unwrap();
        assert_eq!(restored.stories, project.stories);
        assert_eq!(restored.manifest["min_host_version"], "1.4.9");
    }
    assert!(!baseline.contains("mod_dialogue_mood"));
}

#[test]
fn released_dialogues_keep_empty_mood_values_through_edit_save_export_and_reopen() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.14-parallel-base.lommod");
    let original_bytes = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let assets = project.assets.clone();
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("unchanged.lommod");
    project.export(&original).unwrap();
    assert_eq!(
        package::read_package(&original).unwrap()["lua/main.lua"],
        package::read_package(&fixture).unwrap()["lua/main.lua"]
    );
    let chapter = project.stories.get_mut("main").unwrap();
    let start = chapter["start"].clone();
    chapter["nodes"].as_array_mut().unwrap().insert(0,
        json!({"id":"empty_mood","type":"say","mode":"narrative","text":"沿用气泡","mood_icon":"","mood_character":"user:unused.actor","goto":start}));
    chapter["start"] = json!("empty_mood");
    let saved = directory.path().join("edited");
    project.save_to(&saved).unwrap();
    let mut reopened = Project::open(&saved).unwrap();
    assert_eq!(reopened.stories, project.stories);
    reopened.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("编辑后仍沿用气泡");
    let exported = directory.path().join("edited.lommod");
    reopened.export(&exported).unwrap();
    let restored = Project::open(&exported).unwrap();
    assert_eq!(restored.stories, reopened.stories);
    assert_eq!(restored.assets, assets);
    assert_eq!(
        restored.manifest["min_host_version"],
        reopened.manifest["min_host_version"]
    );
    let mut entries = package::read_package(&exported).unwrap();
    assert!(!std::str::from_utf8(&entries["lua/main.lua"])
        .unwrap()
        .contains("mod_dialogue_mood"));
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua replacement')\n");
    let previous_record = std::str::from_utf8(&entries["story-lua.sha256"]).unwrap();
    let mut record = String::from("algorithm=lom-story-lua-sha256-v1\n");
    for line in previous_record
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
    {
        let columns: Vec<_> = line.split('\t').collect();
        record.push_str(&format!(
            "{}\t{:X}\t{}\t{:X}\n",
            columns[0],
            Sha256::digest(&entries[columns[0]]),
            columns[2],
            Sha256::digest(&entries[columns[2]])
        ));
    }
    entries.insert("story-lua.sha256".into(), record.into_bytes());
    let substituted = directory.path().join("substituted.lommod");
    package::write_package(entries, &substituted).unwrap();
    let inspection = package::inspect_package(&substituted).unwrap();
    assert_eq!(inspection["integrity_verified"], true);
    assert_eq!(inspection["source_lua_match"], false);
    assert!(Project::open(&substituted).is_err());
    assert_eq!(std::fs::read(&fixture).unwrap(), original_bytes);
}

#[test]
fn dialogue_mood_wraps_only_its_say() {
    for mode in ["character", "think", "narrative", "center"] {
        let mut input = story();
        input["nodes"][1]["mode"] = json!(mode);
        let legacy = codegen::compile_story(&input, None, None, None).unwrap();
        assert!(!legacy.contains("mod_dialogue_mood"));
        for icon in ["auto", "hide", "shock", "laugh1"] {
            input["nodes"][1]["mood_icon"] = json!(icon);
            validate::validate_story(&input).unwrap();
            let lua = codegen::compile_story(&input, None, None, None).unwrap();
            let scope = format!("local __lom_restore_mood = mod_dialogue_mood(\"player\", \"{icon}\")\n\tsay(luamanager.GetStoryText(\"MOD_MOD_main_line\"))\n\t__lom_restore_mood()");
            assert!(lua.contains(&scope), "{lua}");
            assert_eq!(lua.matches("mod_dialogue_mood(").count(), 1);
        }
    }
    let mut input = story();
    input["nodes"][1]["mode"] = json!("narrative");
    input["nodes"][1]["mood_character"] = json!("artist1");
    input["nodes"][1]["mood_icon"] = json!("silent");
    assert!(codegen::compile_story(&input, None, None, None)
        .unwrap()
        .contains("mod_dialogue_mood(\"artist1\", \"silent\")"));
}

#[test]
fn inherited_dialogue_supports_a_scoped_mood_without_changing_native_settings() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/vanilla-import.json")).unwrap();
    let mut input = fixture["baseline"].clone();
    let node = input["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["type"] == "say")
        .unwrap();
    assert_eq!(node["inherit_dialog"], true);
    node["mood_icon"] = json!("shock");
    node["mood_character"] = json!("player");
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    assert!(
        lua.contains("mod_dialogue_mood(\"player\", \"shock\")\n\tsay("),
        "{lua}"
    );
    assert!(lua.contains("\n\t__lom_restore_mood()"));
}

#[test]
fn a_different_mood_target_is_packaged_and_checked_for_stage_presence() {
    let mut project = Project::new();
    let mut input = story();
    input["nodes"][1]["mood_icon"] = json!("shock");
    input["nodes"][1]["mood_character"] = json!("user:demo.actor");
    input["nodes"][1]["portrait"] = json!("shy");
    assert_eq!(
        lom_core::release::find_stage_issues(&input),
        vec![("line".into(), "user:demo.actor".into())]
    );
    let mut repaired = input.clone();
    assert_eq!(
        lom_core::release::ensure_stage(&mut repaired, "line").unwrap()["character"],
        "user:demo.actor"
    );
    assert!(lom_core::release::find_stage_issues(&repaired).is_empty());
    project.stories.clear();
    project.stories.insert("main".into(), input);
    project.manifest["entry"] = json!("main");
    let meta = lom_core::content::normalize_content_metadata(&json!({
        "schema":1,"id":"demo.actor","type":"character","name":"测试人物",
        "files":{"main":"normal.png"},"portraits":{"normal":"normal.png"}
    }))
    .unwrap();
    project.assets.insert(
        "assets/user/character/demo.actor/content.json".into(),
        serde_json::to_vec(&meta).unwrap(),
    );
    project.assets.insert(
        "assets/user/character/demo.actor/normal.png".into(),
        include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
        )
        .to_vec(),
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("target.lommod");
    project.export(&path).unwrap();
    let entries = package::read_package(&path).unwrap();
    assert!(entries.contains_key("assets/user/character/demo.actor/normal.png"));
    assert!(package::inspect_package(&path).unwrap()["ok"]
        .as_bool()
        .unwrap());
}

#[test]
fn dialogue_mood_requires_a_valid_target_and_icon() {
    let mut input = story();
    input["nodes"][1]["mood_icon"] = json!("shock");
    for target in [json!("missing.actor"), json!("user:../escape")] {
        input["nodes"][1]["mood_character"] = target;
        assert!(validate::validate_story(&input).is_err());
    }
    input["nodes"][1]["mood_character"] = json!("player");
    input["nodes"][1]["mood_icon"] = json!("unknown");
    let error = validate::validate_story(&input).unwrap_err().to_string();
    assert!(error.contains("本句表情气泡选项"), "{error}");
    assert!(!error.contains("必须是null"), "{error}");
    input["nodes"][1] =
        json!({"id":"line","type":"say","mode":"narrative","text":"旁白","mood_icon":"shock"});
    assert!(validate::validate_story(&input).is_err());
}

#[test]
fn dialogue_mood_packages_raise_only_lower_host_versions() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mood.lommod");
    let mut project = Project::new();
    project.stories.clear();
    let mut input = story();
    input["nodes"][1]["mood_icon"] = json!("shock");
    project.stories.insert("main".into(), input);
    project.manifest["entry"] = json!("main");
    for (minimum, expected) in [
        ("1.4.9", "1.4.10"),
        ("1.4.10-beta.1", "1.4.10"),
        ("2.0.0", "2.0.0"),
    ] {
        project.manifest["min_host_version"] = json!(minimum);
        project.export(&path).unwrap();
        let entries = package::read_package(&path).unwrap();
        let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
        assert_eq!(manifest["min_host_version"], expected);
        assert!(package::inspect_package(&path).unwrap()["ok"]
            .as_bool()
            .unwrap());
    }
}
