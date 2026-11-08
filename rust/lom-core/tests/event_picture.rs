use lom_core::{codegen, localization, package, project::Project, validate};
use serde_json::{json, Value};

#[test]
fn event_fade_is_optional_waits_before_text_and_keeps_legacy_output_for_zero() {
    let source = json!({"id":"main","start":"book","nodes":[
        {"id":"book","type":"cg","kind":"event","action":"show","key":"girlroom_work_002","event_title":"来信","event_text":"读信"},
        {"id":"end","type":"end"}
    ]});
    let legacy = codegen::compile_story(&source, None, None, None).unwrap();
    let mut current = source.clone();
    current["nodes"][0]["fade_in"] = json!(0);
    assert_eq!(
        codegen::compile_story(&current, None, None, None).unwrap(),
        legacy
    );
    current["nodes"][0]["fade_in"] = json!(0.8);
    let lua = codegen::compile_story(&current, None, None, None).unwrap();
    let show = lua
        .find("mod_event_picture_show_fade(\"girlroom_work_002\", \"来信\", 0.8)")
        .unwrap();
    let wait = lua[show..].find("wait(0.8)").unwrap() + show;
    let say = lua[show..].find("say(\"读信\")").unwrap() + show;
    assert!(show < wait && wait < say);
    assert!(lua.contains("if mod_event_picture_show_fade == nil then error("));
    for invalid in [json!(-0.1), json!("soon"), Value::Null] {
        current["nodes"][0]["fade_in"] = invalid;
        assert!(validate::validate_story(&current)
            .unwrap_err()
            .to_string()
            .contains("fade_in"));
    }
}

#[test]
fn event_book_exports_custom_image_caption_and_optional_dialogue() {
    let mut project = Project::new();
    project.manifest["min_host_version"] = json!("1.4.1");
    let story = json!({"id":"main","start":"book","nodes":[
        {"id":"book","type":"cg","kind":"event","action":"show",
         "key":"user:demo.image","event_title":"来信","event_text":"第一行\n第二行"},
        {"id":"close","type":"cg","kind":"event","action":"hide"},
        {"id":"end","type":"end"}]});
    let texts = localization::iter_localizable_texts(&story);
    assert_eq!(texts["book.event_title"], "来信");
    assert_eq!(texts["book.event_text"], "第一行\n第二行");
    project.stories.insert("main".into(), story.clone());
    let prefix = "assets/user/image/demo.image";
    project.assets.insert(
        format!("{prefix}/content.json"),
        serde_json::to_vec(&json!({
            "schema":1,"id":"demo.image","type":"image","name":"图片","files":{"main":"image.png"}
        }))
        .unwrap(),
    );
    project.assets.insert(
        format!("{prefix}/image.png"),
        include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
        )
        .to_vec(),
    );
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("book.lommod");
    project.export(&output).unwrap();
    let entries = package::read_package(&output).unwrap();
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(manifest["min_host_version"], "1.4.2");
    assert!(entries.contains_key(&format!("{prefix}/image.png")));
    let lua = String::from_utf8(entries["lua/main.lua"].clone()).unwrap();
    assert!(lua.contains("mod_event_picture_show(\"user:demo.image\", \"来信\")"));
    assert!(lua.contains("setsaydialog(saydialogs.narrative)"));
    assert!(lua.contains("mod_event_picture_hide()"));
    Project::open(&output)
        .unwrap()
        .export(&folder.path().join("again.lommod"))
        .unwrap();

    let mut empty = story;
    empty["nodes"][0]
        .as_object_mut()
        .unwrap()
        .remove("event_text");
    empty["nodes"][0]["key"] = json!("girlroom_work_002");
    let lua = codegen::compile_story(&empty, None, None, None).unwrap();
    assert!(!lua.contains("\tsay("));
}

#[test]
fn campaign_id_errors_distinguish_absence_and_invalid_format() {
    let mut manifest = Project::new().manifest;
    manifest["campaign_id"] = json!("");
    let missing = validate::validate_manifest(&manifest)
        .unwrap_err()
        .to_string();
    assert!(missing.contains("未填写") && missing.contains("存档身份"));
    manifest["campaign_id"] = json!("中文 ID");
    let invalid = validate::validate_manifest(&manifest)
        .unwrap_err()
        .to_string();
    assert!(invalid.contains("格式不合法") && !invalid.contains("缺少必填字段"));
}

#[test]
fn only_a_positive_event_fade_requires_host_1419_and_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.manifest["min_host_version"] = json!("1.4.2");
    project.stories.insert("main".into(), json!({"story_schema":2,"id":"main","start":"book","nodes":[
        {"id":"book","type":"cg","kind":"event","action":"show","key":"girlroom_work_002","fade_in":0},
        {"id":"end","type":"end"}
    ]}));
    for (seconds, required) in [(0.0, "1.4.2"), (0.8, "1.4.19")] {
        project.stories.get_mut("main").unwrap()["nodes"][0]["fade_in"] = json!(seconds);
        let path = dir.path().join(format!("{seconds}.lommod"));
        project.export(&path).unwrap();
        let reopened = Project::open(&path).unwrap();
        assert_eq!(reopened.manifest["min_host_version"], required);
        assert_eq!(reopened.stories, project.stories);
    }
}
