use super::*;
use anyhow::{Context, Result};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};

type Values = BTreeMap<String, BTreeMap<String, Value>>;

pub(super) fn request_editor(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("request-default-parameters"), true));
}

#[derive(Clone)]
struct DefaultsDraft {
    node: Value,
    error: Option<String>,
}

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
                | "mode"
                | "facing"
                | "branch_source"
                | "position"
                | "camera"
        )
}

fn unset_choice(field: &Value, value: &Value) -> bool {
    field[3] == true
        && field[2]
            .as_str()
            .is_some_and(|kind| kind.starts_with("enum:"))
        && (value.is_null() || value.as_str() == Some(""))
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

    #[cfg(test)]
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
        self.store(next)
    }

    fn store(&self, next: Values) -> Result<()> {
        if let Some(path) = &self.path {
            let bytes = lom_core::stable_json(&serde_json::to_value(&next)?)?;
            lom_core::project::atomic_write(path, &bytes)?;
        }
        *self.values.borrow_mut() = next;
        Ok(())
    }

    fn save_draft(
        &self,
        node_type: &str,
        fields: &Value,
        draft: &Value,
        builtin: &Value,
    ) -> Result<()> {
        let mut next = self.values.borrow().clone();
        let mut overrides = next.remove(node_type).unwrap_or_default();
        for field in fields.as_array().into_iter().flatten() {
            let (Some(key), Some(kind)) = (field[0].as_str(), field[2].as_str()) else {
                continue;
            };
            if !supported(kind) || matches!(key, "id" | "type") {
                continue;
            }
            let value = draft.get(key).filter(|value| !unset_choice(field, value));
            if value == builtin.get(key) {
                overrides.remove(key);
            } else {
                overrides.insert(key.into(), value.cloned().unwrap_or(Value::Null));
            }
        }
        if !overrides.is_empty() {
            next.insert(node_type.into(), overrides);
        }
        self.store(next)
    }

    pub(super) fn editor(&self, catalog: &Catalog, ui: &mut Ui, node: &Value) {
        let node_type = node["type"].as_str().unwrap_or("");
        let fields = &catalog.schema["NODE_SCHEMAS"][node_type]["fields"];
        let eligible: Vec<_> = fields
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| {
                field[2].as_str().is_some_and(supported)
                    && !matches!(field[0].as_str(), Some("id" | "type"))
            })
            .collect();
        if eligible.is_empty() {
            return;
        }
        let draft_id = ui.id().with(("new-step-defaults", node_type));
        let mut builtin = catalog.schema["_NODE_DEFAULTS"][node_type].clone();
        if !builtin.is_object() {
            builtin = json!({});
        }
        builtin["type"] = json!(node_type);
        let requested = ui.ctx().data_mut(|data| {
            let id = egui::Id::new("request-default-parameters");
            let requested = data.get_temp::<bool>(id).unwrap_or(false);
            data.remove::<bool>(id);
            requested
        });
        if ui
            .button(crate::i18n::key("editor.defaults.manage"))
            .clicked()
            || requested
        {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    draft_id,
                    DefaultsDraft {
                        node: catalog.new_node(node_type, String::new()),
                        error: None,
                    },
                )
            });
        }
        let Some(mut draft) = ui
            .ctx()
            .data(|data| data.get_temp::<DefaultsDraft>(draft_id))
        else {
            return;
        };
        let mut close = false;
        egui::Modal::new(draft_id)
            .frame(crate::shell::dialog_frame())
            .show(ui.ctx(), |ui| {
                ui.set_width(520.0_f32.min((ui.ctx().content_rect().width() - 48.0).max(280.0)));
                close |= crate::shell::dialog_header(
                    ui,
                    &crate::i18n::message(
                        "editor.defaults.title",
                        &[("node", catalog.label(node_type))],
                    ),
                );
                ui.label(crate::i18n::key("editor.defaults.scope_hint"));
                if ui
                    .button(crate::i18n::key("editor.defaults.copy_current"))
                    .clicked()
                {
                    for field in &eligible {
                        let key = field[0].as_str().unwrap();
                        if let Some(value) = node.get(key) {
                            draft.node[key] = value.clone();
                        } else {
                            draft.node.as_object_mut().unwrap().remove(key);
                        }
                    }
                    draft.error = None;
                }
                ui.separator();
                let timing = |field: &&Value| {
                    matches!(
                        field[0].as_str(),
                        Some(
                            "duration"
                                | "seconds"
                                | "fade"
                                | "fade_out"
                                | "fade_in"
                                | "fadeDuration"
                                | "moveDuration"
                                | "wait_after"
                                | "stop_after"
                        )
                    )
                };
                egui::ScrollArea::vertical()
                    .max_height((ui.ctx().content_rect().height() * 0.55).max(120.0))
                    .show(ui, |ui| {
                        let draw = |ui: &mut Ui, draft: &mut Value, field: &Value| {
                            catalog.field(
                                ui,
                                draft,
                                field[0].as_str().unwrap(),
                                field[1].as_str().unwrap_or(""),
                                field[2].as_str().unwrap(),
                                field[3].as_bool().unwrap_or(false),
                                &[],
                                &[],
                            );
                        };
                        if eligible.iter().any(timing) {
                            ui.strong(crate::i18n::key("editor.defaults.times"));
                            egui::Frame::group(ui.style())
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    for field in eligible.iter().filter(|field| timing(field)) {
                                        let key = field[0].as_str().unwrap();
                                        let mut seconds = draft.node[key].as_f64().unwrap_or(0.0);
                                        ui.push_id(key, |ui| {
                                            ui.horizontal(|ui| {
                                                let label = ui.add_sized(
                                                    [
                                                        (ui.available_width()
                                                            - 120.0
                                                            - 88.0
                                                            - 2.0 * ui.spacing().item_spacing.x)
                                                            .max(100.0),
                                                        32.0,
                                                    ],
                                                    egui::Label::new(crate::i18n::tr(
                                                        field[1].as_str().unwrap_or(key),
                                                    )),
                                                );
                                                if ui
                                                    .add_sized(
                                                        [120.0, 32.0],
                                                        egui::DragValue::new(&mut seconds)
                                                            .speed(0.05)
                                                            .range(0.0..=f64::MAX),
                                                    )
                                                    .labelled_by(label.id)
                                                    .changed()
                                                {
                                                    draft.node[key] = json!(seconds);
                                                }
                                                if ui
                                                    .add_sized(
                                                        [88.0, 32.0],
                                                        egui::Button::new(crate::i18n::key(
                                                            "editor.defaults.field_reset",
                                                        ))
                                                        .small()
                                                        .truncate(),
                                                    )
                                                    .clicked()
                                                {
                                                    if let Some(value) = builtin.get(key) {
                                                        draft.node[key] = value.clone();
                                                    } else {
                                                        draft
                                                            .node
                                                            .as_object_mut()
                                                            .unwrap()
                                                            .remove(key);
                                                    }
                                                }
                                            })
                                        });
                                    }
                                });
                        }
                        if eligible.iter().any(|field| !timing(field)) {
                            egui::CollapsingHeader::new(crate::i18n::key("editor.defaults.other"))
                                .id_salt("other-new-step-defaults")
                                .default_open(!eligible.iter().any(timing))
                                .show(ui, |ui| {
                                    for field in eligible.iter().filter(|field| !timing(field)) {
                                        draw(ui, &mut draft.node, field);
                                    }
                                });
                        }
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(crate::i18n::key("editor.defaults.save"))
                        .clicked()
                    {
                        match self.save_draft(node_type, fields, &draft.node, &builtin) {
                            Ok(()) => {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new("saved-new-step-defaults"),
                                        crate::i18n::message(
                                            "editor.defaults.saved",
                                            &[("node", catalog.label(node_type))],
                                        ),
                                    );
                                });
                                close = true;
                            }
                            Err(error) => draft.error = Some(crate::error_ui::capture(error)),
                        }
                    }
                    if ui
                        .button(crate::i18n::key("editor.defaults.fill_builtin"))
                        .clicked()
                    {
                        draft.node = builtin.clone();
                        draft.error = None;
                    }
                    if ui.button(crate::i18n::tr("取消")).clicked() {
                        close = true;
                    }
                });
                if let Some(error) = &draft.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        if close {
            ui.ctx()
                .data_mut(|data| data.remove::<DefaultsDraft>(draft_id));
        } else {
            ui.ctx().data_mut(|data| data.insert_temp(draft_id, draft));
        }
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
                if value.is_null() || unset_choice(field, value) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_optional_choices_from_older_defaults_do_not_force_a_new_selection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("node-defaults.json");
        std::fs::write(
            &path,
            br#"{"say":{"mood_icon":""},"show":{"appearance":""}}"#,
        )
        .unwrap();
        let mut catalog = Catalog::new();
        catalog.configure_parameter_defaults(path.clone()).unwrap();
        let mut line = catalog.new_node("say", "line".into());
        line["text"] = json!("不设置本句气泡");
        assert!(line.get("mood_icon").is_none(), "{line}");
        assert!(catalog
            .new_node("show", "show1".into())
            .get("appearance")
            .is_none());
        lom_core::validate::validate_story(
            &json!({"id":"main","start":"line","nodes":[line.clone(),{"id":"done","type":"end"}]}),
        )
        .unwrap();
        line["mood_icon"] = json!("");
        let original = line.clone();
        let context = egui::Context::default();
        context.enable_accesskit();
        let output = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                catalog.node_form(ui, &mut line, &[], &[]);
            });
        });
        assert_eq!(
            line, original,
            "viewing a released project must preserve its values"
        );
        assert!(output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.value() == Some(crate::i18n::tr("沿用现有气泡").as_str())));
        let fields = &catalog.schema["NODE_SCHEMAS"]["say"]["fields"];
        let builtin = &catalog.schema["_NODE_DEFAULTS"]["say"];
        catalog
            .parameter_defaults
            .save_draft("say", fields, &line, builtin)
            .unwrap();
        assert!(lom_core::load_json(&path).unwrap().get("say").is_none());
    }

    #[test]
    fn defaults_editor_edits_new_step_timing_and_supports_copy_cancel_and_reset() {
        use egui::accesskit::{Action, ActionData, ActionRequest, Role};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("node-defaults.json");
        let mut catalog = Catalog::new();
        catalog.configure_parameter_defaults(path.clone()).unwrap();
        let builtin = catalog.new_node("show", "builtin".into());
        let hide = catalog.new_node("hide", "hide".into());
        let mut current = json!({"id":"existing","type":"show","character":"brother1","position":"L1",
            "fadeDuration":0.8,"moveDuration":1.2,"wait_after":0.4,"text":"不要复制正文"});
        let context = egui::Context::default();
        context.enable_accesskit();
        let frame = |current: &Value, events| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        catalog.parameter_defaults.editor(&catalog, ui, current)
                    });
                },
            )
        };
        let click = |output: &egui::FullOutput, key: &str| {
            let label = crate::i18n::key(key);
            let target = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(id, node)| {
                    (node.role() == Role::Button && node.label() == Some(label.as_str()))
                        .then_some(*id)
                })
                .unwrap_or_else(|| panic!("missing {label}"));
            vec![egui::Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target,
                data: None,
            })]
        };
        let output = frame(&current, vec![]);
        frame(&current, click(&output, "editor.defaults.manage"));
        let output = frame(&current, vec![]);
        let first_input = |output: &egui::FullOutput| {
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == Role::SpinButton)
                .min_by(|(_, left), (_, right)| {
                    left.bounds()
                        .unwrap()
                        .y0
                        .total_cmp(&right.bounds().unwrap().y0)
                })
                .map(|(id, node)| (*id, node.bounds().unwrap()))
                .unwrap()
        };
        let (_, bounds) = first_input(&output);
        let position = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        );
        for pressed in [true, false] {
            frame(
                &current,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let output = frame(
            &current,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(
            !click(&output, "editor.defaults.save").is_empty(),
            "clicking a time input must keep the dialog open"
        );
        let (target, _) = first_input(&output);
        let output = frame(
            &current,
            vec![egui::Event::AccessKitActionRequest(ActionRequest {
                action: Action::SetValue,
                target,
                data: Some(ActionData::NumericValue(0.65)),
            })],
        );
        assert_eq!(
            catalog.new_node("show", "unsaved".into())["fadeDuration"],
            builtin["fadeDuration"]
        );
        frame(&current, click(&output, "editor.defaults.save"));
        let mut reopened = Catalog::new();
        reopened.configure_parameter_defaults(path.clone()).unwrap();
        assert_eq!(
            reopened.new_node("show", "next".into())["fadeDuration"],
            0.65
        );
        assert_eq!(current["fadeDuration"], 0.8);
        assert_eq!(catalog.new_node("hide", "hide".into()), hide);

        for save in [false, true] {
            let output = frame(&current, vec![]);
            let output = frame(&current, click(&output, "editor.defaults.manage"));
            let output = frame(&current, click(&output, "editor.defaults.copy_current"));
            let events = if save {
                click(&output, "editor.defaults.save")
            } else {
                click(&output, "取消")
            };
            frame(&current, events);
            reopened.configure_parameter_defaults(path.clone()).unwrap();
            let next = reopened.new_node("show", "next".into());
            assert_eq!(
                next["fadeDuration"],
                if save { json!(0.8) } else { json!(0.65) }
            );
            if save {
                assert_eq!(next["moveDuration"], 1.2);
                assert_eq!(next["wait_after"], 0.4);
                assert_eq!(next["character"], builtin["character"]);
                assert!(next.get("text").is_none());
                assert_eq!(next["id"], "next");
            }
        }
        let original = current.clone();
        let output = frame(&current, vec![]);
        let output = frame(&current, click(&output, "editor.defaults.manage"));
        let output = frame(&current, click(&output, "editor.defaults.fill_builtin"));
        frame(&current, click(&output, "editor.defaults.save"));
        reopened.configure_parameter_defaults(path).unwrap();
        let restored = reopened.new_node("show", "builtin".into());
        assert_eq!(restored, builtin);
        assert_eq!(current, original);
        // The manager is also available for a show action without a flow-node id.
        current.as_object_mut().unwrap().remove("id");
        let output = frame(&current, vec![]);
        assert!(!click(&output, "editor.defaults.manage").is_empty());
    }

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
        let original_dialogue = catalog.new_node("say", "old_say".into());
        catalog
            .parameter_defaults
            .update("say", "text", Some(json!("误设为默认的对白正文")))
            .unwrap();
        let mut reopened = Catalog::new();
        reopened.configure_parameter_defaults(path).unwrap();
        let next = reopened.new_node("overlay", "new".into());
        assert_eq!(next["fade"], 0.75);
        assert_eq!(next["scale"], 125);
        assert_eq!(next["id"], "new");
        assert!(next.get("next").is_none());
        assert_eq!(
            reopened.new_node("say", "new_say".into())["text"],
            original_dialogue["text"]
        );
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
