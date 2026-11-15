//! Preview dimensions are editor preferences, never project authoring data.
use super::*;

pub(crate) const DEFAULT: [u32; 2] = [1920, 1080];
const MIN: u32 = 160;
const MAX: u32 = 7680;
const PRESETS: [[u32; 2]; 6] = [
    [1920, 1080],
    [1280, 720],
    [2560, 1440],
    [1920, 1200],
    [1280, 1024],
    [1024, 768],
];

fn valid(value: [u32; 2]) -> bool {
    value
        .into_iter()
        .all(|dimension| (MIN..=MAX).contains(&dimension))
}

pub(crate) fn from_preferences(prefs: &Value) -> [u32; 2] {
    let saved = || {
        let values = prefs["preview_resolution"].as_array()?;
        if values.len() != 2 {
            return None;
        }
        let value = [
            u32::try_from(values[0].as_u64()?).ok()?,
            u32::try_from(values[1].as_u64()?).ok()?,
        ];
        valid(value).then_some(value)
    };
    saved().unwrap_or(DEFAULT)
}

pub(crate) fn save(path: &Path, resolution: [u32; 2]) -> anyhow::Result<()> {
    anyhow::ensure!(
        valid(resolution),
        crate::i18n::key("editor.preview.resolution_invalid")
    );
    let mut prefs = if path.exists() {
        lom_core::load_json(path)?
    } else {
        json!({})
    };
    anyhow::ensure!(
        prefs.is_object(),
        crate::i18n::key("editor.preview.resolution_preferences_invalid")
    );
    prefs["preview_resolution"] = json!(resolution);
    lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)
}

#[derive(Clone)]
struct Draft {
    source: [u32; 2],
    width: u32,
    height: u32,
}
impl Draft {
    fn new(source: [u32; 2]) -> Self {
        Self {
            source,
            width: source[0],
            height: source[1],
        }
    }
}

pub(crate) fn controls(ui: &mut egui::Ui, current: [u32; 2]) -> Option<[u32; 2]> {
    let id = ui.make_persistent_id("preview-resolution-draft");
    let mut draft = ui
        .data_mut(|data| data.get_temp::<Draft>(id))
        .unwrap_or_else(|| Draft::new(current));
    if draft.source != current {
        draft = Draft::new(current);
    }
    let mut selected = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(crate::i18n::key("editor.preview.resolution"))
            .on_hover_text(crate::i18n::key("editor.preview.resolution_hint"));
        egui::ComboBox::from_id_salt("preview-resolution-preset")
            .selected_text(format!("{} × {}", current[0], current[1]))
            .show_ui(ui, |ui| {
                for value in PRESETS {
                    if ui
                        .selectable_label(current == value, format!("{} × {}", value[0], value[1]))
                        .clicked()
                    {
                        selected = Some(value);
                    }
                }
            });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(crate::i18n::key("editor.preview.resolution_width"));
        let width = ui.add(
            egui::DragValue::new(&mut draft.width)
                .range(MIN..=MAX)
                .update_while_editing(false),
        );
        ui.label("×");
        ui.label(crate::i18n::key("editor.preview.resolution_height"));
        let height = ui.add(
            egui::DragValue::new(&mut draft.height)
                .range(MIN..=MAX)
                .update_while_editing(false),
        );
        let value = [draft.width, draft.height];
        let apply = ui.add_enabled(
            valid(value) && value != current,
            egui::Button::new(crate::i18n::key("editor.preview.resolution_apply")),
        );
        if apply.clicked()
            || ((width.lost_focus() || height.lost_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                && valid(value))
        {
            selected = Some(value);
        }
    });
    if let Some(value) = selected {
        draft = Draft::new(value);
    }
    ui.data_mut(|data| data.insert_temp(id, draft));
    selected.filter(|value| *value != current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_resolution_preferences_restore_custom_dimensions_without_changing_other_keys() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("preferences.json");
        let existing = json!({"ui_locale":"ja","preview_library_dir":"private-library","workspace_widths":[290,510],"custom":{"keep":true}});
        std::fs::write(&path, serde_json::to_vec(&existing).unwrap()).unwrap();
        assert_eq!(from_preferences(&existing), DEFAULT);
        save(&path, [1536, 960]).unwrap();
        let mut restored = lom_core::load_json(&path).unwrap();
        assert_eq!(from_preferences(&restored), [1536, 960]);
        restored
            .as_object_mut()
            .unwrap()
            .remove("preview_resolution");
        assert_eq!(restored, existing);
        let before = std::fs::read(&path).unwrap();
        assert!(save(&path, [0, 1080]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn preview_resolution_rejects_invalid_saved_values_and_keeps_supported_bounds() {
        for raw in [
            json!(null),
            json!([]),
            json!([1920]),
            json!([1920, 1080, 1]),
            json!([159, 1080]),
            json!([1920, 7681]),
            json!(["1920", 1080]),
            json!([-1, 1080]),
        ] {
            assert_eq!(
                from_preferences(&json!({"preview_resolution":raw})),
                DEFAULT
            );
        }
        for value in PRESETS.into_iter().chain([[160, 160], [7680, 7680]]) {
            assert_eq!(
                from_preferences(&json!({"preview_resolution":value})),
                value
            );
        }
    }

    #[test]
    fn preview_resolution_controls_apply_custom_dimensions_after_a_click() {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(360.0, 180.0));
        let first = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let id = ui.make_persistent_id("preview-resolution-draft");
                    ui.data_mut(|data| {
                        data.insert_temp(
                            id,
                            Draft {
                                source: DEFAULT,
                                width: 1800,
                                height: 1200,
                            },
                        )
                    });
                    assert_eq!(controls(ui, DEFAULT), None);
                });
            },
        );
        let label = crate::i18n::key("editor.preview.resolution_apply");
        let button = first
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("Apply button must be visible in a narrow preview panel");
        let mut chosen = None;
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        egui::Event::PointerMoved(button),
                        egui::Event::PointerButton {
                            pos: button,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(value) = controls(ui, DEFAULT) {
                            chosen = Some(value);
                        }
                    });
                },
            );
        }
        assert_eq!(chosen, Some([1800, 1200]));
    }
}
