use lom_core::{content_edit, editing, project::Project};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};
fn project() -> Project {
    let mut p = Project::new();
    p.stories = BTreeMap::from([
        (
            "source".into(),
            json!({"id":"source","start":"say1","nodes":[{"id":"say1","type":"say","character":"user:demo.hero","text":"Straße 月下相逢","goto":"choice1"},{"id":"choice1","type":"choice","options":[{"text":"loop","goto":"say1"},{"text":"outside","goto":"end1"}]},{"id":"end1","type":"end","next_script":"target"}]}),
        ),
        (
            "target".into(),
            json!({"id":"target","start":"say1","nodes":[{"id":"say1","type":"say","text":"existing"},{"id":"end1","type":"end"}]}),
        ),
    ]);
    p
}
#[test]
fn search_and_references_include_nested_fields_and_manifest() {
    let p = project();
    let idx = editing::index_project(&p.stories);
    assert_eq!(editing::search(&idx, "STRASSE 月下", "text").len(), 1);
    assert_eq!(editing::search(&idx, "DEMO HERO", "character").len(), 1);
    let m = json!({"entry":"target","campaign":{"triggers":[{"script":"source","when_flag_set":"FLAG_A","when_affinity":{"character":"user:demo.hero","min":2}}]}});
    assert_eq!(
        editing::references(&p.stories, &m, "story", "target", None).len(),
        2
    );
    assert!(editing::references(&p.stories, &m, "node", "target", None).is_empty());
    assert_eq!(
        editing::references(&p.stories, &m, "flag", "FLAG_A", None)[0].node_id,
        None
    );
    assert_eq!(
        editing::references(&p.stories, &m, "character", "user:demo.hero", None).len(),
        2
    );
}
#[test]
fn transfer_retargets_internal_links_warns_external_and_is_transactional() {
    let p = project();
    let before = p.clone();
    let t = editing::transfer(&p.stories, "source", 0, 1, "target", 1).unwrap();
    assert_eq!(t.id_mapping["say1"], "say2");
    assert_eq!(t.after["nodes"][2]["options"][0]["goto"], "say2");
    assert_eq!(t.after["nodes"][2]["options"][1]["goto"], "end1");
    assert!(!t.warnings.is_empty());
    assert_eq!(p, before);
    assert!(editing::transfer(&p.stories, "source", 0, 99, "target", 1).is_err());
    assert!(editing::transfer(&p.stories, "source", 0, 1, "source", 0).is_err());
}
const KEY: &str = "assets/user/character/demo.hero/content.json";
fn with_content() -> Project {
    let mut p = project();
    p.assets.insert(KEY.into(),serde_json::to_vec(&json!({"content_schema":1,"id":"demo.hero","type":"character","name":"Hero","files":{"main":"normal.png"},"portraits":{"normal":"normal.png"}})).unwrap());
    p.assets.insert(
        "assets/user/character/demo.hero/normal.png".into(),
        vec![1, 2, 3],
    );
    p
}
#[test]
fn content_edit_failure_leaves_assets_and_referenced_deletion_is_rejected() {
    let mut p = with_content();
    let before = p.clone();
    let mut m: Value = serde_json::from_slice(&p.assets[KEY]).unwrap();
    m["files"]["main"] = json!("missing.png");
    assert!(content_edit::update(&mut p, KEY, &m, &BTreeMap::new()).is_err());
    assert_eq!(p, before);
    assert!(content_edit::remove(&mut p, KEY).is_err());
    assert_eq!(p, before);
    m["files"]["main"] = json!("replacement.png");
    m["portraits"]["happy"] = json!("replacement.png");
    content_edit::update(
        &mut p,
        KEY,
        &m,
        &BTreeMap::from([("replacement.png".into(), vec![9, 8, 7])]),
    )
    .unwrap();
    assert!(p
        .assets
        .contains_key("assets/user/character/demo.hero/replacement.png"));
    p.stories.clear();
    content_edit::remove(&mut p, KEY).unwrap();
    assert!(p.assets.is_empty());
}
#[test]
fn shared_content_replace_and_delete_keep_recoverable_backup() {
    let d = tempfile::tempdir().unwrap();
    let p = with_content();
    assert!(content_edit::store_shared(d.path(), &p, KEY, false)
        .unwrap()
        .is_none());
    assert!(content_edit::store_shared(d.path(), &p, KEY, false).is_err());
    let backup = content_edit::store_shared(d.path(), &p, KEY, true)
        .unwrap()
        .unwrap();
    assert!(backup.join("normal.png").is_file());
    let removed = content_edit::remove_shared(d.path(), "demo.hero").unwrap();
    assert!(removed.join("content.json").is_file());
    assert!(!d.path().join(KEY).exists());
}
#[cfg(unix)]
#[test]
fn shared_content_cannot_follow_parent_symlink() {
    let d = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("assets/user")).unwrap();
    std::os::unix::fs::symlink(outside.path(), d.path().join("assets/user/character")).unwrap();
    assert!(content_edit::store_shared(d.path(), &with_content(), KEY, true).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
#[test]
fn symbol_analysis_includes_block_variables_and_trigger_flags() {
    let stories = BTreeMap::from([(
        "main".into(),
        json!({"id":"main","start":"block1","nodes":[{"id":"block1","type":"block","flowchart":"common","name":"x","vars":[{"name":"score","value":"1"}]},{"id":"end1","type":"end"}]}),
    )]);
    let result = lom_core::analysis::analyze_project(
        &stories,
        &json!({"campaign":{"triggers":[{"script":"main","when_flag_set":"FLAG_A"}]}}),
    );
    let text = result.to_string();
    assert!(text.contains("score"));
    assert!(text.contains("manifest.campaign.triggers[0].when_flag_set"));
}
#[test]
fn content_removal_honors_manifest_character_references() {
    let mut p = with_content();
    p.stories.clear();
    p.manifest = json!({"campaign":{"triggers":[{"script":"main","when_affinity":{"character":"user:demo.hero","min":1}}]}});
    assert!(content_edit::remove(&mut p, KEY).is_err());
    assert!(p.assets.contains_key(KEY));
}
#[test]
fn authoring_batch_rejects_non_objects_without_panicking() {
    assert!(lom_core::story_api::apply(&project().stories["source"], &json!([42])).is_err());
    assert!(
        lom_core::story_api::apply(&project().stories["source"], &json!([{"op":"new_story"}]))
            .is_err()
    );
}
#[test]
fn campaign_flags_share_mod_session_namespace_without_claiming_entry_state() {
    let stories = BTreeMap::from([(
        "main".into(),
        json!({"id":"main","start":"set","nodes":[{"id":"set","type":"flag","flag":"READY","value":1},{"id":"end","type":"end"}]}),
    )]);
    let r = lom_core::analysis::analyze_project(
        &stories,
        &json!({"entry":"main","campaign":{"triggers":[{"script":"main","when_flag_set":"READY"}]}}),
    );
    let s = r["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "READY")
        .unwrap();
    assert_eq!(s["kind"], "mod_flag");
    assert_eq!(s["reads"], 1);
    assert_eq!(s["writes"], 1);
    assert_eq!(s["unused"], false);
    assert!(s["possibly_read_before_write"].is_null());
}
