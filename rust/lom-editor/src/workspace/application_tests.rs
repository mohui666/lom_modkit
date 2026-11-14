use super::*;

fn app() -> App {
    let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
    let mut app = App::new(&cc, None, None);
    app.recovery_candidates.clear();
    app.auto_save_project = false;
    app.auto_save_seconds = 300;
    app
}

fn saved_preview_fixture(root: &Path, name: &str) -> PathBuf {
    let directory = root.join(name);
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    image::RgbaImage::from_pixel(2, 1, image::Rgba([27, 164, 219, 255]))
        .save(directory.join("assets/preview.png"))
        .unwrap();
    std::fs::write(
        directory.join("preview_map.json"),
        serde_json::to_vec(&json!({"characters":{},"views":{"center":"assets/preview.png"}}))
            .unwrap(),
    )
    .unwrap();
    directory
}

#[test]
fn saved_preview_library_survives_an_unrelated_new_preferences_file() {
    let directory = tempfile::tempdir().unwrap();
    let old_library = saved_preview_fixture(directory.path(), "existing images");
    let settings = directory.path().join("settings.json");
    let original = serde_json::to_vec(&json!({"preview_library_dir":old_library})).unwrap();
    std::fs::write(&settings, &original).unwrap();
    let preferences = directory.path().join("preferences.json");
    let prefs_bytes = br#"{"ui_locale":"chs","quick_shortcuts":["node:wait"]}"#;
    std::fs::write(&preferences, prefs_bytes).unwrap();
    let prefs = lom_core::load_json(&preferences).unwrap();
    let mut preview = Preview::new(&repository());
    restore_saved_preview_library(&mut preview, &prefs, &settings).unwrap();
    assert_eq!(preview.library, old_library);
    assert_eq!(std::fs::read(&settings).unwrap(), original);
    assert_eq!(std::fs::read(&preferences).unwrap(), prefs_bytes);

    let selected = saved_preview_fixture(directory.path(), "new selection");
    std::fs::write(&settings, b"invalid legacy JSON").unwrap();
    restore_saved_preview_library(
        &mut preview,
        &json!({"preview_library_dir":selected}),
        &settings,
    )
    .unwrap();
    assert_eq!(preview.library, selected);
}

#[test]
fn saved_preview_library_failure_keeps_the_cache_and_explains_reselection() {
    let directory = tempfile::tempdir().unwrap();
    let previous = saved_preview_fixture(directory.path(), "working images");
    let original_image = std::fs::read(previous.join("assets/preview.png")).unwrap();
    let broken = directory.path().join("broken index");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("preview_map.json"), b"invalid JSON").unwrap();
    let settings = directory.path().join("settings.json");
    std::fs::write(
        &settings,
        serde_json::to_vec(&json!({"preview_library_dir":previous})).unwrap(),
    )
    .unwrap();
    let mut preview = Preview::new(&repository());
    preview.set_library(&previous).unwrap();
    let error = restore_saved_preview_library(
        &mut preview,
        &json!({"preview_library_dir":broken}),
        &settings,
    )
    .unwrap_err();
    let diagnostics = crate::error_ui::capture(error);
    assert!(diagnostics.contains(broken.to_str().unwrap()));
    assert_eq!(
        crate::error_location::resolve(&diagnostics, &Project::new()),
        crate::error_location::Destination::ExistingPreviewLibrary,
    );
    assert_eq!(
        crate::error_ui::text(&diagnostics),
        crate::i18n::key("editor.preview.restore_failed")
    );
    assert_eq!(preview.library, previous);
    assert_eq!(
        std::fs::read(previous.join("assets/preview.png")).unwrap(),
        original_image
    );
    assert_eq!(
        std::fs::read(broken.join("preview_map.json")).unwrap(),
        b"invalid JSON"
    );
}

#[test]
fn single_layout_loads_previously_selected_widths() {
    let widths = [[245.0, 420.0], [215.0, 365.0], [230.0, 595.0]];
    for selected in 0..3 {
        let prefs = usability::Preferences::load(&json!({
            "workspace_layout":selected, "workspace_widths":widths
        }));
        assert_eq!(prefs.widths, widths[selected]);
    }
    let prefs = usability::Preferences::load(&json!({
        "workspace_widths":[225.0,455.0], "workspace_layout":2
    }));
    assert_eq!(prefs.widths, [225.0, 455.0]);
}

#[test]
fn typing_and_saving_a_mood_default_applies_to_the_next_added_step() {
    use egui::accesskit::Role;
    let mut app = app();
    let mut project = Project::new();
    project.stories.insert(
        "main".into(),
        json!({"id":"main","start":"mood0","nodes":[
        {"id":"mood0","type":"mood","enabled":true},{"id":"done","type":"end"}]}),
    );
    app.install(project);
    app.catalog = Catalog::new();
    let directory = tempfile::tempdir().unwrap();
    let preferences = directory.path().join("node-defaults.json");
    app.catalog
        .configure_parameter_defaults(preferences.clone())
        .unwrap();
    let original = app.story()["nodes"][0].clone();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.steps_panel_open = true;
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1360.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.sidebar(ctx);
                app.step_palette(ctx);
                app.right(ctx);
                app.center(ctx);
            },
        )
    };
    let button = |output: &egui::FullOutput, label: &str| {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.role() == Role::Button && node.label() == Some(label))
                    .then(|| node.bounds().unwrap())
            })
            .unwrap_or_else(|| panic!("missing {label}"))
    };
    let click = |app: &mut App, bounds: egui::accesskit::Rect| {
        let pos = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        );
        for pressed in [true, false] {
            draw(
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    click(
        &mut app,
        button(&output, &crate::i18n::key("editor.defaults.manage")),
    );
    let output = draw(&mut app, vec![]);
    let reset = button(&output, &crate::i18n::key("editor.defaults.field_reset"));
    let input = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::SpinButton)
        .filter_map(|(_, node)| node.bounds())
        .find(|bounds| {
            ((bounds.y0 + bounds.y1 - reset.y0 - reset.y1) / 2.0).abs() < 3.0
                && bounds.x1 <= reset.x0
        })
        .unwrap();
    click(&mut app, input);
    draw(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
    );
    let output = draw(&mut app, vec![egui::Event::Text("0.5".into())]);
    assert_eq!(
        app.story()["nodes"][0],
        original,
        "default editing must keep the current step unchanged"
    );
    click(
        &mut app,
        button(&output, &crate::i18n::key("editor.defaults.save")),
    );
    draw(&mut app, vec![]);
    assert!(ctx
        .data(|data| data.get_temp::<String>(egui::Id::new("saved-new-step-defaults")))
        .is_some());
    assert_eq!(
        lom_core::load_json(&preferences).unwrap()["mood"]["wait_after"],
        json!(0.5)
    );
    app.node_search = "mood".into();
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let mood = app.catalog.label("mood");
    click(&mut app, button(&output, &mood));
    assert_eq!(
        app.story()["nodes"].as_array().unwrap().len(),
        3,
        "{}",
        app.story()
    );
    assert_eq!(
        app.story()["nodes"][app.selected]["wait_after"],
        json!(0.5),
        "{}",
        app.story()
    );
    assert_eq!(app.story()["nodes"][0], original);
    let mut reopened = Catalog::new();
    reopened.configure_parameter_defaults(preferences).unwrap();
    assert_eq!(
        reopened.new_node("mood", "after_restart".into())["wait_after"],
        json!(0.5)
    );
}

#[test]
fn continuous_dialogue_entry_is_beside_add_steps_and_search_is_on_the_right() {
    use egui::accesskit::Role;
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(1280.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let output = ctx.run(input(vec![]), |ctx| app.toolbar(ctx));
    let nodes = output.platform_output.accesskit_update.unwrap().nodes;
    assert!(!nodes.iter().any(|(_, node)| node.role() == Role::ComboBox));
    assert!(!nodes
        .iter()
        .any(|(_, node)| node.label()
            == Some(crate::i18n::key("usability.dialogue.button").as_str())));
    let search = nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some(crate::i18n::key("usability.search.button").as_str())
        })
        .unwrap();
    assert!(search.1.bounds().unwrap().x0 > 1200.0);
    assert!(
        1280.0 - search.1.bounds().unwrap().x1 <= 20.0,
        "the search icon must align to the actual page bar's right edge"
    );
    app.center = Center::Node;
    app.steps_panel_open = true;
    let _ = ctx.run(input(vec![]), |ctx| app.sidebar(ctx));
    let output = ctx.run(input(vec![]), |ctx| app.sidebar(ctx));
    let event = click_named_option(&output, &crate::i18n::key("usability.dialogue.add_steps"));
    let _ = ctx.run(input(vec![event]), |ctx| {
        app.sidebar(ctx);
        app.quick_tools(ctx);
    });
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("continuous-dialogue-source"))
    );
    assert!(snapshot(&app.project) == before);
}

#[test]
fn step_multiselect_checkboxes_only_show_while_mode_is_active() {
    use egui::accesskit::Role;
    let mut app = app();
    let mut project = Project::new();
    project.stories.get_mut("main").unwrap()["nodes"] = json!([
        {"id":"a","type":"wait","seconds":1},
        {"id":"b","type":"wait","seconds":2},
        {"id":"c","type":"wait","seconds":3},
        {"id":"done","type":"end"}
    ]);
    project.stories.get_mut("main").unwrap()["start"] = json!("a");
    app.install(project);
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1280.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        )
    };
    let has_checkboxes = |output: &egui::FullOutput| {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::CheckBox)
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    assert!(!has_checkboxes(&output));
    let event = click_named_option(&output, &crate::i18n::key("usability.steps.multiselect"));
    let output = draw(&mut app, vec![event]);
    assert!(has_checkboxes(&output));
    let event = click_named_option(&output, "选择步骤 2");
    let output = draw(&mut app, vec![event]);
    assert_eq!(app.selected_indices(), [0, 1]);
    app.copy();
    assert_eq!(app.clipboard.len(), 2);
    let event = click_named_option(
        &output,
        &crate::i18n::key("usability.steps.end_multiselect"),
    );
    let output = draw(&mut app, vec![event]);
    assert!(!has_checkboxes(&output));
    assert!(app.multiselect.is_empty());
    assert_eq!(app.selected, 1);
    let event = click_named_option(&output, "03");
    draw(&mut app, vec![event]);
    assert_eq!(app.selected, 2);
    assert!(app.multiselect.is_empty());
    app.select_step(0, false, false);
    app.select_step(2, true, false);
    let output = draw(&mut app, vec![]);
    assert!(has_checkboxes(&output));
    assert_eq!(app.selected_indices(), [0, 1, 2]);
    let event = click_named_option(
        &output,
        &crate::i18n::key("usability.steps.end_multiselect"),
    );
    let output = draw(&mut app, vec![event]);
    assert!(!has_checkboxes(&output));
    assert!(app.multiselect.is_empty());
    assert_eq!(app.selected, 2);
    assert!(snapshot(&app.project) == before);
}

#[test]
fn typing_dialogue_keeps_the_sidebar_width_and_allows_shrinking_it() {
    let mut app = app();
    app.usability.widths[0] = 280.0;
    app.project.stories.insert(
        "main".into(),
        json!({"id":"main","start":"say","nodes":[
        {"id":"say","type":"say","mode":"narrative","text":""},
        {"id":"end","type":"end"}]}),
    );
    app.current = "main".into();
    app.selected = 0;
    app.search.clear();
    let ctx = egui::Context::default();
    style(&ctx);
    let id = egui::Id::new("chapters");
    let draw = |app: &mut App| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        );
        egui::containers::panel::PanelState::load(&ctx, id)
            .unwrap()
            .rect
            .width()
    };
    draw(&mut app);
    let initial = draw(&mut app);
    for length in 1..=80 {
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
            json!("这是一段不停输入的对白WithoutAnySpaces"
                .chars()
                .cycle()
                .take(length)
                .collect::<String>());
        let width = draw(&mut app);
        assert!(
            width <= initial + 1.0,
            "typing {length} characters grew sidebar from {initial} to {width}"
        );
    }
    for length in 1..=80 {
        app.search = "长搜索内容WithoutAnySpaces"
            .chars()
            .cycle()
            .take(length)
            .collect();
        let width = draw(&mut app);
        assert!(
            width <= initial + 1.0,
            "typing search {length} characters grew sidebar from {initial} to {width}"
        );
    }
    ctx.data_mut(|data| {
        data.insert_persisted(
            id,
            egui::containers::panel::PanelState {
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(200.0, 800.0)),
            },
        )
    });
    assert!(
        draw(&mut app) <= 201.0,
        "long dialogue prevents resizing the sidebar down"
    );
}

#[test]
fn authoring_entries_open_the_existing_name_and_talent_workflows() {
    let mut app = app();
    let original = app.story();
    app.open_name_authoring();
    let node = app.story()["nodes"][app.selected].clone();
    assert_eq!(node["type"], "character_title");
    assert_eq!(node["name_action"], "set");
    assert_eq!(node["action"], "keep");
    app.track();
    app.flush();
    app.undo();
    assert_eq!(app.story(), original);
    let source = snapshot(&app.project);
    app.open_custom_talent();
    assert_eq!(app.center, Center::Assets);
    assert_eq!(app.asset_import_dialog.as_ref().unwrap().kind, 10);
    assert!(
        snapshot(&app.project) == source,
        "opening the talent dialog must not create an asset"
    );
}

#[test]
fn persistent_problems_update_after_a_fix_without_opening_an_error_dialog() {
    let mut app = app();
    app.project.stories.insert(
        "main".into(),
        json!({"id":"main","start":"say","nodes":[
        {"id":"say","type":"say","mode":"narrative","text":"测试"},
        {"id":"end","type":"end"}]}),
    );
    app.current = "main".into();
    app.project.manifest["entry"] = json!("main");
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["goto"] = json!("missing");
    app.error = None;
    assert!(!app.check_before_run_or_export());
    assert!(app.problems_open);
    assert!(app.error.is_none());
    assert!(!app.error_issues.is_empty());
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["goto"] = json!("end");
    app.compile();
    assert!(app.error_issues.is_empty(), "{:?}", app.error_issues);
    assert!(app
        .diagnostics
        .iter()
        .all(|issue| issue.severity != "error"));
}

#[test]
fn guided_export_keeps_the_recipient_choice_between_frames_and_cancel_preserves_the_project() {
    let mut app = app();
    app.usability.export_bundle = false;
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.request_export(&ctx);
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1100.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.guided_export(ctx),
        )
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let event = click_named_option(&output, &crate::i18n::key("usability.export.first_time"));
    draw(&mut app, vec![event]);
    draw(&mut app, vec![]);
    assert_eq!(
        ctx.data(|data| data.get_temp::<bool>(egui::Id::new("guided-export-bundle"))),
        Some(true)
    );
    draw(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!ctx.data(|data| data
        .get_temp::<bool>(egui::Id::new("guided-export-open"))
        .unwrap_or(false)));
    assert!(snapshot(&app.project) == before);
    assert!(!app.usability.export_bundle);
}
fn choice_preview() -> App {
    let mut app = app();
    app.project.stories.insert("main".into(), json!({"id":"main","start":"choice1","nodes":[
        {"id":"choice1","type":"choice","options":[{"text":"继续","goto":"choice2"},{"text":"回看","goto":"choice1"}]},
        {"id":"choice2","type":"choice","options":[{"text":"继续","goto":"say1"},{"text":"结束","goto":"end1"}]},
        {"id":"say1","type":"say","mode":"narrative","text":"选择后的对白。"},
        {"id":"end1","type":"end"}]}));
    app.current = "main".into();
    app.selected = 0;
    app.sync_preview_selection();
    app
}
fn assert_mood_drag_updates(action: Option<usize>, second_mood: bool) {
    let mut app = app();
    app.project.stories.insert("main".into(), json!({"story_schema":2,"id":"main","title":"气泡拖动","start":"show","nodes":[
        {"id":"show","type":"show","character":"user:test.actor","position":"M","facing":"left"},
        {"id":"bubble","type":"mood","character":"user:test.actor","enabled":true,"icon":"laugh1"}]}));
    if let Some(action) = action {
        let mood = app.project.stories["main"]["nodes"][1].clone();
        let mut actions =
            vec![json!({"type":"shock","character":"user:test.actor","duration":0.5})];
        let mut mood = mood;
        mood.as_object_mut().unwrap().remove("id");
        actions.insert(action, mood);
        app.project.stories.get_mut("main").unwrap()["nodes"][1] =
            json!({"id":"bubble","type":"parallel","actions":actions});
    }
    if second_mood {
        let story = app.project.stories.get_mut("main").unwrap();
        story["nodes"][0] = json!({"id":"show","type":"parallel","actions":[
            {"type":"show","character":"user:test.actor","position":"L1","facing":"left"},
            {"type":"show","character":"user:test.other","position":"R1","facing":"left"} ]});
        story["nodes"][1]["actions"][1] = json!({"type":"mood","character":"user:test.other","enabled":true,"icon":"shock","offset_x":77,"offset_y":-31});
    }
    let original = app.story()["nodes"][1].clone();
    let mood = |story: Value| -> Value {
        action
            .map(|index| story["nodes"][1]["actions"][index].clone())
            .unwrap_or_else(|| story["nodes"][1].clone())
    };
    app.project.assets.insert(
        "assets/user/character/test.actor/content.json".into(),
        serde_json::to_vec(
            &json!({"id":"test.actor","type":"character","portraits":{"normal":"normal.png"}}),
        )
        .unwrap(),
    );
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(80, 100, image::Rgba([255, 0, 0, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    app.project.assets.insert(
        "assets/user/character/test.actor/normal.png".into(),
        png.into_inner(),
    );
    if second_mood {
        for name in ["content.json", "normal.png"] {
            let bytes =
                app.project.assets[&format!("assets/user/character/test.actor/{name}")].clone();
            app.project
                .assets
                .insert(format!("assets/user/character/test.other/{name}"), bytes);
        }
    }
    app.selected = 1;
    app.view = View::Stage;
    app.center = Center::Node;
    let ctx = egui::Context::default();
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
                app.right(ctx);
                app.center(ctx);
            },
        )
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let center = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.stroke.color == Color32::from_rgb(45, 145, 220) => {
                Some(rect.rect.center())
            }
            _ => None,
        })
        .expect("selected bubble has a drag handle");
    let pointer = |position, pressed| egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
    );
    let target = center + Vec2::new(30.0, -20.0);
    draw(&mut app, vec![egui::Event::PointerMoved(target)]);
    draw(&mut app, vec![pointer(target, false)]);
    let offset = mood(app.story());
    assert!(offset["offset_x"].as_f64().unwrap() > 20.0, "{offset}");
    assert!(offset["offset_y"].as_f64().unwrap() > 20.0, "{offset}");
    draw(&mut app, vec![]);
    assert_eq!(mood(app.story()), offset);
    assert!(app.story()["nodes"][0].get("offset_x").is_none());
    if let Some(action) = action {
        assert!(app.story()["nodes"][1].get("offset_x").is_none());
        assert_eq!(
            app.story()["nodes"][1]["actions"][1 - action],
            original["actions"][1 - action]
        );
        let saved = tempfile::tempdir().unwrap();
        app.project.save_to(saved.path()).unwrap();
        let reopened = Project::open(saved.path()).unwrap();
        assert_eq!(mood(reopened.stories["main"].clone()), offset);
    }
    let output = draw(&mut app, vec![]);
    let center = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.stroke.color == Color32::from_rgb(45, 145, 220) => {
                Some(rect.rect.center())
            }
            _ => None,
        })
        .unwrap();
    let target = center + Vec2::new(20.0, -10.0);
    draw(
        &mut app,
        vec![
            egui::Event::PointerMoved(center),
            pointer(center, true),
            egui::Event::PointerMoved(target),
            pointer(target, false),
        ],
    );
    assert!(mood(app.story())["offset_x"].as_f64().unwrap() > offset["offset_x"].as_f64().unwrap());
}
fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn navigation_frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1200.0, 850.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            app.shortcuts(ctx);
            app.toolbar(ctx);
            app.center(ctx);
        },
    )
}
fn click_named_option(output: &egui::FullOutput, prefix: &str) -> egui::Event {
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    let target = nodes
        .iter()
        .find_map(|(id, node)| {
            (node.supports_action(egui::accesskit::Action::Click)
                && (node.label().is_some_and(|label| label.starts_with(prefix))
                    || node.value().is_some_and(|value| value.starts_with(prefix))))
            .then_some(*id)
        })
        .unwrap_or_else(|| panic!("Missing option {prefix:?}: {nodes:?}"));
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::Click,
        target,
        data: None,
    })
}

#[path = "tests/assets.rs"]
mod assets_tests;
#[path = "tests/diagnostics.rs"]
mod diagnostics_tests;
#[path = "tests/history.rs"]
mod history_tests;
#[path = "tests/input.rs"]
mod input_tests;
#[path = "tests/navigation.rs"]
mod navigation_tests;
#[path = "tests/preview.rs"]
mod preview_tests;
#[path = "tests/projects.rs"]
mod projects_tests;
#[path = "tests/recovery.rs"]
mod recovery_tests;
#[path = "tests/steps.rs"]
mod steps_tests;
#[path = "tests/stress.rs"]
mod stress_tests;
