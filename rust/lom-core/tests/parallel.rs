use lom_core::{codegen, parallel, project::Project, release, validate};
use serde_json::{json, Value};

fn story(actions: Value) -> Value {
    json!({"id":"main","story_schema":2,"start":"together","nodes":[
        {"id":"together","type":"parallel","actions":actions}, {"id":"done","type":"end"}
    ]})
}

#[test]
fn rejects_flow_control_invalid_child_fields_and_competing_outputs() {
    let valid = json!([{"type":"hide","character":"player","fadeDuration":0.5},{"type":"hide","character":"brother1","fadeDuration":0.8}]);
    validate::validate_story(&story(valid.clone())).unwrap();
    for actions in [
        json!([]),
        json!([{"type":"wait","seconds":1}]),
        json!([{"type":"say","text":"no"},{"type":"wait","seconds":1}]),
        json!([{"type":"parallel","actions":[]},{"type":"wait","seconds":1}]),
        json!([{"type":"raw","code":"error('no')"},{"type":"wait","seconds":1}]),
        json!([{"type":"wait","seconds":1,"goto":"done"},{"type":"wait","seconds":1}]),
        json!([{"type":"wait","seconds":1,"id":"nested"},{"type":"wait","seconds":1}]),
        json!([{"type":"wait","seconds":-1},{"type":"wait","seconds":1}]),
        json!([{"type":"move","character":"player","from":"L","to":"R","duration":1},{"type":"hide","character":"player","fadeDuration":1}]),
        json!([{"type":"scene","view":"center"},{"type":"transition","phase":"out"}]),
        json!([{"type":"overlay","action":"hide","slot":"same"},{"type":"overlay","action":"hide","slot":"same"}]),
        json!([{"type":"show","character":"player","position":"M","appearance":"original"},{"type":"wait","seconds":1}]),
        Value::Array(vec![
            json!({"type":"wait","seconds":0});
            parallel::MAX_ACTIONS + 1
        ]),
    ] {
        assert!(
            validate::validate_story(&story(actions.clone())).is_err(),
            "unexpectedly accepted {actions}"
        );
    }
    let conflict =
        parallel::validate_group(&json!({"actions": [valid[0].clone(), valid[0].clone()]}))
            .unwrap_err();
    assert!(conflict.to_string().contains("第 1 和第 2"));
}

#[test]
fn preloads_all_official_characters_before_starting_joined_branches() {
    let source = story(json!([
        {"type":"move","character":"player","from":"L","to":"M","duration":1.5},
        {"type":"hide","character":"brother1","fadeDuration":0.5}
    ]));
    let lua = codegen::compile_story(&source, None, None, None).unwrap();
    let call = lua.find("\t__lom_parallel({").unwrap();
    assert!(
        lua.find("runwait(characters.LoadCharacterAsset(\"player\"))")
            .unwrap()
            < call
    );
    assert!(
        lua.find("runwait(characters.LoadCharacterAsset(\"brother1\"))")
            .unwrap()
            < call
    );
    let body = &lua[call..];
    assert!(!body.contains("LoadCharacterAsset"));
    assert!(body.contains("wait(1.5)"));
    assert!(body.contains("wait(0.5)"));
    assert!(body.find("\t})").unwrap() < body.find("return node_done()").unwrap());
}

#[test]
fn stage_flow_and_reports_track_actions_but_navigate_to_group() {
    let mut s = story(json!([
        {"type":"show","character":"player","position":"L"},
        {"type":"show","character":"brother1","position":"R"}
    ]));
    s["nodes"].as_array_mut().unwrap().insert(
        1,
        json!({"id":"bye","type":"parallel","actions":[
            {"type":"hide","character":"player","fadeDuration":0.5},
            {"type":"hide","character":"brother1","fadeDuration":0.5}
        ]}),
    );
    assert!(release::find_stage_issues(&s).is_empty());
    s["nodes"][0]["actions"][0] = json!({"type":"wait","seconds":0.5});
    assert_eq!(
        release::find_stage_issues(&s),
        vec![("bye".into(), "player".into())]
    );
    let mut p = Project::new();
    p.stories.insert(
        "main".into(),
        story(json!([
            {"type":"show","character":"user:missing.hero","position":"L"},
            {"type":"background","action":"show","image":"user:missing.room","fade":0.5}
        ])),
    );
    let issues = release::run_preflight(&p, release::Profile::Editing, "1.2.0");
    assert!(issues
        .iter()
        .filter(|i| i.code == "missing_content")
        .all(|i| i.node_id == "together"));
    assert!(issues.iter().any(|i| i.message.contains("missing.hero")));
    assert!(issues.iter().any(|i| i.message.contains("missing.room")));
}

#[test]
fn content_references_inside_parallel_are_used_and_package_roundtrips() {
    let mut p = Project::new();
    p.stories.insert(
        "main".into(),
        story(json!([
            {"type":"show","character":"player","position":"L"},
            {"type":"show","character":"brother1","position":"R"}
        ])),
    );
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("parallel.lommod");
    p.export(&output).unwrap();
    let restored = Project::open(&output).unwrap();
    assert_eq!(restored.stories, p.stories);
    let stories = std::collections::BTreeMap::from([(
        "main".into(),
        story(json!([
            {"type":"background","action":"show","image":"user:demo.room","fade":1},
            {"type":"sound","name":"user:demo.chime"}
        ])),
    )]);
    let assets = vec![
        "assets/user/image/demo.room/content.json".into(),
        "assets/user/audio/demo.chime/sound.wav".into(),
    ];
    assert!(release::unused_asset_paths(&stories, &assets).is_empty());
}

#[test]
fn user_images_characters_and_audio_used_only_inside_group_are_packaged() {
    let sample =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/showcase3");
    let mut project = Project::open(&sample).unwrap();
    project.manifest["entry"] = json!("main");
    project.stories.clear();
    project.stories.insert("main".into(), story(json!([
        {"type":"show","character":"user:showcase.lin_deng","position":"R1","portrait":"normal"},
        {"type":"background","action":"show","image":"user:showcase.courier_station","fade":0.2},
        {"type":"sound","kind":"sound","name":"user:showcase.lantern_chime"}
    ])));
    // The sample's trigger points into a chapter absent from this focused fixture.
    project.manifest["campaign"]["triggers"] = json!([]);
    project
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("node_free_triggers");
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("nested-content.lommod");
    project.export(&output).unwrap();
    let entries = lom_core::package::read_package(&output).unwrap();
    for prefix in [
        "assets/user/character/showcase.lin_deng/",
        "assets/user/image/showcase.courier_station/",
        "assets/user/audio/showcase.lantern_chime/",
    ] {
        assert!(
            entries
                .keys()
                .any(|path| path.starts_with(prefix) && path.ends_with("content.json")),
            "missing {prefix}"
        );
        assert!(
            entries
                .keys()
                .any(|path| path.starts_with(prefix) && !path.ends_with("content.json")),
            "missing content bytes for {prefix}"
        );
    }
    let stats = release::calculate_project_statistics(&project.stories, None);
    assert_eq!(stats["nodes"], 2);
    assert_eq!(stats["node_types"]["parallel"], 1);
    assert_eq!(stats["parallel_actions"], 3);
    assert_eq!(stats["characters"], 1);
}
