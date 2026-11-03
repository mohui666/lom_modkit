use lom_core::{
    codegen, content, content_edit, content_library, package, project::Project, release, validate,
};
use serde_json::{json, Value};

fn metadata(kind: &str, id: &str) -> Value {
    let mut meta = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":"演示内容","files":{"main":"icon.png"}});
    meta.as_object_mut().unwrap().extend(
        content::gameplay_defaults(kind)
            .as_object()
            .unwrap()
            .clone(),
    );
    meta
}
fn fixture() -> Project {
    let mut project = Project::new();
    for (kind, id) in [
        ("item", "demo.token"),
        ("martial", "demo.manual"),
        ("support", "demo.friend"),
        ("skill", "demo.strike"),
    ] {
        let meta = metadata(kind, id);
        let dir = content::package_content_dir(kind, id).unwrap();
        project.assets.insert(
            format!("{dir}/content.json"),
            lom_core::stable_json(&meta).unwrap(),
        );
        project
            .assets
            .insert(format!("{dir}/icon.png"), b"test icon".to_vec());
    }
    project.stories.insert("main".into(),json!({"story_schema":2,"id":"main","start":"grant","nodes":[
        {"id":"grant","type":"item","kind":"misc","item":"user:demo.token","count":2},
        {"id":"book","type":"item","kind":"book","item":"user:demo.manual"},
        {"id":"duel","type":"combat","character":"player","player_supports":["user:demo.friend"],"enemy_supports":["user:demo.friend"],"player_skills":["user:demo.strike"],"enemy_skills":["user:demo.strike"],"win":"done","lose":"done"},
        {"id":"done","type":"end"}
    ]}));
    project
}

#[test]
fn gameplay_metadata_roundtrips_and_rejects_unusable_definitions() {
    for kind in content::GAMEPLAY_TYPES {
        let meta = metadata(kind, "demo.test");
        let normalized = content::normalize_content_metadata(&meta).unwrap();
        let payload = content::content_metadata_payload(&normalized);
        assert_eq!(
            normalized,
            content::normalize_content_metadata(&payload).unwrap()
        );
    }
    let mut invalid = metadata("martial", "demo.test");
    invalid["levels"][0]["bonuses"][0]["stat"] = json!("invented-stat");
    assert!(content::normalize_content_metadata(&invalid).is_err());
    invalid = metadata("support", "demo.test");
    invalid["effects"][0]["value"] = json!(-1);
    assert!(content::normalize_content_metadata(&invalid).is_err());
    invalid = metadata("support", "demo.test");
    invalid["chance"] = json!(101);
    assert!(content::normalize_content_metadata(&invalid).is_err());
    invalid = metadata("item", "demo.test");
    invalid["category"] = json!("consumable");
    assert!(content::normalize_content_metadata(&invalid).is_err());
}

#[test]
fn gameplay_content_is_packaged_checked_and_requires_host_1_4() {
    let mut project = fixture();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("project");
    project.save_to(&source).unwrap();
    let path = project.export(&root.path().join("project.lommod")).unwrap();
    let report = package::inspect_package(&path).unwrap();
    assert_eq!(report["manifest"]["min_host_version"], "1.4.0");
    let names = report["entries"].as_array().unwrap();
    for kind in content::GAMEPLAY_TYPES {
        assert!(names.iter().any(|entry| entry["path"]
            .as_str()
            .unwrap()
            .starts_with(&format!("assets/user/{kind}/"))));
    }
    let lua = codegen::compile_story(&project.stories["main"], None, None, None).unwrap();
    assert!(lua.contains("mod_item(\"misc\", \"user:demo.token\", 2)"));
    assert!(lua.contains("player_supports=user:demo.friend"));
    let used = release::referenced_asset_paths(
        &project.stories,
        &project.assets.keys().cloned().collect::<Vec<_>>(),
    );
    assert_eq!(used.len(), 8);
    let issues = release::run_preflight(&project, release::Profile::Release, "1.4.0");
    assert!(
        !issues.iter().any(|issue| issue.severity == "error"),
        "{issues:?}"
    );
    project.stories.get_mut("main").unwrap()["nodes"][0]["kind"] = json!("special");
    project.save_to(&source).unwrap();
    assert!(project.export(&root.path().join("wrong.lommod")).is_err());
}

#[test]
fn gameplay_content_shares_and_references_prevent_removal() {
    let mut project = fixture();
    let root = tempfile::tempdir().unwrap();
    let imported = tempfile::tempdir().unwrap();
    for (kind, id) in [
        ("item", "demo.token"),
        ("martial", "demo.manual"),
        ("support", "demo.friend"),
        ("skill", "demo.strike"),
    ] {
        let key = format!("assets/user/{kind}/{id}/content.json");
        assert!(content_edit::remove(&mut project, &key).is_err());
        content_edit::store_shared(root.path(), &project, &key, false).unwrap();
        assert_eq!(
            content_library::get_content(root.path(), id)
                .unwrap()
                .content_type,
            kind
        );
        let pack = root.path().join(format!("{id}.lomcontent"));
        content_library::export_content_pack(
            root.path(),
            &pack,
            id,
            "1.0.0",
            "tester",
            "MIT",
            &json!([]),
        )
        .unwrap();
        let report = content_library::import_content_pack(imported.path(), &pack).unwrap();
        assert_eq!(report.content_type, kind);
        assert_eq!(
            content_library::get_content(root.path(), id)
                .unwrap()
                .metadata,
            content_library::get_content(imported.path(), id)
                .unwrap()
                .metadata
        );
    }
    assert_eq!(
        content_library::list_contents(root.path()).unwrap().len(),
        4
    );
    let mut story = project.stories["main"].clone();
    story["nodes"][2]["player_supports"] = json!(["user:../bad"]);
    assert!(validate::validate_story(&story).is_err());
    story["nodes"][2]["player_supports"] = json!(["user:demo.friend", "user:demo.friend"]);
    assert!(validate::validate_story(&story).is_err());
}

#[test]
fn consumables_are_explicit_about_where_their_effects_can_run() {
    let mut item = metadata("item", "demo.tonic");
    item["category"] = json!("consumable");
    item["effects"] = json!([{"type":"stat","stat":"life","value":10}]);
    let normalized = content::normalize_content_metadata(&item).unwrap();
    assert_eq!(normalized["use_in"], "field");
    let original = item.clone();
    for effects in [
        json!([]),
        json!([{"type":"heal","value":10}]),
        json!([{"type":"stat","stat":"life","value":0}]),
        json!([{"type":"stat","stat":"life","value":10,"target":"enemy"}]),
    ] {
        item["effects"] = effects;
        assert!(content::normalize_content_metadata(&item).is_err());
    }
    item = original;
    item["use_in"] = json!("combat");
    item["effects"] = json!([{"type":"heal","value":30},{"type":"stamina","value":20}]);
    content::normalize_content_metadata(&item).unwrap();
    item["effects"] = json!([{"type":"stat","stat":"life","value":10}]);
    assert!(content::normalize_content_metadata(&item).is_err());
}

#[test]
fn script_use_rejects_combat_items_and_shops_do_not_accept_consumables() {
    let mut project = Project::new();
    let mut tonic = metadata("item", "demo.tonic");
    tonic["category"] = json!("consumable");
    tonic["effects"] = json!([{"type":"stat","stat":"life","value":10}]);
    let key = "assets/user/item/demo.tonic/content.json";
    project
        .assets
        .insert(key.into(), lom_core::stable_json(&tonic).unwrap());
    project.assets.insert(
        "assets/user/item/demo.tonic/icon.png".into(),
        b"icon".to_vec(),
    );
    project.stories.insert("main".into(),json!({"id":"main","start":"grant","nodes":[
        {"id":"grant","type":"item","kind":"consumable","item":"user:demo.tonic","count":2},
        {"id":"use","type":"use_item","item":"user:demo.tonic"},
        {"id":"check","type":"item_check","category":"consumable","item":"user:demo.tonic","success":"done","failure":"done"},
        {"id":"done","type":"end"}
    ]}));
    let root = tempfile::tempdir().unwrap();
    project.export(&root.path().join("tonic.lommod")).unwrap();
    let lua = codegen::compile_story(&project.stories["main"], None, None, None).unwrap();
    assert!(lua.contains("mod_use_item(\"user:demo.tonic\")"));
    assert!(lua.contains("mod_item(\"consumable\", \"user:demo.tonic\", 2)"));
    tonic["use_in"] = json!("combat");
    tonic["effects"] = json!([{"type":"heal","value":30}]);
    project
        .assets
        .insert(key.into(), lom_core::stable_json(&tonic).unwrap());
    let error = project
        .export(&root.path().join("combat_script.lommod"))
        .unwrap_err();
    assert!(format!("{error:#}").contains("use_item 只能同步使用 field"));
    let shop = json!({"id":"main","start":"shop","nodes":[{"id":"shop","type":"custom_shop","items":[{"category":"consumable","item":"user:demo.tonic","count":1}]},{"id":"done","type":"end"}]});
    assert!(validate::validate_story(&shop).is_err());
    for (kind, category) in [("item", "kind"), ("item_check", "category")] {
        let mut node =
            json!({"id":"test","type":kind,"item":"1234","success":"done","failure":"done"});
        node[category] = json!("consumable");
        if kind == "item" {
            node.as_object_mut().unwrap().remove("success");
            node.as_object_mut().unwrap().remove("failure");
        }
        assert!(validate::validate_story(
            &json!({"id":"main","start":"test","nodes":[node,{"id":"done","type":"end"}]})
        )
        .is_err());
    }
}

#[test]
fn skill_animation_frames_roundtrip_and_are_all_packaged() {
    let mut project = fixture();
    let mut skill = metadata("skill", "demo.strike");
    skill["animation"] = json!({"frames":["frame_01.png","frame_02.png","frame_01.png"],"fps":12,"hit_frame":1,"scale":100});
    skill["effects"] = json!([{"type":"damage","target":"enemy","value":5}]);
    let checked = content::normalize_content_metadata(&skill).unwrap();
    assert_eq!(content::listed_content_files(&checked).len(), 3);
    let prefix = "assets/user/skill/demo.strike/";
    project.assets.insert(
        format!("{prefix}content.json"),
        lom_core::stable_json(&content::content_metadata_payload(&checked)).unwrap(),
    );
    for name in ["frame_01.png", "frame_02.png"] {
        project
            .assets
            .insert(format!("{prefix}{name}"), b"frame".to_vec());
    }
    let root = tempfile::tempdir().unwrap();
    let path = project.export(&root.path().join("skill.lommod")).unwrap();
    let report = package::inspect_package(&path).unwrap();
    for name in ["frame_01.png", "frame_02.png"] {
        assert!(report["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == format!("{prefix}{name}")));
    }
    let lua = codegen::compile_story(&project.stories["main"], None, None, None).unwrap();
    assert!(lua.contains("player_skills=user:demo.strike"));
    let library = root.path().join("library");
    content_edit::store_shared(&library, &project, &format!("{prefix}content.json"), false)
        .unwrap();
    let pack = root.path().join("skill.lomcontent");
    content_library::export_content_pack(
        &library,
        &pack,
        "demo.strike",
        "1.0.0",
        "tester",
        "MIT",
        &json!([]),
    )
    .unwrap();
    let imported = root.path().join("imported");
    content_library::import_content_pack(&imported, &pack).unwrap();
    assert_eq!(
        content_library::get_content(&imported, "demo.strike")
            .unwrap()
            .metadata["animation"]["frames"],
        skill["animation"]["frames"]
    );
    for animation in [
        json!({"frames":[]}),
        json!({"frames":["../escape.png"]}),
        json!({"frames":["not_png.jpg"]}),
        json!({"frames":["frame.png"],"hit_frame":1}),
        json!({"frames":["frame.png"],"fps":61}),
    ] {
        skill["animation"] = animation;
        assert!(content::normalize_content_metadata(&skill).is_err());
    }
}

#[test]
fn gameplay_upgrade_requires_authors_to_review_an_existing_tested_version() {
    let fresh = Project::new_scenario("addon");
    assert_eq!(
        fresh.manifest["tested_host_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        fresh.manifest["min_host_version"],
        lom_core::scenario::MIN_HOST
    );
    let mut old = fixture();
    old.manifest["tested_host_version"] = json!("1.3.10");
    let before = old.clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("review.lommod");
    let error = old.export(&path).unwrap_err();
    assert!(error.to_string().contains("Host 1.4.0"));
    assert!(error.to_string().contains("已测试 Host 版本"));
    assert!(!path.exists());
    assert_eq!(old, before);
    old.manifest["tested_host_version"] = json!("1.4.0");
    old.export(&path).unwrap();
    let report = package::inspect_package(&path).unwrap();
    assert_eq!(report["manifest"]["min_host_version"], "1.4.0");
    assert_eq!(report["manifest"]["tested_host_version"], "1.4.0");
}

#[test]
fn scenario_only_custom_rewards_are_packaged_and_protected() {
    let mut project = Project::new_scenario("addon");
    let meta = metadata("item", "demo.reward");
    let key = "assets/user/item/demo.reward/content.json";
    project
        .assets
        .insert(key.into(), lom_core::stable_json(&meta).unwrap());
    project.assets.insert(
        "assets/user/item/demo.reward/icon.png".into(),
        b"icon".to_vec(),
    );
    project.scenario.as_mut().unwrap()["events"][0]["outcomes"]["completed"]["actions"] = json!([
        {"type":"item","key":"user:demo.reward","kind":"misc","op":"add","value":1}
    ]);
    assert!(content_edit::remove(&mut project, key).is_err());
    let dir = tempfile::tempdir().unwrap();
    let packed = project.export(&dir.path().join("addon.lommod")).unwrap();
    let report = package::inspect_package(&packed).unwrap();
    assert_eq!(report["manifest"]["min_host_version"], "1.4.0");
    assert!(report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["path"] == key));
    let issues = release::run_preflight(&project, release::Profile::Release, "1.4.0");
    assert!(
        !issues
            .iter()
            .any(|issue| issue.severity == "error" || issue.code == "unused_content"),
        "{issues:?}"
    );
}

#[test]
fn activity_only_custom_reward_is_included() {
    let mut project = fixture();
    project.stories.get_mut("main").unwrap()["nodes"][0] = json!({"id":"grant","type":"activity","kind":"custom","stat":"internal","op":">=","value":0,"success":"book","failure":"book","success_rewards":[{"kind":"item","category":"misc","key":"user:demo.token","amount":2}]});
    let dir = tempfile::tempdir().unwrap();
    let path = project.export(&dir.path().join("activity.lommod")).unwrap();
    let report = package::inspect_package(&path).unwrap();
    assert!(report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["path"] == "assets/user/item/demo.token/content.json"));
    let lua = codegen::compile_story(&project.stories["main"], None, None, None).unwrap();
    assert!(lua.contains("mod_item(\"misc\", \"user:demo.token\", 2)"));
}
