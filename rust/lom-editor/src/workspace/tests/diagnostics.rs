use super::*;

#[test]
fn live_check_drops_a_stale_result_and_generates_lua_only_when_visible() {
    let mut app = app();
    let ctx = egui::Context::default();
    let node = &mut app.project.stories.get_mut("main").unwrap()["nodes"][0];
    *node = json!({"id":"n1", "type":"say", "mode":"narrative", "text":"检查前的对白"});
    app.project.stories.get_mut("main").unwrap()["start"] = json!("n1");
    app.view = View::Stage;
    app.frame = 1;
    app.update_live_compile(&ctx);
    assert!(app.live_compile.is_some());

    app.frame = 2;
    app.project.stories.get_mut("main").unwrap()["title"] = json!("输入后的标题");
    app.track();
    let wait_for_result = |app: &mut App| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.live_compile.is_some() {
            assert!(Instant::now() < deadline, "live check did not finish");
            app.last_edit = Instant::now();
            app.update_live_compile(&ctx);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    wait_for_result(&mut app);
    assert!(app.needs_compile);
    assert!(app.lua.is_empty());

    app.last_edit = Instant::now() - live_compile::DEBOUNCE;
    app.update_live_compile(&ctx);
    wait_for_result(&mut app);
    assert!(!app.needs_compile);
    assert!(app.lua.is_empty());

    app.view = View::Lua;
    app.needs_compile = true;
    app.last_edit = Instant::now() - live_compile::DEBOUNCE;
    app.update_live_compile(&ctx);
    wait_for_result(&mut app);
    assert!(app.lua.contains("输入后的标题"), "{}", app.lua);
}

#[test]
fn error_dialog_shows_an_action_and_preserves_raw_diagnostics() {
    for (message, action) in [
        (
            "原版音频试听组件缺失：C:/Modkit/audio-decoder/vgmstream-cli.exe",
            "重新打开",
        ),
        (
            "无法保存：C:/作品/story/chapter1.json: Permission denied (os error 5)",
            "另存为",
        ),
        (
            "打开失败：unrecognized project metadata in C:/作品/project.json",
            "开发者",
        ),
    ] {
        let mut app = app();
        app.error = Some(message.into());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1200.0, 1000.0),
                )),
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        let update = output.platform_output.accesskit_update.unwrap();
        let text = update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(action), "{text}");
        assert!(!text.contains(message), "{text}");
        assert_eq!(app.error.as_deref(), Some(message));
        assert!(output.platform_output.commands.is_empty());
    }
}

#[test]
fn feedback_from_error_previews_only_structured_diagnostics() {
    let mut app = app();
    let error =
        "无法保存 C:/private-project/story/private-chapter.json: Permission denied (os error 5)";
    app.error = Some(error.into());
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.style_mut(|style| style.animation_time = 0.0);
    let mut render = |events| {
        ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                app.dialogs(ctx);
            },
        )
    };
    let output = render(vec![]);
    render(vec![click_named_option(
        &output,
        &crate::i18n::key("feedback.title"),
    )]);
    let output = render(vec![]);
    let output = render(vec![click_named_option(
        &output,
        &crate::i18n::key("feedback.details"),
    )]);
    let text = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("editor.error.ops_permission"), "{text}");
    assert!(text.contains(r#""os_error": 5"#), "{text}");
    assert!(!text.contains("private-project"), "{text}");
    assert_eq!(app.error.as_deref(), Some(error));
}

#[test]
fn lua_copy_and_export_recompile_instead_of_treating_error_prose_as_a_script() {
    for valid in [true, false] {
        let mut app = app();
        if !valid {
            app.project.stories.insert(
                "main".into(),
                json!({"id":"main","story_schema":2,"start":"broken","nodes":[
                    {"id":"broken","type":"effect","name":""}, {"id":"end","type":"end"}
                ]}),
            );
        }
        app.view = View::Lua;
        app.lua = "缓存中的提示文字，不能当作脚本复制或导出".into();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let output = ctx.run(input(vec![]), |ctx| app.right(ctx));
        let click = click_named_option(&output, &tr("复制 Lua"));
        let output = ctx.run(input(vec![click]), |ctx| app.right(ctx));
        let copied = output
            .platform_output
            .commands
            .iter()
            .find_map(|cmd| match cmd {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            });
        if valid {
            assert_eq!(
                copied,
                Some(compile_with_assets(&app.project, &app.story().clone()).unwrap())
            );
            assert!(app.error.is_none());
        } else {
            assert!(copied.is_none());
            assert!(app.error.as_deref().unwrap().contains("特效名称不能为空"));
            app.error = None;
            let click = click_named_option(&output, &tr("导出 Lua…"));
            let _ = ctx.run(input(vec![click]), |ctx| app.right(ctx));
            assert!(app.error.as_deref().unwrap().contains("特效名称不能为空"));
        }
        assert!(snapshot(&app.project) == before);
    }
}
#[test]
fn error_button_opens_and_focuses_the_manifest_field_without_changing_the_project() {
    let mut app = app();
    app.project.manifest["play_mode"] = json!("sandbox");
    app.project.manifest["campaign_id"] = json!("");
    let before = snapshot(&app.project);
    app.error = Some("导出失败\nmanifest.json: 存档身份未填写。".into());
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let output = ctx.run(input(vec![]), |ctx| app.dialogs(ctx));
    let click = click_named_option(&output, "去填写存档身份");
    let _ = ctx.run(input(vec![click]), |ctx| app.dialogs(ctx));
    assert!(app.error.is_none());
    assert_eq!(app.center, Center::Manifest);
    let _ = ctx.run(input(vec![]), |ctx| app.center(ctx));
    assert!(ctx.memory(|memory| memory.focused().is_some()));
    assert!(ctx
        .data(|data| data.get_temp::<String>(egui::Id::new("problem-field")))
        .is_none());
    assert!(snapshot(&app.project) == before);
}
