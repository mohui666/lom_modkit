//! Task state choices keep their stored IDs; labels are only for authoring.
use crate::{i18n, scenario_quick::state_name};
use eframe::egui;
use serde_json::{json, Value};

const COMMON: &[&str] = &[
    "locked",
    "available",
    "active",
    "accepted",
    "completed",
    "declined",
    "failed",
    "cancelled",
];

pub fn states(task: &Value) -> Vec<String> {
    task["states"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|state| state.as_str().map(str::to_owned))
        .collect()
}

fn set_enabled(task: &mut Value, state: &str, enabled: bool) {
    if !task["states"].is_array() {
        task["states"] = json!([]);
    }
    let states = task["states"].as_array_mut().unwrap();
    if enabled {
        if !states.iter().any(|value| value == state) {
            states.push(json!(state));
        }
    } else {
        states.retain(|value| value != state);
    }
}

pub fn transition(task: &Value) -> Option<Value> {
    let states = states(task);
    let from = task["initial"]
        .as_str()
        .filter(|id| states.iter().any(|state| state == id))
        .or_else(|| states.first().map(String::as_str))?;
    let to = states.iter().find(|state| state.as_str() != from)?;
    Some(json!({"from":from,"to":to,"when":{"all":[]}}))
}

pub fn definitions(ui: &mut egui::Ui, task: &mut Value) {
    egui::CollapsingHeader::new(i18n::key("common.task_states.heading"))
        .id_salt("task-state-definitions")
        .default_open(true)
        .show(ui, |ui| {
            ui.small(i18n::key("common.task_states.hint"));
            let current = states(task);
            let mut choices = COMMON
                .iter()
                .map(|state| (*state).to_owned())
                .collect::<Vec<_>>();
            for state in &current {
                if !choices.contains(state) {
                    choices.push(state.clone());
                }
            }
            let columns = if ui.available_width() >= 280.0 { 2 } else { 1 };
            egui::Grid::new("task-state-choices")
                .num_columns(columns)
                .show(ui, |ui| {
                    for (index, state) in choices.iter().enumerate() {
                        let mut enabled = current.contains(state);
                        let response = ui
                            .add_enabled(
                                current.len() < 32 || enabled,
                                egui::Checkbox::new(&mut enabled, state_name(state)),
                            )
                            .on_hover_text(state);
                        if response.changed() {
                            set_enabled(task, state, enabled);
                        }
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.collapsing(i18n::key("common.task_states.custom"), |ui| {
                let id = ui.id().with("new-state");
                let mut draft = ui
                    .ctx()
                    .data(|data| data.get_temp::<String>(id))
                    .unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut draft)
                        .hint_text(i18n::key("common.task_states.custom_hint")),
                );
                let state = draft.trim();
                let valid = !state.is_empty()
                    && state.len() <= 64
                    && state
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
                if ui
                    .add_enabled(
                        valid && current.len() < 32 && !current.iter().any(|id| id == state),
                        egui::Button::new(i18n::key("common.task_states.add")),
                    )
                    .clicked()
                {
                    set_enabled(task, state, true);
                    draft.clear();
                }
                ui.ctx().data_mut(|data| data.insert_temp(id, draft));
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choosing_accepted_preserves_custom_ids_and_valid_transition_defaults() {
        let mut task = json!({"id":"letter","states":["waiting","Accepted","delivered"],"initial":"waiting","transitions":[]});
        set_enabled(&mut task, "accepted", true);
        set_enabled(&mut task, "accepted", true);
        assert_eq!(
            task["states"],
            json!(["waiting", "Accepted", "delivered", "accepted"])
        );
        assert_eq!(
            transition(&task).unwrap(),
            json!({"from":"waiting","to":"Accepted","when":{"all":[]}})
        );
        assert_eq!(task["initial"], "waiting");
        assert!(transition(&json!({"initial":"waiting","states":["waiting"]})).is_none());
    }

    #[test]
    fn viewing_legacy_task_choices_preserves_its_source() {
        let context = egui::Context::default();
        let mut task = json!({"id":"letter","states":["locked","accepted","custom_state"],"initial":"custom_state","transitions":[{"from":"custom_state","to":"accepted","when":{"all":[]}}]});
        let original = task.clone();
        for width in [160.0, 600.0] {
            let _ = context.run(egui::RawInput::default(), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    ui.set_width(width);
                    definitions(ui, &mut task);
                });
            });
            assert_eq!(task, original);
        }
    }

    #[test]
    fn released_task_edits_save_export_reopen_without_renaming_old_states() {
        use lom_core::{package, project::Project};
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lom-core/tests/fixtures/released-1.5.9-scenario-modes.lommod");
        let source = std::fs::read(&fixture).unwrap();
        let mut project = Project::open(&fixture).unwrap();
        let original = project.clone();
        let task = &mut project.scenario.as_mut().unwrap()["tasks"][0];
        set_enabled(task, "accepted", true);
        task["initial"] = json!("accepted");
        task["transitions"] = json!([transition(task).unwrap()]);
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("edited");
        project.save_to(&folder).unwrap();
        let edited = Project::open(&folder).unwrap();
        let output = directory.path().join("edited.lommod");
        edited.export(&output).unwrap();
        let reopened = Project::open(&output).unwrap();
        let task = &reopened.scenario.as_ref().unwrap()["tasks"][0];
        assert_eq!(task["states"], json!(["locked", "done", "accepted"]));
        assert_eq!(task["initial"], "accepted");
        assert_eq!(reopened.stories, original.stories);
        assert_eq!(reopened.assets, original.assets);
        assert_eq!(
            reopened.manifest["campaign_id"],
            original.manifest["campaign_id"]
        );
        let mut entries = package::read_package(&output).unwrap();
        entries.remove("package-content.sha256");
        entries
            .get_mut("lua/main.lua")
            .unwrap()
            .extend_from_slice(b"\nerror('unrelated substitution')\n");
        let tampered = directory.path().join("tampered.lommod");
        package::write_package(entries, &tampered).unwrap();
        assert!(Project::open(&tampered).is_err());
        assert_eq!(std::fs::read(fixture).unwrap(), source);
    }
}
