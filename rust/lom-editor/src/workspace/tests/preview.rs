use super::*;

#[test]
#[cfg(windows)]
fn preview_audio_waits_for_preparation_and_follows_transport_without_changing_the_project() {
    let mut app = app();
    app.install(
        Project::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/showcase3"))
            .unwrap(),
    );
    app.current = "main".into();
    app.selected = app.story()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["id"] == "audio_preview_mix")
        .unwrap();
    app.center = Center::Node;
    app.view = View::Stage;
    app.preview_sound = true;
    app.sync_preview_selection();
    let (mixer, _source) = rodio::mixer::mixer(1, 8000);
    app.preview_audio.test_mixer = Some(mixer);
    let original = snapshot(&app.project);
    app.toggle_preview();
    assert!(
        app.tick_preview_audio(),
        "both clips must be ready before the visual clock continues"
    );
    let frozen = app.preview_elapsed.unwrap();
    assert!(frozen < 0.1);
    assert!(!app.tick_preview_audio());
    assert!(app.preview_audio.warnings().is_empty());
    app.toggle_preview();
    assert!(!app.auto);
    app.toggle_preview();
    assert!(app.auto);
    assert!(
        !app.tick_preview_audio(),
        "resuming must reuse prepared audio"
    );
    app.last_step = Instant::now() - Duration::from_secs(4);
    app.advance_preview();
    app.sync_preview_selection();
    assert!(app.auto, "automatic navigation preserves playback");
    app.tick_preview_audio();
    assert_eq!(app.preview_audio.location().unwrap().1, "scene1");
    app.selected += 1;
    app.sync_preview_selection();
    assert!(!app.auto);
    assert!(app.preview_audio.location().is_none());
    app.toggle_preview();
    app.tick_preview_audio();
    assert!(
        app.preview_audio.location().is_some(),
        "a new start restores earlier persistent music"
    );
    app.stop_preview();
    assert!(!app.auto);
    assert!(app.preview_audio.location().is_none());
    assert_eq!(app.preview_elapsed, None);
    assert!(snapshot(&app.project) == original);
}

#[test]
fn preview_choices_resume_playback_through_consecutive_branches() {
    let mut app = choice_preview();
    app.auto = true;
    for target in ["choice2", "say1"] {
        app.last_step = Instant::now() - Duration::from_secs(60);
        app.advance_preview();
        assert!(app.auto);
        assert!(app.preview_waiting_branch);
        let waiting = app.selected;
        app.advance_preview();
        assert_eq!(app.selected, waiting);
        app.follow_preview_target(target);
        app.sync_preview_selection();
        assert!(app.auto, "a choice must not act like manual node selection");
        assert!(!app.preview_waiting_branch);
        assert_eq!(app.preview_elapsed, Some(0.0));
    }
    app.last_step = Instant::now() - Duration::from_secs(60);
    app.advance_preview();
    assert_eq!(app.story()["nodes"][app.selected]["id"], "end1");
    app.last_step = Instant::now() - Duration::from_secs(60);
    app.advance_preview();
    assert!(!app.auto);
}
#[test]
fn preview_choice_preserves_pause_and_manual_navigation_stops_playback() {
    let mut app = choice_preview();
    app.follow_preview_target("say1");
    app.sync_preview_selection();
    assert!(!app.auto);
    app.selected = 0;
    app.sync_preview_selection();
    app.auto = true;
    app.last_step = Instant::now() - Duration::from_secs(60);
    app.advance_preview();
    app.follow_preview_target("missing");
    assert_eq!(app.selected, 0);
    assert!(app.preview_waiting_branch);
    app.follow_preview_target("choice1");
    assert!(app.auto);
    assert!(!app.preview_waiting_branch);
    app.selected = 2;
    app.sync_preview_selection();
    assert!(!app.auto);
    assert!(!app.preview_waiting_branch);
}
#[test]
fn playtest_start_uses_the_chapter_entry_without_changing_selection() {
    let mut app = app();
    app.project.stories.insert(
        "main".into(),
        json!({"id":"main","title":"测试章","start":"intro","nodes":[
        {"id":"later","type":"end"},{"id":"intro","type":"wait","seconds":1}]}),
    );
    app.current = "main".into();
    app.selected = 0;
    assert_eq!(app.playtest_node(false), "later");
    assert_eq!(app.playtest_node(true), "intro");
    assert_eq!(app.selected, 0);
}

#[test]
fn event_test_entry_selects_the_current_story_and_reveals_settings_without_starting_the_game() {
    let mut app = app();
    let mut project = Project::new_scenario("addon");
    let event = project.scenario.as_mut().unwrap()["events"][0].clone();
    let mut unrelated = event.clone();
    unrelated["id"] = json!("unrelated");
    unrelated["script"] = json!("other");
    project.scenario.as_mut().unwrap()["events"] = json!([unrelated, event]);
    app.install(project);
    app.current = "main".into();
    app.scenario_selection.event = Some("unrelated".into());
    let original = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open_event_test(&ctx);
    assert_eq!(app.center, Center::Events);
    assert_eq!(app.scenario_selection.event.as_deref(), Some("opening"));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 1200.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(crate::scenario_test::show(
                    ui,
                    &app.project,
                    app.scenario_selection.event.as_deref(),
                    &app.catalog
                )
                .is_none());
            });
        },
    );
    assert!(output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some(crate::i18n::key("editor.event_test.run").as_str())));
    assert!(snapshot(&app.project) == original);
}
#[test]
fn dragging_a_mood_updates_the_same_node_as_the_form() {
    assert_mood_drag_updates(None, false);
}
#[test]
fn dragging_a_group_mood_updates_only_its_action() {
    for action in [0, 1] {
        assert_mood_drag_updates(Some(action), false);
    }
    assert_mood_drag_updates(Some(0), true);
}
#[test]
fn preview_drag_and_corner_resize_commit_one_undo_each() {
    let mut app = app();
    app.project.stories.insert("main".into(), json!({"story_schema":2,"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"user:test.actor","position":"M","facing":"left","goto":"early"},
        {"id":"early","type":"say","text":"前面的对白保持原来位置。","mode":"narrative","goto":"line"},
        {"id":"line","type":"say","text":"摆好的人物会继续留在这句对白里。","mode":"narrative","goto":"end"},
        {"id":"end","type":"end"}]}));
    app.project.assets.insert("assets/user/character/test.actor/content.json".into(),
        serde_json::to_vec(&json!({"content_schema":1,"id":"test.actor","type":"character","name":"测试人物","portraits":{"normal":"normal.png"}})).unwrap());
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(80, 160, image::Rgba([255, 0, 0, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    app.project.assets.insert(
        "assets/user/character/test.actor/normal.png".into(),
        png.into_inner(),
    );
    app.selected = 2;
    app.preview.layout_mode = true;
    app.view = View::Stage;
    app.center = Center::Node;
    app.last = snapshot(&app.project);
    let original = snapshot(&app.project);
    let earlier = preview::simulate(&app.story(), "early")["actors"].clone();
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
    let bounds = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::Managed(0) => {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .expect("actor portrait is drawn");
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: Default::default(),
    };
    let center = bounds.center();
    let moved = center + Vec2::new(30.0, -20.0);
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
    );
    draw(&mut app, vec![egui::Event::PointerMoved(moved)]);
    assert_eq!(
        app.story()["nodes"].as_array().unwrap().len(),
        4,
        "live dragging is a draft"
    );
    draw(&mut app, vec![pointer(moved, false)]);
    let offset = app.story()["nodes"][3].clone();
    assert_eq!(offset["type"], "offset");
    assert_eq!(offset["absolute"], true);
    assert!(offset["x"].as_f64().unwrap() > 30.0);
    assert!(offset["y"].as_f64().unwrap() > 20.0);
    assert_eq!(app.story()["nodes"][app.selected]["id"], "offset1");
    assert_eq!(preview::simulate(&app.story(), "early")["actors"], earlier);
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert!(snapshot(&app.project) == original);
    app.redo();
    let moved_snapshot = snapshot(&app.project);
    app.selected = 3;
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let corner = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(shape)
                if shape.stroke.color == Color32::from_rgb(25, 130, 220)
                    && shape.rect.width() < 20.0 =>
            {
                Some(shape.rect.center())
            }
            _ => None,
        })
        .expect("selected portrait has resize handles");
    let larger = corner + Vec2::new(-24.0, -30.0);
    // Native input can deliver an entire short gesture in one frame.
    draw(
        &mut app,
        vec![
            egui::Event::PointerMoved(corner),
            pointer(corner, true),
            egui::Event::PointerMoved(larger),
            pointer(larger, false),
        ],
    );
    let scaled = app.story()["nodes"][3].clone();
    assert_eq!(
        app.story()["nodes"].as_array().unwrap().len(),
        5,
        "a second gesture reuses the layout step"
    );
    assert!(scaled["scale"].as_f64().unwrap() > 100.0, "{scaled}");
    assert_eq!(scaled["x"], offset["x"]);
    assert_eq!(scaled["y"], offset["y"]);
    assert_eq!(app.undo.len(), 2);
    app.undo();
    assert!(snapshot(&app.project) == moved_snapshot);
    app.selected = 3;
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let center = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::Managed(0) => {
                Some(mesh.calc_bounds().center())
            }
            _ => None,
        })
        .unwrap();
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
    );
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center + Vec2::splat(20.0))],
    );
    draw(
        &mut app,
        vec![key_event(egui::Key::Escape, Default::default())],
    );
    draw(&mut app, vec![pointer(center + Vec2::splat(20.0), false)]);
    assert!(
        snapshot(&app.project) == moved_snapshot,
        "Esc discards the uncommitted gesture"
    );
    assert_eq!(app.undo.len(), 1);
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
    );
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center + Vec2::splat(20.0))],
    );
    app.view = View::Graph;
    draw(&mut app, vec![pointer(center + Vec2::splat(20.0), false)]);
    app.view = View::Stage;
    draw(&mut app, vec![pointer(center + Vec2::splat(40.0), false)]);
    assert!(
        snapshot(&app.project) == moved_snapshot,
        "leaving the stage discards a gesture"
    );
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
    );
    draw(
        &mut app,
        vec![egui::Event::PointerMoved(center + Vec2::splat(20.0))],
    );
    app.install(app.project.clone());
    app.selected = 3;
    draw(&mut app, vec![pointer(center + Vec2::splat(40.0), false)]);
    assert!(
        snapshot(&app.project) == moved_snapshot,
        "a new project cannot inherit a pending drag"
    );
    app.selected = 3;
    app.delete();
    app.track();
    let deleted = app.story();
    assert_eq!(deleted["nodes"][0]["goto"], "early");
    assert_eq!(deleted["nodes"][1]["goto"], "line");
    assert!(preview::route(&deleted, "line").is_some());
    assert!(preview::route(&deleted, "end").is_some());
    app.undo();
    assert!(snapshot(&app.project) == moved_snapshot);
    app.redo();
    assert_eq!(app.story(), deleted);
}
