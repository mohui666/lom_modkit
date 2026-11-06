use lom_core::{codegen, package, project::Project, validate, vanilla_import, vanilla_patch};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/vanilla-import.json")).unwrap()
}

#[test]
fn imports_native_scopes_without_dropping_branches_choices_or_original_bytes() {
    let fixture = fixture();
    let translations = serde_json::from_value(fixture["translations"].clone()).unwrap();
    let story = vanilla_import::import(
        fixture["source"].as_str().unwrap(),
        "native_import",
        "整章导入与来源判定",
        "DEMO",
        &translations,
    )
    .unwrap();
    assert_eq!(story, fixture["baseline"]);
    let imported = vanilla_import::parse(&story).unwrap().unwrap();
    assert_eq!(imported.source(), fixture["source"]);
    let lua = codegen::compile_story(&story, Some(&json!({"id":"test"})), None, None).unwrap();
    assert!(lua.contains("local total = 3"));
    assert!(lua.contains("mark_progress(total + selected)"));
    assert!(!lua.contains("mod_hide_all()"));
    assert!(!lua.contains("ChangeScene"));
    let crlf = fixture["source"].as_str().unwrap().replace('\n', "\r\n");
    let story = vanilla_import::import(
        &format!("\u{feff}{crlf}"),
        "test",
        "test",
        "DEMO",
        &translations,
    )
    .unwrap();
    assert_eq!(
        vanilla_import::parse(&story).unwrap().unwrap().source(),
        crlf
    );
}

#[test]
fn editable_insertions_export_and_reopen_with_the_native_chapter_routing() {
    let fixture = fixture();
    let mut project = Project::new_vanilla_patch("DEMO").unwrap();
    project.stories.clear();
    project
        .stories
        .insert("native_import".into(), fixture["story"].clone());
    project.manifest["entry"] = json!("native_import");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("import.lommod");
    project.export(&path).unwrap();
    let summary = package::inspect_package(&path).unwrap();
    assert_eq!(summary["source_lua_match"], true);
    let reopened = Project::open(&path).unwrap();
    assert_eq!(reopened.stories, project.stories);
    let lua =
        codegen::compile_story(&fixture["story"], Some(&project.manifest), None, None).unwrap();
    assert!(lua.contains("mod_vanilla_source_node(\"inserted\")"));
    assert!(lua.contains("N_IMPORT_B"));
    assert!(lua.contains("MOD_"));
}

#[test]
fn old_anchored_text_and_conditional_insertions_survive_full_import() {
    let fixture = fixture();
    let mut story = vanilla_patch::new_story("native_import", "DEMO").unwrap();
    story["vanilla_patch"]["edits"] = json!([{
        "find":"say(luamanager.GetStoryText(\"N_IMPORT_A\"))","occurrence":1,"kind":"say",
        "text":"修改甲","before":"新增甲","after":"新增乙",
        "before_condition":{"source":"mod","flag":"extra","op":">=","value":1,"invert":false}
    },{"find":"\"O_ONE\"","occurrence":1,"kind":"option","text":"改选项"}]);
    let imported = vanilla_import::import_patch(
        &story,
        fixture["source"].as_str().unwrap(),
        &serde_json::from_value(fixture["translations"].clone()).unwrap(),
    )
    .unwrap();
    let nodes = imported["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|node| node["text"] == "修改甲"));
    assert!(nodes.iter().any(|node| node["text"] == "新增甲"));
    assert!(nodes.iter().any(|node| node["type"] == "flag_check"));
    let lua = codegen::compile_story(&imported, None, None, None).unwrap();
    assert!(lua.contains("改选项"));
    assert!(lua.contains("mark_progress(total + selected)"));
}

#[test]
fn resuming_original_flow_keeps_the_next_native_step_reachable() {
    let mut story = fixture()["baseline"].clone();
    story["nodes"].as_array_mut().unwrap().insert(
        5,
        json!({"id":"resume","type":"end","resume_original":true}),
    );
    validate::validate_story(&story).unwrap();
    assert_eq!(lom_core::analysis::successors(&story, 5), ["original_6"]);
    let lua = codegen::compile_story(&story, None, None, None).unwrap();
    assert!(lua.contains("node_resume = function()\n\t__lom_enter_custom()"));
    assert!(lua.contains("\treturn node_original_6()"));
}

#[test]
fn cross_scope_jumps_and_native_only_fields_outside_imports_are_rejected() {
    let mut story = fixture()["baseline"].clone();
    story["nodes"][1]["goto"] = json!("original_6");
    assert!(validate::validate_story(&story)
        .unwrap_err()
        .to_string()
        .contains("跨过"));
    let mut normal = json!({"id":"test","start":"end","nodes":[{"id":"end","type":"end","resume_original":true}]});
    assert!(validate::validate_story(&normal).is_err());
    normal["nodes"] = json!([{"id":"end","type":"say","text":"测试","inherit_dialog":true},{"id":"done","type":"end"}]);
    assert!(validate::validate_story(&normal).is_err());
    let raw = vanilla_import::import(
        "local a=1\nreturn a\n",
        "test",
        "test",
        "DEMO",
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(raw["nodes"].as_array().unwrap().len(), 1);
    let lua = codegen::compile_story(&raw, None, None, None).unwrap();
    assert!(lua.ends_with("return a\n\n"));
}
