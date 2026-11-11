//! Project mode and the shared scenario/manifest opening binding.
use crate::{forms::Catalog, i18n};
use eframe::egui;
use lom_core::{project::Project, scenario_edit};

#[derive(Clone, Default)]
struct Draft {
    mode: String,
    trigger: String,
    position: String,
    story: String,
}

fn choice(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    selected: &mut String,
    options: &[(String, String)],
    empty: &str,
) -> bool {
    ui.label(i18n::key(label));
    let before = selected.clone();
    let caption = options
        .iter()
        .find(|(id, _)| id == selected)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| {
            if selected.is_empty() {
                i18n::key(empty)
            } else {
                selected.clone()
            }
        });
    egui::ComboBox::from_id_salt(salt)
        .width(ui.available_width().clamp(80.0, 320.0))
        .truncate()
        .selected_text(caption)
        .show_ui(ui, |ui| {
            for (id, name) in options {
                ui.selectable_value(selected, id.clone(), name)
                    .on_hover_text(id);
            }
        });
    before != *selected
}

fn record(ui: &egui::Ui, id: egui::Id, result: anyhow::Result<()>) {
    ui.ctx().data_mut(|data| match result {
        Ok(()) => {
            data.remove::<String>(id);
        }
        Err(error) => data.insert_temp(id, format!("{error:#}")),
    });
}
fn problem(ui: &mut egui::Ui, id: egui::Id) {
    if let Some(error) = ui.ctx().data(|data| data.get_temp::<String>(id)) {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
}

pub fn mode(ui: &mut egui::Ui, project: &mut Project, catalog: &Catalog) {
    let current = project.manifest["play_mode"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    if !["sandbox", "addon"].contains(&current.as_str()) || project.scenario.is_none() {
        return;
    }
    let id = ui.id().with((
        "scenario-mode",
        project.manifest["id"].as_str().unwrap_or(""),
    ));
    let error_id = id.with("error");
    let mut draft = ui
        .ctx()
        .data(|data| data.get_temp::<Draft>(id))
        .unwrap_or_default();
    let modes = vec![
        ("sandbox".into(), i18n::key("editor.mode.sandbox")),
        ("addon".into(), i18n::key("editor.mode.addon")),
    ];
    let mode_name = modes
        .iter()
        .find(|(mode, _)| mode == &current)
        .unwrap()
        .1
        .clone();
    ui.heading(i18n::key("editor.mode.heading"));
    ui.label(i18n::message("editor.mode.current", &[("mode", mode_name)]));
    choice(
        ui,
        "target-mode",
        "editor.mode.choose",
        &mut draft.mode,
        &modes,
        "editor.mode.pick",
    );
    if !draft.mode.is_empty() && draft.mode != current {
        ui.small(i18n::key("editor.mode.persistence_hint"));
        let story_ids = project.stories.keys().cloned().collect::<Vec<_>>();
        let names = crate::scenario_quick::Names::new(&project.stories, project.scenario.as_ref());
        let mut ready = true;
        if draft.mode == "addon" {
            let starts = project.scenario.as_ref().unwrap()["events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|event| event["trigger"] == "start")
                .count();
            ui.small(i18n::key("editor.mode.addon_plan"));
            if starts > 0 {
                choice(
                    ui,
                    "former-opening-trigger",
                    "editor.mode.opening_policy",
                    &mut draft.trigger,
                    &[
                        ("auto".into(), i18n::key("editor.mode.auto")),
                        ("position".into(), i18n::key("editor.mode.position")),
                    ],
                    "editor.mode.pick_trigger",
                );
                ready = ["auto", "position"].contains(&draft.trigger.as_str());
                if draft.trigger == "position" {
                    let places =
                        catalog.options("free_position", &serde_json::Value::Null, &[], &story_ids);
                    choice(
                        ui,
                        "former-opening-position",
                        "editor.mode.position_label",
                        &mut draft.position,
                        &places,
                        "editor.mode.pick_position",
                    );
                    ready &= places
                        .iter()
                        .any(|(position, _)| position == &draft.position);
                }
            }
        } else {
            ui.small(i18n::key("editor.mode.sandbox_plan"));
            if draft.story.is_empty() {
                draft.story = project.manifest["entry"].as_str().unwrap_or("").to_owned();
            }
            choice(
                ui,
                "new-opening-story",
                "editor.opening.story",
                &mut draft.story,
                &names.stories,
                "editor.opening.pick_story",
            );
            ready = project.stories.contains_key(&draft.story);
        }
        let name = modes
            .iter()
            .find(|(mode, _)| mode == &draft.mode)
            .unwrap()
            .1
            .clone();
        if ui
            .add_enabled(
                ready,
                egui::Button::new(i18n::message("editor.mode.apply", &[("mode", name)])),
            )
            .clicked()
        {
            let result = if draft.mode == "addon" {
                scenario_edit::to_addon(
                    project,
                    &draft.trigger,
                    (draft.trigger == "position").then_some(draft.position.as_str()),
                )
            } else {
                scenario_edit::to_sandbox(project, &draft.story)
            };
            record(ui, error_id, result);
        }
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, draft));
    problem(ui, error_id);
}

pub fn opening(ui: &mut egui::Ui, project: &mut Project) {
    if project.manifest["play_mode"] != "sandbox" {
        return;
    }
    let Some(config) = project.scenario.as_ref() else {
        return;
    };
    let Some(events) = config["events"].as_array() else {
        return;
    };
    let names = crate::scenario_quick::Names::new(&project.stories, Some(config));
    let openings = events
        .iter()
        .filter(|event| event["trigger"] == "start")
        .filter_map(|event| {
            Some((
                event["id"].as_str()?.to_owned(),
                names.event(event),
                event["script"].as_str().unwrap_or("").to_owned(),
            ))
        })
        .collect::<Vec<_>>();
    let configured = config["entry_event"].as_str().unwrap_or("").to_owned();
    let error_id = ui.id().with("scenario-entry-error");
    ui.heading(i18n::key("editor.opening.heading"));
    ui.small(i18n::key("editor.opening.link_hint"));
    let manifest_entry = project.manifest["entry"].as_str().unwrap_or("").to_owned();
    let entry_name = names
        .stories
        .iter()
        .find(|(id, _)| id == &manifest_entry)
        .map(|(_, name)| name.as_str())
        .unwrap_or(&manifest_entry);
    ui.label(i18n::message(
        "editor.opening.manifest_entry",
        &[("story", format!("{entry_name} ({manifest_entry})"))],
    ));
    if openings.is_empty() {
        ui.small(i18n::key("editor.opening.missing"));
        let id = ui.id().with("new-entry-story");
        let mut story = ui
            .ctx()
            .data(|data| data.get_temp::<String>(id))
            .unwrap_or(manifest_entry);
        choice(
            ui,
            "create-entry-story",
            "editor.opening.story",
            &mut story,
            &names.stories,
            "editor.opening.pick_story",
        );
        if ui
            .add_enabled(
                project.stories.contains_key(&story),
                egui::Button::new(i18n::key("editor.opening.create")),
            )
            .clicked()
        {
            record(
                ui,
                error_id,
                scenario_edit::create_entry(project, &story).map(|_| ()),
            );
        }
        ui.ctx().data_mut(|data| data.insert_temp(id, story));
    } else {
        let mut event = if openings.iter().any(|(id, _, _)| id == &configured) {
            configured.clone()
        } else {
            openings[0].0.clone()
        };
        let options = openings
            .iter()
            .map(|(id, name, _)| (id.clone(), name.clone()))
            .collect::<Vec<_>>();
        let event_changed = choice(
            ui,
            "opening-event",
            "editor.opening.event",
            &mut event,
            &options,
            "editor.opening.pick_event",
        );
        let original_story = openings
            .iter()
            .find(|(id, _, _)| id == &event)
            .unwrap()
            .2
            .clone();
        let mut story = original_story.clone();
        let story_changed = choice(
            ui,
            "opening-story",
            "editor.opening.story",
            &mut story,
            &names.stories,
            "editor.opening.pick_story",
        );
        if event_changed || story_changed {
            record(
                ui,
                error_id,
                scenario_edit::set_entry(project, &event, &story),
            );
        } else if (event != configured || original_story != manifest_entry)
            && ui.button(i18n::key("editor.opening.repair")).clicked()
        {
            record(
                ui,
                error_id,
                scenario_edit::set_entry(project, &event, &story),
            );
        }
    }
    problem(ui, error_id);
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(
        ctx: &egui::Context,
        project: &mut Project,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.enable_accesskit();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 700.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| opening(ui, project));
            },
        )
    }
    #[test]
    fn stale_opening_is_visible_and_repair_button_fixes_the_actual_project() {
        let mut project = Project::new_scenario("sandbox");
        let mut side = project.stories["main"].clone();
        side["id"] = json!("side");
        project.stories.insert("side".into(), side);
        project.scenario.as_mut().unwrap()["events"][0]["script"] = json!("side");
        let before = project.clone();
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut project, vec![]);
        assert_eq!(
            project, before,
            "browsing must not silently change an invalid association"
        );
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(i18n::key("editor.opening.repair").as_str()))
                .then_some(id)
            })
            .expect("a broken association must expose an actionable repair button");
        frame(
            &ctx,
            &mut project,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        assert_eq!(project.manifest["entry"], "side");
        assert_eq!(project.scenario, before.scenario);
        assert_eq!(project.stories, before.stories);
        lom_core::scenario::validate(
            project.scenario.as_ref().unwrap(),
            &project.manifest,
            &project.stories,
        )
        .unwrap();
        assert_eq!(
            crate::error_location::resolve(
                "scenario.json: entry_event 必须是开场事件且脚本与 manifest.entry 相同",
                &project
            ),
            crate::error_location::Destination::Manifest
        );
    }

    #[test]
    fn settings_browsing_preserves_mode_identity_and_all_business_rules() {
        let catalog = Catalog::new();
        for mode_name in ["sandbox", "addon"] {
            let mut project = Project::new_scenario(mode_name);
            let before = project.clone();
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::manifest_panel::show_project(ui, &mut project, &catalog)
                });
            });
            assert_eq!(project, before);
        }
    }
}
