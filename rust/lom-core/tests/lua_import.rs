use lom_core::{lua_import, project::Project};
use std::fs;

const PRESENTATION: &str = r#"-- 旧剧情，人物与差分均为实际节点值
runwait(flowcharts.LoadView("center"))
getvar(flowcharts.view, "ViewName").value = "center"
runblock(flowcharts.view, "view")
local hero = characters.Get("player")
local smile = characters.GetPortrait("player", "normal")
runwait(characters.LoadCharacterAsset("player"))
characters.LoadCharacterPortrait("player", "normal")
stage.show {
    character = hero, portrait = smile,
    fromPosition = "M", toPosition = "M", facing = "right",
    fadeDuration = 0.3, moveDuration = 0, useDefaultSettings = false
}
setsaydialog(saydialogs.character)
setcharacter(hero, smile)
sayoptions.waitforinput = true
sayoptions.fadewhendone = true
say('你好，\"江湖\"！')
wait(0.5)
characters.LoadCharacterPortrait("player", "angry1")
stage.showPortrait(hero, characters.GetPortrait("player", "angry1"))
setcharacter(hero, characters.GetPortrait("player", "angry1"))
say([=[
第二句，含有 -- 注释符号和逗号, )。
]=])
"#;

#[test]
fn imports_scenes_character_aliases_portrait_changes_and_literal_dialogue() {
    let imported = lua_import::import(PRESENTATION, "main", "旧故事").unwrap();
    assert!(imported.report.converted, "{:?}", imported.report);
    let n = imported.story["nodes"].as_array().unwrap();
    assert_eq!(
        n.iter()
            .map(|n| n["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["scene", "show", "say", "wait", "show", "say", "end"]
    );
    assert_eq!(n[0]["view"], "center");
    assert_eq!(n[2]["text"], "你好，\"江湖\"！");
    assert_eq!(n[4]["portrait"], "angry1");
    assert_eq!(n[5]["portrait"], "angry1");
    assert_eq!(n[5]["text"], "第二句，含有 -- 注释符号和逗号, )。\n");
    lom_core::codegen::compile_story(&imported.story, None, None, None).unwrap();
}

#[test]
fn unknown_flow_preserves_whole_scope_and_stops_static_preview_before_branch() {
    let source = format!("{PRESENTATION}\nlocal value = ReadGameState()\nif value then\nrunblock(flowcharts.view, 'out')\nend");
    let imported = lua_import::import(&source, "main", "旧故事").unwrap();
    assert!(!imported.report.converted);
    assert_eq!(imported.story["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(imported.story["nodes"][0]["type"], "raw");
    assert_eq!(imported.story["nodes"][0]["code"], source);
    let (preview, report) = lua_import::preview_nodes(&source);
    assert_eq!(preview.last().unwrap()["portrait"], "angry1");
    assert!(preview.iter().all(|node| node["view"] != "out"));
    assert!(report.preview_stopped_at.is_some());
    lom_core::codegen::compile_story(&imported.story, None, None, None).unwrap();
}

#[test]
fn project_keeps_original_utf8_bom_and_saves_without_overwriting_lua() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("原始.LUA");
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(PRESENTATION.as_bytes());
    fs::write(&original, &bytes).unwrap();
    let mut project = Project::open(&original).unwrap();
    assert_eq!(project.source, None);
    assert_eq!(project.assets["assets/imported/source.lua"], bytes);
    let target = temp.path().join("导入的项目");
    project.save_to(&target).unwrap();
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(
        fs::read(target.join("assets/imported/source.lua")).unwrap(),
        bytes
    );
    assert!(Project::open(&target).is_ok());
}

#[test]
fn comments_escapes_and_unknown_calls_are_never_executed() {
    let source = "--[==[ 多行\n注释 ]==]\nsetsaydialog(saydialogs.narrative);setcharacter(narrative)\nsay('a\\x20b\\033') -- 注释\nsay([[\nhello]])\nos.execute('bad')\nsay('unreachable preview')";
    let (nodes, report) = lua_import::preview_nodes(source);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["text"], "a b!");
    assert_eq!(nodes[1]["text"], "hello");
    assert!(!report.converted);
    assert_eq!(report.preview_stopped_at, Some(7));
    let imported = lua_import::import(source, "main", "保留").unwrap();
    assert_eq!(imported.story["nodes"][0]["code"], source);
}

#[test]
fn malformed_or_dynamic_expressions_stay_raw_and_empty_source_is_rejected() {
    for source in [
        "say('unfinished",
        "stage.show{character=f()}",
        "say('a' .. Read())",
        "if true then say('x') end",
    ] {
        let imported = lua_import::import(source, "main", "待处理").unwrap();
        assert!(!imported.report.converted);
        assert_eq!(imported.story["nodes"][0]["code"], source);
    }
    assert!(lua_import::import(" ", "main", "空").is_err());
}

#[test]
fn native_defaults_and_asynchronous_motion_keep_source_but_have_static_end_pose() {
    let source = "stage.show{character=characters.Get('player'),portrait=characters.GetPortrait('player','angry1'),fromPosition='L1',toPosition='R1',facing='left'}";
    let result = lua_import::import(source, "main", "异步").unwrap();
    assert!(!result.report.converted);
    assert_eq!(result.story["nodes"][0]["code"], source);
    let (nodes, report) = lua_import::preview_nodes(source);
    assert_eq!(nodes[0]["position"], "R1");
    assert_eq!(nodes[0]["portrait"], "angry1");
    assert!(report.preview_stopped_at.is_none());
    assert!(report.notes[0].contains("原版默认补间或异步移动"));
}
