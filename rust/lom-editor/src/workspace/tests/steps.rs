use super::*;

#[test]
fn step_rows_show_notes_or_ids_alongside_the_existing_summary() {
    let mut app = app();
    let mut project = Project::new();
    project.stories.get_mut("main").unwrap()["nodes"][0]["_editor"] = json!({"note":"开场备注"});
    app.install(project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        },
        |ctx| app.sidebar(ctx),
    );
    let labels: Vec<_> = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .filter_map(|(_, node)| node.label().map(str::to_owned))
        .collect();
    assert!(labels.iter().any(|text| text.contains("开场备注")));
    let id = app.story().clone()["nodes"][1]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(labels
        .iter()
        .any(|text| text.contains(&id) && text.contains("返回自由模式")));
}
#[test]
fn long_step_lists_only_draw_visible_rows_and_follow_offscreen_selection() {
    let mut app = app();
    let mut project = Project::new();
    project.stories.get_mut("main").unwrap()["nodes"] = Value::Array(
        (0..2000)
            .map(|index| {
                json!({"id":format!("n{index}"),"type":"say","text":format!("滚动节点_{index:04}")})
            })
            .collect(),
    );
    project.stories.get_mut("main").unwrap()["start"] = json!("n0");
    app.install(project);
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    for selected in [0, 1999, 1000] {
        app.selected = selected;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        );
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, node)| node.label().map(str::to_owned))
            .filter(|label| label.contains("滚动节点_"))
            .collect();
        assert!(labels.len() < 30, "offscreen steps must not create widgets");
        assert!(labels
            .iter()
            .any(|label| label.contains(&format!("滚动节点_{selected:04}"))));
        app.track();
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
    }
}
#[test]
fn full_original_import_button_preserves_source_undo_and_editable_step_defaults() {
    let mut app = app();
    app.install(Project::new_vanilla_patch("DEMO").unwrap());
    let source="setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nlocal amount=3\nif original then\nsay(\"原对白\")\nend\nadvance_original(amount)\n";
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
            |ctx| app.center(ctx),
        )
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let target = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find(|(_, node)| node.label() == Some("导入整章到步骤编辑器"))
        .unwrap()
        .0;
    draw(
        &mut app,
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target,
                data: None,
            },
        )],
    );
    assert!(app.story().clone().get("vanilla_patch").is_none());
    assert_eq!(
        lom_core::vanilla_import::parse(&app.story().clone())
            .unwrap()
            .unwrap()
            .source(),
        source
    );
    let imported = snapshot(&app.project);
    app.undo();
    assert!(snapshot(&app.project) == before);
    app.redo();
    assert!(snapshot(&app.project) == imported);
    app.selected = app.story().clone()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["type"] == "say")
        .unwrap();
    draw(&mut app, vec![]);
    assert!(
        snapshot(&app.project) == imported,
        "Opening an imported step must not rewrite its baseline fields"
    );
    app.add_node("say");
    assert_eq!(
        app.story().clone()["nodes"][app.selected]["inherit_dialog"],
        true
    );
    app.project.stories.get_mut("main").unwrap()["nodes"][app.selected]["text"] = json!("新增对白");
    app.copy();
    app.paste();
    assert!(app.story().clone()["nodes"][app.selected]["_editor"]
        .get("vanilla_origin")
        .is_none());
    app.add_node("end");
    assert_eq!(
        app.story().clone()["nodes"][app.selected]["resume_original"],
        true
    );
    lom_core::validate::validate_story(&app.story().clone()).unwrap();
}
#[test]
fn whole_segment_buttons_copy_a_chapter_and_undo_as_one_edit() {
    for before in [true, false] {
        let mut app = app();
        let mut project = Project::new_vanilla_patch("DEMO").unwrap();
        let extra = json!({"story_schema":2,"id":"extra","title":"整段剧情","start":"line","nodes":[
            {"id":"line","type":"say","mode":"narrative","text":"整段开场"},
            {"id":"pause","type":"wait","seconds":0.1},
            {"id":"done","type":"end"}
        ]});
        project.stories.insert("extra".into(), extra.clone());
        app.install(project);
        let source =
            "setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nlocal amount=3\nif original then\nsay(\"原对白\")\nend\nadvance_original(amount)\n";
        app.vanilla = crate::vanilla_panel::Panel::with_source("DEMO", source);
        let original = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 1800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.center(ctx),
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let label = crate::i18n::key(if before {
            "vanilla.segment_before"
        } else {
            "vanilla.segment_after"
        });
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, node)| node.label() == Some(label.as_str()))
            .unwrap()
            .0;
        draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        let after = draw(&mut app, vec![]);
        assert!(
            app.story().clone().get("vanilla_patch").is_none(),
            "{:?}",
            after
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().or(node.value()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            app.story().clone()["nodes"][app.selected]["text"],
            "整段开场"
        );
        assert_eq!(app.multiselect.len(), 3);
        assert_eq!(app.project.stories["extra"], extra);
        assert_eq!(
            lom_core::vanilla_import::parse(&app.story().clone())
                .unwrap()
                .unwrap()
                .source(),
            source
        );
        let inserted = snapshot(&app.project);
        app.undo();
        assert!(snapshot(&app.project) == original);
        app.redo();
        assert!(snapshot(&app.project) == inserted);
        draw(&mut app, vec![]);
        assert!(snapshot(&app.project) == inserted);
    }
}

fn structured_branch_app() -> App {
    let mut app = app();
    let mut project = Project::new_vanilla_patch("DEMO").unwrap();
    project.stories.insert("main".into(), lom_core::vanilla_import::import(
        "setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nif original then\nsay(\"真支原文\")\nelse\nsay(\"假支原文\")\nend\nsay(\"汇合原文\")\n",
        "main", "原版分支", "DEMO", &BTreeMap::new(),
    ).unwrap());
    app.install(project);
    app
}

#[test]
fn original_negated_choice_routes_match_the_compiled_condition() {
    let mut project = Project::new_vanilla_patch("DEMO").unwrap();
    let story = lom_core::vanilla_import::import(
        "local options = {}\noptions[2] = \"向右\"\nlocal choice = choose(options)\nif choice ~= 2 then\nsay(\"未选向右\")\nelse\nsay(\"选了向右\")\nend\n",
        "main",
        "选项否定判断",
        "DEMO",
        &BTreeMap::new(),
    ).unwrap();
    let gate = story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "native_branch")
        .unwrap();
    assert_eq!(crate::native_forms::summary(gate), "未选择第 2 项 · 向右");
    let id = |field: &str| gate[field].as_str().unwrap().to_owned();
    let edges = crate::preview::edges(&story);
    for (field, label) in [("success", "条件成立"), ("failure", "条件不成立")] {
        assert!(edges.contains(&(id("id"), id(field), label.into())));
    }
    project.stories.insert("main".into(), story.clone());
    let lua = compile_with_assets(&project, &story).unwrap();
    assert!(lua.contains(&format!(
        "if {} then return \"{}\" else return \"{}\" end",
        lom_core::native_flow::expression(&gate["condition"]).unwrap(),
        id("success"),
        id("failure")
    )));
}
#[test]
fn structured_additions_follow_the_true_branch_and_resume_its_original_target() {
    let mut app = structured_branch_app();
    app.selected = app.story().clone()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["type"] == "native_branch")
        .unwrap();
    let branch_index = app.selected;
    let before = app.story().clone()["nodes"][branch_index].clone();
    app.add_node("say");
    let added = app.story().clone()["nodes"][app.selected].clone();
    assert_eq!(
        app.story().clone()["nodes"][branch_index]["success"],
        added["id"]
    );
    assert_eq!(
        app.story().clone()["nodes"][branch_index]["failure"],
        before["failure"]
    );
    assert!(added.get("goto").is_none());
    assert_eq!(
        lom_core::analysis::successors(app.story(), app.selected),
        vec![before["success"].as_str().unwrap().to_owned()]
    );
    app.project.stories.get_mut("main").unwrap()["nodes"][app.selected]["text"] =
        json!("新增真支对白");

    app.selected = app.story().clone()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["text"] == "真支原文")
        .unwrap();
    let tail = app.selected;
    let original_next = app.story().clone()["nodes"][tail]["goto"].clone();
    app.add_node("end");
    let end = app.story().clone()["nodes"][app.selected].clone();
    assert_eq!(end["resume_original"], true);
    assert_eq!(end["resume_node"], original_next);
    assert_eq!(app.story().clone()["nodes"][tail]["goto"], end["id"]);
    assert_eq!(
        lom_core::analysis::successors(&app.story().clone(), app.selected),
        vec![original_next.as_str().unwrap().to_owned()]
    );
    lom_core::validate::validate_story(&app.story().clone()).unwrap();
    compile_with_assets(&app.project, &app.story().clone()).unwrap();
}
#[test]
fn legacy_original_links_are_normalized_in_batch_and_spliced_without_renumbering() {
    let mut app = structured_branch_app();
    let chapter = app.current.clone();
    let story = app.project.stories.get_mut(&chapter).unwrap();
    let nodes = story["nodes"].as_array().unwrap();
    let index = nodes
        .windows(2)
        .position(|pair| {
            lom_core::analysis::follows_next(&pair[0])
                && lom_core::vanilla_import::origin_index(&pair[0]).is_some()
        })
        .unwrap();
    let anchor = nodes[index]["id"].clone();
    let next = nodes[index + 1]["id"].clone();
    let origin = lom_core::vanilla_import::origin_index(&nodes[index]).unwrap();
    story["nodes"][index]["goto"] = next.clone();
    story["vanilla_import"]["origins"][origin]["node"]["goto"] = next.clone();
    let mut second = story.clone();
    second["id"] = json!("second");
    second["vanilla_import"]["target"] = json!("SECOND");
    app.project.stories.insert("second".into(), second);
    app.track();
    app.flush();
    let baseline = snapshot(&app.project);
    let originals = app.project.stories.clone();
    let before = compile_with_assets(&app.project, app.story()).unwrap();

    app.normalize_original_links();
    for story in app.project.stories.values() {
        assert!(story["nodes"][index].get("goto").is_none());
        assert_eq!(story["nodes"][index]["id"], anchor);
        assert_eq!(story["nodes"][index + 1]["id"], next);
        let original = &originals[story["id"].as_str().unwrap()];
        for i in 0..story["nodes"].as_array().unwrap().len() {
            assert_eq!(
                lom_core::analysis::successors(story, i),
                lom_core::analysis::successors(original, i)
            );
        }
    }
    assert_eq!(before, compile_with_assets(&app.project, app.story()).unwrap());
    app.undo();
    assert!(snapshot(&app.project) == baseline);

    let mut edited = app.story().clone();
    let old_nodes = edited["nodes"].as_array().unwrap().clone();
    let at = crate::workspace::steps::insert_nodes_after(
        &mut edited,
        index,
        vec![json!({"id":"batch_insert","type":"say","mode":"narrative","text":"插入验证"})],
    )
    .unwrap();
    assert_eq!(at, index + 1);
    assert_eq!(lom_core::analysis::successors(&edited, index), ["batch_insert"]);
    assert_eq!(
        lom_core::analysis::successors(&edited, at),
        [next.as_str().unwrap()]
    );
    let retained: Vec<_> = edited["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["id"] != "batch_insert")
        .map(|node| &node["id"])
        .collect();
    assert_eq!(
        retained,
        old_nodes.iter().map(|node| &node["id"]).collect::<Vec<_>>()
    );
    lom_core::validate::validate_story(&edited).unwrap();
    compile_with_assets(&app.project, &edited).unwrap();
}

#[test]
fn structured_paste_preserves_internal_links_and_reconnects_the_external_exit() {
    let mut app = structured_branch_app();
    app.selected = app.story().clone()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["text"] == "真支原文")
        .unwrap();
    let anchor = app.selected;
    let continuation = app.story().clone()["nodes"][anchor]["goto"].clone();
    app.clipboard = vec![
        json!({"id":"copy_a","type":"say","text":"新增甲","inherit_dialog":true,"goto":"copy_b","_editor":{"vanilla_origin":1}}),
        json!({"id":"copy_b","type":"say","text":"新增乙","inherit_dialog":true,"goto":"other_chapter_exit"}),
    ];
    app.paste();
    let story = app.story().clone();
    let first = &story["nodes"][app.selected];
    let last = &story["nodes"][app.selected + 1];
    assert_eq!(story["nodes"][anchor]["goto"], first["id"]);
    assert_eq!(first["goto"], last["id"]);
    assert_eq!(last["goto"], continuation);
    assert!(first["_editor"].get("vanilla_origin").is_none());
    let imported = lom_core::vanilla_import::parse(&story).unwrap().unwrap();
    let nodes = story["nodes"].as_array().unwrap();
    let origins = imported.associations(nodes);
    assert!(lom_core::vanilla_import::Imported::unchanged(
        origins[anchor].unwrap(), &nodes[anchor]
    ));
    lom_core::validate::validate_story(&story).unwrap();
    compile_with_assets(&app.project, &story).unwrap();
}

#[test]
fn pasted_condition_default_rejoins_a_nonadjacent_original_destination() {
    let mut app = structured_branch_app();
    app.selected = app.story()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["text"] == "真支原文")
        .unwrap();
    let destination = app.story()["nodes"][app.selected]["goto"].clone();
    app.clipboard = vec![
        json!({"id":"condition","type":"branch","source":"mod","flag":"mark",
               "cases":[{"value":1,"goto":"another_chapter_step"}]}),
    ];
    app.paste();
    assert!(app.error.is_none(), "{:?}", app.error);
    let story = app.story();
    let condition = &story["nodes"][app.selected];
    let fallback = &story["nodes"][app.selected + 1];
    assert_eq!(condition["cases"][0]["goto"], destination);
    assert_eq!(fallback["goto"], destination);
    let successors = lom_core::analysis::successors(story, app.selected);
    assert!(successors.iter().any(|id| fallback["id"] == *id));
    compile_with_assets(&app.project, story).unwrap();
}

#[test]
fn structured_delete_reconnects_a_single_successor_or_prompts_to_choose_a_branch() {
    let mut app = structured_branch_app();
    app.selected = app.story().clone()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["text"] == "真支原文")
        .unwrap();
    let removed = app.story().clone()["nodes"][app.selected].clone();
    app.delete();
    assert!(app.error.is_none());
    let story = app.story().clone();
    assert!(!story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == removed["id"]));
    let branch = story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["type"] == "native_branch")
        .unwrap();
    assert_eq!(story["nodes"][branch]["success"], removed["goto"]);
    lom_core::validate::validate_story(&story).unwrap();
    compile_with_assets(&app.project, &story).unwrap();

    app.selected = branch;
    let before = snapshot(&app.project);
    let gate = story["nodes"][branch]["id"].as_str().unwrap();
    let otherwise = story["nodes"][branch]["failure"].as_str().unwrap();
    app.delete();
    assert!(snapshot(&app.project) == before);
    assert!(app.error.is_none());
    app.confirm_step_deletion();
    assert!(snapshot(&app.project) == before);
    app.step_delete_dialog
        .as_mut()
        .unwrap()
        .choose(gate, otherwise);
    app.confirm_step_deletion();
    assert!(app.step_delete_dialog.is_none());
    let after = app.story();
    assert_eq!(
        lom_core::analysis::successors(after, branch - 1),
        vec![otherwise.to_owned()]
    );
    assert!(after["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["text"] == "假支原文"));
    assert!(after["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["text"] == "汇合原文"));
    compile_with_assets(&app.project, after).unwrap();
}

#[test]
fn deleting_the_entry_branch_keeps_both_bodies_and_connects_to_the_chosen_path() {
    let mut app = structured_branch_app();
    app.selected = 0;
    app.delete();
    assert!(app.step_delete_dialog.is_none());
    assert_eq!(app.story()["nodes"][0]["type"], "native_branch");
    let gate = app.story()["nodes"][0]["id"].as_str().unwrap().to_owned();
    let otherwise = app.story()["nodes"][0]["failure"]
        .as_str()
        .unwrap()
        .to_owned();
    app.delete();
    app.step_delete_dialog
        .as_mut()
        .unwrap()
        .choose(&gate, &otherwise);
    app.confirm_step_deletion();
    let story = app.story();
    assert_eq!(story["start"], story["nodes"][0]["id"]);
    assert_eq!(lom_core::analysis::successors(story, 0), [otherwise]);
    assert!(story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["text"] == "真支原文"));
    assert!(story["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["text"] == "假支原文"));
    compile_with_assets(&app.project, story).unwrap();
}

#[test]
fn deleting_an_active_ending_in_the_middle_reconnects_the_surviving_steps() {
    let mut app = structured_branch_app();
    let anchor = app.selected;
    let continuation = app.story()["nodes"][anchor + 1]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.add_node("end");
    let index = app.selected;
    let ending = &mut app.project.stories.get_mut("main").unwrap()["nodes"][index];
    ending.as_object_mut().unwrap().remove("resume_original");
    ending.as_object_mut().unwrap().remove("resume_node");
    let ending_id = ending["id"].clone();
    app.track();
    app.delete();
    assert!(app.step_delete_dialog.is_some());
    app.confirm_step_deletion();
    assert!(app.step_delete_dialog.is_none());
    assert!(!app.story()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == ending_id));
    assert_eq!(
        lom_core::analysis::successors(app.story(), anchor),
        [continuation]
    );
    compile_with_assets(&app.project, app.story()).unwrap();
}

#[test]
fn structured_delete_chapter_end_leaves_the_final_linear_step_valid() {
    let mut app = structured_branch_app();
    app.selected = app.story()["nodes"].as_array().unwrap().len() - 2;
    app.add_node("hide_all");
    assert!(app.error.is_none());
    let end_index = app.story()["nodes"].as_array().unwrap().len() - 1;
    let end = app.story()["nodes"][end_index]["id"].clone();
    app.project.stories.get_mut("main").unwrap()["nodes"][end_index - 1]["goto"] = end;
    app.track();
    app.flush();
    let before = app.story().clone();
    assert_eq!(before["nodes"][end_index]["type"], "end");
    assert_eq!(
        before["nodes"][end_index - 1]["goto"],
        before["nodes"][end_index]["id"]
    );
    app.selected = end_index;
    app.delete();
    assert!(app.error.is_none(), "{:?}", app.error);
    let after = app.story().clone();
    let tail = after["nodes"].as_array().unwrap().last().unwrap();
    assert_eq!(tail["type"], "hide_all");
    assert!(tail.get("goto").is_none());
    lom_core::validate::validate_story(&after).unwrap();
    let analysis = lom_core::analysis::analyze_project(&app.project.stories, &app.project.manifest);
    assert!(!analysis["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["severity"] == "error"));

    let folder = tempfile::tempdir().unwrap();
    let package = folder.path().join("ending.lommod");
    app.project.export(&package).unwrap();
    assert_eq!(Project::open(&package).unwrap().stories["main"], after);
    app.undo();
    assert_eq!(*app.story(), before);
}
#[test]
fn structured_template_insertion_reuses_the_connected_paste_path() {
    let mut app = structured_branch_app();
    app.selected = app.story().clone()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["text"] == "真支原文")
        .unwrap();
    let anchor = app.selected;
    let continuation = app.story().clone()["nodes"][anchor]["goto"].clone();
    app.pending_template = Some((
        vec![
            json!({"id":"template_say","type":"say","text":"模板对白","inherit_dialog":true,"goto":"outside"}),
        ],
        vec!["外部引用 outside 会保留，请确认目标章节中存在该 ID".into()],
    ));
    let context = egui::Context::default();
    context.enable_accesskit();
    let output = context.run(egui::RawInput::default(), |context| app.dialogs(context));
    let target = output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("接回当前流程并插入"))
        .unwrap()
        .0;
    let _ = context.run(
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
        |context| app.dialogs(context),
    );
    assert!(app.pending_template.is_none());
    let story = app.story().clone();
    assert_eq!(
        story["nodes"][anchor]["goto"],
        story["nodes"][app.selected]["id"]
    );
    assert_eq!(story["nodes"][app.selected]["goto"], continuation);
    lom_core::validate::validate_story(&story).unwrap();
}
#[test]
fn legacy_original_conversion_button_is_transactional_and_undoable() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../lom-core/tests/fixtures/vanilla-import.json"
    ))
    .unwrap();
    for edited_raw in [false, true] {
        let mut app = app();
        let mut project = Project::new_vanilla_patch("DEMO").unwrap();
        let mut story = fixture["story"].clone();
        if edited_raw {
            story["nodes"][0]["code"] = json!(format!(
                "{}\nuser_change()",
                story["nodes"][0]["code"].as_str().unwrap()
            ));
        }
        project.stories.clear();
        project.stories.insert("native_import".into(), story);
        project.manifest["entry"] = json!("native_import");
        app.install(project);
        let before = snapshot(&app.project);
        let context = egui::Context::default();
        context.enable_accesskit();
        let draw = |app: &mut App, events| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 1200.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| app.chapter(ui));
                },
            )
        };
        let output = draw(&mut app, vec![]);
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("转换原版分支与高级步骤"))
            .unwrap()
            .0;
        draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        if edited_raw {
            assert!(snapshot(&app.project) == before);
            assert!(app.error.as_ref().unwrap().contains("高级 Lua 已修改"));
        } else {
            assert_eq!(app.story().clone()["vanilla_import"]["schema"], 2);
            app.track();
            app.flush();
            let upgraded = snapshot(&app.project);
            app.undo();
            assert!(snapshot(&app.project) == before);
            app.redo();
            assert!(snapshot(&app.project) == upgraded);
        }
    }
}
#[test]
fn moving_front_follows_only_a_front_entry() {
    let mut app = app();
    let first = app.story().clone()["nodes"][0]["id"].clone();
    let last = app.story().clone()["nodes"][1]["id"].clone();
    app.selected = 1;
    app.move_node(0);
    assert_eq!(app.story().clone()["start"], last);
    app.project.stories.get_mut("main").unwrap()["start"] = first.clone();
    app.selected = 1;
    app.move_node(0);
    assert_eq!(app.story().clone()["start"], first);
}
#[test]
fn batch_step_moves_keep_order_links_selection_and_undo() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["nodes"] = json!([
        {"id":"a","type":"say","text":"a","goto":"d"},
        {"id":"b","type":"say","text":"b"},
        {"id":"c","type":"choice","options":[{"text":"分支","goto":"a"}]},
        {"id":"d","type":"say","text":"d"},
        {"id":"e","type":"end"}
    ]);
    app.project.stories.get_mut("main").unwrap()["start"] = json!("a");
    let original = app.story().clone();
    app.last = snapshot(&app.project);
    app.mark_saved();
    app.select_step(1, false, false);
    app.select_step(3, false, true);
    app.move_steps(&app.selected_indices(), 0);
    let ids = |app: &App| {
        app.story().clone()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&app), ["b", "d", "a", "c", "e"]);
    assert_eq!(app.story().clone()["start"], "b");
    assert_eq!(app.selected, 1);
    assert_eq!(app.selected_indices(), [0, 1]);
    assert_eq!(app.story().clone()["nodes"][2], original["nodes"][0]);
    assert_eq!(app.story().clone()["nodes"][3], original["nodes"][2]);
    app.track();
    app.flush();
    app.undo();
    assert_eq!(app.story().clone(), original);
    app.redo();
    assert_eq!(ids(&app), ["b", "d", "a", "c", "e"]);
    // Contiguous downward drop uses an insertion boundary from the original list.
    app.move_steps(&[0, 1], 5);
    assert_eq!(ids(&app), ["a", "c", "e", "b", "d"]);
    assert_eq!(app.selected_indices(), [3, 4]);
    let before = app.story().clone();
    app.move_steps(&[3, 4], 4);
    assert_eq!(app.story().clone(), before);
}
#[test]
fn batch_step_selection_extends_from_anchor_and_buttons_move_whole_block() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["nodes"] = json!([
        {"id":"a","type":"wait"}, {"id":"b","type":"wait"},
        {"id":"c","type":"wait"}, {"id":"d","type":"wait"},
        {"id":"e","type":"end"}
    ]);
    app.select_step(1, false, false);
    app.select_step(2, true, false);
    app.select_step(3, true, false);
    assert_eq!(app.selected_indices(), [1, 2, 3]);
    app.select_step(2, false, true);
    assert_eq!(app.selected_indices(), [1, 3]);
    assert!(app.selected_indices().contains(&app.selected));
    app.select_step(1, false, false);
    app.select_step(2, true, false);
    app.move_node(app.selected + 1);
    assert_eq!(app.story().clone()["nodes"][1]["id"], "d");
    assert_eq!(app.selected_indices(), [2, 3]);
    app.move_node(app.selected - 1);
    assert_eq!(app.story().clone()["nodes"][1]["id"], "b");
    assert_eq!(app.story().clone()["nodes"][2]["id"], "c");
}
#[test]
fn step_drag_edge_scrolling_only_runs_inside_top_and_bottom_edges() {
    let rect = egui::Rect::from_min_max(egui::pos2(10.0, 100.0), egui::pos2(300.0, 600.0));
    assert!(step_edge_scroll(rect, egui::pos2(100.0, 101.0)) < -500.0);
    assert!(step_edge_scroll(rect, egui::pos2(100.0, 599.0)) > 500.0);
    assert_eq!(step_edge_scroll(rect, rect.center()), 0.0);
    assert_eq!(step_edge_scroll(rect, egui::pos2(310.0, 599.0)), 0.0);
}
#[test]
fn step_drag_stationary_edge_pointer_scrolls_virtual_list_without_reordering() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["nodes"] = json!((0..200)
        .map(|i| json!({"id":format!("n{i}"),"type":"wait","duration":1.0}))
        .collect::<Vec<_>>());
    app.project.stories.get_mut("main").unwrap()["start"] = json!("n0");
    app.multiselect = [0, 1].into();
    let before = app.story().clone();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draw = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        )
    };
    let checkbox_numbers = |out: egui::FullOutput| {
        out.platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, n)| {
                n.label()
                    .and_then(|s| s.strip_prefix("选择步骤 "))
                    .and_then(|s| s.parse::<usize>().ok())
            })
            .collect::<Vec<_>>()
    };
    let initial = checkbox_numbers(draw(&mut app, vec![]));
    let pointer = egui::pos2(100.0, 840.0);
    let _ = draw(
        &mut app,
        vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    egui::DragAndDrop::set_payload(
        &ctx,
        StepDrag {
            chapter: "main".into(),
            indices: vec![0, 1],
        },
    );
    for _ in 0..90 {
        let _ = draw(&mut app, vec![]);
    }
    let scrolled = checkbox_numbers(draw(&mut app, vec![]));
    assert!(
        scrolled.iter().max() > initial.iter().max(),
        "stationary drag must reveal later rows: {initial:?} -> {scrolled:?}"
    );
    assert_eq!(
        app.story().clone(),
        before,
        "hover scrolling must not move steps before drop"
    );
    let _ = draw(
        &mut app,
        vec![egui::Event::PointerMoved(egui::pos2(100.0, 225.0))],
    );
    for _ in 0..90 {
        let _ = draw(&mut app, vec![]);
    }
    let upward = checkbox_numbers(draw(&mut app, vec![]));
    assert!(
        upward.iter().min() < scrolled.iter().min(),
        "top edge must reveal earlier rows"
    );
    let _ = draw(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!egui::DragAndDrop::has_payload_of_type::<StepDrag>(&ctx));
    assert_eq!(app.story().clone(), before);
}
#[test]
fn deleting_dragged_offsets_preserves_branches_parallel_actions_and_nonadjacent_targets() {
    for parallel in [false, true] {
        let mut app = app();
        let show = if parallel {
            json!({"id":"a","type":"parallel","goto":"line","actions":[
                {"type":"show","character":"player","position":"L1"},
                {"type":"show","character":"girl1","position":"R1"}
            ]})
        } else {
            json!({"id":"a","type":"show","character":"player","goto":"line"})
        };
        let original = json!({"story_schema":2,"id":"main","start":"choice","nodes":[
            {"id":"choice","type":"choice","options":[{"goto":"a","text":"人物"},{"goto":"b","text":"其他分支"}]},
            show,
            {"id":"b","type":"wait","seconds":1,"goto":"line"},
            {"id":"line","type":"say","mode":"narrative","text":"汇合对白"},
            {"id":"end","type":"end"}
        ]});
        let mut story = original.clone();
        let mut selected = 3;
        for character in if parallel {
            vec!["player", "girl1"]
        } else {
            vec!["player"]
        } {
            selected = crate::preview_layout::apply(
                &mut story,
                selected,
                &crate::preview_layout::Change {
                    target: crate::preview_layout::Target::Character(character.into()),
                    x: 120.0,
                    y: 60.0,
                    scale: 100.0,
                    effect_scale: None,
                },
            )
            .unwrap();
        }
        app.project.stories.insert("main".into(), story);
        app.multiselect = app.story().clone()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter_map(|(i, node)| (node["type"] == "offset").then_some(i))
            .collect();
        app.delete();
        assert_eq!(app.story().clone(), original);
        assert!(preview::route(&app.story().clone(), "end").is_some());
    }
}
#[test]
fn deleting_offsets_reconnects_incoming_choice_dice_and_check_targets() {
    let mut app = app();
    let original = json!({"story_schema":2,"id":"main","start":"offset1","nodes":[
        {"id":"choice","type":"choice","options":[{"text":"offset1","goto":"offset1"},{"text":"保留","goto":"end"}]},
        {"id":"dice","type":"dice","options":[{"goto_大成功":"offset1","goto_成功":"offset1","goto_失败":"end"}]},
        {"id":"check","type":"stat_check","success":"offset1","failure":"end"},
        {"id":"offset1","type":"offset","character":"player","goto":"offset2"},
        {"id":"offset2","type":"offset","character":"player"},
        {"id":"line","type":"say","mode":"narrative","text":"offset1"},
        {"id":"end","type":"end"}
    ]});
    app.project.stories.insert("main".into(), original.clone());
    app.multiselect = BTreeSet::from([3, 4]);
    app.delete();
    let result = app.story().clone();
    assert_eq!(result["start"], "line");
    assert_eq!(result["nodes"][0]["options"][0]["goto"], "line");
    assert_eq!(result["nodes"][0]["options"][0]["text"], "offset1");
    assert_eq!(
        result["nodes"][0]["options"][1],
        original["nodes"][0]["options"][1]
    );
    assert_eq!(result["nodes"][1]["options"][0]["goto_大成功"], "line");
    assert_eq!(result["nodes"][1]["options"][0]["goto_成功"], "line");
    assert_eq!(result["nodes"][1]["options"][0]["goto_失败"], "end");
    assert_eq!(result["nodes"][2]["success"], "line");
    assert_eq!(result["nodes"][2]["failure"], "end");
    assert_eq!(result["nodes"][3]["text"], "offset1");
}
#[test]
fn deleting_a_final_drag_offset_restores_the_end_and_ambiguous_deletion_changes_nothing() {
    let mut app = app();
    let original = json!({"story_schema":2,"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"player"},
        {"id":"hold","type":"wait","seconds":1}
    ]});
    let mut story = original.clone();
    let offset = crate::preview_layout::apply(
        &mut story,
        1,
        &crate::preview_layout::Change {
            target: crate::preview_layout::Target::Character("player".into()),
            x: 120.0,
            y: 60.0,
            scale: 100.0,
            effect_scale: None,
        },
    )
    .unwrap();
    app.project.stories.insert("main".into(), story);
    app.selected = offset;
    app.delete();
    assert_eq!(app.story().clone(), original);

    let ambiguous = json!({"story_schema":2,"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"player","goto":"offset1"},
        {"id":"offset1","type":"offset","character":"player","goto":"choice"},
        {"id":"choice","type":"choice","options":[{"goto":"a"},{"goto":"b"}]},
        {"id":"a","type":"end"}, {"id":"b","type":"end"}
    ]});
    app.project.stories.insert("main".into(), ambiguous.clone());
    app.multiselect = BTreeSet::from([1, 2]);
    app.delete();
    assert_eq!(app.story().clone(), ambiguous);
    assert!(app.error.as_ref().unwrap().contains("位移后续不明确"));
}
#[test]
fn deleting_nodes_repairs_sections_groups_and_localizations_without_rewriting_links() {
    let mut app = app();
    let story = json!({"id":"main","story_schema":2,"start":"a","nodes":[{"id":"a","type":"say","text":"a"},{"id":"b","type":"say","text":"b"},{"id":"c","type":"end"}],"_editor":{"sections":[{"id":"s","start":"a","end":"c","groups":[{"id":"g","start":"a","end":"a"}]}]},"localization":{"default_locale":"chs","translations":{"cht":{"a.text":"甲","b.text":"乙"}}}});
    app.project.stories.insert("main".into(), story);
    app.selected = 0;
    app.delete();
    let result = app.story().clone();
    assert_eq!(result["start"], "b");
    assert_eq!(result["_editor"]["sections"][0]["start"], "b");
    assert!(result["_editor"]["sections"][0]["groups"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(result["localization"]["translations"]["cht"]
        .get("a.text")
        .is_none());
    assert_eq!(
        result["localization"]["translations"]["cht"]["b.text"],
        "乙"
    );
}
