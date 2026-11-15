use super::*;

fn app(ctx: &egui::Context) -> App {
    let mut app = App::from_context(ctx, None, None);
    app.recovery_candidates.clear();
    app.auto_save_project = false;
    app
}

fn workspace(ctx: &egui::Context, path: &Path) -> Workspace {
    Workspace::with_session(ctx, app(ctx), Some(path.to_owned()))
}

#[test]
fn tab_controls_select_and_close_only_the_requested_project() {
    let ctx = egui::Context::default();
    let root = tempfile::tempdir().unwrap();
    let mut work = workspace(&ctx, &root.path().join("workspace.json"));
    work.open(Pending::NewAddon, &ctx);
    let remaining_id = work.tabs[1].id;
    let name = work.tabs[0].app.project.manifest["name"]
        .as_str()
        .unwrap()
        .to_owned();
    ctx.enable_accesskit();
    let frame = |work: &mut Workspace, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 400.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| work.strip(ctx),
        )
    };
    let output = frame(&mut work, vec![]);
    let target = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| (node.label() == Some(name.as_str())).then_some(id))
        .unwrap();
    let click = |target| {
        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target,
            data: None,
        })
    };
    frame(&mut work, vec![click(target)]);
    assert_eq!(work.active, 0);
    let output = frame(&mut work, vec![]);
    let label = format!("{} · {name}", crate::i18n::key("workspace.tabs.close"));
    let close = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| (node.label() == Some(label.as_str())).then_some(id))
        .unwrap();
    frame(&mut work, vec![click(close)]);
    work.actions(&ctx);
    assert_eq!(work.tabs.len(), 1);
    assert_eq!(work.tabs[0].id, remaining_id);
}

#[test]
fn switching_projects_preserves_independent_history_and_widget_drafts() {
    let ctx = egui::Context::default();
    let root = tempfile::tempdir().unwrap();
    let mut workspace = workspace(&ctx, &root.path().join("workspace.json"));
    workspace.tabs[0].app.preview.resolution = [1280, 1024];
    let first = snapshot(&workspace.tabs[0].app.project);
    workspace.tabs[0].app.add_node("wait");
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("draft"), "first".to_owned()));
    workspace.open(Pending::NewAddon, &ctx);
    assert_eq!(workspace.active, 1);
    assert_eq!(workspace.tabs[1].app.preview.resolution, [1280, 1024]);
    workspace.tabs[1].app.preview.resolution = [1024, 768];
    assert!(ctx
        .data(|data| data.get_temp::<String>(egui::Id::new("draft")))
        .is_none());
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("draft"), "second".to_owned()));
    assert_ne!(
        workspace.tabs[0].app.recovery_dir,
        workspace.tabs[1].app.recovery_dir
    );
    let second = snapshot(&workspace.tabs[1].app.project);
    workspace.switch(&ctx, 0);
    assert_eq!(workspace.tabs[0].app.preview.resolution, [1024, 768]);
    assert_eq!(
        ctx.data(|data| data.get_temp::<String>(egui::Id::new("draft")))
            .as_deref(),
        Some("first")
    );
    assert!(workspace.tabs[0].app.dirty());
    workspace.tabs[0].app.undo();
    assert!(snapshot(&workspace.tabs[0].app.project) == first);
    workspace.switch(&ctx, 1);
    assert_eq!(
        ctx.data(|data| data.get_temp::<String>(egui::Id::new("draft")))
            .as_deref(),
        Some("second")
    );
    assert!(snapshot(&workspace.tabs[1].app.project) == second);
}

#[test]
fn saved_tabs_restore_selection_then_edit_save_export_and_reopen() {
    let ctx = egui::Context::default();
    let root = tempfile::tempdir().unwrap();
    let session = root.path().join("workspace.json");
    let first_path = root.path().join("first");
    let second_path = root.path().join("second");
    for (path, name) in [(&first_path, "作品一"), (&second_path, "作品二")] {
        let mut project = Project::new();
        project.manifest["name"] = json!(name);
        project
            .assets
            .insert("assets/kept.bin".into(), b"asset".to_vec());
        project.save_to(path).unwrap();
    }
    let mut work = workspace(&ctx, &session);
    work.tabs[0].app.install(open_project(&first_path).unwrap());
    work.tabs[0].app.selected = 1;
    work.tabs[0].app.view = View::Graph;
    work.open(Pending::Open(second_path.clone()), &ctx);
    work.tabs[1].app.center = Center::Chapter;
    assert!(work.persist());
    drop(work);
    let mut restored = workspace(&ctx, &session);
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.active, 1);
    assert_eq!(restored.tabs[0].app.selected, 1);
    assert!(restored.tabs[0].app.view == View::Graph);
    assert_eq!(restored.tabs[1].app.center, Center::Chapter);
    assert_eq!(
        restored.tabs[1].app.project.assets["assets/kept.bin"],
        b"asset"
    );
    // Opening an already open directory selects it instead of creating another editor.
    restored.open(Pending::Open(first_path.clone()), &ctx);
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.active, 0);
    let app = &mut restored.tabs[0].app;
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("恢复后继续编辑");
    app.track();
    assert!(app.save_at(&first_path, false));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.saving() {
        assert!(Instant::now() < deadline);
        app.poll_background_save(&ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.dirty(), "{:?}", app.error);
    let package = root.path().join("saved.lommod");
    app.project.export(&package).unwrap();
    assert_eq!(
        Project::open(&package).unwrap().stories["main"]["nodes"][0]["text"],
        "恢复后继续编辑"
    );
    assert!(restored.persist());
    let reopened = workspace(&ctx, &session);
    assert_eq!(
        reopened.tabs[0].app.project.stories["main"]["nodes"][0]["text"],
        "恢复后继续编辑"
    );
    assert!(reopened.tabs[0].app.undo.is_empty());
}

#[test]
fn closing_a_window_checks_each_dirty_tab_and_cancel_keeps_every_project() {
    let ctx = egui::Context::default();
    let root = tempfile::tempdir().unwrap();
    let mut work = workspace(&ctx, &root.path().join("workspace.json"));
    work.tabs[0].app.add_node("wait");
    work.open(Pending::New, &ctx);
    work.tabs[1].app.add_node("wait");
    work.begin_close(Pending::Quit);
    work.actions(&ctx);
    assert_eq!(work.active, 0);
    assert!(matches!(work.tabs[0].app.pending, Some(Pending::CloseTab)));
    work.tabs[0].app.discard_and_continue(&ctx);
    work.actions(&ctx);
    assert_eq!(work.active, 1);
    assert!(matches!(work.tabs[1].app.pending, Some(Pending::CloseTab)));
    work.tabs[1].app.cancel_pending_action();
    work.actions(&ctx);
    assert!(work.closing.is_none());
    assert!(!work.allow_close);
    assert_eq!(work.tabs.len(), 2);
    assert!(work
        .tabs
        .iter()
        .all(|tab| tab.app.story()["nodes"].as_array().unwrap().len() == 3));
    work.tabs[1].app.request(Pending::CloseTab, &ctx);
    work.tabs[1].app.discard_and_continue(&ctx);
    work.actions(&ctx);
    assert_eq!(work.tabs.len(), 1);
    let saved: Session =
        serde_json::from_value(lom_core::load_json(root.path().join("workspace.json")).unwrap())
            .unwrap();
    assert_eq!(saved.tabs.len(), 1);
}

#[test]
fn an_unavailable_project_keeps_its_path_and_shows_the_restore_error() {
    let ctx = egui::Context::default();
    let root = tempfile::tempdir().unwrap();
    let session_path = root.path().join("workspace.json");
    let mut work = workspace(&ctx, &session_path);
    let missing = root.path().join("missing");
    let mut location = work.tabs[0].location();
    location.source = Some(missing.clone());
    work.tabs[0].restore(&location);
    assert!(work.tabs[0].app.error.as_ref().unwrap().contains("missing"));
    assert!(work.persist());
    let restored = workspace(&ctx, &session_path);
    assert_eq!(restored.tabs[0].location().source, Some(missing));
    assert!(restored.tabs[0].app.error.is_some());
}

#[test]
fn outer_directory_choices_open_only_the_selected_project_and_reuse_its_tab() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let temp = tempfile::tempdir().unwrap();
    let outer = temp.path().join("download");
    let folder = outer.join("project");
    let package = outer.join("work.lommod");
    let mut project = Project::new();
    project.save_to(&folder).unwrap();
    project.export(&package).unwrap();
    let mut work = workspace(&ctx, &temp.path().join("workspace.json"));
    work.tabs[0].app.error = None;
    work.tabs[0].app.add_node("wait");
    work.tabs[0].app.track();
    let original = snapshot(&work.tabs[0].app.project);
    work.open(Pending::Open(outer.clone()), &ctx);
    assert_eq!(work.tabs.len(), 1);
    assert!(snapshot(&work.tabs[0].app.project) == original);
    assert!(work.blocked());
    assert_eq!(
        work.tabs[0]
            .app
            .project_choice
            .as_ref()
            .unwrap()
            .candidates
            .len(),
        2
    );

    // Escape cancels without touching the current dirty project or creating a tab.
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
        |ctx| work.tabs[0].app.dialogs(ctx),
    );
    assert!(work.tabs[0].app.project_choice.is_none());
    assert!(snapshot(&work.tabs[0].app.project) == original);
    assert!(work.tabs[0].app.dirty());

    work.open(Pending::Open(outer.clone()), &ctx);
    let choice = work.tabs[0].app.project_choice.as_mut().unwrap();
    choice.selected = choice
        .candidates
        .iter()
        .position(|path| path == &package.canonicalize().unwrap())
        .unwrap();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        work.tabs[0].app.dialogs(ctx)
    });
    let target = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| {
            (node.label() == Some(crate::i18n::key("workspace.open.selected").as_str()))
                .then_some(id)
        })
        .unwrap();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
            ..Default::default()
        },
        |ctx| work.tabs[0].app.dialogs(ctx),
    );
    work.actions(&ctx);
    assert_eq!(work.tabs.len(), 2);
    assert_eq!(work.active, 1);
    assert_eq!(
        work.tabs[1].app.project.source,
        Some(package.canonicalize().unwrap())
    );
    assert_eq!(
        work.tabs[1].app.project.manifest["campaign_id"],
        project.manifest["campaign_id"]
    );
    assert!(snapshot(&work.tabs[0].app.project) == original);
    work.open(Pending::Open(package.clone()), &ctx);
    assert_eq!(work.tabs.len(), 2);
    std::fs::remove_file(&package).unwrap();
    work.open(Pending::Open(outer.clone()), &ctx);
    assert_eq!(work.tabs.len(), 3);
    assert_eq!(
        work.tabs[2].app.project.source,
        Some(folder.canonicalize().unwrap())
    );
    work.open(Pending::Open(outer), &ctx);
    assert_eq!(work.tabs.len(), 3);
    assert_eq!(work.active, 2);
    assert!(work.persist());
    let restored = workspace(&ctx, &temp.path().join("workspace.json"));
    assert_eq!(
        restored.tabs[2].app.project.source,
        Some(folder.canonicalize().unwrap())
    );
}

#[test]
fn startup_directory_choice_survives_restoring_an_existing_workspace() {
    let ctx = egui::Context::default();
    let temp = tempfile::tempdir().unwrap();
    let outer = temp.path().join("download");
    for name in ["one", "two"] {
        Project::new().save_to(&outer.join(name)).unwrap();
    }
    let session = temp.path().join("workspace.json");
    let mut previous = workspace(&ctx, &session);
    previous.open(Pending::NewAddon, &ctx);
    previous.persist();
    let mut incoming = App::from_context(&ctx, Some(outer), None);
    incoming.recovery_candidates.clear();
    incoming.error = None;
    assert!(incoming.project_choice.is_some());
    let restored = Workspace::with_session(&ctx, incoming, Some(session));
    assert_eq!(restored.active, 0);
    assert!(restored.tabs[0].app.project_choice.is_some());
    assert_eq!(restored.tabs.len(), 3);
}
