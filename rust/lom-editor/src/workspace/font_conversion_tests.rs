use super::*;

fn app(project: Project) -> App {
    let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
    let mut app = App::new(&cc, None, None);
    app.recovery_candidates.clear();
    app.auto_save_project = false;
    app.install(project);
    app
}

#[test]
fn all_numeric_and_named_font_sizes_convert_once_without_touching_defaults_or_invalid_values() {
    let mut project = Project::new();
    let mut nodes: Vec<_> = (1..=200)
        .map(|size| {
            json!({"id":format!("n{size}"),"type":"say","mode":"narrative",
        "text":"正文与 {size=60}富文本{/size} 保持原值","font_size":size})
        })
        .collect();
    for (id, value) in [
        ("small", json!("small")),
        ("large", json!("large")),
        ("zero", json!(0)),
        ("normal", json!("normal")),
        ("null", Value::Null),
        ("float", json!(40.0)),
        ("string", json!("40")),
        ("negative", json!(-1)),
        ("too_large", json!(201)),
        ("boolean", json!(true)),
    ] {
        nodes.push(json!({"id":id,"type":"say","text":"保留","font_size":value}));
    }
    nodes.push(json!({"id":"unset","type":"say","text":"默认"}));
    nodes.push(json!({"id":"raw","type":"raw","code":"author_owned()","font_size":40}));
    nodes.push(json!({"id":"end","type":"end"}));
    project.stories.insert(
        "main".into(),
        json!({"id":"main","story_schema":2,"start":"n1","nodes":nodes,
        "_editor":{"note":"保留其他编辑信息","custom":{"value":40}}}),
    );
    let mut app = app(project);
    let before = snapshot(&app.project);
    let preview = plan(&app.project, "main", Scope::Project);
    assert_eq!(preview.count(), 201);
    assert_eq!(
        preview
            .rows
            .iter()
            .filter(|row| row.status == "font_conversion.status.invalid")
            .map(|row| row.count)
            .sum::<usize>(),
        6
    );
    assert!(preview
        .rows
        .iter()
        .any(|row| row.old == "40" && row.new == "28"));
    assert!(preview
        .rows
        .iter()
        .any(|row| row.old == crate::i18n::key("font_conversion.small") && row.new == "17"));
    assert!(preview
        .rows
        .iter()
        .any(|row| row.old == crate::i18n::key("font_conversion.large") && row.new == "42"));
    assert!(snapshot(&app.project) == before);
    assert_eq!(
        app.convert_legacy_fonts("main", Scope::Project).unwrap(),
        201
    );
    let story = &app.project.stories["main"];
    for old in 1..=200 {
        assert_eq!(
            story["nodes"][old - 1]["font_size"],
            json!(((old * 7 + 5) / 10).clamp(1, 200))
        );
        assert_eq!(
            story["nodes"][old - 1]["text"],
            "正文与 {size=60}富文本{/size} 保持原值"
        );
    }
    for (id, expected) in [
        ("small", json!(17)),
        ("large", json!(42)),
        ("zero", json!(0)),
        ("normal", json!("normal")),
        ("null", Value::Null),
        ("float", json!(40.0)),
        ("string", json!("40")),
        ("negative", json!(-1)),
        ("too_large", json!(201)),
        ("boolean", json!(true)),
    ] {
        assert_eq!(
            story["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| node["id"] == id)
                .unwrap()["font_size"],
            expected,
            "{id}"
        );
    }
    assert!(story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == "unset")
        .unwrap()
        .get("font_size")
        .is_none());
    assert_eq!(story["_editor"]["note"], "保留其他编辑信息");
    assert_eq!(story["_editor"]["custom"]["value"], 40);
    assert_eq!(story["_editor"][RECORD]["rule"], RULE);
    let converted = snapshot(&app.project);
    assert_eq!(app.undo.len(), 1);
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    assert!(snapshot(&app.project) == converted);
    app.mark_saved();
    let undo_count = app.undo.len();
    assert_eq!(app.convert_legacy_fonts("main", Scope::Project).unwrap(), 0);
    assert!(!app.dirty());
    assert_eq!(app.undo.len(), undo_count);
    assert!(snapshot(&app.project) == converted);
}

#[test]
fn chapter_scope_and_saved_records_allow_new_old_chapters_without_reconverting_processed_ones() {
    let mut project = Project::new();
    project.stories.get_mut("main").unwrap()["nodes"][0]["font_size"] = json!(40);
    let mut side = project.stories["main"].clone();
    side["id"] = json!("side");
    side["nodes"][0]["font_size"] = json!(60);
    project.stories.insert("side".into(), side);
    let mut app = app(project);
    assert_eq!(app.convert_legacy_fonts("main", Scope::Chapter).unwrap(), 1);
    assert_eq!(app.project.stories["side"]["nodes"][0]["font_size"], 60);
    let preview = plan(&app.project, "main", Scope::Project);
    assert_eq!(preview.processed_chapters, 1);
    assert_eq!(preview.count(), 1);
    assert_eq!(app.convert_legacy_fonts("main", Scope::Project).unwrap(), 1);
    assert_eq!(app.project.stories["main"]["nodes"][0]["font_size"], 28);
    assert_eq!(app.project.stories["side"]["nodes"][0]["font_size"], 42);
    assert_eq!(plan(&app.project, "main", Scope::Project).count(), 0);
}

#[test]
fn conflicting_editor_metadata_is_preserved_without_dirtying_an_empty_conversion() {
    for editor in [
        json!("author-owned"),
        json!({(RECORD):{"rule":"unknown_rule"},"note":"keep"}),
    ] {
        let mut project = Project::new();
        project.stories.get_mut("main").unwrap()["nodes"][0]["font_size"] = json!(40);
        project.stories.get_mut("main").unwrap()["_editor"] = editor;
        let mut app = app(project);
        let before = snapshot(&app.project);
        assert_eq!(plan(&app.project, "main", Scope::Project).issues.len(), 1);
        assert_eq!(app.convert_legacy_fonts("main", Scope::Project).unwrap(), 0);
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
        assert!(app.undo.is_empty());
    }
}

#[test]
fn uniform_size_overrides_every_dialogue_mode_and_default_without_changing_other_content() {
    let modes = ["character", "think", "narrative", "center", "offscreen"];
    let values = [
        None,
        Some(json!(0)),
        Some(json!("normal")),
        Some(json!("small")),
        Some(json!("large")),
    ];
    let mut nodes = Vec::new();
    for (index, (mode, value)) in modes.into_iter().zip(values).enumerate() {
        let mut node = json!({"id":format!("line{index}"),"type":"say","mode":mode,"character":"player","text":"原文 {size=40}格式{/size}","font_size":99});
        if let Some(value) = value {
            node["font_size"] = value;
        } else {
            node.as_object_mut().unwrap().remove("font_size");
        }
        nodes.push(node);
    }
    nodes.extend([
        json!({"id":"same","type":"say","text":"不需改动","font_size":28}),
        json!({"id":"invalid","type":"say","text":"修正旧值","font_size":"invalid"}),
        json!({"id":"raw","type":"raw","code":"author_owned()","font_size":99}),
        json!({"id":"end","type":"end"}),
    ]);
    let mut project = Project::new();
    project.stories.insert(
        "main".into(),
        json!({"id":"main","start":"line0","nodes":nodes,
        "_editor":{"note":"保留","legacy_font_size_conversion":{"rule":RULE}}}),
    );
    project.stories.insert("side".into(), json!({"id":"side","start":"say","nodes":[
        {"id":"say","type":"say","mode":"center","text":"另一章","font_size":60}, {"id":"end","type":"end"}
    ]}));
    let mut app = app(project);
    let before = snapshot(&app.project);
    let metadata = app.project.stories["main"]["_editor"].clone();
    for invalid in [0, 201] {
        assert!(app
            .set_uniform_fonts("main", Scope::Project, invalid)
            .is_err());
        assert!(snapshot(&app.project) == before);
    }
    assert_eq!(
        uniform_plan(&app.project, "main", Scope::Chapter, 28).count(),
        6
    );
    assert_eq!(
        app.set_uniform_fonts("main", Scope::Chapter, 28).unwrap(),
        6
    );
    for node in app.project.stories["main"]["nodes"].as_array().unwrap() {
        if node["type"] == "say" {
            assert_eq!(node["font_size"], 28);
        }
    }
    assert_eq!(
        app.project.stories["main"]["nodes"][0]["text"],
        "原文 {size=40}格式{/size}"
    );
    assert_eq!(app.project.stories["main"]["nodes"][7]["font_size"], 99);
    assert_eq!(app.project.stories["main"]["_editor"], metadata);
    assert_eq!(app.project.stories["side"]["nodes"][0]["font_size"], 60);
    assert_eq!(app.undo.len(), 1);
    app.mark_saved();
    assert_eq!(
        app.set_uniform_fonts("main", Scope::Chapter, 28).unwrap(),
        0
    );
    assert!(!app.dirty());
    assert_eq!(app.undo.len(), 1);
    assert_eq!(
        app.set_uniform_fonts("main", Scope::Project, 28).unwrap(),
        1
    );
    let unified = snapshot(&app.project);
    app.undo();
    assert_eq!(app.project.stories["side"]["nodes"][0]["font_size"], 60);
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    app.redo();
    assert!(snapshot(&app.project) == unified);
}

#[test]
fn released_font_project_converts_edits_saves_exports_reopens_and_retains_its_original() {
    for uniform in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lom-core/tests/fixtures/released-1.5.18-picture-fades.lommod");
        let original = std::fs::read(&fixture).unwrap();
        let mut app = app(Project::open(&fixture).unwrap());
        if uniform {
            assert!(app.set_uniform_fonts("main", Scope::Project, 28).unwrap() > 0);
        } else {
            assert_eq!(plan(&app.project, "main", Scope::Project).count(), 2);
            assert_eq!(app.convert_legacy_fonts("main", Scope::Project).unwrap(), 2);
        }
        app.project.stories.get_mut("main").unwrap()["title"] = json!("旧作转换后继续编辑");
        let saved = temp.path().join("saved");
        app.project.save_to(&saved).unwrap();
        let reopened = open_project(&saved.join("story/main.json")).unwrap();
        assert_eq!(reopened.stories, app.project.stories);
        if uniform {
            assert_eq!(
                uniform_plan(&reopened, "main", Scope::Project, 28).count(),
                0
            );
        } else {
            assert_eq!(plan(&reopened, "main", Scope::Project).count(), 0);
        }
        let output = temp.path().join("converted.lommod");
        reopened.export(&output).unwrap();
        let exported = Project::open(&output).unwrap();
        assert_eq!(exported.stories, reopened.stories);
        if uniform {
            assert_eq!(
                uniform_plan(&exported, "main", Scope::Project, 28).count(),
                0
            );
        } else {
            assert_eq!(plan(&exported, "main", Scope::Project).count(), 0);
        }
        assert_eq!(std::fs::read(&fixture).unwrap(), original);
        let mut entries = lom_core::package::read_package(&output).unwrap();
        entries.remove("package-content.sha256");
        entries
            .get_mut("lua/main.lua")
            .unwrap()
            .extend_from_slice(b"\nerror('unrelated Lua')\n");
        let tampered = temp.path().join("tampered.lommod");
        lom_core::package::write_package(entries, &tampered).unwrap();
        assert!(Project::open(&tampered).is_err());
    }
}

#[test]
fn font_field_button_opens_a_preview_and_only_explicit_apply_changes_the_project() {
    fn click(output: &egui::FullOutput, label: &str) -> egui::Event {
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
                    && node.label().is_some_and(|text| text.starts_with(label)))
                .then_some(*id)
            })
            .unwrap_or_else(|| panic!("missing button: {label}"));
        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target,
            data: None,
        })
    }
    fn draw(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.center(ctx);
                app.font_conversion(ctx);
            },
        )
    }
    for uniform in [false, true] {
        let mut project = Project::new();
        project.stories.get_mut("main").unwrap()["nodes"][0]["font_size"] = json!(40);
        let mut app = app(project);
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = draw(&mut app, &ctx, vec![]);
        let open_key = if uniform {
            "font_uniform.open"
        } else {
            "font_conversion.open"
        };
        let event = click(&output, &crate::i18n::key(open_key));
        let output = draw(&mut app, &ctx, vec![event]);
        assert!(app.font_conversion_dialog.is_some());
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
        let cancel = click(&output, &tr("取消"));
        draw(&mut app, &ctx, vec![cancel]);
        assert!(app.font_conversion_dialog.is_none());
        assert!(snapshot(&app.project) == before);
        let output = draw(&mut app, &ctx, vec![]);
        let event = click(&output, &crate::i18n::key(open_key));
        let output = draw(&mut app, &ctx, vec![event]);
        let event = click(
            &output,
            &crate::i18n::key(if uniform {
                "font_uniform.apply"
            } else {
                "font_conversion.apply"
            }),
        );
        draw(&mut app, &ctx, vec![event]);
        assert!(app.font_conversion_dialog.is_none());
        assert_eq!(app.project.stories["main"]["nodes"][0]["font_size"], 28);
        assert_eq!(app.undo.len(), 1);
        app.undo();
        assert!(snapshot(&app.project) == before);
    }
}
