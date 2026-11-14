use super::*;

#[test]
#[ignore = "manual timing of editor hot paths on a large project"]
fn manual_editor_performance_check() {
    fn measure(label: &str, runs: usize, mut operation: impl FnMut()) {
        let mut elapsed = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            operation();
            elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        elapsed.sort_by(f64::total_cmp);
        println!(
            "PERF {label} median_ms={:.3} p95_ms={:.3}",
            elapsed[runs / 2],
            elapsed[runs * 95 / 100]
        );
    }
    let mut app = app();
    let mut project = Project::new();
    let story = json!({"id":"main","start":"n0","nodes":(0..2000).map(|index| {
        json!({"id":format!("n{index}"),"type":"say","character":"player","portrait":"normal","text":format!("性能检查对白 {index}")})
    }).collect::<Vec<_>>()});
    project.stories.insert("main".into(), story);
    for index in 1..10 {
        let id = format!("chapter{index}");
        let mut story = project.stories["main"].clone();
        story["id"] = json!(id);
        project.stories.insert(id, story);
    }
    project
        .assets
        .insert("perf-unused.bin".into(), vec![42; 64 * 1024 * 1024]);
    app.install(project);
    measure("story_copy", 30, || {
        std::hint::black_box(app.story());
    });
    measure("saved_status", 30, || {
        std::hint::black_box(app.dirty());
    });
    measure("change_tracking", 30, || app.track());
    let node = app.story()["nodes"][0].clone();
    measure("character_options", 30, || {
        std::hint::black_box(app.catalog.options("character_group", &node, &[], &[]));
    });
    let ctx = egui::Context::default();
    let draw = |app: &mut App| {
        std::hint::black_box(ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.sidebar(ctx);
                app.right(ctx);
                app.center(ctx);
            },
        ));
    };
    for _ in 0..3 {
        draw(&mut app);
    }
    measure("stage_frame", 30, || {
        draw(&mut app);
        app.track();
        std::hint::black_box(app.dirty());
    });
    app.view = View::Graph;
    for _ in 0..3 {
        draw(&mut app);
    }
    measure("graph_frame", 30, || {
        draw(&mut app);
        app.track();
        std::hint::black_box(app.dirty());
    });
}
#[test]
fn game_bookmarks_locate_dialogue_preserve_edits_and_use_the_unsaved_project_guard() {
    let mark = lom_core::story_bookmarks::Bookmark {
        schema: 1,
        script: "DEMO".into(),
        text_key: "D_SECOND".into(),
        text: String::new(),
        game_dir: PathBuf::new(),
        year: 1,
        month: 4,
        stage: 1,
        captured_utc: "2026-11-05T12:00:00Z".into(),
    };
    let mut app = app();
    app.install(Project::new_vanilla_patch("DEMO").unwrap());
    app.vanilla = crate::vanilla_panel::Panel::with_source(
        "DEMO",
        "say(luamanager.GetStoryText(\"D_FIRST\"))\nsay(luamanager.GetStoryText(\"D_SECOND\"))",
    );
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    app.open_story_bookmark(mark.clone(), &ctx);
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 1600.0),
            )),
            ..Default::default()
        },
        |ctx| app.center(ctx),
    );
    assert_eq!(app.vanilla.selected_key("DEMO"), Some("D_SECOND"));
    assert!(snapshot(&app.project) == before);
    let mut other = mark.clone();
    other.script = "OTHER".into();
    app.open_story_bookmark(other, &ctx);
    app.track();
    app.flush();
    assert_eq!(app.project.stories.len(), 2);
    assert_eq!(
        app.project.stories["main"],
        before.stories["main"].to_value()
    );
    app.undo();
    assert_eq!(snapshot(&app.project).stories, before.stories);
    app.install(Project::new());
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("未保存修改");
    app.track();
    let dirty = snapshot(&app.project);
    app.open_story_bookmark(mark, &ctx);
    assert!(matches!(app.pending, Some(Pending::OpenVanillaBookmark(_))));
    assert!(snapshot(&app.project) == dirty);
}
#[test]
fn editor_update_uses_the_existing_unsaved_project_decision() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("unsaved update test");
    app.track();
    app.flush();
    app.request(Pending::EditorUpdate, &ctx);
    assert!(matches!(app.pending, Some(Pending::EditorUpdate)));
    assert!(!app.updater.open);
    app.pending = None; // Cancelling must retain the unsaved project.
    assert!(app.dirty());
    assert_eq!(app.story()["nodes"][0]["text"], "unsaved update test");
}

#[test]
fn file_menu_groups_secondary_actions_without_changing_the_project() {
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = navigation_frame(&mut app, &ctx, vec![]);
    let event = click_named_option(&output, &tr("文件"));
    navigation_frame(&mut app, &ctx, vec![event]);
    let output = navigation_frame(&mut app, &ctx, vec![]);
    let labels: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label())
        .collect();
    for key in [
        "editor.workspace.new_project",
        "editor.workspace.original_tools",
        "editor.workspace.export_menu",
        "editor.workspace.preview_assets",
    ] {
        assert!(
            labels
                .iter()
                .any(|label| label.starts_with(&crate::i18n::key(key))),
            "missing menu {key}: {labels:?}"
        );
    }
    assert!(!labels.contains(&crate::i18n::key("editor.workspace.import_original").as_str()));
    assert!(!labels.contains(&tr("从游戏提取预览素材…").as_str()));
    assert!(!labels.contains(&tr("导出诊断报告…").as_str()));
    let event = click_named_option(
        &output,
        &crate::i18n::key("editor.workspace.original_tools"),
    );
    navigation_frame(&mut app, &ctx, vec![event]);
    let output = navigation_frame(&mut app, &ctx, vec![]);
    let labels: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label())
        .collect();
    for key in [
        "editor.workspace.import_original",
        "editor.workspace.routes",
        "editor.workspace.bookmark",
    ] {
        assert!(
            labels.contains(&crate::i18n::key(key).as_str()),
            "missing action {key}: {labels:?}"
        );
    }
    assert!(snapshot(&app.project) == before);
}

#[test]
fn primary_tabs_are_direct_click_targets_and_preserve_story_selection() {
    let mut app = app();
    let before = snapshot(&app.project);
    let selection = (app.current.clone(), app.selected);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = navigation_frame(&mut app, &ctx, vec![]);
    let nodes = output.platform_output.accesskit_update.unwrap().nodes;
    for (section, label) in [
        (Section::Materials, "素材管理"),
        (Section::Game, "游戏与 MOD"),
        (Section::Release, "发布"),
        (Section::Story, "剧情编辑"),
    ] {
        let target = nodes
            .iter()
            .find_map(|(id, node)| {
                (node.label() == Some(tr(label).as_str())
                    && node.role() == egui::accesskit::Role::Button)
                    .then_some(*id)
            })
            .expect("task tab must be directly visible");
        navigation_frame(
            &mut app,
            &ctx,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        assert_eq!(app.section(), section, "{label}");
        assert_eq!(app.story_panels_visible(), section == Section::Story);
        assert_eq!((app.current.clone(), app.selected), selection);
        assert!(snapshot(&app.project) == before);
    }
    app.open_tool_page(ToolPage::Editing);
    app.navigate(Section::Story);
    assert_eq!(
        app.center,
        Center::Node,
        "story tab returns from editing tools"
    );
}
#[test]
fn non_story_pages_do_not_accept_hidden_node_delete_or_paste() {
    let mut app = app();
    app.copy();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    for section in [Section::Materials, Section::Game, Section::Release] {
        app.navigate(section);
        navigation_frame(
            &mut app,
            &ctx,
            vec![
                key_event(egui::Key::Delete, egui::Modifiers::NONE),
                key_event(egui::Key::V, egui::Modifiers::COMMAND),
            ],
        );
        assert!(
            snapshot(&app.project) == before,
            "hidden story mutated from {section:?}"
        );
    }
}
#[test]
fn navigation_keeps_invalid_material_input_until_the_user_resolves_it() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hero.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    app.import_assets(vec![image], "test.hero", "hero", 0)
        .unwrap();
    app.center = Center::Assets;
    let mut draft: Value =
        serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    draft["files"]["main"] = json!("../outside.png");
    app.content_panel = crate::content_panel::ContentPanel::pending_fixture(
        &app.project,
        &app.asset_selection,
        draft,
    );
    let before = snapshot(&app.project);
    for section in [Section::Game, Section::Release, Section::Story] {
        app.navigate(section);
        assert_eq!(app.center, Center::Assets);
        assert!(app.content_panel.has_pending());
        assert!(app.error.is_some());
        assert!(snapshot(&app.project) == before);
    }
    app.open_tool_page(ToolPage::Library);
    assert_eq!(app.center, Center::Assets);
    let ctx = egui::Context::default();
    for event in [
        key_event(
            egui::Key::F,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        ),
        key_event(egui::Key::F7, egui::Modifiers::NONE),
    ] {
        navigation_frame(&mut app, &ctx, vec![event]);
        assert_eq!(app.center, Center::Assets);
        assert!(app.content_panel.has_pending());
        assert!(snapshot(&app.project) == before);
    }
}
#[test]
fn every_task_page_can_be_read_without_modifying_the_project() {
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    for page in [
        ToolPage::Editing,
        ToolPage::Previews,
        ToolPage::Library,
        ToolPage::Game,
        ToolPage::Release,
        ToolPage::Maintenance,
    ] {
        app.open_tool_page(page);
        navigation_frame(&mut app, &ctx, vec![]);
        assert!(snapshot(&app.project) == before, "{page:?}");
    }
}
#[test]
fn visible_help_button_and_f1_open_the_guide_without_changing_the_project() {
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                app.toolbar(ctx);
                app.tools_panel.show_dialog(ctx, &app.project);
            },
        )
    };
    let output = draw(&mut app, vec![]);
    let help = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| {
            (node.role() == egui::accesskit::Role::Button
                && node.label() == Some(crate::i18n::key("menu.help_item").as_str()))
            .then_some(id)
        })
        .expect("Help must be directly visible on the toolbar");
    let output = draw(
        &mut app,
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target: help,
                data: None,
            },
        )],
    );
    assert!(app.tools_panel.dialog_open());
    let text = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains(&crate::i18n::key("help.guide_tab")), "{text}");
    assert!(
        text.contains(&crate::i18n::key("help.reference_tab")),
        "{text}"
    );
    assert!(
        text.contains("F6"),
        "The guide must be open without another expansion: {text}"
    );
    draw(
        &mut app,
        vec![key_event(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    assert!(app.pending.is_none());
    draw(
        &mut app,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!app.tools_panel.dialog_open());
    draw(
        &mut app,
        vec![key_event(egui::Key::F1, egui::Modifiers::NONE)],
    );
    assert!(app.tools_panel.dialog_open());
    assert!(snapshot(&app.project) == before);
    assert!(!app.dirty());
}
#[test]
fn node_description_button_opens_its_own_reference_without_editing() {
    let mut app = app();
    app.selected = 1;
    assert_eq!(app.story()["nodes"][1]["type"], "end");
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1200.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.center(ctx);
                app.tools_panel.show_dialog(ctx, &app.project);
            },
        )
    };
    let output = draw(&mut app, vec![]);
    let target = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| {
            (node.role() == egui::accesskit::Role::Button
                && node.label() == Some(tr("说明").as_str()))
            .then_some(id)
        })
        .expect("Description must be a clickable button, not just a hover label");
    let output = draw(
        &mut app,
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target,
                data: None,
            },
        )],
    );
    assert!(app.tools_panel.dialog_open());
    let text = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .filter_map(|(_, n)| n.label().or_else(|| n.value()).map(str::to_owned))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("结束后回到哪里"),
        "Opened the wrong article: {text}"
    );
    assert!(
        text.contains("下一章节"),
        "Missing end-node guidance: {text}"
    );
    assert!(snapshot(&app.project) == before);
    assert!(!app.dirty());
}
#[test]
fn advanced_node_rename_preserves_links_and_localizations() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["goto"] = json!("end1");
    app.project.stories.get_mut("main").unwrap()["localization"] =
        json!({"translations":{"cht":{"say1.text":"春日"}}});
    app.rename_dialog = Some(RenameDialog {
        target: RenameTarget::Node {
            story: "main".into(),
            node: "say1".into(),
        },
        value: "greeting".into(),
        error: None,
        focus_name: false,
    });
    app.commit_rename().unwrap();
    assert_eq!(app.story()["start"], "greeting");
    assert_eq!(app.story()["nodes"][0]["id"], "greeting");
    assert_eq!(app.story()["nodes"][0]["goto"], "end1");
    assert_eq!(
        app.story()["localization"]["translations"]["cht"]["greeting.text"],
        "春日"
    );
    assert!(app.story()["localization"]["translations"]["cht"]
        .get("say1.text")
        .is_none());
}
#[test]
fn import_confirmation_generates_unique_identity_and_keeps_display_name() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("春日.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    app.project.manifest["id"] = json!("my_mod");
    app.asset_import_dialog = Some(AssetImportDialog {
        extends: String::new(),
        kind: 0,
        name: "".into(),
        paths: vec![image.clone()],
        error: None,
    });
    let before = snapshot(&app.project);
    assert!(app.commit_asset_import().is_err());
    assert!(snapshot(&app.project) == before);
    app.asset_import_dialog.as_mut().unwrap().name = "春日背景".into();
    app.commit_asset_import().unwrap();
    let first = app.project.assets.clone();
    let meta: Value = serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    assert_eq!(meta["id"], "my_mod.image1");
    assert_eq!(meta["name"], "春日背景");
    app.asset_import_dialog = Some(AssetImportDialog {
        extends: String::new(),
        kind: 0,
        name: "另一张背景".into(),
        paths: vec![image],
        error: None,
    });
    app.commit_asset_import().unwrap();
    let meta: Value = serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    assert_eq!(meta["id"], "my_mod.image2");
    for (key, bytes) in first {
        assert_eq!(app.project.assets[&key], bytes);
    }
    let committed = snapshot(&app.project);
    app.asset_import_dialog = Some(AssetImportDialog {
        extends: String::new(),
        kind: 1,
        name: "取消".into(),
        paths: vec![],
        error: None,
    });
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    assert!(app.asset_import_dialog.is_none());
    assert!(snapshot(&app.project) == committed);
}
