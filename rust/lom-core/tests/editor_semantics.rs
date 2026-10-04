//! Data ownership and conservative-analysis regressions from the editor audit.
use lom_core::{analysis, project::Project};
use serde_json::json;
use std::{collections::BTreeMap, fs};

#[test]
fn new_campaign_identity_is_unique_valid_and_persisted_without_rewriting_imports() {
    let mut first = Project::new();
    let second = Project::new();
    let identity = first.manifest["campaign_id"].as_str().unwrap().to_owned();
    assert_ne!(
        first.manifest["campaign_id"],
        second.manifest["campaign_id"]
    );
    assert!(regex::Regex::new(r"^[a-z0-9_-]{1,64}$")
        .unwrap()
        .is_match(&identity));
    let dir = tempfile::tempdir().unwrap();
    first.save_to(dir.path()).unwrap();
    assert_eq!(first.manifest["campaign_id"], identity);
    assert_eq!(
        Project::open(dir.path()).unwrap().manifest["campaign_id"],
        identity
    );
    first.manifest["campaign_id"] = json!("existing_campaign");
    first.save_to(dir.path()).unwrap();
    assert_eq!(
        Project::open(dir.path()).unwrap().manifest["campaign_id"],
        "existing_campaign"
    );
    first
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("campaign_id");
    first.save_to(dir.path()).unwrap();
    assert!(Project::open(dir.path())
        .unwrap()
        .manifest
        .get("campaign_id")
        .is_none());
    fs::remove_file(dir.path().join("manifest.json")).unwrap();
    assert!(Project::open(dir.path())
        .unwrap()
        .manifest
        .get("campaign_id")
        .is_none());
    assert!(Project::open(&dir.path().join("story/main.json"))
        .unwrap()
        .manifest
        .get("campaign_id")
        .is_none());
}

#[test]
fn standalone_story_save_persists_asset_changes_at_the_original_root() {
    for subdir in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let folder = if subdir {
            dir.path().join("story")
        } else {
            dir.path().to_path_buf()
        };
        fs::create_dir_all(&folder).unwrap();
        let file = folder.join("main.json");
        let mut original = Project::new();
        original.save_to(&file).unwrap();
        let asset = dir.path().join("assets/user/image/test.picture/main.png");
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(&asset, b"original").unwrap();
        let mut p = Project::open(&file).unwrap();
        p.assets.insert(
            "assets/user/image/test.picture/main.png".into(),
            b"changed".to_vec(),
        );
        p.assets.insert("assets/new.png".into(), b"new".to_vec());
        p.stories.get_mut("main").unwrap()["title"] = json!("保存后的章节");
        p.save_to(&file).unwrap();
        let reloaded = Project::open(&file).unwrap();
        assert_eq!(
            reloaded.assets["assets/user/image/test.picture/main.png"],
            b"changed"
        );
        assert_eq!(reloaded.assets["assets/new.png"], b"new");
        assert_eq!(reloaded.stories["main"]["title"], "保存后的章节");
    }
}
#[test]
fn standalone_save_as_collision_preserves_existing_files_and_source_state() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(second.join("assets")).unwrap();
    let mut p = Project::new();
    p.save_to(&first.join("main.json")).unwrap();
    p.assets
        .insert("assets/picture.png".into(), b"owned".to_vec());
    let foreign = second.join("assets/picture.png");
    fs::write(&foreign, b"foreign").unwrap();
    let before = p.clone();
    assert!(p.save_to(&second.join("main.json")).is_err());
    assert_eq!(p, before);
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign");
    assert!(!second.join("main.json").exists());
}
#[cfg(unix)]
#[test]
fn standalone_save_rejects_symlink_assets_without_writing_story() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.json");
    let mut p = Project::new();
    p.save_to(&source).unwrap();
    let before = fs::read(&source).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("picture.png"), b"foreign").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("assets")).unwrap();
    p.stories.get_mut("main").unwrap()["title"] = json!("not saved");
    p.assets
        .insert("assets/picture.png".into(), b"changed".to_vec());
    assert!(p.save_to(&source).is_err());
    assert_eq!(fs::read(&source).unwrap(), before);
    assert_eq!(
        fs::read(outside.path().join("picture.png")).unwrap(),
        b"foreign"
    );
}
#[test]
fn covered_boolean_branch_has_no_implicit_fallthrough() {
    for source in ["mod", "condition"] {
        let s = json!({"start":"branch","nodes":[{"id":"branch","type":"branch","source":source,"flag":"x","cases":[{"value":1,"goto":"a"},{"value":2,"goto":"b"}]},{"id":"unexpected","type":"end"},{"id":"a","type":"end"},{"id":"b","type":"end"}]});
        assert_eq!(
            analysis::successors(&s, 0),
            vec!["a".to_owned(), "b".to_owned()]
        );
        let report = analysis::analyze_project(
            &BTreeMap::from([("main".into(), s)]),
            &json!({"entry":"main"}),
        );
        assert!(report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["code"] == "unreachable" && i["node"] == "unexpected"));
    }
}
#[test]
fn gameplay_continuations_do_not_count_as_final_endings() {
    for node in [
        json!({"id":"loop","type":"combat","win":"loop","lose":"loop"}),
        json!({"id":"loop","type":"stat_check","success":"loop","failure":"loop"}),
    ] {
        let stories = BTreeMap::from([("main".into(), json!({"start":"loop","nodes":[node]}))]);
        let report = analysis::analyze_project(&stories, &json!({"entry":"main"}));
        assert!(report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["code"] == "missing_ending"));
    }
}
#[test]
fn raw_flag_mutation_prevents_constant_condition_proof() {
    let story = json!({"start":"set","nodes":[{"id":"set","type":"flag","flag":"x"},{"id":"raw","type":"raw","code":"modflags.x = nil"},{"id":"check","type":"branch","flag":"x","cases":[{"value":1,"goto":"end"},{"value":2,"goto":"end"}]},{"id":"end","type":"end"}]});
    let report = analysis::analyze_project(
        &BTreeMap::from([("main".into(), story)]),
        &json!({"entry":"main"}),
    );
    assert_eq!(report["conditions"][0]["proof"], "unknown");
    assert!(!report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["code"] == "dead_branch"));
}
#[test]
fn offline_assertion_treats_unset_mod_flag_as_false() {
    let stories = BTreeMap::from([(
        "main".into(),
        json!({"start":"end","nodes":[{"id":"end","type":"end"}]}),
    )]);
    let result = analysis::run_story_tests(
        &stories,
        &json!([{"name":"unset","story":"main","assert":{"flags":{"never_set":false}}}]),
    )
    .unwrap();
    assert_eq!(result[0]["status"], "pass");
}
