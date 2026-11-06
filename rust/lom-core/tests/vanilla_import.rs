use lom_core::{codegen, package, project::Project, validate, vanilla_import, vanilla_patch};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/vanilla-import.json")).unwrap()
}

#[test]
fn fixed_f5_import_preloads_constant_character_resources_without_staging_them() {
    let source = r#"
-- characters.Get("comment_only")
local note = "characters.Get('string_only')"
if checkpointmanager.Condition("PREVIEW_BRANCH") then
    stage.show{character=characters.Get("girl7_3"),toPosition="R2",moveDuration=1,useDefaultSettings=false}
else
    stage.showPortrait(characters.Get("brother1"), characters.GetPortrait("brother1", "laugh1"))
end
characters.GetPortrait("girl7_3", requested_portrait)
characters.Get(requested_character)
statmodifymanager.SetFlag("ORIGINAL_FLAG", 1)
"#;
    let story =
        vanilla_import::import(source, "main", "Inherited cast", "DEMO", &BTreeMap::new()).unwrap();
    let before = story.clone();
    let preview = codegen::compile_story(
        &story,
        Some(&json!({"id":"lom_modkit_preview"})),
        None,
        None,
    )
    .unwrap();
    let preamble = preview.split("__lom_nodes[\"").next().unwrap();
    for character in ["girl7_3", "brother1"] {
        assert_eq!(
            preamble
                .matches(&format!(
                    "runwait(characters.LoadCharacterAsset(\"{character}\"))"
                ))
                .count(),
            1
        );
    }
    assert!(preamble.contains("characters.LoadCharacterPortrait(\"brother1\", \"laugh1\")"));
    assert!(!preamble.contains("stage.show{"));
    assert!(preamble.contains("return stage.show(options)"));
    assert!(preamble.contains("options.portrait == nil and not character.State.onScreen and character.State.portrait == nil"));
    assert!(preamble.contains("local portrait = character.Portraits[1]"));
    assert!(!preamble.contains("character.State.portrait = portrait"));
    assert!(!preamble.contains("characters.Get("));
    assert!(!preamble.contains("characters.GetPortrait("));
    assert!(!preamble.contains("SetFlag"));
    assert!(!preamble.contains("comment_only"));
    assert!(!preamble.contains("string_only"));
    assert!(!preamble.contains("LoadCharacterAsset(requested_character)"));
    let formal =
        codegen::compile_story(&story, Some(&json!({"id":"released_mod"})), None, None).unwrap();
    assert!(!formal.contains("LoadCharacterAsset"));
    assert!(!formal.contains("LoadCharacterPortrait"));
    assert!(!formal.contains("__lom_preview_show"));
    assert_eq!(story, before);
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
    assert_eq!(story["vanilla_import"]["schema"], 2);
    assert!(story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["type"] != "raw"));
    assert!(story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["type"] == "native_branch"));
    let imported = vanilla_import::parse(&story).unwrap().unwrap();
    assert_eq!(imported.source(), fixture["source"]);
    let lua = codegen::compile_story(&story, Some(&json!({"id":"test"})), None, None).unwrap();
    assert!(lua.contains("__lom_locals[\"v0\"] = 3"));
    assert!(lua.contains("mark_progress((__lom_locals[\"v0\"] + __lom_locals[\"v2\"]))"));
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
    assert!(lua.contains("mark_progress((__lom_locals[\"v0\"] + __lom_locals[\"v2\"]))"));
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
    assert!(raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["type"] != "raw"));
    let lua = codegen::compile_story(&raw, None, None, None).unwrap();
    assert!(lua.contains("__lom_result = __lom_locals[\"v0\"]"));
}

#[test]
fn structured_elseif_paths_rejoin_without_falling_through_the_other_dialogue() {
    let source = "setsaydialog(saydialogs.narrative)\nlocal options={}\noptions[1]=\"O_LEFT\"\noptions[2]=\"O_RIGHT\"\nlocal selected=choose(options)\nif selected==1 then\nsay(\"left\")\nelseif selected==2 then\nsay(\"right\")\nend\nsay(\"joined\")\n";
    let story = vanilla_import::import(
        source,
        "main",
        "test",
        "DEMO",
        &BTreeMap::from([
            ("O_LEFT".into(), "向左".into()),
            ("O_RIGHT".into(), "向右".into()),
        ]),
    )
    .unwrap();
    let nodes = story["nodes"].as_array().unwrap();
    let left = nodes.iter().find(|n| n["text"] == "left").unwrap();
    let right = nodes.iter().find(|n| n["text"] == "right").unwrap();
    assert_eq!(left["goto"], right["goto"]);
    let branches: Vec<_> = nodes
        .iter()
        .filter(|n| n["type"] == "native_branch")
        .collect();
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[0]["success"], left["id"]);
    assert_eq!(branches[0]["failure"], branches[1]["id"]);
    assert_eq!(branches[1]["success"], right["id"]);
    assert_eq!(branches[1]["failure"], left["goto"]);
    assert_eq!(
        branches[0]["_editor"]["native_condition"],
        json!({"kind":"choice","index":"1","labels":["向左"]})
    );
    assert_eq!(
        branches[1]["_editor"]["native_condition"],
        json!({"kind":"choice","index":"2","labels":["向右"]})
    );
    assert_eq!(
        vanilla_import::parse(&story).unwrap().unwrap().source(),
        source
    );
    let mut story = story;
    for node in story["nodes"].as_array_mut().unwrap() {
        for operation in node["operations"].as_array_mut().into_iter().flatten() {
            if operation["value"]["value"] == "O_LEFT" {
                operation["value"]["value"] = json!("改走左边 {count}");
                operation["value"]
                    .as_object_mut()
                    .unwrap()
                    .remove("display");
            }
        }
    }
    lom_core::native_flow::refresh_branch_labels(&mut story);
    let branch = story["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["type"] == "native_branch")
        .unwrap();
    assert_eq!(
        branch["_editor"]["native_condition"]["labels"],
        json!(["改走左边 {count}"])
    );
    branch["_editor"]["note"] = json!("作者备注");
    branch["condition"] = json!({"kind":"literal","value":true});
    lom_core::native_flow::refresh_branch_labels(&mut story);
    let branch = story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "native_branch")
        .unwrap();
    assert!(branch["_editor"]["native_condition"].is_null());
    assert_eq!(branch["_editor"]["note"], "作者备注");
}

#[test]
fn structured_locals_keep_nested_shadowing_and_empty_chapters_are_valid() {
    let source =
        "local count=1\nif enabled then\nlocal count=count+1\nmark(count)\nend\nmark(count)\n";
    let story = vanilla_import::import(source, "main", "test", "DEMO", &BTreeMap::new()).unwrap();
    let lua = codegen::compile_story(&story, None, None, None).unwrap();
    assert!(lua.contains("__lom_locals[\"v1\"] = (__lom_locals[\"v0\"] + 1)"));
    assert!(lua.contains("mark(__lom_locals[\"v1\"])"));
    assert!(lua.contains("mark(__lom_locals[\"v0\"])"));
    let empty = vanilla_import::import("", "main", "empty", "DEMO", &BTreeMap::new()).unwrap();
    assert_eq!(empty["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(empty["nodes"][0]["type"], "end");
    codegen::compile_story(&empty, None, None, None).unwrap();
}

#[test]
fn copied_option_declarations_get_new_cells_without_rebinding_outer_variables() {
    let source="local amount=1\nlocal options={}\noptions[1]=\"A\"\noptions[2]=\"B\"\nlocal selected=choose(options)\nmark(amount+selected)\n";
    let story = vanilla_import::import(source, "main", "test", "DEMO", &BTreeMap::new()).unwrap();
    let before = story.clone();
    let mut copied = vec![story["nodes"][0].clone()];
    copied[0]["operations"].as_array_mut().unwrap().remove(0);
    lom_core::native_flow::rebind_copied_locals(&mut copied, &story);
    let ops = copied[0]["operations"].as_array().unwrap();
    let table = ops[0]["target"]["symbol"].clone();
    assert_ne!(table, "v1");
    assert_eq!(ops[1]["target"]["object"]["symbol"], table);
    assert_eq!(ops[3]["value"]["args"][0]["symbol"], table);
    assert_eq!(ops[4]["expression"]["args"][0]["left"]["symbol"], "v0");
    assert_eq!(
        ops[4]["expression"]["args"][0]["right"]["symbol"],
        ops[3]["target"]["symbol"]
    );
    assert_eq!(story, before);
}
