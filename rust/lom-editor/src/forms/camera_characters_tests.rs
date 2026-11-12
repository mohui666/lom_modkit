use super::*;
use egui::accesskit::{Action, ActionRequest, Role};

fn frame(
    catalog: &Catalog,
    ctx: &egui::Context,
    node: &mut Value,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, bool) {
    ctx.enable_accesskit();
    let mut changed = false;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 600.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                changed = catalog.field(
                    ui,
                    node,
                    "exclude_characters",
                    "exclude_characters",
                    "official_characters",
                    true,
                    &[],
                    &[],
                );
            });
        },
    );
    (output, changed)
}

fn action(
    output: &egui::FullOutput,
    role: Role,
    label: Option<&str>,
    action: Action,
) -> egui::Event {
    let target = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == role && label.is_none_or(|label| node.label() == Some(label))
        })
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("No {role:?} with label {label:?}"));
    egui::Event::AccessKitActionRequest(ActionRequest {
        action,
        target,
        data: None,
    })
}

fn catalog() -> Catalog {
    let mut catalog = Catalog::new();
    catalog.data["characters"] = json!([
        {"id":"one", "name":"人物甲"},
        {"id":"two", "name":"人物乙"},
        {"id":"user:project.actor", "name":"自定义人物"}
    ]);
    catalog
}

fn search(output: &egui::FullOutput, query: &str) -> Vec<egui::Event> {
    vec![
        action(output, Role::TextInput, None, Action::Focus),
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                command: true,
                ..Default::default()
            },
        },
        egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Text(query.into()),
    ]
}

fn open(catalog: &Catalog, ctx: &egui::Context, node: &mut Value) -> egui::FullOutput {
    let (output, changed) = frame(catalog, ctx, node, vec![]);
    assert!(!changed);
    let click = action(&output, Role::ComboBox, None, Action::Click);
    let (_, changed) = frame(catalog, ctx, node, vec![click]);
    assert!(!changed);
    frame(catalog, ctx, node, vec![]).0
}

#[test]
fn camera_character_search_stays_open_after_pointer_focus_and_multi_selection() {
    let catalog = catalog();
    let ctx = egui::Context::default();
    let mut node = json!({
        "id":"camera", "type":"camera", "name":"stage-memory", "active":true,
        "exclude_characters":["one","user:missing-from-this-catalog"]
    });
    let before = node.clone();
    let output = open(&catalog, &ctx, &mut node);
    let bounds = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::TextInput)
        .unwrap()
        .1
        .bounds()
        .unwrap();
    let point = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    );
    let pointer = |pressed| egui::Event::PointerButton {
        pos: point,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &catalog,
        &ctx,
        &mut node,
        vec![egui::Event::PointerMoved(point), pointer(true)],
    );
    frame(&catalog, &ctx, &mut node, vec![pointer(false)]);
    assert!(
        egui::Popup::is_any_open(&ctx),
        "Clicking the search field must keep the multi-select menu open"
    );
    let (output, changed) = frame(
        &catalog,
        &ctx,
        &mut node,
        vec![egui::Event::Text("乙".into())],
    );
    assert!(!changed);
    assert_eq!(node, before, "Searching cannot change existing exclusions");
    let (_, changed) = frame(&catalog, &ctx, &mut node, vec![]);
    assert!(!changed);
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert!(nodes
        .iter()
        .any(|(_, node)| node.role() == Role::CheckBox && node.label() == Some("人物乙")));
    assert!(!nodes
        .iter()
        .any(|(_, node)| node.role() == Role::CheckBox && node.label() == Some("人物甲")));
    let click = action(&output, Role::CheckBox, Some("人物乙"), Action::Click);
    let (_, changed) = frame(&catalog, &ctx, &mut node, vec![click]);
    assert!(changed);
    assert_eq!(
        node["exclude_characters"],
        json!(["one", "user:missing-from-this-catalog", "two"])
    );
    assert!(
        egui::Popup::is_any_open(&ctx),
        "Selecting one character must allow selecting another"
    );

    let output = frame(&catalog, &ctx, &mut node, vec![]).0;
    let events = search(&output, "自定义");
    let (output, changed) = frame(&catalog, &ctx, &mut node, events);
    assert!(!changed);
    let click = action(&output, Role::CheckBox, Some("自定义人物"), Action::Click);
    let (_, changed) = frame(&catalog, &ctx, &mut node, vec![click]);
    assert!(changed);
    assert!(egui::Popup::is_any_open(&ctx));
    assert_eq!(
        node["exclude_characters"],
        json!([
            "one",
            "user:missing-from-this-catalog",
            "two",
            "user:project.actor"
        ])
    );
    let output = frame(&catalog, &ctx, &mut node, vec![]).0;
    let events = search(&output, "乙");
    let (output, changed) = frame(&catalog, &ctx, &mut node, events);
    assert!(!changed);
    let click = action(&output, Role::CheckBox, Some("人物乙"), Action::Click);
    let (_, changed) = frame(&catalog, &ctx, &mut node, vec![click]);
    assert!(changed);
    assert_eq!(
        node["exclude_characters"],
        json!([
            "one",
            "user:missing-from-this-catalog",
            "user:project.actor"
        ])
    );
    let output = frame(&catalog, &ctx, &mut node, vec![]).0;
    let click = action(
        &output,
        Role::Button,
        Some(&crate::i18n::key("editor.form.filter_exclusions_clear")),
        Action::Click,
    );
    let (_, changed) = frame(&catalog, &ctx, &mut node, vec![click]);
    assert!(changed);
    assert_eq!(node["exclude_characters"], json!([]));
    assert!(egui::Popup::is_any_open(&ctx));
    frame(
        &catalog,
        &ctx,
        &mut node,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!egui::Popup::is_any_open(&ctx));
}

#[test]
fn camera_character_search_preserves_absent_and_legacy_values_until_explicit_edit() {
    let catalog = catalog();
    for value in [
        None,
        Some(json!(["one", "legacy-character"])),
        Some(json!("legacy-invalid-value")),
    ] {
        let ctx = egui::Context::default();
        let mut node = json!({"id":"camera","type":"camera","name":"stage-memory","active":true});
        if let Some(value) = value {
            node["exclude_characters"] = value;
        }
        let before = node.clone();
        let output = open(&catalog, &ctx, &mut node);
        let events = search(&output, "人物");
        let (_, changed) = frame(&catalog, &ctx, &mut node, events);
        assert!(!changed);
        assert_eq!(node, before);
        let outside = egui::pos2(690.0, 590.0);
        for pressed in [true, false] {
            frame(
                &catalog,
                &ctx,
                &mut node,
                vec![
                    egui::Event::PointerMoved(outside),
                    egui::Event::PointerButton {
                        pos: outside,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(node, before);
        open(&catalog, &ctx, &mut node);
        assert_eq!(node, before);
    }
}
