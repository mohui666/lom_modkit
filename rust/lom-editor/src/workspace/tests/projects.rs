use super::*;

#[test]
fn preflight_errors_show_details_and_target_the_owning_chapter() {
    let mut app = app();
    for id in ["main", "other"] {
        app.project.stories.insert(
            id.into(),
            json!({"id":id,"title":id,"start":"effect1","nodes":[
            {"id":"effect1","type":"effect","name":""},{"id":"end","type":"end"}]}),
        );
    }
    assert!(!app.check_before_run_or_export());
    assert!(app.error.is_none());
    assert!(app.problems_open);
    assert!(app.status.contains('2'));
    assert_eq!(app.error_issues.len(), 2);
    for issue in &app.error_issues {
        assert!(issue.message.contains("特效名称不能为空"));
        assert_eq!(
            crate::error_location::issue_destination(issue, &app.project),
            crate::error_location::Destination::Story(
                issue.story_id.clone(),
                Some("effect1".into())
            )
        );
    }
    let report = crate::error_location::issue_report(&app.error_issues, &app.project);
    assert!(report.contains("main") && report.contains("other") && report.contains("effect1"));
}
#[test]
fn chapter_groups_support_rename_move_undo_and_project_roundtrip() {
    let mut app = app();
    let mut project = Project::new();
    let mut second = project.stories["main"].clone();
    second["id"] = json!("second");
    project.stories.insert("second".into(), second);
    app.install(project);
    let lua = compile_with_assets(&app.project, &app.story()).unwrap();
    app.begin_chapter_group(false);
    app.chapter_dialog.as_mut().unwrap().name = "主线".into();
    app.commit_chapter().unwrap();
    assert_eq!(chapter_group(&app.project.stories["main"]), "主线");
    assert_eq!(
        compile_with_assets(&app.project, &app.story()).unwrap(),
        lua
    );
    app.current = "second".into();
    app.move_chapter_to_group("主线").unwrap();
    app.begin_chapter_group(true);
    app.chapter_dialog.as_mut().unwrap().name = "任务".into();
    app.commit_chapter().unwrap();
    assert!(app
        .project
        .stories
        .values()
        .all(|story| chapter_group(story) == "任务"));
    app.undo();
    assert!(app
        .project
        .stories
        .values()
        .all(|story| chapter_group(story) == "主线"));
    app.redo();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("project");
    app.project.save_to(&path).unwrap();
    assert!(Project::open(&path)
        .unwrap()
        .stories
        .values()
        .all(|story| chapter_group(story) == "任务"));
    app.move_chapter_to_group("").unwrap();
    assert_eq!(chapter_group(&app.project.stories["second"]), "");
    assert_eq!(chapter_group(&app.project.stories["main"]), "任务");
}
#[test]
fn native_nodes_are_rejected_outside_structured_original_chapters() {
    let mut app = app();
    let before = snapshot(&app.project);
    for kind in ["native_action", "native_branch"] {
        app.add_node(kind);
        assert!(app.error.is_some());
        assert!(snapshot(&app.project) == before);
    }
    app.clipboard = vec![json!({"id":"native","type":"native_action","operations":[]})];
    app.paste();
    assert!(snapshot(&app.project) == before);
}
#[test]
fn duplicate_chapter_names_select_distinct_ids_without_renaming_stories() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["title"] = json!("同名章节");
    let mut second = app.project.stories["main"].clone();
    second["id"] = json!("second");
    app.project.stories.insert("second".into(), second);
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut events = vec![];
    for option in ["同名章节 1", "同名章节 2"] {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1280.0, 900.0),
                )),
                events: std::mem::take(&mut events),
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        );
        events.push(click_named_option(&output, option));
    }
    let _ = ctx.run(
        egui::RawInput {
            events,
            ..Default::default()
        },
        |ctx| app.sidebar(ctx),
    );
    assert_eq!(app.current, "second");
    assert!(snapshot(&app.project) == before);
}
#[test]
fn compiler_diagnostic_link_selects_the_error_node_across_chapters() {
    let mut app = app();
    let mut second = app.story();
    second["id"] = json!("second");
    app.project.stories.insert("second".into(), second);
    app.project.stories.insert("main".into(), json!({
        "story_schema": 2, "id": "main", "start": "first",
        "nodes": [
            {"id":"first", "type":"message", "text":"第一步"},
            {"id":"next", "type":"message", "text":"第二步"},
            {"id":"say_left", "type":"say", "mode":"narrative", "text":"错误在第三步", "goto":"missing"},
            {"id":"end", "type":"end"}
        ]
    }));
    app.current = "second".into();
    app.selected = 0;
    app.multiselect.insert(0);
    app.search = "隐藏目标的旧过滤".into();
    app.compile();
    assert_eq!(
        app.diagnostics
            .iter()
            .filter(|issue| issue.severity == "error")
            .count(),
        1
    );
    assert_eq!(app.diagnostics[0].story_id, "main");
    assert_eq!(app.diagnostics[0].node_id, "say_left");
    assert!(app.diagnostics[0].message.contains("missing"));
    let before = snapshot(&app.project);
    app.view = View::Checks;
    app.problems_open = true;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(1280.0, 900.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run(raw.clone(), |ctx| app.problems_panel(ctx));
    let output = ctx.run(raw.clone(), |ctx| app.problems_panel(ctx));
    let label = crate::error_location::issue_label(&app.diagnostics[0], &app.project);
    let _ = ctx.run(
        egui::RawInput {
            events: vec![click_named_option(&output, &label)],
            ..raw
        },
        |ctx| app.problems_panel(ctx),
    );
    assert_eq!(app.current, "main");
    assert_eq!(app.selected, 2);
    assert!(app.center == Center::Node);
    assert!(app.multiselect.is_empty());
    assert!(app.search.is_empty());
    assert!(snapshot(&app.project) == before);
}
#[test]
fn compiler_diagnostic_does_not_guess_nodes_for_chapter_errors() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["start"] = json!("missing");
    app.compile();
    assert_eq!(app.diagnostics.len(), 1);
    assert_eq!(app.diagnostics[0].story_id, "main");
    assert!(app.diagnostics[0].node_id.is_empty());
    assert!(app.diagnostics[0].message.contains("missing"));
    let preflight = lom_core::release::run_preflight(
        &app.project,
        lom_core::release::Profile::Editing,
        "1.1.3",
    );
    assert!(preflight.contains(&app.diagnostics[0]));
}
#[test]
fn ime_confirmation_keeps_dialog_open_then_return_creates_the_chapter() {
    let mut app = app();
    app.begin_chapter(false);
    app.chapter_dialog.as_mut().unwrap().name.clear();
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.dialogs(ctx));
    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::Ime(egui::ImeEvent::Enabled),
                egui::Event::Ime(egui::ImeEvent::Preedit("chun".into())),
            ],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::Ime(egui::ImeEvent::Commit("春日".into())),
                key_event(egui::Key::Enter, egui::Modifiers::NONE),
            ],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    assert_eq!(app.project.stories.len(), 1);
    assert_eq!(app.chapter_dialog.as_ref().unwrap().name, "春日");
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    assert!(app.chapter_dialog.is_none());
    assert_eq!(app.story()["title"], "春日");
    app.begin_chapter(false);
    app.chapter_dialog.as_mut().unwrap().name = "Next chapter".into();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Ime(egui::ImeEvent::Enabled)],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)],
            ..Default::default()
        },
        |ctx| app.dialogs(ctx),
    );
    assert!(app.chapter_dialog.is_none());
    assert_eq!(app.story()["title"], "Next chapter");
}
#[test]
fn chapter_controls_are_buttons_and_deletion_can_be_cancelled() {
    let mut app = app();
    app.begin_chapter(false);
    app.commit_chapter().unwrap();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draw = |app: &mut App, events| {
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
                app.sidebar(ctx);
                app.center(ctx);
                app.dialogs(ctx);
            },
        )
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let settings = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| {
            (node.role() == egui::accesskit::Role::Button
                && node.label() == Some(tr("章节设置").as_str()))
            .then_some(id)
        })
        .expect("chapter settings is a visible button");
    draw(
        &mut app,
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target: settings,
                data: None,
            },
        )],
    );
    assert_eq!(app.center, Center::Chapter);
    let output = draw(&mut app, vec![]);
    let delete = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find_map(|(id, node)| {
            (node.role() == egui::accesskit::Role::Button
                && node.label() == Some(tr("删除章节…").as_str()))
            .then_some(id)
        })
        .expect("delete is visible beside the chapter settings heading");
    draw(
        &mut app,
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target: delete,
                data: None,
            },
        )],
    );
    assert!(app.delete_chapter_dialog.is_some());
    draw(
        &mut app,
        vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert!(snapshot(&app.project) == before, "the modal blocks undo");
    draw(
        &mut app,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.delete_chapter_dialog.is_none());
    assert!(snapshot(&app.project) == before);
}
#[test]
fn chapter_deletion_retargets_references_undoes_and_stays_deleted_after_save() {
    let mut app = app();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for id in ["keep", "other"] {
        let mut story = app.story();
        story["id"] = json!(id);
        story["title"] = json!(id);
        app.project.stories.insert(id.into(), story);
    }
    let other = app.project.stories.get_mut("other").unwrap();
    other["nodes"][1]["next_script"] = json!("main");
    other["_editor"] = json!({"tests":[{"story":"main"}]});
    app.project
        .paths
        .insert("main".into(), "owned-opening.json".into());
    app.project.save_to(root).unwrap();
    let before = snapshot(&app.project);
    app.last = before.clone();
    app.saved = before.clone();
    assert_eq!(app.chapter_reference_count("main"), 4);
    app.begin_delete_chapter();
    app.delete_chapter_dialog.as_mut().unwrap().replacement = "main".into();
    assert!(app.commit_delete_chapter().is_err());
    assert!(snapshot(&app.project) == before);
    app.delete_chapter_dialog.as_mut().unwrap().replacement = "keep".into();
    app.commit_delete_chapter().unwrap();
    assert_eq!(app.current, "keep");
    assert!(!app.project.stories.contains_key("main"));
    assert!(!app.project.paths.contains_key("main"));
    assert_eq!(app.project.manifest["entry"], "keep");
    assert_eq!(
        app.project.scenario.as_ref().unwrap()["events"][0]["script"],
        "keep"
    );
    assert_eq!(
        app.project.stories["other"]["nodes"][1]["next_script"],
        "keep"
    );
    assert_eq!(
        app.project.stories["other"]["_editor"]["tests"][0]["story"],
        "keep"
    );
    lom_core::scenario::validate(
        app.project.scenario.as_ref().unwrap(),
        &app.project.manifest,
        &app.project.stories,
    )
    .unwrap();
    assert!(
        root.join("story/owned-opening.json").exists(),
        "deletion waits for save"
    );
    assert!(app.dirty());
    assert_eq!(app.undo.len(), 1);
    let deleted = snapshot(&app.project);
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    assert!(snapshot(&app.project) == deleted);
    crate::persistence::save_project(
        &mut app.project,
        root,
        &before.assets,
        &before.stories,
        &before.paths,
    )
    .unwrap();
    let reopened = Project::open(root).unwrap();
    assert!(!reopened.stories.contains_key("main"));
    assert_eq!(reopened.stories.len(), 2);
    assert_eq!(reopened.manifest["entry"], "keep");
    assert!(!root.join("story/owned-opening.json").exists());
    assert_eq!(reopened.assets, *before.assets);
}
#[test]
fn chapter_deletion_keeps_the_last_chapter() {
    let mut app = app();
    let before = snapshot(&app.project);
    app.begin_delete_chapter();
    assert!(app.delete_chapter_dialog.is_none());
    assert!(app.error.as_deref().unwrap().contains("至少"));
    assert!(snapshot(&app.project) == before);
    assert!(!app.dirty());
}
#[test]
fn chapter_creation_uses_unique_identity_and_preserves_owned_filenames() {
    let mut app = app();
    app.project
        .paths
        .insert("main".into(), "CHAPTER1.json".into());
    let before = snapshot(&app.project);
    app.last = before.clone();
    app.saved = before.clone();
    app.begin_chapter(false);
    app.chapter_dialog.as_mut().unwrap().name = "  春日初遇  ".into();
    app.commit_chapter().unwrap();
    assert_eq!(app.current, "chapter2");
    assert_eq!(app.story()["title"], "春日初遇");
    lom_core::validate::validate_story(&app.story()).unwrap();
    assert_eq!(app.project.paths["main"], PathBuf::from("CHAPTER1.json"));
    assert!(!app.project.paths.contains_key("chapter2"));
    assert!(app.chapter_dialog.is_none());
    assert!(app.dirty());
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    assert_eq!(app.project.stories["chapter2"]["title"], "春日初遇");
}
#[test]
fn chapter_name_input_survives_frames_and_cancel_or_invalid_name_never_changes_project() {
    let mut app = app();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    app.begin_chapter(false);
    app.chapter_dialog.as_mut().unwrap().name.clear();
    assert!(app.commit_chapter().is_err());
    assert!(app.chapter_dialog.is_some());
    assert!(snapshot(&app.project) == before);
    for events in [vec![], vec![egui::Event::Text("春日".into())], vec![]] {
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
    }
    assert_eq!(app.chapter_dialog.as_ref().unwrap().name, "春日");
    assert!(snapshot(&app.project) == before);
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
    assert!(app.chapter_dialog.is_none());
    assert!(snapshot(&app.project) == before);
    assert!(!app.dirty());
}
#[test]
fn advanced_chapter_rename_keeps_owned_filename_and_repairs_entry_and_links() {
    let mut app = app();
    app.project
        .paths
        .insert("main".into(), "original.json".into());
    app.project.stories.get_mut("main").unwrap()["nodes"][1]["next_script"] = json!("main");
    app.project.stories.get_mut("main").unwrap()["_editor"] = json!({"tests":[{"story":"main"}]});
    let mut other = app.story();
    other["id"] = json!("other");
    app.project.stories.insert("other".into(), other);
    let before = snapshot(&app.project);
    app.last = before.clone();
    app.saved = before.clone();
    app.rename_dialog = Some(RenameDialog {
        target: RenameTarget::Chapter("main".into()),
        value: "other".into(),
        error: None,
        focus_name: false,
    });
    assert!(app.commit_rename().is_err());
    assert!(snapshot(&app.project) == before);
    app.rename_dialog.as_mut().unwrap().value = "opening".into();
    app.commit_rename().unwrap();
    assert_eq!(app.current, "opening");
    assert_eq!(app.project.paths["opening"], PathBuf::from("original.json"));
    assert_eq!(app.project.manifest["entry"], "opening");
    assert_eq!(app.story()["nodes"][1]["next_script"], "opening");
    assert_eq!(
        app.project.stories["other"]["nodes"][1]["next_script"],
        "opening"
    );
    assert_eq!(app.story()["_editor"]["tests"][0]["story"], "opening");
    app.undo();
    assert!(snapshot(&app.project) == before);
}
#[test]
fn multi_node_copy_across_chapters_rewrites_legacy_dice_targets_and_undoes() {
    let mut app = app();
    let mut second = Project::new().stories["main"].clone();
    second["id"] = json!("second");
    app.project.stories.insert("second".into(), second);
    app.project.stories.get_mut("main").unwrap()["nodes"] = json!([{"id":"d","type":"dice","options":[{"goto_大成功":"end","goto_成功":"end","goto_失败":"end"}]},{"id":"end","type":"end"}]);
    app.last = snapshot(&app.project);
    app.mark_saved();
    app.multiselect = BTreeSet::from([0, 1]);
    app.copy();
    app.current = "second".into();
    app.selected = 0;
    app.multiselect.clear();
    app.paste();
    app.track();
    app.flush();
    let nodes = app.project.stories["second"]["nodes"].as_array().unwrap();
    let goto = nodes[1]["options"][0]["goto_成功"].as_str().unwrap();
    assert_eq!(goto, nodes[2]["id"].as_str().unwrap());
    assert_ne!(goto, "end");
    app.undo();
    assert_eq!(
        app.project.stories["second"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    app.redo();
    assert_eq!(
        app.project.stories["second"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
#[test]
fn deleting_dragged_offsets_keeps_the_rest_of_the_chapter_reachable() {
    for explicit in [false, true] {
        let mut app = app();
        let mut story = json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"player","position":"M","facing":"left"},
            {"id":"line","type":"say","mode":"narrative","text":"对白"},
            {"id":"bubble","type":"mood","character":"player","enabled":true,"icon":"laugh1"},
            {"id":"end","type":"end"}
        ]});
        if explicit {
            story["nodes"][0]["goto"] = json!("line");
        }
        app.project.stories.insert("main".into(), story.clone());
        app.last = snapshot(&app.project);
        let change = crate::preview_layout::Change {
            target: crate::preview_layout::Target::Character("player".into()),
            x: 120.0,
            y: 60.0,
            scale: 100.0,
            effect_scale: None,
        };
        for _ in 0..2 {
            crate::preview_layout::apply(app.project.stories.get_mut("main").unwrap(), 1, &change)
                .unwrap();
            app.track();
            app.flush();
            let dragged = app.story();
            assert_eq!(dragged["nodes"][2]["type"], "offset");
            assert_eq!(
                preview::simulate(&dragged, "bubble")["actors"]["player"]["offset_x"],
                120.0
            );
            app.selected = 2;
            app.delete();
            app.track();
            app.flush();
            let deleted = app.story();
            assert_eq!(deleted, story);
            for id in ["line", "bubble", "end"] {
                assert!(preview::route(&deleted, id).is_some(), "{id}: {deleted}");
            }
            assert_eq!(
                deleted["nodes"].as_array().unwrap()[1..],
                story["nodes"].as_array().unwrap()[1..]
            );
            assert_eq!(
                preview::simulate(&deleted, "line")["actors"],
                preview::simulate(&story, "line")["actors"]
            );
            lom_core::validate::validate_story(&deleted).unwrap();
            compile_with_assets(&app.project, &deleted).unwrap();
            app.undo();
            assert_eq!(app.story(), dragged);
            app.redo();
            assert_eq!(app.story(), deleted);
        }
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("project");
        app.project.save_to(&path).unwrap();
        let reopened = Project::open(&path).unwrap();
        assert_eq!(reopened.stories["main"], app.story());
        assert!(preview::route(&reopened.stories["main"], "end").is_some());
    }
}
#[test]
fn all_six_original_starter_templates_have_valid_story_schemas() {
    let catalog: Value =
        serde_json::from_str(include_str!("../../../data/project-templates.json")).unwrap();
    assert_eq!(catalog.as_object().unwrap().len(), 6);
    for key in catalog.as_object().unwrap().keys() {
        let project = template_project(key).unwrap();
        assert_eq!(project.manifest["play_mode"], "sandbox");
        lom_core::validate::validate_manifest(&project.manifest).unwrap();
        let config = project.scenario.as_ref().expect("模板必须有开场剧情入口");
        lom_core::scenario::validate(config, &project.manifest, &project.stories).unwrap();
        assert_ne!(
            project.manifest["campaign"]["disable_official_events"], true,
            "模板应保留已有的原版事件设置"
        );
        if key == "free_mode_story" {
            let event = &config["events"][1];
            assert_eq!(event["script"], "training_event");
            assert_eq!(event["position"], "Center");
            assert_eq!(event["when"]["all"][0]["value"], 4);
            assert_eq!(event["when"]["all"][1]["value"], 1);
            assert_eq!(
                event["when"]["all"][2]["key"],
                format!(
                    "{}_event_done",
                    project.manifest["campaign_id"].as_str().unwrap()
                )
            );
            let folder = tempfile::tempdir().unwrap();
            let package = folder.path().join("template.lommod");
            project.export(&package).unwrap();
            assert_eq!(Project::open(&package).unwrap().scenario, project.scenario);
        }
        assert!(project.manifest["campaign_id"]
            .as_str()
            .unwrap()
            .starts_with("campaign_"));
        for story in project.stories.values() {
            lom_core::validate::validate_story(story).unwrap();
        }
    }
}
#[test]
fn settings_and_initial_navigation_are_visible_in_legacy_and_template_projects() {
    let mut app = app();
    for project in [Project::new(), template_project("empty").unwrap()] {
        app.install(project);
        app.center = Center::Manifest;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let before = snapshot(&app.project);
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1000.0, 760.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.sidebar(ctx);
                app.right(ctx);
                app.center(ctx);
            },
        );
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let visible = nodes
            .iter()
            .filter_map(|(_, node)| node.label().or(node.value()))
            .collect::<Vec<_>>()
            .join("\n");
        for label in [
            tr("作品设置"),
            crate::i18n::key("editor.workspace.initial"),
            tr("作品编号"),
        ] {
            assert!(
                visible.contains(&label),
                "missing {label:?} ({:?}): {visible}",
                label.as_bytes()
            );
        }
        assert!(snapshot(&app.project) == before);
    }
}
#[test]
fn reading_every_main_panel_preserves_project() {
    let mut app = app();
    let ctx = egui::Context::default();
    let original = snapshot(&app.project);
    for panel in [
        Center::Node,
        Center::Chapter,
        Center::Initial,
        Center::Manifest,
        Center::Events,
        Center::Tasks,
        Center::Localization,
        Center::Assets,
        Center::Tools,
        Center::Advanced,
    ] {
        app.center = panel;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(900.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| app.center(ctx),
        );
        assert!(
            snapshot(&app.project) == original,
            "Opening a panel modified the project"
        );
    }
}
#[test]
fn event_configuration_uses_existing_undo_and_read_only_panel_lifecycle() {
    let mut app = app();
    let ctx = egui::Context::default();
    for mode in ["addon", "sandbox"] {
        app.install(Project::new_scenario(mode));
        let original = snapshot(&app.project);
        for center in [Center::Manifest, Center::Events, Center::Tasks] {
            app.center = center;
            let _ = ctx.run(Default::default(), |ctx| app.center(ctx));
            assert!(
                snapshot(&app.project) == original,
                "painting event panels must preserve source"
            );
        }
        app.project.scenario.as_mut().unwrap()["events"][0]["once"] = json!(false);
        app.track();
        app.flush();
        app.undo();
        assert!(
            snapshot(&app.project) == original,
            "undo must include independent configuration"
        );
    }
}
#[test]
fn scenario_navigation_follows_relationships_without_changing_the_project() {
    let mut app = app();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/event-addon");
    app.install(Project::open(&root).unwrap());
    let original = snapshot(&app.project);
    let ctx = egui::Context::default();
    for link in [
        crate::scenario_panel::Navigate::Event("july_letter".into()),
        crate::scenario_panel::Navigate::Task("delivery".into()),
        crate::scenario_panel::Navigate::Story("meeting".into()),
    ] {
        app.follow_scenario_link(link);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1280.0, 760.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.sidebar(ctx);
                app.right(ctx);
                app.center(ctx);
            },
        );
        assert!(
            snapshot(&app.project) == original,
            "navigation must not rewrite scenario or chapter data"
        );
    }
    assert_eq!(app.center, Center::Node);
    assert_eq!(app.current, "meeting");
    app.center = Center::Tasks;
    let original = snapshot(&app.project);
    let _ = ctx.run(
        egui::RawInput {
            events: vec![key_event(egui::Key::Delete, egui::Modifiers::NONE)],
            ..Default::default()
        },
        |ctx| app.shortcuts(ctx),
    );
    assert!(
        snapshot(&app.project) == original,
        "task navigation must not accept hidden chapter deletion"
    );
}
