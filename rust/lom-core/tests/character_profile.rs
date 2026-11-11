use lom_core::{
    character_profile, codegen, content, package, project::Project, release, story_api, validate,
};
use serde_json::{json, Value};

const ID: &str = "user:profile.person";
const METADATA: &str = "assets/user/character/profile.person/content.json";
fn metadata() -> Value {
    json!({"schema":1,"id":"profile.person","type":"character","name":"自定义人物",
        "files":{"main":"normal.png"},"portraits":{"normal":"normal.png"}})
}
fn add_character(project: &mut Project, metadata: &Value) {
    project
        .assets
        .insert(METADATA.into(), serde_json::to_vec(metadata).unwrap());
    project.assets.insert(
        "assets/user/character/profile.person/normal.png".into(),
        include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
        )
        .to_vec(),
    );
}
fn story(node: Value) -> Value {
    let mut node = node;
    node["id"] = json!("profile");
    node["type"] = json!("character_profile");
    json!({"id":"main","start":"profile","nodes":[node,{"id":"done","type":"end"}]})
}
fn pages() -> Value {
    json!([{"text":"  第1页\n保留空白。 ","unlock_level":0},
        {"text":"第2页","unlock_level":3},{"text":"","unlock_level":10},{"text":"第2页","unlock_level":3}])
}

#[test]
fn profile_metadata_retains_text_empty_drafts_repeated_pages_and_optional_defaults() {
    let original = metadata();
    let old =
        content::content_metadata_payload(&content::normalize_content_metadata(&original).unwrap());
    assert!(old.get("initially_known").is_none() && old.get("biography_pages").is_none());
    let mut raw = original;
    raw["initially_known"] = json!(false);
    raw["biography_pages"] = pages();
    let normalized = content::normalize_content_metadata(&raw).unwrap();
    let payload = content::content_metadata_payload(&normalized);
    assert_eq!(payload["biography_pages"], raw["biography_pages"]);
    assert_eq!(payload["initially_known"], false);
    raw["biography_pages"] = json!([]);
    assert_eq!(
        content::normalize_content_metadata(&raw).unwrap()["biography_pages"],
        json!([])
    );
    let mut legacy = metadata();
    legacy["biography"] = json!("旧版单页传记");
    let payload =
        content::content_metadata_payload(&content::normalize_content_metadata(&legacy).unwrap());
    assert_eq!(payload["biography"], legacy["biography"]);
    assert!(payload.get("biography_pages").is_none());
    legacy["biography_pages"] = json!([]);
    let normalized = content::normalize_content_metadata(&legacy).unwrap();
    character_profile::validate_reference(
        &json!({"character":ID,"op":"unlock_biography","page":1}),
        &normalized,
    )
    .unwrap();
}

#[test]
fn profile_metadata_validates_native_types_limits_and_ownership() {
    for bad in [
        json!(null),
        json!({}),
        json!("text"),
        Value::Array(vec![json!({"text":"text","unlock_level":0}); 5]),
        json!([{"text":7,"unlock_level":0}]),
        json!([{"text":"text"}]),
        json!([{"text":"text","unlock_level":-1}]),
        json!([{"text":"text","unlock_level":11}]),
        json!([{"text":"text","unlock_level":2.5}]),
        json!([{"text":"text","unlock_level":"3"}]),
        json!([{"text":"text","unlock_level":0,"other":true}]),
    ] {
        let mut raw = metadata();
        raw["biography_pages"] = bad;
        assert!(
            content::normalize_content_metadata(&raw).is_err(),
            "accepted {}",
            raw["biography_pages"]
        );
    }
    for bad in [json!(null), json!(0), json!("false")] {
        let mut raw = metadata();
        raw["initially_known"] = bad;
        assert!(content::normalize_content_metadata(&raw).is_err());
    }
    let mut extension = metadata();
    extension["extends"] = json!("player");
    extension["initially_known"] = json!(true);
    assert!(content::normalize_content_metadata(&extension).is_err());
    let mut image = metadata();
    image["type"] = json!("image");
    image["biography_pages"] = json!([]);
    assert!(content::normalize_content_metadata(&image).is_err());
}

#[test]
fn profile_nodes_compile_exact_bridge_and_reject_invalid_operations_and_original_people() {
    for op in character_profile::OPERATIONS {
        let mut node = json!({"character":ID,"op":op});
        let page = if matches!(*op, "unlock_biography" | "lock_biography") {
            node["page"] = json!(4);
            4
        } else {
            0
        };
        let input = story(node);
        validate::validate_story(&input).unwrap();
        let lua = codegen::compile_story(&input, None, None, None).unwrap();
        assert!(lua.contains(&format!(
            "mod_character_profile(\"{ID}\", \"{op}\", {page})"
        )));
        assert!(lua.contains("Host 1.5.10"));
        assert!(!lua.contains("characters.LoadCharacterAsset"));
        assert!(release::required_character(&input["nodes"][0]).is_none());
    }
    for node in [
        json!({"character":"player","op":"meet"}),
        json!({"character":"user:../bad","op":"meet"}),
        json!({"character":ID,"op":"unknown"}),
        json!({"character":ID,"op":"unlock_biography"}),
        json!({"character":ID,"op":"meet","page":1}),
        json!({"character":ID,"op":"forget","page":null}),
    ] {
        assert!(validate::validate_story(&story(node)).is_err());
    }
    for page in [json!(0), json!(5), json!(1.5), json!("1")] {
        assert!(validate::validate_story(&story(
            json!({"character":ID,"op":"lock_biography","page":page})
        ))
        .is_err());
    }
    assert_eq!(
        story_api::new_node("character_profile", "profile").unwrap()["character"],
        ""
    );
}

#[test]
fn object_api_switches_operation_without_staging_or_silently_accepting_a_page_for_meet() {
    let original = json!({"id":"main","start":"done","nodes":[{"id":"done","type":"end"}]});
    let added = story_api::execute(
        "add_node",
        &json!({"story":original,"node_type":"character_profile",
        "fields":{"character":ID,"op":"unlock_biography","page":2}}),
    )
    .unwrap();
    assert_eq!(added["after"]["nodes"].as_array().unwrap().len(), 2);
    assert!(!added["after"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["type"] == "show"));
    let changed = story_api::execute(
        "update_node",
        &json!({"story":added["after"],"node_id":added["result"]["id"],
        "fields":{"op":"meet"}}),
    )
    .unwrap();
    assert!(changed["result"].get("page").is_none());
    assert!(story_api::execute(
        "add_node",
        &json!({"story":original,"node_type":"character_profile",
        "fields":{"character":ID,"op":"meet","page":1}})
    )
    .is_err());
}

#[test]
fn profile_only_reference_bundles_character_and_new_metadata_requires_current_host() {
    let mut project = Project::new();
    add_character(&mut project, &metadata());
    project
        .stories
        .insert("main".into(), story(json!({"character":ID,"op":"meet"})));
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("profile.lommod");
    project.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert!(reopened.assets.contains_key(METADATA));
    assert!(reopened
        .assets
        .contains_key("assets/user/character/profile.person/normal.png"));
    assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
    for only_known in [true, false] {
        let mut project = Project::new();
        let mut meta = metadata();
        if only_known {
            meta["initially_known"] = json!(false);
        } else {
            meta["biography_pages"] = pages();
        }
        add_character(&mut project, &meta);
        project.export(&output).unwrap();
        let reopened = Project::open(&output).unwrap();
        assert!(reopened.assets.contains_key(METADATA));
        assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
        let diagnostics = release::run_preflight(&project, release::Profile::Editing, "1.5.10");
        assert!(
            !diagnostics
                .iter()
                .any(|issue| issue.code == "unused_content"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn referenced_page_must_exist_and_extensions_are_reported_at_the_source_step() {
    let mut project = Project::new();
    let mut meta = metadata();
    meta["biography_pages"] = json!([{"text":"page one","unlock_level":1}]);
    add_character(&mut project, &meta);
    project.stories.insert(
        "main".into(),
        story(json!({"character":ID,"op":"unlock_biography","page":2})),
    );
    let diagnostics = release::run_preflight(&project, release::Profile::Editing, "1.5.10");
    assert!(diagnostics
        .iter()
        .any(|issue| issue.code == "invalid_character_profile"
            && issue.story_id == "main"
            && issue.node_id == "profile"));
    let directory = tempfile::tempdir().unwrap();
    let error = project
        .export(&directory.path().join("bad.lommod"))
        .unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("main") && message.contains("profile") && message.contains("仅配置 1 页"),
        "{message}"
    );
    let mut extension = metadata();
    extension["extends"] = json!("player");
    add_character(&mut project, &extension);
    project
        .stories
        .insert("main".into(), story(json!({"character":ID,"op":"meet"})));
    assert!(project
        .export(&directory.path().join("extension.lommod"))
        .is_err());
}

#[test]
fn conflicting_legacy_and_new_pages_remain_openable_editable_and_saved_but_export_explains_conflict(
) {
    let mut project = Project::new();
    let mut meta = metadata();
    meta["biography"] = json!("必须保留的旧传记原稿");
    meta["biography_pages"] = pages();
    assert!(content::normalize_content_metadata(&meta).is_ok());
    add_character(&mut project, &meta);
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("draft");
    project.save_to(&folder).unwrap();
    let original = std::fs::read(folder.join(METADATA)).unwrap();
    let mut editable = Project::open(&folder).unwrap();
    editable.save_to(&folder).unwrap();
    assert_eq!(std::fs::read(folder.join(METADATA)).unwrap(), original);
    let diagnostics = release::run_preflight(&editable, release::Profile::Editing, "1.5.10");
    assert!(diagnostics
        .iter()
        .any(|issue| issue.code == "conflicting_biography"
            && issue.message.contains("user:profile.person")));
    let error = editable
        .export(&directory.path().join("conflict.lommod"))
        .unwrap_err();
    assert!(format!("{error:#}").contains("两份文稿已保留"));
    meta.as_object_mut().unwrap().remove("biography");
    add_character(&mut editable, &meta);
    editable
        .export(&directory.path().join("fixed.lommod"))
        .unwrap();
}

#[test]
fn released_154_project_opens_adds_pages_and_profile_node_saves_exports_reopens_and_rejects_lua_swap(
) {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.4-name-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let mut meta = metadata();
    meta["initially_known"] = json!(false);
    meta["biography_pages"] = pages();
    add_character(&mut project, &meta);
    project.stories.get_mut("main").unwrap()["nodes"].as_array_mut().unwrap().insert(1,
        json!({"id":"unlock_profile","type":"character_profile","character":ID,"op":"unlock_biography","page":4}));
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("edited");
    project.save_to(&folder).unwrap();
    let edited = Project::open(&folder).unwrap();
    let output = directory.path().join("profile.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    let reopened_metadata: Value = serde_json::from_slice(&reopened.assets[METADATA]).unwrap();
    assert_eq!(
        reopened_metadata["biography_pages"],
        meta["biography_pages"]
    );
    assert_eq!(reopened_metadata["initially_known"], false);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua substitution')\n");
    let bad = directory.path().join("tampered.lommod");
    package::write_package(entries, &bad).unwrap();
    assert!(Project::open(&bad).is_err());
}
