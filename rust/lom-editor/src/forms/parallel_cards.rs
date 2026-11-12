use super::{Catalog, Value};
use crate::i18n::{key, message, tr};
use eframe::egui::{self, Color32, Ui};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct CardState {
    initialized: bool,
    expanded: Option<usize>,
    observed: Option<usize>,
}

impl CardState {
    fn sync(&mut self, count: usize, requested: Option<usize>) -> Option<usize> {
        let requested = requested.filter(|index| *index < count);
        self.expanded = self.expanded.filter(|index| *index < count);
        if !self.initialized {
            self.initialized = true;
            self.expanded = requested.or((count > 0).then_some(0));
            self.observed = requested;
            None
        } else if requested != self.observed {
            self.observed = requested;
            if let Some(index) = requested {
                self.expanded = Some(index);
            }
            requested
        } else {
            None
        }
    }

    fn choose(&mut self, index: usize) {
        self.expanded = Some(index);
        self.observed = Some(index);
    }

    fn remove(&mut self, index: usize, count: usize) {
        let remap = |selected: Option<usize>| {
            selected.and_then(|selected| {
                if selected == index {
                    (count > 0).then_some(index.min(count.saturating_sub(1)))
                } else {
                    Some(selected - usize::from(selected > index))
                }
            })
        };
        self.expanded = remap(self.expanded);
        self.observed = remap(self.observed);
    }

    fn swap(&mut self, a: usize, b: usize) {
        let remap = |selected: Option<usize>| {
            selected.map(|index| match index {
                index if index == a => b,
                index if index == b => a,
                index => index,
            })
        };
        self.expanded = remap(self.expanded);
        self.observed = remap(self.observed);
    }
}

fn select_preview(ctx: &egui::Context, parent: &str, index: Option<usize>) {
    let id = egui::Id::new("preview-character-action");
    ctx.data_mut(|data| {
        if let Some(index) = index {
            data.insert_temp(id, (parent.to_owned(), index));
        } else {
            data.remove::<(String, usize)>(id);
        }
    });
}

/// A click is also a request to reveal the card when this action was already selected.
pub(crate) fn reveal_parallel_action(ctx: &egui::Context, parent: &str, index: usize) {
    select_preview(ctx, parent, Some(index));
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("reveal-parallel-action"),
            (parent.to_owned(), index),
        );
    });
    ctx.request_repaint();
}

fn seconds(duration: f64) -> String {
    format!("{duration:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn timing_label(duration: f64) -> String {
    if duration > 0.0 {
        message("editor.parallel.seconds", &[("seconds", seconds(duration))])
    } else {
        key("editor.parallel.started")
    }
}

fn title(catalog: &Catalog, action: &Value, index: usize, duration: f64) -> String {
    let kind = action["type"].as_str().unwrap_or("");
    let character = action["character"].as_str().unwrap_or("");
    let target = if character.is_empty() {
        ["image", "name", "slot"]
            .into_iter()
            .find_map(|key| action[key].as_str().filter(|value| !value.is_empty()))
            .unwrap_or("")
            .to_owned()
    } else {
        catalog
            .options("character", action, &[], &[])
            .into_iter()
            .find_map(|(id, name)| (id == character).then_some(name))
            .unwrap_or_else(|| character.to_owned())
    };
    let target = if target.is_empty() {
        String::new()
    } else {
        format!("{} · ", target)
    };
    format!(
        "{} · {}{} · {}",
        index + 1,
        target,
        catalog.label(kind),
        timing_label(duration)
    )
}

pub(super) fn show(catalog: &Catalog, ui: &mut Ui, node: &mut Value) -> bool {
    ui.label(tr("这些动作会一起开始，全部完成后才继续下一步。"));
    ui.small(tr(
        "音乐、音效和循环特效在启动后即视为完成；可添加“等待”控制整组停留时间。",
    ));
    let Some(actions) = node.get("actions").and_then(Value::as_array) else {
        ui.colored_label(
            Color32::from_rgb(179, 55, 49),
            tr("同时动作数据不是数组，请先修正原始数据。"),
        );
        return false;
    };
    let parent = node["id"].as_str().unwrap_or("").to_owned();
    let state_id = ui.id().with(("parallel-cards", &parent));
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<CardState>(state_id))
        .unwrap_or_default();
    let requested = ui
        .ctx()
        .data(|data| data.get_temp::<(String, usize)>(egui::Id::new("preview-character-action")))
        .filter(|(id, _)| id == &parent)
        .map(|(_, index)| index);
    let mut scroll_to = state.sync(actions.len(), requested);
    let reveal = ui.ctx().data_mut(|data| {
        let id = egui::Id::new("reveal-parallel-action");
        let request = data
            .get_temp::<(String, usize)>(id)
            .filter(|(id, index)| id == &parent && *index < actions.len());
        if request.is_some() {
            data.remove::<(String, usize)>(id);
        }
        request.map(|(_, index)| index)
    });
    if let Some(index) = reveal {
        state.choose(index);
        scroll_to = Some(index);
    }
    let durations: Vec<_> = actions
        .iter()
        .map(crate::preview::action_duration)
        .collect();
    let total = durations.iter().copied().fold(0.0, f64::max);
    let labels: Vec<_> = actions
        .iter()
        .enumerate()
        .map(|(index, action)| title(catalog, action, index, durations[index]))
        .collect();
    egui::CollapsingHeader::new(message(
        "editor.parallel.timeline",
        &[("seconds", seconds(total))],
    ))
    .id_salt(("parallel-timeline", &parent))
    .default_open(actions.len() <= 6)
    .show(ui, |ui| {
        ui.small(key("editor.parallel.timeline_hint"));
        egui::ScrollArea::vertical()
            .id_salt("parallel-timeline-rows")
            .max_height(160.0)
            .show(ui, |ui| {
                for (index, label) in labels.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                [ui.available_width() * 0.65, 22.0],
                                egui::Button::selectable(state.expanded == Some(index), label)
                                    .truncate(),
                            )
                            .clicked()
                        {
                            state.choose(index);
                            select_preview(ui.ctx(), &parent, Some(index));
                            scroll_to = Some(index);
                        }
                        let fraction = if total > 0.0 {
                            (durations[index] / total) as f32
                        } else {
                            0.0
                        };
                        if ui
                            .add(
                                egui::ProgressBar::new(fraction)
                                    .desired_width(ui.available_width())
                                    .text(timing_label(durations[index])),
                            )
                            .interact(egui::Sense::click())
                            .clicked()
                        {
                            state.choose(index);
                            select_preview(ui.ctx(), &parent, Some(index));
                            scroll_to = Some(index);
                        }
                    });
                }
            });
    });
    let mut actions = actions.clone();
    let mut changed = false;
    let mut remove = None;
    let mut swap = None;
    for (index, action) in actions.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            let mut action_changed = false;
            let response = egui::Frame::group(ui.style()).show(ui, |ui| {
                let header = ui.horizontal_wrapped(|ui| {
                    let expanded = state.expanded == Some(index);
                    let symbol = if expanded { "▾" } else { "▸" };
                    if ui
                        .add(egui::Button::new(format!("{symbol} {}", labels[index])).frame(false))
                        .clicked()
                    {
                        state.expanded = if expanded { None } else { Some(index) };
                        state.observed = Some(index);
                        select_preview(ui.ctx(), &parent, Some(index));
                        if !expanded {
                            scroll_to = Some(index);
                        }
                    }
                    if ui
                        .add_enabled(index > 0, egui::Button::new(tr("上移")))
                        .clicked()
                    {
                        swap = Some((index, index - 1));
                    }
                    if ui.small_button(tr("移除")).clicked() {
                        remove = Some(index);
                    }
                });
                if scroll_to == Some(index) {
                    header.response.scroll_to_me(Some(egui::Align::Min));
                }
                if state.expanded != Some(index) {
                    return;
                }
                let old = action["type"].as_str().unwrap_or("").to_owned();
                let mut selected = old.clone();
                egui::ComboBox::from_id_salt("parallel-action-type")
                    .selected_text(catalog.label(&old))
                    .show_ui(ui, |ui| {
                        for kind in lom_core::parallel::ACTION_TYPES {
                            ui.selectable_value(&mut selected, (*kind).into(), catalog.label(kind));
                        }
                    });
                if selected != old {
                    *action = catalog.new_node(&selected, String::new());
                    action.as_object_mut().unwrap().remove("id");
                    changed = true;
                    action_changed = true;
                }
                let kind = action["type"].as_str().unwrap_or("");
                if !lom_core::parallel::ACTION_TYPES.contains(&kind) {
                    ui.colored_label(
                        Color32::from_rgb(179, 55, 49),
                        tr("这个步骤不能同时执行，请改为演出动作或移到组外。"),
                    );
                    return;
                }
                let fields = catalog.schema["NODE_SCHEMAS"][kind]["fields"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                super::set_preview_action(ui.ctx(), Some(index));
                for field in fields {
                    if field[0] == "appearance" || field[0] == "wait_for_completion" {
                        continue;
                    }
                    if let (Some(key), Some(label), Some(field_kind)) =
                        (field[0].as_str(), field[1].as_str(), field[2].as_str())
                    {
                        let field_changed = catalog.field(
                            ui,
                            action,
                            key,
                            label,
                            field_kind,
                            field[3].as_bool().unwrap_or(false),
                            &[],
                            &[],
                        );
                        changed |= field_changed;
                        action_changed |= field_changed;
                    }
                }
                super::set_preview_action(ui.ctx(), None);
                for key in ["id", "goto", "appearance"] {
                    if action.get(key).is_some() {
                        ui.colored_label(
                            Color32::from_rgb(179, 55, 49),
                            format!("{}: {key}", tr("组内不支持此字段")),
                        );
                        if ui
                            .small_button(format!("{} {key}", tr("移除非法字段")))
                            .clicked()
                        {
                            action.as_object_mut().unwrap().remove(key);
                            changed = true;
                            action_changed = true;
                        }
                    }
                }
            });
            if action_changed
                || response.response.contains_pointer() && ui.input(|i| i.pointer.any_pressed())
            {
                state.observed = Some(index);
                select_preview(ui.ctx(), &parent, Some(index));
            }
        });
    }
    if let Some(index) = remove {
        actions.remove(index);
        state.remove(index, actions.len());
        select_preview(ui.ctx(), &parent, state.observed);
        changed = true;
    } else if let Some((a, b)) = swap {
        actions.swap(a, b);
        state.swap(a, b);
        select_preview(ui.ctx(), &parent, state.observed);
        changed = true;
    }
    ui.add_enabled_ui(actions.len() < lom_core::parallel::MAX_ACTIONS, |ui| {
        ui.menu_button(tr("添加同时动作"), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("parallel-add-actions")
                .max_height((ui.ctx().content_rect().height() * 0.6).max(1.0))
                .show(ui, |ui| {
                    for kind in lom_core::parallel::ACTION_TYPES {
                        if ui.button(catalog.label(kind)).clicked() {
                            let mut action = catalog.new_node(kind, String::new());
                            action.as_object_mut().unwrap().remove("id");
                            actions.push(action);
                            let index = actions.len() - 1;
                            state.choose(index);
                            select_preview(ui.ctx(), &parent, Some(index));
                            changed = true;
                            ui.close();
                        }
                    }
                });
        });
    });
    ui.ctx().data_mut(|data| data.insert_temp(state_id, state));
    if changed {
        node["actions"] = Value::Array(actions);
    }
    if let Err(error) = lom_core::parallel::validate_group(node) {
        crate::error_ui::show_with_actions(ui, &crate::error_ui::capture(error));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn focus_opens_one_card_without_undoing_a_deliberate_collapse() {
        let mut state = CardState::default();
        assert_eq!(state.sync(3, None), None);
        assert_eq!(state.expanded, Some(0));
        assert_eq!(state.sync(3, Some(2)), Some(2));
        assert_eq!(state.expanded, Some(2));
        state.expanded = None;
        assert_eq!(state.sync(3, Some(2)), None);
        assert_eq!(state.expanded, None);
        assert_eq!(state.sync(3, Some(1)), Some(1));
        assert_eq!(state.expanded, Some(1));
        assert_eq!(state.sync(3, Some(99)), None);
        assert_eq!(state.expanded, Some(1));
    }

    #[test]
    fn selected_action_tracks_moves_and_removal() {
        let mut state = CardState::default();
        state.sync(4, Some(2));
        state.swap(2, 1);
        assert_eq!(state.expanded, Some(1));
        assert_eq!(state.observed, Some(1));
        state.remove(0, 3);
        assert_eq!(state.expanded, Some(0));
        assert_eq!(state.observed, Some(0));
        state.remove(0, 2);
        assert_eq!(state.expanded, Some(0));
        state.remove(0, 0);
        assert_eq!(state.expanded, None);
        assert_eq!(state.observed, None);
    }

    fn frame(
        catalog: &Catalog,
        context: &egui::Context,
        node: &mut Value,
    ) -> (egui::FullOutput, bool) {
        frame_with_events(catalog, context, node, vec![])
    }

    fn frame_with_events(
        catalog: &Catalog,
        context: &egui::Context,
        node: &mut Value,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, bool) {
        context.enable_accesskit();
        let mut changed = false;
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    changed = show(catalog, ui, node);
                });
            },
        );
        (output, changed)
    }

    #[test]
    fn external_preview_selection_only_changes_visible_form_not_the_story() {
        use egui::accesskit::Role;
        let catalog = Catalog::new();
        let context = egui::Context::default();
        let mut node = json!({"id":"group","type":"parallel","actions":[
            {"type":"wait","seconds":1.5},
            {"type":"portrait","character":"player","portrait":"angry1"},
            {"type":"move","character":"brother1","from":"M","to":"R1","duration":2.0}
        ]});
        let original = node.clone();
        let (output, changed) = frame(&catalog, &context, &mut node);
        assert!(!changed);
        assert_eq!(node, original);
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let combos: Vec<_> = nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::ComboBox)
            .collect();
        assert_eq!(
            combos.len(),
            1,
            "Only the first action's type selector is expanded"
        );
        assert_eq!(combos[0].1.value(), Some(catalog.label("wait").as_str()));
        select_preview(&context, "group", Some(2));
        let (output, changed) = frame(&catalog, &context, &mut node);
        assert!(!changed);
        assert_eq!(node, original);
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        assert!(nodes.iter().any(|(_, node)| node.role() == Role::ComboBox
            && node.value() == Some(catalog.label("move").as_str())));
        assert!(!nodes.iter().any(|(_, node)| node.role() == Role::ComboBox
            && node.value() == Some(catalog.label("wait").as_str())));

        let header = format!(
            "▾ {}",
            title(
                &catalog,
                &node["actions"][2],
                2,
                crate::preview::action_duration(&node["actions"][2]),
            )
        );
        let target = nodes
            .iter()
            .find_map(|(id, node)| {
                (node.role() == Role::Button && node.label() == Some(header.as_str()))
                    .then_some(*id)
            })
            .unwrap();
        let (output, changed) = frame_with_events(
            &catalog,
            &context,
            &mut node,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        assert!(!changed);
        assert!(!output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::ComboBox));
        reveal_parallel_action(&context, "group", 2);
        let (output, changed) = frame(&catalog, &context, &mut node);
        assert!(!changed);
        assert_eq!(node, original);
        assert!(output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::ComboBox
                && node.value() == Some(catalog.label("move").as_str())));
    }
}
