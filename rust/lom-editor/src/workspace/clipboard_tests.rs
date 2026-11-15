use super::*;

fn app(ctx: &egui::Context) -> App {
    let mut app = App::from_context(ctx, None, None);
    app.recovery_candidates.clear();
    app.auto_save_project = false;
    app
}

#[test]
fn separate_editors_transfer_steps_through_copy_and_paste_events_and_undo_once() {
    let source_ctx = egui::Context::default();
    let target_ctx = egui::Context::default();
    let mut source = app(&source_ctx);
    source.project.stories.get_mut("main").unwrap()["nodes"] = json!([
        {"id":"line","type":"choice","options":[{"text":"line","goto":"pause"}]},
        {"id":"pause","type":"wait","seconds":2}
    ]);
    source.multiselect = BTreeSet::from([0, 1]);
    let output = source_ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| source.shortcuts(ctx),
    );
    let text = output
        .platform_output
        .commands
        .into_iter()
        .find_map(|command| {
            if let egui::OutputCommand::CopyText(text) = command {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();
    let mut target = app(&target_ctx);
    let before = snapshot(&target.project);
    assert!(target.clipboard.is_empty());
    let _ = target_ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Paste(text)],
            ..Default::default()
        },
        |ctx| target.shortcuts(ctx),
    );
    target.track();
    target.flush();
    let nodes = target.story()["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[1]["options"][0]["text"], "line");
    assert_eq!(nodes[1]["options"][0]["goto"], nodes[2]["id"]);
    assert_ne!(nodes[1]["id"], "line");
    target.undo();
    assert!(snapshot(&target.project) == before);
}

#[test]
fn unrelated_or_malformed_clipboard_never_pastes_stale_steps() {
    let ctx = egui::Context::default();
    let mut target = app(&ctx);
    target.clipboard = vec![json!({"id":"stale","type":"wait","seconds":1})];
    let before = snapshot(&target.project);
    for text in [
        "普通文字",
        r#"{"format":"lom-modkit/steps","version":1,"nodes":[null]}"#,
        r#"{"format":"lom-modkit/steps","version":1,"nodes":[{"id":"n","type":"missing"}]}"#,
        r#"{"format":"lom-modkit/steps","version":1,"nodes":[{"id":"n","type":"wait"},{"id":"n","type":"end"}]}"#,
    ] {
        target.paste_from_system(text);
        assert!(target.error.take().is_some());
        assert!(snapshot(&target.project) == before);
    }
}

#[test]
fn paste_menu_requests_the_native_clipboard_even_with_an_empty_local_buffer() {
    let ctx = egui::Context::default();
    let mut target = app(&ctx);
    let output = ctx.run(Default::default(), |ctx| target.request_system_paste(ctx));
    assert!(output.viewport_output.values().any(|viewport| viewport
        .commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::RequestPaste))));
    assert!(target.node_paste_requested);
    assert!(target.clipboard.is_empty());
    // An empty/image clipboard sends no paste event; the request must expire
    // before a later copy inside a text field can be mistaken for step copying.
    let _ = ctx.run(Default::default(), |ctx| target.shortcuts(ctx));
    assert!(!target.node_paste_requested);
}
