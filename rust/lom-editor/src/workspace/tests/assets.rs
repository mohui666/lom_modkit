use super::*;

#[test]
fn copying_all_project_images_keeps_layout_variants_parallel_groups_and_one_undo() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lom-core/tests/fixtures/released-1.5.9-scenario-modes.lommod");
    let original_package = std::fs::read(&fixture).unwrap();
    let mut incoming = Project::open(&fixture).unwrap();
    let pixels = temp.path().join("normal.png");
    image::RgbaImage::new(2, 3).save(&pixels).unwrap();
    let pixels = std::fs::read(pixels).unwrap();
    for (kind, id, extends) in [
        ("image", "copy.picture", ""),
        ("character", "copy.hero", ""),
        ("character", "copy.player", "player"),
    ] {
        let directory = format!("assets/user/{kind}/{id}");
        let mut meta = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":id,
            "files":{"main":"normal.png"}});
        if kind == "character" {
            meta["portraits"] = json!({"normal":"normal.png","happy":"normal.png"});
            meta["scale"] = json!(370);
            meta["portrait_layouts"] = json!({"normal":{"scale":100,"x":0,"y":0}});
            if !extends.is_empty() {
                meta["extends"] = json!(extends);
            }
        }
        incoming.assets.insert(
            format!("{directory}/content.json"),
            lom_core::stable_json(&meta).unwrap(),
        );
        incoming
            .assets
            .insert(format!("{directory}/normal.png"), pixels.clone());
    }
    let mut original_layout = json!({"id":"custom","title":"原有图片演出","story_schema":2,"start":"first","nodes":[
        {"id":"first","type":"show","character":"user:copy.hero","position":"L","portrait":"normal","scale":82,"facing":"right"},
        {"id":"first_offset","type":"offset","character":"user:copy.hero","absolute":true,"x":-1.5,"y":2,"scale":126,"duration":0},
        {"id":"picture","type":"overlay","action":"show","slot":"main","image":"user:copy.picture","position":"left","x":-22,"y":13,"scale":146},
        {"id":"second","type":"parallel","actions":[
            {"type":"offset","character":"user:copy.hero","absolute":true,"x":6.5,"y":-7,"scale":94,"duration":0,"start_delay":0.3},
            {"type":"overlay","action":"show","slot":"main","image":"user:copy.picture","position":"right","x":14,"y":22,"scale":211},
            {"type":"show","character":"player","position":"R","portrait":"happy","scale":97}
        ]},
        {"id":"player_offset","type":"offset","character":"player","absolute":true,"x":3,"y":5,"scale":184,"duration":0},
        {"id":"line","type":"say","character":"user:copy.hero","portrait":"happy","text":"不要复制原正文","voice":"user:missing.voice"},
        {"id":"official","type":"cg","kind":"item","action":"show","key":"book1"},
        {"id":"last","type":"custom_cg","action":"show","image":"user:copy.picture","scale":74,"x":3,"y":-6},
        {"id":"end","type":"end"}
    ]});
    let nodes = original_layout["nodes"].as_array_mut().unwrap();
    let end = nodes.pop().unwrap();
    for kind in ["event", "picture", "item", "big"] {
        nodes.push(json!({"id":format!("custom_{kind}"),"type":"cg","kind":kind,"action":"show","key":"user:copy.picture","scale":123}));
        nodes.push(json!({"id":format!("hide_{kind}"),"type":"cg","kind":kind,"action":"hide"}));
    }
    nodes.push(end);
    incoming
        .stories
        .insert("custom".into(), original_layout.clone());
    let source = temp.path().join("source");
    incoming.save_to(&source).unwrap();
    let original_story = std::fs::read(source.join("story/custom.json")).unwrap();

    let mut app = app();
    let before = snapshot(&app.project);
    let entry = app.project.manifest["entry"].clone();
    let scenario = app.project.scenario.clone();
    app.copy_project_images_at(&source);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.project.manifest["entry"], entry);
    assert_eq!(app.project.scenario, scenario);
    assert!(
        app.project.assets == incoming.assets,
        "copy preserves exact asset bytes"
    );
    // The released fixture already has one image layout in main; both that
    // historical layout and the newly authored custom layout are retained.
    assert_eq!(app.project.stories.len(), before.stories.len() + 2);
    let layout = &app.project.stories["image_layout1"];
    assert!(!serde_json::to_string(layout)
        .unwrap()
        .contains("不要复制原正文"));
    assert!(!serde_json::to_string(layout)
        .unwrap()
        .contains("missing.voice"));
    let pictures: Vec<_> = layout["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["type"] == "cg")
        .collect();
    assert_eq!(pictures.len(), 8);
    assert!(pictures
        .iter()
        .filter(|node| node["action"] == "show")
        .all(|node| node["key"] == "user:copy.picture" && node["scale"] == 123));
    let second = layout["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "parallel")
        .unwrap();
    assert_eq!(second["actions"], original_layout["nodes"][3]["actions"]);
    let second_id = second["id"].as_str().unwrap();
    let original_state = crate::preview::simulate(&original_layout, "second");
    let copied_state = crate::preview::simulate(layout, second_id);
    for key in ["actors", "overlays"] {
        assert_eq!(copied_state[key], original_state[key], "{key}");
    }
    let copied = snapshot(&app.project);
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    assert!(snapshot(&app.project) == copied);
    let saved = temp.path().join("saved");
    app.project.save_to(&saved).unwrap();
    let reopened = open_project(&saved).unwrap();
    assert_eq!(reopened.stories, app.project.stories);
    assert!(
        reopened.assets == incoming.assets,
        "project save preserves exact asset bytes"
    );
    let output = temp.path().join("copied.lommod");
    reopened.export(&output).unwrap();
    let exported = open_project(&output).unwrap();
    assert_eq!(exported.stories, reopened.stories);
    assert!(exported.assets.keys().eq(incoming.assets.keys()));
    for (path, original) in &incoming.assets {
        if path.ends_with("/content.json") {
            // Package export already normalizes metadata (e.g. 0 -> 0.0 in
            // portrait registration); verify that all authored settings agree.
            let normalized = |bytes: &[u8]| {
                lom_core::content::normalize_content_metadata(
                    &serde_json::from_slice(bytes).unwrap(),
                )
                .unwrap()
            };
            assert_eq!(
                normalized(&exported.assets[path]),
                normalized(original),
                "{path}"
            );
        } else {
            assert!(&exported.assets[path] == original, "{path}");
        }
    }
    assert_eq!(
        std::fs::read(source.join("story/custom.json")).unwrap(),
        original_story
    );
    assert_eq!(std::fs::read(fixture).unwrap(), original_package);
}

#[test]
fn image_copy_without_layout_does_not_add_chapters_and_conflicts_leave_everything_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let mut incoming = Project::new();
    let meta = "assets/user/image/copy.one/content.json";
    let image = "assets/user/image/copy.one/main.png";
    incoming.assets.insert(
        meta.into(),
        lom_core::stable_json(&json!({
            "schema":1,"content_schema":1,"id":"copy.one","type":"image","name":"尚未使用的图片","files":{"main":"main.png"}
        }))
        .unwrap(),
    );
    incoming
        .assets
        .insert(image.into(), b"original picture".to_vec());
    let source = temp.path().join("source");
    incoming.save_to(&source).unwrap();
    let mut app = app();
    let stories = app.project.stories.clone();
    app.copy_project_images_at(&source);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.project.stories, stories);
    assert_eq!(app.project.assets, incoming.assets);
    let before = snapshot(&app.project);
    std::fs::write(source.join(image), b"different picture").unwrap();
    app.copy_project_images_at(&source);
    assert!(app.error.as_ref().unwrap().contains("同名图片素材"));
    assert!(snapshot(&app.project) == before);
}

#[test]
fn error_navigation_selects_the_owned_node_and_asset() {
    let mut app = app();
    let ctx = egui::Context::default();
    let story = json!({"id":"main","nodes":[{"id":"shared","type":"wait","seconds":1}]});
    app.project.stories.insert("main".into(), story.clone());
    app.project.stories.insert("other".into(), story);
    let raw = "story/other.json: 节点 \"shared\"(wait): 字段 \"seconds\" 必须是数字";
    app.go_to_problem(&ctx, crate::error_location::resolve(raw, &app.project));
    assert_eq!(app.current, "other");
    assert_eq!(app.selected, 0);
    assert_eq!(app.center, Center::Node);
    let path = "assets/user/image/demo.picture/content.json";
    app.project.assets.insert(path.into(), b"{}".to_vec());
    app.go_to_problem(
        &ctx,
        crate::error_location::resolve("用户内容 demo.picture 的图片不存在", &app.project),
    );
    assert_eq!(app.center, Center::Assets);
    assert_eq!(app.asset_selection, path);
}
#[cfg(windows)]
#[test]
#[ignore = "requires installed game settings; run the test executable from the complete Windows bundle"]
fn old_project_audio_failure_does_not_block_existing_or_new_nodes() {
    fn draw(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1600.0, 1200.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.center(ctx);
                app.dialogs(ctx);
            },
        )
    }
    fn start(app: &mut App) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = draw(app, &ctx, vec![]);
        let event = click_named_option(&output, &tr("试听音频"));
        draw(app, &ctx, vec![event]);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.audio.playing());
    }
    fn prepare(app: &mut App) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.audio.preparing() {
            app.audio.update()?;
            assert!(Instant::now() < deadline, "试听准备超时");
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    let temp = tempfile::tempdir().unwrap();
    let legacy = temp.path().join("旧工程.json");
    // Older nodes omit the optional op/kind fields. Loading must preserve
    // those fields and a failed old reference must not poison later previews.
    let original = lom_core::stable_json(&json!({
        "story_schema":2,"id":"main","title":"旧工程","start":"sound1",
        "nodes":[
            {"id":"sound1","type":"sound","name":"Sword_Fight_001"},
            {"id":"music1","type":"music","name":"主題_001"},
            {"id":"sound2","type":"sound","name":"Cymbal_001"},
            {"id":"env1","type":"sound","kind":"env","name":"DrumRoll_001"},
            {"id":"end1","type":"end"}
        ]
    }))
    .unwrap();
    std::fs::write(&legacy, &original).unwrap();
    let package =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/showcase3-legacy.lommod");
    for path in [&legacy, &package] {
        let mut app = app();
        let game = app.tools_panel.game_directory();
        assert!(game.join("Mortal_Data").is_dir(), "游戏目录未配置");
        app.install(Project::open(path).unwrap());
        app.recovery_dir = temp.path().join("recovery");
        let existing: Vec<_> = app.story().clone()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| {
                matches!(node["type"].as_str(), Some("music" | "sound"))
                    && node["name"] != "Sword_Fight_001"
            })
            .map(|node| node["id"].as_str().unwrap().to_owned())
            .collect();
        let before = snapshot(&app.project);
        app.add_node("sound");
        let selected = app.selected;
        app.project.stories.get_mut("main").unwrap()["nodes"][selected]["name"] =
            json!("Sword_Fight_001");
        start(&mut app);
        let error = prepare(&mut app).unwrap_err().to_string();
        assert!(error.contains("Sword_Fight_001"), "{error}");
        assert!(!error.contains('�'), "{error}");
        assert!(!app.audio.playing());
        app.error = Some(error);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = draw(&mut app, &ctx, vec![]);
        let event = click_named_option(&output, &tr("关闭"));
        draw(&mut app, &ctx, vec![event]);
        assert!(app.error.is_none());
        for id in existing {
            app.selected = app.story().clone()["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .position(|node| node["id"] == id)
                .unwrap();
            start(&mut app);
            prepare(&mut app).unwrap();
            app.audio.stop();
        }
        for (kind, name) in [("music", "主題_001"), ("sound", "Cymbal_001")] {
            app.add_node(kind);
            let selected = app.selected;
            app.project.stories.get_mut("main").unwrap()["nodes"][selected]["name"] = json!(name);
            start(&mut app);
            prepare(&mut app).unwrap();
            app.audio.stop();
        }
        assert_eq!(app.tools_panel.game_directory(), game);
        assert_eq!(std::fs::read(&legacy).unwrap(), original);
        // Playback must leave all existing nodes and assets intact.
        let after = snapshot(&app.project);
        assert_eq!(before.assets, after.assets);
        for node in before.stories["main"].to_value()["nodes"]
            .as_array()
            .unwrap()
        {
            assert!(after.stories["main"].to_value()["nodes"]
                .as_array()
                .unwrap()
                .contains(node));
        }
    }
}
#[test]
fn duplicate_content_names_select_distinct_assets_without_renaming_metadata() {
    let mut app = app();
    for id in ["demo.a", "demo.b"] {
        app.project.assets.insert(
            format!("assets/user/image/{id}/content.json"),
            serde_json::to_vec(
                &json!({"id":id,"type":"image","name":"同名素材","files":{"main":"main.png"}}),
            )
            .unwrap(),
        );
    }
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(700.0, 900.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.assets(ui));
        },
    );
    let _ = click_named_option(&output, "同名素材 1 ");
    let event = click_named_option(&output, "同名素材 2 ");
    let _ = ctx.run(
        egui::RawInput {
            events: vec![event],
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.assets(ui));
        },
    );
    assert_eq!(app.asset_selection, "assets/user/image/demo.b/content.json");
    assert!(snapshot(&app.project) == before);
}
#[test]
fn copied_chapter_preserves_all_content_and_does_not_redirect_explicit_links() {
    let mut app = app();
    let mut source = app.story().clone();
    source["nodes"][1]["next_script"] = json!("main");
    source["localization"] = json!({"translations":{"cht":{"say1.text":"春日"}}});
    source["_editor"] = json!({"tests":[{"story":"main","name":"本章测试"}],"sections":[{"id":"a","start":"say1","end":"end1"}]});
    app.project.stories.insert("main".into(), source.clone());
    let mut occupied = source.clone();
    occupied["id"] = json!("chapter1");
    app.project.stories.insert("chapter1".into(), occupied);
    app.begin_chapter(true);
    app.chapter_dialog.as_mut().unwrap().name = "另一段故事".into();
    app.commit_chapter().unwrap();
    assert_eq!(app.current, "chapter2");
    let mut expected = source.clone();
    expected["id"] = json!("chapter2");
    expected["title"] = json!("另一段故事");
    assert_eq!(app.story().clone(), expected);
    assert_eq!(app.project.stories["main"], source);
    assert_eq!(app.project.manifest["entry"], "main");
    app.begin_chapter(true);
    app.commit_chapter().unwrap();
    assert_eq!(app.current, "chapter3");
}
#[test]
fn undo_and_redo_keep_invalid_content_drafts_and_existing_history() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hero.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    app.import_assets(vec![image], "test.hero", "hero", 0)
        .unwrap();
    app.track();
    app.flush();
    app.project.manifest["name"] = json!("new title");
    app.track();
    app.flush();
    app.undo();
    assert!(!app.undo.is_empty());
    assert!(!app.redo.is_empty());
    let mut draft: Value =
        serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    draft["files"]["main"] = json!("../outside.png");
    app.content_panel = crate::content_panel::ContentPanel::pending_fixture(
        &app.project,
        &app.asset_selection,
        draft,
    );
    let project = snapshot(&app.project);
    let history = (app.undo.len(), app.redo.len());
    app.undo();
    assert!(snapshot(&app.project) == project);
    assert!(app.content_panel.has_pending());
    assert!(app.error.is_some());
    app.redo();
    assert!(snapshot(&app.project) == project);
    assert!(app.content_panel.has_pending());
    assert_eq!((app.undo.len(), app.redo.len()), history);
}
#[test]
fn character_import_accepts_timestamp_filename_and_independent_display_name() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("26_04_23_17_53_52.png");
    image::RgbaImage::new(1, 1).save(&image).unwrap();
    for name in ["normal", "我的角色"] {
        let mut app = app();
        app.asset_import_dialog = Some(AssetImportDialog {
            extends: String::new(),
            kind: 1,
            name: name.into(),
            paths: vec![image.clone()],
            error: None,
        });
        app.commit_asset_import().unwrap();
        let meta: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        assert_eq!(meta["name"], name);
        assert_eq!(meta["portraits"], json!({"normal":"26_04_23_17_53_52.png"}));
        assert_eq!(meta["files"]["main"], "26_04_23_17_53_52.png");
        assert_eq!(app.project.assets.len(), 2);
    }
}
#[test]
fn official_character_import_lists_new_expressions_under_the_original_identity() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["tired.png", "sleep.png"]
        .into_iter()
        .map(|name| {
            let path = directory.path().join(name);
            image::RgbaImage::new(2, 3).save(&path).unwrap();
            path
        })
        .collect();
    let mut app = app();
    app.import_assets_with_target(paths, "demo.xia", "Extra expressions", 1, "special825")
        .unwrap();
    let meta: Value = serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    assert_eq!(meta["extends"], "special825");
    assert_eq!(
        meta["portraits"],
        json!({"tired":"tired.png", "sleep":"sleep.png"})
    );
    app.catalog.sync_assets(&app.project.assets);
    let options = app
        .catalog
        .options("portrait", &json!({"character":"special825"}), &[], &[]);
    for name in ["tired", "sleep", "normal"] {
        assert!(options.iter().any(|(id, _)| id == name));
    }
    assert!(!app.catalog.data["characters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "user:demo.xia"));
    app.project.stories.get_mut("main").unwrap()["nodes"] = json!([
        {"id":"say1","type":"show","character":"special825","position":"M","portrait":"tired"},
        {"id":"end1","type":"end"}]);
    let lua = compile_with_assets(&app.project, &app.project.stories["main"]).unwrap();
    assert!(lua.contains("characters.LoadCharacterPortrait(\"special825\", \"tired\")"));
}
#[test]
fn character_import_appends_selections_and_preserves_same_named_files() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = [
        ("a/normal.png", b"first".as_slice()),
        ("b/normal.png", b"second".as_slice()),
        ("c/normal_1.png", b"reserved".as_slice()),
        ("d/NORMAL.PNG", b"fourth".as_slice()),
    ]
    .into_iter()
    .map(|(name, bytes)| {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    })
    .collect();
    let mut draft = AssetImportDialog {
        extends: String::new(),
        kind: 1,
        name: String::new(),
        paths: vec![],
        error: None,
    };
    draft.select_files(vec![paths[0].clone()]);
    draft.name = "自定人物".into();
    draft.select_files(vec![paths[0].clone(), paths[1].clone()]);
    draft.select_files(paths[2..].to_vec());
    assert_eq!(draft.paths, paths);
    assert_eq!(draft.name, "自定人物");
    let mut app = app();
    app.asset_import_dialog = Some(draft);
    app.commit_asset_import().unwrap();
    let meta: Value = serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    let prefix = app.asset_selection.strip_suffix("content.json").unwrap();
    assert_eq!(meta["files"]["main"], "normal.png");
    assert_eq!(meta["portraits"]["normal"], "normal.png");
    assert_eq!(meta["portraits"]["portrait_1"], "normal_2.png");
    assert_eq!(meta["portraits"]["normal_1"], "normal_1.png");
    assert_eq!(meta["portraits"]["NORMAL"], "NORMAL_3.PNG");
    for (name, bytes) in [
        ("normal.png", b"first".as_slice()),
        ("normal_2.png", b"second".as_slice()),
        ("normal_1.png", b"reserved".as_slice()),
        ("NORMAL_3.PNG", b"fourth".as_slice()),
    ] {
        assert_eq!(app.project.assets[&format!("{prefix}{name}")], bytes);
    }
}
#[test]
fn character_import_keeps_every_image_and_never_overwrites_normal_or_named_portraits() {
    let dir = tempfile::tempdir().unwrap();
    let names = [
        "26_04_23_17_53_52.png",
        "中文.png",
        "portrait_1.png",
        "happy.png",
        "happy.jpg",
        "normal.png",
        "abcdefghijklmnopqrstuvwxyzabcdefg.png",
        "face-smile.png",
    ];
    let paths = names
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, b"image fixture").unwrap();
            path
        })
        .collect();
    let mut app = app();
    app.import_assets(paths, "test.hero", "角色", 1).unwrap();
    let meta: Value = serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    assert_eq!(meta["portraits"]["normal"], names[0]);
    assert_eq!(meta["portraits"]["portrait_1"], "portrait_1.png");
    assert_eq!(meta["portraits"]["happy"], "happy.png");
    let portraits = meta["portraits"].as_object().unwrap();
    assert_eq!(portraits.len(), names.len());
    for name in names {
        assert!(
            portraits.values().any(|value| value == name),
            "missing {name}"
        );
    }
    lom_core::content::normalize_content_metadata(&meta).unwrap();
}
#[test]
fn gameplay_content_imports_defaults_and_exposes_typed_selectors() {
    let dir = tempfile::tempdir().unwrap();
    let icon = dir.path().join("icon.png");
    std::fs::write(&icon, b"sample icon").unwrap();
    let mut app = app();
    app.import_assets(vec![], "demo.awakening", "万木逢春", 10)
        .unwrap();
    app.catalog.sync_assets(&app.project.assets);
    assert!(app
        .catalog
        .options("talent", &Value::Null, &[], &[])
        .iter()
        .any(|(id, name)| id == "user:demo.awakening" && name.contains("万木逢春")));
    for (kind, index, id) in [
        ("item", 5, "demo.token"),
        ("martial", 6, "demo.manual"),
        ("support", 7, "demo.aid"),
    ] {
        app.import_assets(vec![icon.clone()], id, "测试内容", index)
            .unwrap();
        let key = format!("assets/user/{kind}/{id}/content.json");
        let meta: Value = serde_json::from_slice(&app.project.assets[&key]).unwrap();
        assert_eq!(meta["type"], kind);
        lom_core::content::normalize_content_metadata(&meta).unwrap();
    }
    app.catalog.sync_assets(&app.project.assets);
    assert!(app
        .catalog
        .options("item", &json!({"kind":"misc"}), &[], &[])
        .iter()
        .any(|(id, _)| id == "user:demo.token"));
    assert!(app
        .catalog
        .options("item", &json!({"kind":"book"}), &[], &[])
        .iter()
        .any(|(id, _)| id == "user:demo.manual"));
    assert!(app
        .catalog
        .options("support", &Value::Null, &[], &[])
        .iter()
        .any(|(id, _)| id == "user:demo.aid"));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&icon, bytes.into_inner()).unwrap();
    app.import_assets(vec![icon.clone(), icon.clone()], "demo.strike", "试招", 8)
        .unwrap();
    app.import_assets(vec![icon], "demo.tonic", "丹药", 9)
        .unwrap();
    app.catalog.sync_assets(&app.project.assets);
    let skill: Value =
        serde_json::from_slice(&app.project.assets["assets/user/skill/demo.strike/content.json"])
            .unwrap();
    assert_eq!(skill["animation"]["frames"].as_array().unwrap().len(), 2);
    assert_ne!(
        skill["animation"]["frames"][0],
        skill["animation"]["frames"][1]
    );
    assert!(app
        .catalog
        .options("skill", &Value::Null, &[], &[])
        .iter()
        .any(|(id, _)| id == "user:demo.strike"));
    assert!(app
        .catalog
        .options("field_consumable", &Value::Null, &[], &[])
        .iter()
        .any(|(id, _)| id == "user:demo.tonic"));
}
#[test]
fn asset_import_is_atomic_and_undo_redo_restores_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hero.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    assert!(app
        .import_assets(vec![image.clone()], "Invalid-ID", "hero", 0)
        .is_err());
    assert!(app.project.assets.is_empty());
    app.import_assets(vec![image], "my_mod.hero", "hero", 0)
        .unwrap();
    app.track();
    app.flush();
    let imported = app.project.assets.clone();
    assert_eq!(imported.len(), 2);
    assert!(app.dirty());
    app.undo();
    assert!(app.project.assets.is_empty());
    assert!(!app.dirty());
    app.redo();
    assert_eq!(app.project.assets, imported);
    assert!(app.dirty());
}
#[test]
fn project_intro_compilation_uses_project_assets_not_global_repository() {
    let mut project = Project::new();
    let prefix = "assets/user/character/rust_fixture.hero";
    let meta = json!({"schema":1,"content_schema":1,"id":"rust_fixture.hero","type":"character","name":"hero","files":{"main":"normal.png"},"portraits":{"normal":"normal.png"},"intro":{"name":"项目内人物","title":"项目内标题","text":"来自项目素材"}});
    project.assets.insert(
        format!("{prefix}/content.json"),
        serde_json::to_vec(&meta).unwrap(),
    );
    project
        .assets
        .insert(format!("{prefix}/normal.png"), vec![1]);
    let story = json!({"id":"main","story_schema":2,"start":"intro","nodes":[{"id":"intro","type":"intro","intro_source":"character","character":"user:rust_fixture.hero"},{"id":"end","type":"end"}]});
    let lua = compile_with_assets(&project, &story).unwrap();
    assert!(lua.contains("项目内人物"));
    assert!(lua.contains("来自项目素材"));
}
#[test]
fn project_open_entries_preserve_settings_events_assets_and_all_chapters() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("project");
    let mut project = template_project("linear_dialogue").unwrap();
    project.manifest["name"] = json!("完整作品设置");
    project.save_to(&target).unwrap();
    for path in [
        target.clone(),
        target.join("story"),
        target.join("manifest.json"),
        target.join("story/main.json"),
        target.join("scenario.json"),
    ] {
        let mut opened = open_project(&path).unwrap();
        assert_eq!(opened.manifest, project.manifest, "{}", path.display());
        assert_eq!(opened.scenario, project.scenario);
        assert_eq!(opened.stories, project.stories);
        assert_eq!(opened.source, project.source);
        opened.manifest["description"] = json!("编辑后保存");
        opened.save_to(&target).unwrap();
        project = open_project(&target).unwrap();
        assert_eq!(project.manifest["description"], "编辑后保存");
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lom-core/tests/fixtures/released-1.5.4-name-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut legacy = open_project(&fixture).unwrap();
    legacy.manifest["description"] = json!("旧版作品仍可修改");
    legacy.save_to(&root.path().join("legacy")).unwrap();
    let package = root.path().join("edited.lommod");
    legacy.export(&package).unwrap();
    assert_eq!(open_project(&package).unwrap().stories, legacy.stories);
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
}
#[test]
fn imported_step_image_is_applied_and_undone_with_its_asset() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("image.png");
    std::fs::write(&file, b"fixture image").unwrap();
    for kind in ["overlay", "event", "picture", "item", "big"] {
        let mut app = app();
        let id = app.story().clone()["nodes"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut node = json!({"id":id,"type":"cg","kind":kind,"action":"show"});
        if kind == "overlay" {
            node = json!({"id":id,"type":"overlay","slot":"main","action":"show"});
        }
        app.project.stories.get_mut(&app.current).unwrap()["nodes"][0] = node;
        app.track();
        app.flush();
        let before = snapshot(&app.project);
        app.image_import_target = Some((app.current.clone(), id));
        app.asset_import_dialog = Some(AssetImportDialog {
            extends: String::new(),
            kind: 0,
            name: "图片".into(),
            paths: vec![file.clone()],
            error: None,
        });
        app.commit_asset_import().unwrap();
        let field = if kind == "overlay" { "image" } else { "key" };
        assert!(app.story().clone()["nodes"][0][field]
            .as_str()
            .unwrap()
            .starts_with("user:"));
        assert!(kind == "overlay" || app.story().clone()["nodes"][0].get("image").is_none());
        assert!(!app.project.assets.is_empty());
        app.undo();
        assert!(snapshot(&app.project) == before);
    }
}
