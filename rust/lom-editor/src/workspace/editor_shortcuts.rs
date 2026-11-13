//! Contextual authoring commands; text fields, IMEs and popups retain their keys.
use super::*;

enum Action {
    Duplicate,
    Up,
    Down,
    Dialogue,
    DialogueBatch,
    SelectAll,
    Find,
    Preview,
}

impl App {
    pub(super) fn editor_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if !matches!(self.center, Center::Node | Center::Chapter)
            || self.is_vanilla_patch()
            || egui::Popup::is_any_open(ctx)
            || ctx
                .memory(|memory| memory.focused())
                .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
            || ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            })
        {
            return false;
        }
        let preview = self.view == View::Stage && !ctx.wants_keyboard_input();
        let action = ctx.input_mut(|input| {
            let selected = input.events.iter().enumerate().find_map(|(index, event)| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } = event
                else {
                    return None;
                };
                let action = if modifiers.matches_exact(egui::Modifiers::ALT) {
                    match key {
                        egui::Key::ArrowUp => Action::Up,
                        egui::Key::ArrowDown => Action::Down,
                        _ => return None,
                    }
                } else if *repeat {
                    return None;
                } else if modifiers.matches_exact(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT)
                    && *key == egui::Key::Enter
                {
                    Action::DialogueBatch
                } else if modifiers.matches_exact(egui::Modifiers::COMMAND) {
                    match key {
                        egui::Key::D => Action::Duplicate,
                        egui::Key::Enter => Action::Dialogue,
                        egui::Key::A => Action::SelectAll,
                        egui::Key::F => Action::Find,
                        _ => return None,
                    }
                } else if preview && modifiers.is_none() && *key == egui::Key::Space {
                    Action::Preview
                } else {
                    return None;
                };
                Some((index, action))
            });
            selected.map(|(index, action)| {
                input.events.remove(index);
                action
            })
        });
        let Some(action) = action else {
            return false;
        };
        match action {
            Action::Duplicate => self.duplicate_steps(),
            Action::Up => {
                if self.selected > 0 {
                    self.move_node(self.selected - 1);
                }
            }
            Action::Down => self.move_node(self.selected + 1),
            Action::Dialogue => self.add_node("say"),
            Action::DialogueBatch => self.open_dialogue_authoring(ctx),
            Action::SelectAll => {
                self.multiselect =
                    (0..self.story()["nodes"].as_array().map_or(0, Vec::len)).collect()
            }
            Action::Find => self.open_step_search(ctx),
            Action::Preview => self.toggle_preview(),
        }
        true
    }

    pub(super) fn open_step_search(&mut self, ctx: &egui::Context) {
        if self.open_center(Center::Node) {
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("step-search-open"), true);
                data.insert_temp(egui::Id::new("step-search-focus"), true);
            });
        }
    }

    pub(super) fn duplicate_steps(&mut self) {
        let original_count = self.story()["nodes"].as_array().map_or(0, Vec::len);
        let selected = self.selected;
        let clipboard = std::mem::take(&mut self.clipboard);
        self.copy();
        let count = self.clipboard.len();
        if let Some(&last) = self.selected_indices().last() {
            self.selected = last;
        }
        self.paste();
        self.clipboard = clipboard;
        if self.story()["nodes"].as_array().map_or(0, Vec::len) == original_count {
            self.selected = selected;
        } else {
            self.status = crate::i18n::message(
                "editor.shortcuts.duplicated",
                &[("count", count.to_string())],
            );
        }
    }
}
