use super::*;

#[derive(Clone)]
pub(super) struct LayoutObject {
    pub target: LayoutTarget,
    pub bounds: Rect,
    pub pivot: Pos2,
    pub angle: f32,
    pub value: LayoutChange,
    pub units: Vec2,
}

#[derive(Clone)]
pub(super) struct LayoutDrag {
    object: LayoutObject,
    origin: Pos2,
    resize_anchor: Option<(Pos2, Pos2)>,
    next: LayoutChange,
}

fn rotate(point: Pos2, pivot: Pos2, angle: f32) -> Pos2 {
    pivot + egui::emath::Rot2::from_angle(angle.to_radians()) * (point - pivot)
}

impl LayoutObject {
    fn corners(&self) -> [Pos2; 4] {
        [
            self.bounds.left_top(),
            self.bounds.right_top(),
            self.bounds.right_bottom(),
            self.bounds.left_bottom(),
        ]
        .map(|point| rotate(point, self.pivot, self.angle))
    }

    fn contains(&self, point: Pos2) -> bool {
        self.bounds.contains(rotate(point, self.pivot, -self.angle))
    }
}

impl LayoutDrag {
    fn at(&self, point: Pos2) -> LayoutChange {
        let mut change = self.object.value.clone();
        let delta = if let Some((anchor, corner)) = self.resize_anchor {
            let axis = corner - anchor;
            let factor =
                (1.0 + (point - self.origin).dot(axis) / axis.length_sq().max(1.0)).max(0.001);
            change.scale *= factor as f64;
            if matches!(change.target, LayoutTarget::Character(_)) {
                Vec2::ZERO
            } else {
                (self.object.pivot - anchor) * (factor - 1.0)
            }
        } else {
            point - self.origin
        };
        change.x += (delta.x / self.object.units.x) as f64;
        change.y -= (delta.y / self.object.units.y) as f64;
        if !matches!(change.target, LayoutTarget::Character(_)) {
            change.x = change.x.clamp(-100.0, 100.0);
            change.y = change.y.clamp(-100.0, 100.0);
        }
        change
    }
}

impl Preview {
    pub(super) fn layout_controls(&mut self, ui: &mut egui::Ui, project: &Project, state: &Value) {
        let mut choices = Vec::new();
        for cid in state["actors"]
            .as_object()
            .into_iter()
            .flat_map(|actors| actors.keys())
        {
            choices.push((
                LayoutTarget::Character(cid.clone()),
                character_display_name(project, cid),
            ));
        }
        for (slot, image) in state["overlays"].as_object().into_iter().flatten() {
            choices.push((
                LayoutTarget::Overlay(slot.clone()),
                format!(
                    "插图：{}",
                    character_display_name(project, text(image, "image"))
                ),
            ));
        }
        if state["custom_cg"].is_object() {
            choices.push((
                LayoutTarget::Cg,
                format!(
                    "全屏图片：{}",
                    character_display_name(project, text(&state["custom_cg"], "image"))
                ),
            ));
        }
        if !choices
            .iter()
            .any(|(target, _)| Some(target) == self.layout_selected.as_ref())
        {
            self.layout_selected = None;
            self.layout_drag = None;
        }
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.layout_mode, crate::i18n::tr("调整画面"));
            if self.layout_mode {
                egui::ComboBox::from_id_salt("layout-object")
                    .selected_text(
                        choices
                            .iter()
                            .find(|(target, _)| Some(target) == self.layout_selected.as_ref())
                            .map(|(_, label)| label.as_str())
                            .unwrap_or("选择人物或插图"),
                    )
                    .show_ui(ui, |ui| {
                        for (target, label) in &choices {
                            if ui
                                .selectable_value(
                                    &mut self.layout_selected,
                                    Some(target.clone()),
                                    label,
                                )
                                .changed()
                            {
                                self.layout_drag = None;
                            }
                        }
                    });
                if ui
                    .add_enabled(
                        self.layout_selected.is_some(),
                        egui::Button::new(crate::i18n::tr("复位")),
                    )
                    .clicked()
                {
                    self.layout_edit = self.layout_selected.clone().map(|target| LayoutChange {
                        target,
                        x: 0.0,
                        y: 0.0,
                        scale: 100.0,
                    });
                    self.layout_drag = None;
                }
            }
        });
        if self.layout_mode {
            ui.small(crate::i18n::tr(
                "拖动人物或插图移动，拉动四角等比缩放；Esc 取消本次拖动。",
            ));
        } else {
            self.layout_drag = None;
        }
    }

    pub(super) fn apply_layout_draft(&self, state: &mut Value) {
        if let Some(drag) = &self.layout_drag {
            let change = &drag.next;
            match &change.target {
                LayoutTarget::Character(cid) if state["actors"][cid].is_object() => {
                    state["actors"][cid]["offset_x"] = json!(change.x);
                    state["actors"][cid]["offset_y"] = json!(change.y);
                    state["actors"][cid]["layout_scale"] = json!(change.scale);
                }
                LayoutTarget::Overlay(slot) if state["overlays"][slot].is_object() => {
                    state["overlays"][slot]["x"] = json!(change.x);
                    state["overlays"][slot]["y"] = json!(change.y);
                    state["overlays"][slot]["scale"] = json!(change.scale);
                }
                LayoutTarget::Cg if state["custom_cg"].is_object() => {
                    state["custom_cg"]["x"] = json!(change.x);
                    state["custom_cg"]["y"] = json!(change.y);
                    state["custom_cg"]["scale"] = json!(change.scale);
                }
                _ => (),
            }
        }
    }

    pub fn cancel_layout_drag(&mut self) {
        self.layout_drag = None;
        self.layout_edit = None;
    }

    pub(super) fn layout_handles(
        &mut self,
        ui: &mut egui::Ui,
        stage: Rect,
        objects: &[LayoutObject],
    ) {
        if !self.layout_mode || !ui.is_enabled() {
            self.layout_drag = None;
            return;
        }
        let painter = ui.painter().with_clip_rect(stage);
        let selected = objects
            .iter()
            .find(|object| Some(&object.target) == self.layout_selected.as_ref());
        let mut handles = Vec::new();
        if let Some(object) = selected {
            let corners = object.corners();
            painter.add(egui::Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.5_f32, Color32::from_rgb(25, 130, 220)),
            ));
            for (index, corner) in corners.iter().enumerate() {
                if !corner.is_finite() {
                    continue;
                }
                let center = stage.shrink(7.0).clamp(*corner);
                let handle = Rect::from_center_size(center, Vec2::splat(10.0));
                painter.rect_filled(handle, 1.0, Color32::WHITE);
                painter.rect_stroke(
                    handle,
                    1.0,
                    Stroke::new(1.5_f32, Color32::from_rgb(25, 130, 220)),
                    egui::StrokeKind::Inside,
                );
                let response = ui
                    .interact(
                        handle,
                        ui.id().with(("layout-resize", index)),
                        egui::Sense::drag(),
                    )
                    .on_hover_text(crate::i18n::tr("拖动边角等比缩放"))
                    .on_hover_cursor(if index % 2 == 0 {
                        egui::CursorIcon::ResizeNwSe
                    } else {
                        egui::CursorIcon::ResizeNeSw
                    });
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        format!("拖动{}角缩放", ["左上", "右上", "右下", "左下"][index]),
                    )
                });
                let anchor = if matches!(object.target, LayoutTarget::Character(_)) {
                    object.pivot
                } else {
                    corners[(index + 2) % 4]
                };
                handles.push((handle, anchor, *corner));
            }
        }
        for (index, object) in objects.iter().enumerate() {
            let response = ui
                .interact(
                    object.bounds.intersect(stage),
                    ui.id().with(("layout-move", index)),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::Grab);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    format!("拖动画面对象 {}", index + 1),
                )
            });
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.layout_drag = None;
            ui.ctx().request_repaint();
            return;
        }
        let press = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        if let Some(origin) = press.filter(|point| {
            stage.contains(*point) && ui.ctx().layer_id_at(*point) == Some(ui.layer_id())
        }) {
            let resize = handles
                .iter()
                .find(|(handle, _, _)| handle.contains(origin));
            let object = if resize.is_some() {
                selected
            } else {
                objects.iter().rev().find(|object| object.contains(origin))
            };
            if let Some(object) = object {
                self.layout_selected = Some(object.target.clone());
                self.layout_drag = Some(LayoutDrag {
                    object: object.clone(),
                    origin,
                    resize_anchor: resize.map(|(_, anchor, corner)| (*anchor, *corner)),
                    next: object.value.clone(),
                });
            } else {
                self.layout_selected = None;
            }
        }
        if let Some(drag) = &mut self.layout_drag {
            let point = ui
                .input(|input| input.pointer.latest_pos())
                .unwrap_or(drag.origin);
            drag.next = drag.at(point);
            let released = ui.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::PointerButton {
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            ..
                        }
                    )
                })
            });
            if released {
                if (drag.next.x - drag.object.value.x).abs() > 0.00001
                    || (drag.next.y - drag.object.value.y).abs() > 0.00001
                    || (drag.next.scale - drag.object.value.scale).abs() > 0.00001
                {
                    self.layout_edit = Some(drag.next.clone());
                }
                self.layout_drag = None;
            } else if !ui.input(|input| input.pointer.primary_down()) {
                self.layout_drag = None;
            }
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn opaque_rect(&self, texture: &TextureHandle, dest: Rect, flipped: bool) -> Rect {
        let mut bounds = self
            .cache
            .values()
            .find(|cached| cached.texture.id() == texture.id())
            .map(|cached| cached.opaque)
            .unwrap_or_else(uv);
        if flipped {
            bounds = Rect::from_min_max(
                Pos2::new(1.0 - bounds.max.x, bounds.min.y),
                Pos2::new(1.0 - bounds.min.x, bounds.max.y),
            );
        }
        Rect::from_min_max(
            dest.min + bounds.min.to_vec2() * dest.size(),
            dest.min + bounds.max.to_vec2() * dest.size(),
        )
    }
}

pub(super) fn opaque_bounds(image: &image::RgbaImage) -> Rect {
    let (mut left, mut top, mut right, mut bottom) = (image.width(), image.height(), 0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 8 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right == 0 || bottom == 0 {
        return uv();
    }
    Rect::from_min_max(
        Pos2::new(
            left as f32 / image.width() as f32,
            top as f32 / image.height() as f32,
        ),
        Pos2::new(
            right as f32 / image.width() as f32,
            bottom as f32 / image.height() as f32,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resized_actor_keeps_its_feet_and_image_keeps_its_opposite_corner() {
        let object = LayoutObject {
            target: LayoutTarget::Character("actor".into()),
            bounds: Rect::from_min_max(Pos2::new(50.0, 50.0), Pos2::new(150.0, 150.0)),
            pivot: Pos2::new(100.0, 150.0),
            angle: 30.0,
            value: LayoutChange {
                target: LayoutTarget::Character("actor".into()),
                x: 12.5,
                y: -20.0,
                scale: 100.0,
            },
            units: Vec2::splat(10.0),
        };
        let corner = object.corners()[0];
        let drag = LayoutDrag {
            object: object.clone(),
            origin: corner,
            resize_anchor: Some((object.pivot, corner)),
            next: object.value.clone(),
        };
        let changed = drag.at(corner + (corner - object.pivot) * 0.5);
        assert!((changed.scale - 150.0).abs() < 0.001);
        assert_eq!((changed.x, changed.y), (12.5, -20.0));

        let mut image = object;
        image.target = LayoutTarget::Cg;
        image.value = LayoutChange {
            target: LayoutTarget::Cg,
            x: 0.0,
            y: 0.0,
            scale: 100.0,
        };
        image.pivot = Pos2::new(100.0, 100.0);
        image.angle = 0.0;
        let origin = image.bounds.left_top();
        let drag = LayoutDrag {
            object: image.clone(),
            origin,
            resize_anchor: Some((image.bounds.right_bottom(), origin)),
            next: image.value.clone(),
        };
        let changed = drag.at(origin - Vec2::splat(25.0));
        assert_eq!(changed.scale, 125.0);
        assert_eq!((changed.x, changed.y), (-1.25, 1.25));
        let new_center = image.pivot + Vec2::new(changed.x as f32 * 10.0, -changed.y as f32 * 10.0);
        assert_eq!(
            new_center + image.bounds.size() * 1.25 / 2.0,
            image.bounds.right_bottom()
        );
    }
}
