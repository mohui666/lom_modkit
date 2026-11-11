use super::*;

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
                Some(compile_with_assets(&app.project, &app.story()).unwrap())
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
