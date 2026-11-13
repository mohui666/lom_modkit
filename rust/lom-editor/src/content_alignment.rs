use super::*;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use lom_core::portrait_alignment as alignment;

impl ContentPanel {
    pub(super) fn alignment_controls(&mut self, ui: &mut egui::Ui, project: &Project, key: &str) {
        self.alignment_dragging = false;
        if !matches!(self.slot, 0 | 1) {
            return;
        }
        let portrait = if self.slot == 1 {
            self.portrait.clone()
        } else {
            "normal".into()
        };
        if self.draft["portraits"].get(&portrait).is_none() {
            return;
        }
        ui.collapsing(crate::i18n::key("common.alignment.section"), |ui| {
            let custom_mood = self.draft["extends"].as_str().unwrap_or("").is_empty();
            if !custom_mood {
                self.alignment_mood = false;
            }
            let mut reference = alignment::reference(&self.draft).to_owned();
            let before = reference.clone();
            crate::forms::SearchCombo::from_id_salt("portrait-reference")
                .selected_text(crate::i18n::message(
                    "common.alignment.reference_value",
                    &[("portrait", reference.clone())],
                ))
                .show_ui(ui, |ui, query| {
                    for name in self.draft["portraits"]
                        .as_object()
                        .into_iter()
                        .flat_map(|v| v.keys())
                    {
                        if !crate::forms::resource_matches(name, "", query) {
                            continue;
                        }
                        if ui
                            .selectable_value(&mut reference, name.clone(), name)
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                });
            if reference != before {
                self.draft["portrait_reference"] = json!(reference);
            }
            ui.small(crate::i18n::message(
                "common.alignment.current_value",
                &[("portrait", portrait.clone())],
            ));
            let root = key.strip_suffix("content.json").unwrap_or("");
            let files = || {
                self.draft["portraits"]
                    .as_object()
                    .into_iter()
                    .flat_map(|v| v.values())
                    .filter_map(Value::as_str)
                    .filter_map(|name| {
                        self.files
                            .get(name)
                            .or_else(|| project.assets.get(&format!("{root}{name}")))
                            .map(|bytes| (name.to_owned(), bytes.clone()))
                    })
                    .collect::<BTreeMap<_, _>>()
            };
            if ui
                .button(crate::i18n::key("common.alignment.auto_all"))
                .clicked()
            {
                let images = files();
                self.draft["portrait_reference"] = json!(reference);
                let result = alignment::auto_align(&mut self.draft, &images)
                    .map(|_| crate::i18n::key("common.alignment.auto_done"));
                self.set_message(result);
            }
            let mut transform = alignment::layout(&self.draft, &portrait);
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (index, label) in [(0, "横向偏移"), (1, "纵向偏移"), (2, "表情缩放（%）")]
                {
                    ui.label(tr(label));
                    let response = ui.add(
                        egui::DragValue::new(&mut transform[index]).speed(if index == 2 {
                            0.2
                        } else {
                            1.0
                        }),
                    );
                    changed |= response.changed();
                    self.alignment_dragging |=
                        response.dragged() && ui.input(|input| input.pointer.primary_down());
                }
                if ui
                    .small_button(crate::i18n::key("common.alignment.restore"))
                    .clicked()
                {
                    transform = [0.0, 0.0, 100.0];
                    if let Some(layouts) = self.draft["portrait_layouts"].as_object_mut() {
                        layouts.remove(&portrait);
                    }
                    changed = false;
                }
            });
            let filename = self.draft["portraits"][&portrait]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let refname = self.draft["portraits"][&reference]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let bytes = |name: &str| {
                self.files
                    .get(name)
                    .or_else(|| project.assets.get(&format!("{root}{name}")))
            };
            let (Some(current_bytes), Some(reference_bytes)) = (bytes(&filename), bytes(&refname))
            else {
                return;
            };
            self.alignment_textures.retain(|cache_key, _| {
                cache_key == &format!("{root}{filename}:{}", current_bytes.len())
                    || cache_key == &format!("{root}{refname}:{}", reference_bytes.len())
            });
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(crate::i18n::key("common.alignment.bottom"))
                    .clicked()
                {
                    match alignment::visible_rect(&current_bytes, transform) {
                        Ok(bounds) => {
                            transform[1] += bounds[3] - 1080.0;
                            changed = true;
                        }
                        Err(error) => {
                            self.message_is_error = true;
                            self.message = crate::error_ui::capture(error);
                        }
                    }
                }
                if ui
                    .add_enabled(
                        custom_mood,
                        egui::Button::new(crate::i18n::key("common.alignment.auto_bubble")),
                    )
                    .on_disabled_hover_text(crate::i18n::key("common.alignment.official_hint"))
                    .clicked()
                {
                    match alignment::visible_rect(
                        &reference_bytes,
                        alignment::layout(&self.draft, &reference),
                    ) {
                        Ok(bounds) => {
                            let image = image::load_from_memory(&reference_bytes)
                                .expect("visible_rect decoded the image");
                            let rect = alignment::image_rect(
                                [image.width(), image.height()],
                                alignment::layout(&self.draft, &reference),
                            );
                            self.draft["mood_x"] =
                                json!((((bounds[0] + bounds[2]) / 2.0 - rect[0])
                                    / (rect[2] - rect[0])
                                    * 100.0)
                                    .round()
                                    .clamp(0.0, 100.0)
                                    as i64);
                            self.draft["mood_y"] =
                                json!(((rect[3] - bounds[1] - (bounds[3] - bounds[1]) * 0.08)
                                    / (rect[3] - rect[1])
                                    * 100.0)
                                    .round()
                                    .clamp(0.0, 100.0)
                                    as i64);
                            self.draft["portrait_reference"] = json!(reference);
                            self.message_is_error = false;
                            self.message = crate::i18n::key("common.alignment.bubble_done");
                        }
                        Err(error) => {
                            self.message_is_error = true;
                            self.message = crate::error_ui::capture(error);
                        }
                    }
                }
                ui.add_enabled(
                    custom_mood,
                    egui::Checkbox::new(
                        &mut self.alignment_mood,
                        crate::i18n::key("common.alignment.bubble_anchor"),
                    ),
                )
                .on_disabled_hover_text(crate::i18n::key("common.alignment.official_hint"));
            });
            if changed {
                self.draft["portrait_reference"] = json!(reference);
                self.draft["portrait_layouts"][&portrait] =
                    json!({"x":transform[0],"y":transform[1],"scale":transform[2]});
            }
            ui.small(crate::i18n::key("common.alignment.hint"));
            let width = ui.available_width().min(520.0);
            let (canvas, response) =
                ui.allocate_exact_size(Vec2::new(width, width * 9.0 / 16.0), egui::Sense::drag());
            let painter = ui.painter().with_clip_rect(canvas);
            self.alignment_dragging |=
                response.dragged() && ui.input(|input| input.pointer.primary_down());
            painter.rect_filled(canvas, 3.0, Color32::from_rgb(221, 225, 230));
            let map = |rect: [f64; 4]| {
                Rect::from_min_max(
                    canvas.min
                        + Vec2::new(
                            rect[0] as f32 / 1920.0 * canvas.width(),
                            rect[1] as f32 / 1080.0 * canvas.height(),
                        ),
                    canvas.min
                        + Vec2::new(
                            rect[2] as f32 / 1920.0 * canvas.width(),
                            rect[3] as f32 / 1080.0 * canvas.height(),
                        ),
                )
            };
            let mut reference_rect = None;
            for (name, bytes, is_reference) in [
                (&refname, &reference_bytes, true),
                (&filename, &current_bytes, false),
            ] {
                let cache_key = format!("{root}{name}:{}", bytes.len());
                if !self.alignment_textures.contains_key(&cache_key) {
                    if let Ok(image) = image::load_from_memory(bytes) {
                        let size = [image.width(), image.height()];
                        let rgba = image.thumbnail(1024, 1024).to_rgba8();
                        let texture = ui.ctx().load_texture(
                            &cache_key,
                            egui::ColorImage::from_rgba_unmultiplied(
                                [rgba.width() as usize, rgba.height() as usize],
                                rgba.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        );
                        self.alignment_textures
                            .insert(cache_key.clone(), (texture, size));
                    }
                }
                if let Some((texture, size)) = self.alignment_textures.get(&cache_key) {
                    let value = if is_reference {
                        alignment::layout(&self.draft, &reference)
                    } else {
                        transform
                    };
                    let rect = map(alignment::image_rect(*size, value));
                    painter.image(
                        texture.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::from_white_alpha(if is_reference { 65 } else { 220 }),
                    );
                    if is_reference {
                        reference_rect = Some(rect);
                    }
                }
            }
            if let Some(rect) = reference_rect.filter(|_| custom_mood) {
                let point = Pos2::new(
                    rect.left()
                        + rect.width() * self.draft["mood_x"].as_f64().unwrap_or(50.0) as f32
                            / 100.0,
                    rect.bottom()
                        - rect.height() * self.draft["mood_y"].as_f64().unwrap_or(90.0) as f32
                            / 100.0,
                );
                painter.circle_stroke(
                    point,
                    6.0,
                    Stroke::new(2.0_f32, Color32::from_rgb(0, 115, 200)),
                );
                painter.line_segment(
                    [point - Vec2::new(10.0, 0.0), point + Vec2::new(10.0, 0.0)],
                    Stroke::new(1.0_f32, Color32::BLUE),
                );
                if response.dragged() && self.alignment_mood {
                    if let Some(point) = response.interact_pointer_pos() {
                        self.draft["mood_x"] =
                            json!(((point.x - rect.left()) / rect.width() * 100.0)
                                .round()
                                .clamp(0.0, 100.0) as i64);
                        self.draft["mood_y"] =
                            json!(((rect.bottom() - point.y) / rect.height() * 100.0)
                                .round()
                                .clamp(0.0, 100.0) as i64);
                        self.draft["portrait_reference"] = json!(reference);
                    }
                }
            }
            if response.dragged() && !self.alignment_mood {
                let delta = ui.input(|input| input.pointer.delta());
                transform[0] += delta.x as f64 / canvas.width() as f64 * 1920.0;
                transform[1] -= delta.y as f64 / canvas.height() as f64 * 1080.0;
                self.draft["portrait_reference"] = json!(reference);
                self.draft["portrait_layouts"][&portrait] =
                    json!({"x":transform[0],"y":transform[1],"scale":transform[2]});
            }
        });
    }
}
