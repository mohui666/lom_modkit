use super::*;

#[test]
fn startup_update_preference_preserves_other_settings_and_the_project() {
    let mut app = app();
    let before = snapshot(&app.project);
    let directory = tempfile::tempdir().unwrap();
    let prefs = directory.path().join("preferences.json");
    std::fs::write(
        &prefs,
        r#"{"ui_locale":"chs","preview_library_dir":"keep previews","custom":"keep data"}"#,
    )
    .unwrap();
    app.set_startup_update_check(false, &prefs).unwrap();
    let value = lom_core::load_json(&prefs).unwrap();
    assert_eq!(value["check_updates_on_startup"], false);
    assert_eq!(value["ui_locale"], "chs");
    assert_eq!(value["preview_library_dir"], "keep previews");
    assert_eq!(value["custom"], "keep data");
    assert!(!app.check_updates_on_startup);
    assert!(snapshot(&app.project) == before);
}
#[test]
fn focused_text_input_keeps_project_undo_redo_and_delete_shortcuts() {
    let mut app = app();
    app.project.manifest["name"] = json!("keep this project edit");
    app.track();
    app.flush();
    let before = snapshot(&app.project);
    let history = (app.undo.len(), app.redo.len());
    let ctx = egui::Context::default();
    let id = egui::Id::new("typing-dialogue");
    let mut text = "已有台词".to_owned();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).id(id))
                .request_focus();
        });
    });
    for (key, modifiers) in [
        (egui::Key::Z, egui::Modifiers::COMMAND),
        (
            egui::Key::Z,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        ),
        (egui::Key::Y, egui::Modifiers::COMMAND),
        (egui::Key::Backspace, egui::Modifiers::NONE),
        (egui::Key::Delete, egui::Modifiers::NONE),
        (egui::Key::D, egui::Modifiers::COMMAND),
        (egui::Key::Enter, egui::Modifiers::COMMAND),
        (
            egui::Key::Enter,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        ),
        (egui::Key::A, egui::Modifiers::COMMAND),
        (egui::Key::F, egui::Modifiers::COMMAND),
        (egui::Key::ArrowUp, egui::Modifiers::ALT),
        (egui::Key::ArrowDown, egui::Modifiers::ALT),
        (egui::Key::Space, egui::Modifiers::NONE),
    ] {
        let _ = ctx.run(
            egui::RawInput {
                modifiers,
                events: vec![key_event(key, modifiers)],
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                assert!(
                    ctx.input(|input| input.key_pressed(key)),
                    "the input must receive {key:?}"
                );
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut text).id(id));
                });
            },
        );
        assert!(snapshot(&app.project) == before);
        assert_eq!((app.undo.len(), app.redo.len()), history);
        assert!(app.multiselect.is_empty());
        assert!(!app.auto);
    }
    ctx.memory_mut(|memory| memory.surrender_focus(id));
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert!(
        snapshot(&app.project) != before,
        "project undo still works outside text inputs"
    );
}

#[test]
fn duplicate_shortcut_keeps_internal_edges_clipboard_and_one_undo() {
    for modifiers in [
        egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND,
    ] {
        let mut app = app();
        app.project.stories.insert(
            "main".into(),
            json!({"id":"main","start":"a","nodes":[
            {"id":"a","type":"wait","seconds":0.1,"goto":"b"},
            {"id":"b","type":"wait","seconds":0.2,"goto":"done"},
            {"id":"done","type":"end"}]}),
        );
        app.selected = 0;
        app.multiselect = [0, 1].into();
        app.clipboard = vec![json!({"id":"keep","type":"wait","seconds":9})];
        app.track();
        app.flush();
        let original = snapshot(&app.project);
        let clipboard = app.clipboard.clone();
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::D, modifiers)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        app.track();
        app.flush();
        assert_eq!(app.clipboard, clipboard);
        let story = app.story();
        let nodes = story["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[0]["goto"], "b");
        assert_eq!(nodes[2]["goto"], nodes[3]["id"]);
        assert_eq!(nodes[3]["goto"], "done");
        assert_eq!(app.multiselect, [2, 3].into());
        app.undo();
        assert!(snapshot(&app.project) == original);
    }
}

#[test]
fn authoring_shortcuts_move_select_and_add_steps_without_changing_existing_content() {
    let mut app = app();
    app.project.stories.insert(
        "main".into(),
        json!({"id":"main","start":"a","nodes":[
        {"id":"a","type":"wait","seconds":0.1},{"id":"b","type":"wait","seconds":0.2},
        {"id":"c","type":"wait","seconds":0.3},{"id":"done","type":"end"}]}),
    );
    let original = app.story();
    app.selected = 2;
    app.multiselect = [1, 2].into();
    let ctx = egui::Context::default();
    let press = |app: &mut App, key, modifiers| {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(key, modifiers)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
    };
    press(&mut app, egui::Key::ArrowUp, egui::Modifiers::ALT);
    assert_eq!(app.story()["nodes"][0]["id"], "b");
    assert_eq!(app.story()["nodes"][1]["id"], "c");
    press(&mut app, egui::Key::ArrowDown, egui::Modifiers::ALT);
    assert_eq!(app.story(), original);
    press(&mut app, egui::Key::A, egui::Modifiers::COMMAND);
    assert_eq!(app.multiselect.len(), 4);
    press(&mut app, egui::Key::Enter, egui::Modifiers::COMMAND);
    assert_eq!(app.story()["nodes"].as_array().unwrap().len(), 5);
    assert_eq!(app.story()["nodes"][app.selected]["type"], "say");
}

#[test]
fn shortcut_search_gets_focus_and_preserves_the_existing_global_search_binding() {
    let mut app = app();
    let original = snapshot(&app.project);
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events: vec![key_event(egui::Key::F, egui::Modifiers::COMMAND)],
            ..Default::default()
        },
        |ctx| {
            app.shortcuts(ctx);
            app.sidebar(ctx);
        },
    );
    let search = egui::Id::new("story-step-search");
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(search));
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("查找台词".into())],
            ..Default::default()
        },
        |ctx| {
            app.shortcuts(ctx);
            app.sidebar(ctx);
        },
    );
    assert_eq!(app.search, "查找台词");
    ctx.memory_mut(|memory| memory.surrender_focus(search));
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(
                egui::Key::F,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            )],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert_eq!(app.center, Center::Advanced);
    assert_eq!(app.advanced.page, 0);
    assert!(snapshot(&app.project) == original);
}

#[test]
fn authoring_shortcuts_leave_ime_popup_and_repeat_events_alone() {
    let mut app = app();
    let original = snapshot(&app.project);
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::Ime(egui::ImeEvent::Preedit("候选".into())),
                key_event(egui::Key::D, egui::Modifiers::COMMAND),
            ],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    let mut release = key_event(egui::Key::D, egui::Modifiers::COMMAND);
    if let egui::Event::Key { pressed, .. } = &mut release {
        *pressed = false;
    }
    let _ = ctx.run(
        egui::RawInput {
            events: vec![release],
            ..Default::default()
        },
        |_| {},
    );
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(egui::Key::D, egui::Modifiers::COMMAND)],
            ..Default::default()
        },
        |ctx| {
            egui::Popup::open_id(ctx, egui::Id::new("shortcut-popup"));
            app.shortcuts(ctx);
            egui::Popup::close_all(ctx);
        },
    );
    let mut repeat = key_event(egui::Key::D, egui::Modifiers::COMMAND);
    if let egui::Event::Key {
        repeat: repeated, ..
    } = &mut repeat
    {
        *repeated = true;
    }
    let _ = ctx.run(
        egui::RawInput {
            events: vec![repeat],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert!(snapshot(&app.project) == original);
}

#[test]
fn space_shortcut_pauses_and_resumes_editor_preview_without_launching_the_game() {
    let mut app = app();
    let original = snapshot(&app.project);
    let ctx = egui::Context::default();
    let press = |app: &mut App| {
        let down = key_event(egui::Key::Space, egui::Modifiers::NONE);
        let mut up = down.clone();
        if let egui::Event::Key { pressed, .. } = &mut up {
            *pressed = false;
        }
        let _ = ctx.run(
            egui::RawInput {
                events: vec![down, up],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
    };
    press(&mut app);
    assert!(app.auto);
    app.last_step = Instant::now() - Duration::from_millis(200);
    press(&mut app);
    assert!(!app.auto);
    assert!(app.preview_elapsed.unwrap() >= 0.2);
    press(&mut app);
    assert!(app.auto);
    assert!(snapshot(&app.project) == original);
    app.auto = false;
    app.view = View::Graph;
    press(&mut app);
    assert!(!app.auto);
}

#[test]
fn system_clipboard_events_copy_and_paste_steps_once() {
    let mut app = app();
    let original_count = app.story()["nodes"].as_array().unwrap().len();
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::Copy,
                key_event(egui::Key::C, egui::Modifiers::COMMAND),
            ],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert_eq!(app.clipboard.len(), 1);
    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::Paste(String::new()),
                key_event(egui::Key::V, egui::Modifiers::COMMAND),
            ],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert_eq!(
        app.story()["nodes"].as_array().unwrap().len(),
        original_count + 1
    );
}
#[test]
fn settings_shortcut_and_language_preferences_do_not_modify_the_project() {
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers::COMMAND,
            events: vec![key_event(egui::Key::Comma, egui::Modifiers::COMMAND)],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert!(app.settings_open);
    let _ = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers::COMMAND,
            events: vec![key_event(egui::Key::N, egui::Modifiers::COMMAND)],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert!(snapshot(&app.project) == before);
    let dir = tempfile::tempdir().unwrap();
    let prefs = dir.path().join("preferences.json");
    std::fs::write(&prefs, br#"{"preview_library":"keep-me"}"#).unwrap();
    let locale = crate::i18n::locale();
    app.set_interface_locale(locale, &prefs).unwrap();
    let saved = lom_core::load_json(&prefs).unwrap();
    assert_eq!(saved["ui_locale"], locale);
    assert_eq!(saved["preview_library"], "keep-me");
    assert!(app.set_interface_locale("unsupported", &prefs).is_err());
    assert_eq!(lom_core::load_json(&prefs).unwrap(), saved);
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    assert!(!app.settings_open);
    assert!(snapshot(&app.project) == before);
    assert!(!app.dirty());
}
#[test]
#[cfg(windows)]
fn windows_settings_shortcut_accepts_observed_release_without_keydown() {
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            // The real Windows event retained Ctrl even though the frame's
            // final modifier state was already NONE.
            modifiers: egui::Modifiers::NONE,
            events: vec![egui::Event::Key {
                key: egui::Key::Comma,
                physical_key: Some(egui::Key::Comma),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert!(app.settings_open);
    assert!(snapshot(&app.project) == before);
}
#[test]
#[cfg(windows)]
fn windows_settings_shortcut_does_not_repeat_normal_keydown_or_plain_comma() {
    let mut app = app();
    let ctx = egui::Context::default();
    let command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
    for initial_modifiers in [command, egui::Modifiers::NONE] {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::Comma, initial_modifiers)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert_eq!(app.settings_open, initial_modifiers.command);
        // Closing before key-up must not reopen settings. A plain comma
        // followed by Ctrl must not become a shortcut either.
        app.settings_open = false;
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Comma,
                    physical_key: Some(egui::Key::Comma),
                    pressed: false,
                    repeat: false,
                    modifiers: command,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(!app.settings_open);
    }
}
#[test]
#[cfg(windows)]
fn windows_settings_release_respects_modal_focus_and_exact_modifiers() {
    let mut app = app();
    let ctx = egui::Context::default();
    let command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
    let before = snapshot(&app.project);
    for (modal, focused, modifiers) in [
        (true, true, command),
        (false, false, command),
        (false, true, egui::Modifiers::NONE),
        (false, true, command | egui::Modifiers::ALT),
        (false, true, command | egui::Modifiers::SHIFT),
    ] {
        if modal {
            app.begin_chapter(false);
        }
        let _ = ctx.run(
            egui::RawInput {
                focused,
                events: vec![egui::Event::Key {
                    key: egui::Key::Comma,
                    physical_key: Some(egui::Key::Comma),
                    pressed: false,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(!app.settings_open);
        assert!(snapshot(&app.project) == before);
        app.chapter_dialog = None;
    }
}
#[test]
fn modal_blocks_background_clicks_and_project_shortcuts() {
    let mut app = app();
    app.project.manifest["name"] = json!("pending title");
    app.track();
    app.flush();
    app.begin_chapter(false);
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    let mut button_rect = egui::Rect::NOTHING;
    let mut clicked = false;
    let mut draw = |ctx: &egui::Context| {
        app.shortcuts(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            let response = ui.button("background action");
            button_rect = response.rect;
            clicked |= response.clicked();
        });
        app.dialogs(ctx);
    };
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(900.0, 800.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run(raw.clone(), &mut draw);
    let pos = egui::pos2(20.0, 20.0);
    for pressed in [true, false] {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..raw.clone()
            },
            &mut draw,
        );
    }
    let _ = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers::COMMAND,
            events: vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
            ..raw
        },
        &mut draw,
    );
    assert!(button_rect.contains(pos));
    assert!(!clicked);
    assert!(app.chapter_dialog.is_some());
    assert!(snapshot(&app.project) == before);
}
#[test]
fn tool_result_modal_blocks_edit_shortcuts_and_preserves_project() {
    let mut app = app();
    app.project.manifest["name"] = json!("pending title");
    app.track();
    app.flush();
    let before = snapshot(&app.project);
    app.tools_panel
        .present("安装检查", json!({"healthy":true,"game_tested":false}));
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers::COMMAND,
            events: vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
            ..Default::default()
        },
        |ctx| {
            app.shortcuts(ctx);
            app.dialogs(ctx);
            app.tools_panel.show_dialog(ctx, &app.project);
        },
    );
    assert!(app.tools_panel.dialog_open());
    assert!(snapshot(&app.project) == before);
}
#[test]
fn command_shift_z_redoes_instead_of_consuming_undo() {
    let mut app = app();
    let original = app.project.manifest["name"].clone();
    app.project.manifest["name"] = json!("edited");
    app.track();
    app.flush();
    app.undo();
    assert_eq!(app.project.manifest["name"], original);
    let ctx = egui::Context::default();
    let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    let _ = ctx.run(
        egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert_eq!(app.project.manifest["name"], "edited");
    assert!(app.redo.is_empty());
}
