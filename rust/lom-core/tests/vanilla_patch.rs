use lom_core::{
    codegen, package,
    project::Project,
    vanilla_patch::{self, Condition, Edit},
};
use serde_json::{json, Value};

fn insertion(find: &str) -> Edit {
    Edit {
        find: find.into(),
        occurrence: 1,
        kind: "say".into(),
        text: None,
        before: "新增内容".into(),
        after: String::new(),
        before_condition: Some(Condition {
            source: "mod".into(),
            flag: "extra".into(),
            ..Default::default()
        }),
        after_condition: None,
    }
}

#[test]
fn dialogue_positions_ignore_comments_strings_and_keep_repeated_occurrences() {
    let source = r#"-- say("重复")
local quoted = 'say("字符串里的代码")'
--[=[ say("长注释里的代码") ]=]
if original_condition() then
    say("重复")
    say(luamanager.GetStoryText("D_DEMO"))
else
    say("重复")
end
options[1] = "O_DEMO"
"#;
    let sites = vanilla_patch::text_sites(source);
    assert_eq!(sites.len(), 4);
    assert_eq!(sites[0].occurrence, 2);
    assert_eq!(sites[2].occurrence, 3);
    assert_eq!(sites[1].key.as_deref(), Some("D_DEMO"));
    assert_eq!(sites[3].kind, "option");
    let mut edit = insertion(&sites[2].find);
    edit.occurrence = sites[2].occurrence;
    let changed = vanilla_patch::apply(source, &[edit]).unwrap();
    assert!(changed.starts_with(&source[..source.rfind("say(\"重复\")").unwrap()]));
    assert!(changed.contains("else\n    if modflags[\"extra\"] then"));
    assert!(changed.ends_with("end\noptions[1] = \"O_DEMO\"\n"));
    assert_eq!(changed.matches("say(\"重复\")").count(), 3);
}

#[test]
fn flag_insertions_preserve_the_original_call_and_progression() {
    let original = "local keep = 3\r\nif ready then\r\n say(luamanager.GetStoryText(\"D_DEMO\"))\r\n keep = keep + 1\r\nend\r\nadvance(keep)\r\n";
    let mut edit = insertion("say(luamanager.GetStoryText(\"D_DEMO\"))");
    edit.after = "数值满足时追加".into();
    edit.after_condition = Some(Condition {
        source: "flag_value".into(),
        flag: "FLAG_DEMO".into(),
        value: 2,
        ..Default::default()
    });
    let changed = vanilla_patch::apply(original, &[edit.clone()]).unwrap();
    assert_eq!(
        changed,
        original.replacen(&edit.find, &edit.replacement(), 1)
    );
    assert!(changed.contains(&edit.find));
    assert!(changed.contains("tonumber(luamanager.GetFlagData(\"FLAG_DEMO\")) >= 2"));
    assert!(changed.ends_with(" keep = keep + 1\r\nend\r\nadvance(keep)\r\n"));
    let mut project = Project::new_vanilla_patch("DEMO").unwrap();
    project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"] = json!([edit]);
    let lua = codegen::compile_story(
        &project.stories["main"],
        Some(&project.manifest),
        None,
        None,
    )
    .unwrap();
    assert!(lua.contains("mod_load_vanilla_story(\"DEMO\""));
    assert!(!lua.contains("mod_hide_all()"));
    assert!(!lua.contains("advance(keep)"));
}

#[test]
fn invalid_and_overlapping_changes_are_rejected_without_changing_source() {
    let source = "say(\"hello\")\nadvance()";
    assert!(vanilla_patch::apply(source, &[insertion("missing")]).is_err());
    let edit = insertion("say(\"hello\")");
    assert!(vanilla_patch::apply(source, &[edit.clone(), edit]).is_err());
    assert_eq!(source, "say(\"hello\")\nadvance()");
    for target in ["../ch1_1", "MOD_some_package", "", "chapter/name"] {
        assert!(Project::new_vanilla_patch(target).is_err());
    }
    let mut project = Project::new_vanilla_patch("DEMO").unwrap();
    let mut edit = insertion("say(\"hello\")");
    edit.before_condition.as_mut().unwrap().flag.clear();
    project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"] = json!([edit]);
    assert!(vanilla_patch::parse(&project.stories["main"]).is_err());
}

#[test]
fn exported_patch_contains_only_changes_and_roundtrips_with_its_original_trigger() {
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new_vanilla_patch("CH_DEMO").unwrap();
    project.manifest["id"] = json!("my_mod");
    project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"] =
        json!([insertion("say(\"hello\")")]);
    project.save_to(&root.path().join("project")).unwrap();
    let reopened = Project::open(&root.path().join("project")).unwrap();
    assert_eq!(project.stories, reopened.stories);
    let archive = root.path().join("patch.lommod");
    reopened.export(&archive).unwrap();
    let entries = package::read_package(&archive).unwrap();
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(manifest["vanilla_overrides"]["CH_DEMO"], "main");
    assert_eq!(manifest["min_host_version"], "1.4.9");
    let scenario: Value = serde_json::from_slice(&entries["scenario.json"]).unwrap();
    assert!(scenario["events"].as_array().unwrap().is_empty());
    assert!(!entries.keys().any(|name| name.starts_with("assets/")));
    let texts: Value = serde_json::from_slice(&entries["texts.json"]).unwrap();
    assert_eq!(texts["MOD_my_mod_main_patch_0_before"], "新增内容");
    assert!(std::str::from_utf8(&entries["lua/main.lua"])
        .unwrap()
        .contains("MOD_my_mod_main_patch_0_before"));
    let imported = Project::open(&archive).unwrap();
    assert_eq!(imported.stories, project.stories);
    assert!(package::inspect_package(&archive).unwrap()["ok"]
        .as_bool()
        .unwrap());
    let second = vanilla_patch::new_story("other", "ch_demo").unwrap();
    project.stories.insert("other".into(), second);
    assert!(vanilla_patch::targets(&project.stories).is_err());
}
