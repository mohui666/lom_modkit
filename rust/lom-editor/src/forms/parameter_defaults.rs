use super::*;
use anyhow::{Context, Result};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};

type Values = BTreeMap<String, BTreeMap<String, Value>>;

#[derive(Default)]
pub(super) struct ParameterDefaults {
    values: RefCell<Values>,
    path: Option<PathBuf>,
}

fn supported(kind: &str) -> bool {
    kind.starts_with("enum:")
        || matches!(
            kind,
            "bool"
                | "int"
                | "float"
                | "bool_int"
                | "discount_toggle"
                | "percent_scale"
                | "percent_cg_scale"
                | "percent_position"
                | "percent_offset"
                | "percent_opacity"
                | "line"
                | "str"
                | "text"
                | "multiline"
                | "mode"
                | "facing"
                | "branch_source"
                | "position"
                | "camera"
        )
}

impl ParameterDefaults {
    pub(super) fn load(&mut self, path: PathBuf) -> Result<()> {
        self.path = Some(path.clone());
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("{}", path.display())),
        };
        *self.values.get_mut() =
            serde_json::from_slice(&bytes).with_context(|| format!("{}", path.display()))?;
        Ok(())
    }

    fn update(&self, node_type: &str, key: &str, value: Option<Value>) -> Result<()> {
        let mut next = self.values.borrow().clone();
        if let Some(value) = value {
            next.entry(node_type.into())
                .or_default()
                .insert(key.into(), value);
        } else if let Some(fields) = next.get_mut(node_type) {
            fields.remove(key);
            if fields.is_empty() {
                next.remove(node_type);
            }
        }
        if let Some(path) = &self.path {
            let bytes = lom_core::stable_json(&serde_json::to_value(&next)?)?;
            lom_core::project::atomic_write(path, &bytes)?;
        }
        *self.values.borrow_mut() = next;
        Ok(())
    }

    pub(super) fn apply(&self, node: &mut Value, fields: &Value) {
        let values = self.values.borrow();
        let Some(overrides) = node["type"].as_str().and_then(|kind| values.get(kind)) else {
            return;
        };
        for field in fields.as_array().into_iter().flatten() {
            let (Some(key), Some(kind)) = (field[0].as_str(), field[2].as_str()) else {
                continue;
            };
            if !supported(kind) || matches!(key, "id" | "type") {
                continue;
            }
            if let Some(value) = overrides.get(key) {
                if value.is_null() {
                    node.as_object_mut().unwrap().remove(key);
                } else if value.is_string() || value.is_number() || value.is_boolean() {
                    if node["type"] == "effect" && key == "space" {
                        set_effect_space(node, value.as_str().unwrap_or("world"));
                    } else {
                        node[key] = value.clone();
                        if node["type"] == "effect" && key == "play" && node.get("space").is_none()
                        {
                            node["space"] = json!("world");
                        }
                    }
                }
            }
        }
        if node["type"] == "say" && node["mode"] == "offscreen" {
            node.as_object_mut().unwrap().remove("character");
            node.as_object_mut().unwrap().remove("portrait");
        }
    }

    pub(super) fn width(&self, ui: &Ui, node: &Value, key: &str, kind: &str) -> f32 {
        if !supported(kind) || matches!(key, "id" | "type") {
            return 0.0;
        }
        let node_type = node["type"].as_str().unwrap_or("");
        if node_type.is_empty() {
            return 0.0;
        }
        ui.painter()
            .layout_no_wrap(
                format!("{} ✓", crate::i18n::key("editor.defaults.set")),
                egui::TextStyle::Button.resolve(ui.style()),
                ui.visuals().text_color(),
            )
            .size()
            .x
            + ui.spacing().button_padding.x * 2.0
            + ui.spacing().item_spacing.x
    }

    pub(super) fn button(
        &self,
        ui: &mut Ui,
        node: &Value,
        key: &str,
        kind: &str,
        value: &Value,
    ) -> Option<String> {
        if self.width(ui, node, key, kind) == 0.0 {
            return None;
        }
        let node_type = node["type"].as_str().unwrap();
        let error_id = ui.id().with(("parameter-default-error", node_type, key));
        let saved = self
            .values
            .borrow()
            .get(node_type)
            .and_then(|fields| fields.get(key))
            .cloned();
        let is_saved = saved.as_ref() == Some(value);
        let mut label = crate::i18n::key("editor.defaults.set");
        if is_saved {
            label.push_str(" ✓");
        }
        let response = ui
            .add(egui::Button::new(label).small().selected(is_saved))
            .on_hover_text(if is_saved {
                format!(
                    "{}\n{}",
                    crate::i18n::key("editor.defaults.saved"),
                    crate::i18n::key("editor.defaults.hint")
                )
            } else {
                crate::i18n::key("editor.defaults.hint")
            });
        let mut result = response
            .clicked()
            .then(|| self.update(node_type, key, Some(value.clone())));
        response.context_menu(|ui| {
            if ui
                .add_enabled(
                    saved.is_some(),
                    egui::Button::new(crate::i18n::key("editor.defaults.reset")),
                )
                .clicked()
            {
                result = Some(self.update(node_type, key, None));
                ui.close();
            }
        });
        if let Some(result) = result {
            ui.data_mut(|data| {
                if let Err(error) = result {
                    data.insert_temp(error_id, crate::error_ui::capture(error));
                } else {
                    data.remove::<String>(error_id);
                }
            });
        }
        ui.data(|data| data.get_temp::<String>(error_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn released_project_keeps_existing_parameters_with_custom_defaults_and_reexports() {
        use lom_core::{package, project::Project};
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/showcase3-legacy.lommod");
        let original_bytes = std::fs::read(&fixture).unwrap();
        let mut project = Project::open(&fixture).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::new();
        catalog
            .configure_parameter_defaults(directory.path().join("node-defaults.json"))
            .unwrap();
        catalog
            .parameter_defaults
            .update("show", "fadeDuration", Some(json!(0.85)))
            .unwrap();
        catalog
            .parameter_defaults
            .update("show", "wait_after", Some(json!(0.4)))
            .unwrap();
        let old = project
            .stories
            .values_mut()
            .flat_map(|story| story["nodes"].as_array_mut().unwrap())
            .find(|node| node["type"] == "show")
            .unwrap();
        let original = old.clone();
        let context = egui::Context::default();
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!catalog.field(
                    ui,
                    old,
                    "fadeDuration",
                    "fadeDuration",
                    "float",
                    true,
                    &[],
                    &[]
                ));
                assert!(!catalog.field(
                    ui,
                    old,
                    "wait_after",
                    "wait_after",
                    "float",
                    true,
                    &[],
                    &[]
                ));
            });
        });
        assert_eq!(*old, original);
        old["wait_after"] = json!(0.2);
        let expected = project.stories.clone();
        let saved = directory.path().join("edited");
        project.save_to(&saved).unwrap();
        let exported = directory.path().join("edited.lommod");
        Project::open(&saved).unwrap().export(&exported).unwrap();
        assert_eq!(Project::open(&exported).unwrap().stories, expected);
        assert_eq!(std::fs::read(&fixture).unwrap(), original_bytes);
        let mut entries = package::read_package(&exported).unwrap();
        entries.remove("package-content.sha256");
        let lua = entries
            .iter_mut()
            .find(|(name, _)| name.starts_with("lua/") && name.ends_with(".lua"))
            .unwrap()
            .1;
        lua.extend_from_slice(b"\nerror('unrelated Lua substitution')\n");
        let tampered = directory.path().join("tampered.lommod");
        package::write_package(entries, &tampered).unwrap();
        assert!(Project::open(&tampered).is_err());
    }

    #[test]
    fn saved_parameters_apply_to_new_steps_and_reset_to_builtin_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("node-defaults.json");
        let mut catalog = Catalog::new();
        catalog.configure_parameter_defaults(path.clone()).unwrap();
        let original = catalog.new_node("overlay", "old".into());
        catalog
            .parameter_defaults
            .update("overlay", "fade", Some(json!(0.75)))
            .unwrap();
        catalog
            .parameter_defaults
            .update("overlay", "scale", Some(json!(125)))
            .unwrap();
        catalog
            .parameter_defaults
            .update("show", "fadeDuration", Some(json!(0.3)))
            .unwrap();
        // Structural links are never applied as global defaults, even from a modified settings file.
        catalog
            .parameter_defaults
            .update("overlay", "next", Some(json!("other_project")))
            .unwrap();
        let mut reopened = Catalog::new();
        reopened.configure_parameter_defaults(path).unwrap();
        let next = reopened.new_node("overlay", "new".into());
        assert_eq!(next["fade"], 0.75);
        assert_eq!(next["scale"], 125);
        assert_eq!(next["id"], "new");
        assert!(next.get("next").is_none());
        assert_eq!(
            reopened.new_node("show", "show1".into())["fadeDuration"],
            0.3
        );
        assert_ne!(original["fade"], next["fade"]);
        reopened
            .parameter_defaults
            .update("overlay", "fade", None)
            .unwrap();
        assert_eq!(
            reopened.new_node("overlay", "reset".into())["fade"],
            original["fade"]
        );
    }
}
