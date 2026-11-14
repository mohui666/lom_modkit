use lom_core::{codegen, package, project::Project, release, validate};
use serde_json::{json, Value};

fn story() -> Value {
    json!({"id":"main","story_schema":2,"start":"show","nodes":[
        {"id":"show","type":"show","character":"player","position":"L1"},
        {"id":"line","type":"say","mode":"narrative","text":"两人对望。"},
        {"id":"next","type":"say","character":"player","text":"接下来。"},
        {"id":"done","type":"end"}
    ]})
}

#[test]
fn selection_wraps_only_narration_and_an_empty_selection_preserves_output() {
    let mut input = story();
    let baseline = codegen::compile_story(&input, None, None, None).unwrap();
    input["nodes"][1]["highlight_characters"] = json!([]);
    assert_eq!(
        baseline,
        codegen::compile_story(&input, None, None, None).unwrap()
    );
    input["nodes"][1]["highlight_characters"] = json!(["player", "brother1"]);
    let lua = codegen::compile_story(&input, None, None, None).unwrap();
    let begin = lua
        .find("mod_begin_say_highlights({\"player\", \"brother1\"})")
        .unwrap();
    let speaker = lua[begin..].find("setcharacter(narrative)").unwrap() + begin;
    let line = lua[speaker..].find("say(luamanager.GetStoryText(").unwrap() + speaker;
    let end = lua.find("mod_end_say_highlights()").unwrap();
    assert!(begin < speaker && speaker < line && line < end);
    assert_eq!(lua.matches("mod_begin_say_highlights(").count(), 1);
    for mode in ["character", "think", "center", "offscreen"] {
        input["nodes"][1]["mode"] = json!(mode);
        input["nodes"][1]["character"] = json!("player");
        assert!(!codegen::compile_story(&input, None, None, None)
            .unwrap()
            .contains("mod_begin_say_highlights"));
    }
}

#[test]
fn selection_is_validated_and_reports_absent_actors_at_the_narration_step() {
    let mut input = story();
    input["nodes"][1]["highlight_characters"] = json!(["player", "brother1"]);
    assert_eq!(
        release::find_stage_issues(&input),
        vec![("line".into(), "brother1".into())]
    );
    for invalid in [
        json!([""]),
        json!([true]),
        json!(["missing.actor"]),
        json!(["player", "player"]),
        json!(["user:../escape"]),
        json!("player"),
    ] {
        input["nodes"][1]["highlight_characters"] = invalid;
        assert!(validate::validate_story(&input).is_err(), "{input}");
    }
}

#[test]
fn exported_selection_keeps_its_character_asset_and_requires_the_updated_host() {
    let sample =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/showcase3");
    let mut project = Project::open(&sample).unwrap();
    let mut input = story();
    input["nodes"][1]["highlight_characters"] = json!(["player", "user:showcase.lin_deng"]);
    project.stories.clear();
    project.stories.insert("main".into(), input);
    project.manifest["entry"] = json!("main");
    project.manifest["campaign"]["triggers"] = json!([]);
    project
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("node_free_triggers");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("highlight.lommod");
    for (minimum, expected) in [
        ("1.4.9", "1.5.20"),
        ("1.5.20-beta.1", "1.5.20"),
        ("2.0.0", "2.0.0"),
    ] {
        project.manifest["min_host_version"] = json!(minimum);
        project.export(&path).unwrap();
        let entries = package::read_package(&path).unwrap();
        let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
        assert_eq!(manifest["min_host_version"], expected);
        assert!(entries.contains_key("assets/user/character/showcase.lin_deng/normal.png"));
        let reopened = Project::open(&path).unwrap();
        assert_eq!(reopened.stories, project.stories);
    }
}
