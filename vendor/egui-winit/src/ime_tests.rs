use super::*;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

struct NoDisplay;
impl HasDisplayHandle for NoDisplay {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Err(raw_window_handle::HandleError::Unavailable)
    }
}

fn frame(ctx: &egui::Context, text: &mut String, events: Vec<egui::Event>) {
    let _ = ctx.run(
        egui::RawInput {
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::TextEdit::multiline(text)
                    .id(egui::Id::new("dialogue"))
                    .show(ui);
            });
        },
    );
}

#[test]
fn ime_processed_backspace_cannot_delete_committed_dialogue_but_normal_backspace_can() {
    let ctx = egui::Context::default();
    let mut backend = State::new(ctx.clone(), ViewportId::ROOT, &NoDisplay, None, None, None);
    backend.set_allow_ime(true);
    let mut text = String::from("original dialogue");
    frame(&ctx, &mut text, vec![]);
    let id = egui::Id::new("dialogue");
    ctx.memory_mut(|m| m.request_focus(id));
    let mut edit = egui::TextEdit::load_state(&ctx, id).unwrap();
    edit.cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(text.chars().count()),
        )));
    edit.store(&ctx, id);

    // VK_PROCESSKEY carries a physical Backspace after an IME has consumed it.
    // The candidate may already be cleared, so TextEdit's ime_enabled is false.
    let physical = PhysicalKey::Code(KeyCode::Backspace);
    backend.on_keyboard_input(
        &physical,
        &Key::Named(NamedKey::Process),
        None,
        ElementState::Pressed,
    );
    frame(
        &ctx,
        &mut text,
        std::mem::take(&mut backend.egui_input.events),
    );
    assert_eq!(
        text, "original dialogue",
        "IME-handled Backspace deleted committed text"
    );

    backend.on_keyboard_input(
        &physical,
        &Key::Named(NamedKey::Backspace),
        None,
        ElementState::Released,
    );
    assert!(
        backend.egui_input.events.is_empty(),
        "paired key release must also be consumed"
    );
    backend.on_keyboard_input(
        &physical,
        &Key::Named(NamedKey::Backspace),
        None,
        ElementState::Pressed,
    );
    frame(
        &ctx,
        &mut text,
        std::mem::take(&mut backend.egui_input.events),
    );
    assert_eq!(
        text, "original dialogu",
        "ordinary Backspace must still edit text"
    );
}
