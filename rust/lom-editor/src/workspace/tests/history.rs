use super::*;

#[test]
fn cached_saved_status_follows_reverts_save_undo_and_redo() {
    let mut app = app();
    let clean = app.project.stories.clone();
    assert!(!app.dirty());
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("修改");
    app.track();
    assert!(app.dirty());
    app.project.stories = clean;
    app.track();
    assert!(!app.dirty());
    app.flush();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("保存内容");
    app.track();
    app.flush();
    app.mark_saved();
    assert!(!app.dirty());
    app.undo();
    assert!(app.dirty());
    app.redo();
    assert!(!app.dirty());
}

#[test]
fn original_dialogue_flag_insertion_ui_undoes_and_does_not_save_original_lua() {
    let mut app = app();
    app.install(Project::new_vanilla_patch("DEMO").unwrap());
    let source = "if original then say(\"原对白\") end\nadvance_original()";
    app.vanilla = crate::vanilla_panel::Panel::with_source("DEMO", source);
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1600.0),
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
    // AccessKit returns nodes by ID, not screen order. Select the checkbox
    // for the before-line insertion by its position, not either label's ID.
    let target = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some("仅在旗标检定成立时插入"))
        .min_by(|(_, a), (_, b)| a.bounds().unwrap().y0.total_cmp(&b.bounds().unwrap().y0))
        .unwrap()
        .0;
    let click = egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::Click,
        target,
        data: None,
    });
    draw(&mut app, vec![click]);
    app.track();
    app.flush();
    assert_eq!(
        app.story()["vanilla_patch"]["edits"][0]["before_condition"]["source"],
        "condition"
    );
    assert!(!serde_json::to_string(&app.project.stories)
        .unwrap()
        .contains("advance_original"));
    assert!(app.project.assets.is_empty());
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    app.project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"][0]["before"] =
        json!("新增对白");
    app.project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"][0]["before_condition"]
        ["flag"] = json!("DEMO_CHECKPOINT");
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("project");
    app.project.save_to(&path).unwrap();
    assert_eq!(Project::open(&path).unwrap().stories, app.project.stories);
    let pending_before = snapshot(&app.project);
    app.track();
    app.request(Pending::NewVanillaPatch, &ctx);
    assert!(matches!(app.pending, Some(Pending::NewVanillaPatch)));
    assert!(snapshot(&app.project) == pending_before);
}
#[test]
fn legacy_rotation_preference_preserves_project_angles_and_other_settings() {
    let mut app = app();
    let before = snapshot(&app.project);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("preferences.json");
    std::fs::write(&path, r#"{"custom":"keep","ui_locale":"chs"}"#).unwrap();
    app.set_legacy_character_rotation(true, &path).unwrap();
    assert!(app.preview.legacy_character_rotation);
    let prefs = lom_core::load_json(&path).unwrap();
    assert_eq!(prefs["legacy_character_rotation"], true);
    assert_eq!(prefs["custom"], "keep");
    app.set_legacy_character_rotation(false, &path).unwrap();
    assert!(!app.preview.legacy_character_rotation);
    assert!(snapshot(&app.project) == before);
}
