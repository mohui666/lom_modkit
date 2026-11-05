use super::*;
#[path = "preview_alignment.rs"]
mod alignment;
pub(super) use alignment::{Clipboard as LayoutClipboard, Command as LayoutCommand};

fn adjustment_name(project: &Project, state: &Value, target: &LayoutTarget) -> Option<String> {
    if matches!(target, LayoutTarget::Effect { .. }) {
        return effects::item(state, target).map(effects::label);
    }
    let (kind, id) = match target {
        LayoutTarget::Character(cid) if state["actors"][cid].is_object() => ("人物", cid.as_str()),
        LayoutTarget::Mood(cid) if state["actors"][cid].is_object() => ("表情气泡", cid.as_str()),
        LayoutTarget::Overlay(slot) if state["overlays"][slot].is_object() => {
            ("插图", text(&state["overlays"][slot], "image"))
        }
        LayoutTarget::Cg if state["custom_cg"].is_object() => {
            ("全屏图片", text(&state["custom_cg"], "image"))
        }
        _ => return None,
    };
    (!id.is_empty()).then(|| {
        format!(
            "{} · {}",
            crate::i18n::tr(kind),
            character_display_name(project, id)
        )
    })
}

fn badge_rect(stage: Rect, anchor: Pos2, size: Vec2) -> Rect {
    let inset = 4.0_f32.min(stage.width() / 4.0).min(stage.height() / 4.0);
    let safe = stage.shrink(inset);
    let size = Vec2::new(size.x.min(safe.width()), size.y.min(safe.height()));
    let above = anchor.y - size.y - 10.0;
    let y = if above >= safe.top() {
        above
    } else {
        anchor.y + 10.0
    };
    Rect::from_min_size(
        Pos2::new(
            (anchor.x + 8.0).clamp(safe.left(), (safe.right() - size.x).max(safe.left())),
            y.clamp(safe.top(), (safe.bottom() - size.y).max(safe.top())),
        ),
        size,
    )
}

pub(super) fn adjustment_badge(painter: &egui::Painter, stage: Rect, anchor: Pos2, label: &str) {
    if !stage.is_positive() || !anchor.is_finite() || label.is_empty() {
        return;
    }
    let padding = Vec2::new(7.0, 4.0);
    let mut job = egui::text::LayoutJob::simple(
        label.to_owned(),
        FontId::proportional(12.0),
        Color32::WHITE,
        (stage.width() - 22.0).max(1.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(stage));
    let galley = painter.layout_job(job);
    let rect = badge_rect(stage, anchor, galley.size() + padding * 2.0);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(25, 105, 180));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(180, 220, 255)),
        egui::StrokeKind::Inside,
    );
    // Painting only: the nameplate never registers a hit target over the handles.
    painter.galley(rect.min + padding, galley, Color32::WHITE);
}

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
            let delta = point - self.origin;
            if matches!(change.target, LayoutTarget::Mood(_)) {
                egui::emath::Rot2::from_angle(-self.object.angle.to_radians()) * delta
            } else {
                delta
            }
        };
        change.x += (delta.x / self.object.units.x) as f64;
        change.y -= (delta.y / self.object.units.y) as f64;
        if matches!(change.target, LayoutTarget::Overlay(_) | LayoutTarget::Cg) {
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
                    "{}：{}",
                    crate::i18n::tr("插图"),
                    character_display_name(project, text(image, "image"))
                ),
            ));
        }
        for (cid, setting) in state["mood_overrides"].as_object().into_iter().flatten() {
            if setting["enabled"] == true && state["actors"][cid].is_object() {
                choices.push((
                    LayoutTarget::Mood(cid.clone()),
                    format!(
                        "{} · {}",
                        crate::i18n::tr("表情气泡"),
                        character_display_name(project, cid)
                    ),
                ));
            }
        }
        if state["custom_cg"].is_object() {
            choices.push((
                LayoutTarget::Cg,
                format!(
                    "{}：{}",
                    crate::i18n::tr("全屏图片"),
                    character_display_name(project, text(&state["custom_cg"], "image"))
                ),
            ));
        }
        for (index, effect) in state["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(target) = effects::target(effect) {
                choices.push((
                    target,
                    format!(
                        "{} · {} {}",
                        effects::label(effect),
                        crate::i18n::tr("实例"),
                        index + 1
                    ),
                ));
            }
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
                    .width((ui.available_width() - 64.0).clamp(100.0, 260.0))
                    .truncate()
                    .selected_text(
                        choices
                            .iter()
                            .find(|(target, _)| Some(target) == self.layout_selected.as_ref())
                            .map(|(_, label)| label.clone())
                            .unwrap_or_else(|| crate::i18n::tr("选择人物、插图或特效")),
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
                let reset = ui.add_enabled(
                    self.layout_selected.is_some(),
                    egui::Button::new(crate::i18n::tr(
                        if matches!(self.layout_selected, Some(LayoutTarget::Effect { .. })) {
                            "位置归中"
                        } else if matches!(self.layout_selected, Some(LayoutTarget::Mood(_))) {
                            "气泡归位"
                        } else {
                            "复位"
                        },
                    )),
                );
                let reset = if matches!(self.layout_selected, Some(LayoutTarget::Effect { .. })) {
                    reset.on_hover_text(crate::i18n::tr(
                        "特效仅恢复中心位置，保留横纵缩放；缩放参数可在步骤属性中分别设置。",
                    ))
                } else {
                    reset
                };
                if reset.clicked() {
                    self.layout_edit = self.layout_selected.clone().map(|target| {
                        let center = if effects::item(state, &target)
                            .is_some_and(|item| item["space"] == "screen")
                        {
                            50.0
                        } else {
                            0.0
                        };
                        LayoutChange {
                            target,
                            x: center,
                            y: center,
                            scale: 100.0,
                            effect_scale: None,
                        }
                    });
                    self.layout_drag = None;
                }
            }
        });
        if self.layout_mode {
            let name = self
                .layout_selected
                .as_ref()
                .and_then(|target| adjustment_name(project, state, target));
            egui::Frame::new()
                .fill(if name.is_some() {
                    Color32::from_rgb(25, 105, 180)
                } else {
                    Color32::from_rgb(90, 105, 120)
                })
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(9, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let label = name
                        .map(|name| format!("{}：{name}", crate::i18n::tr("正在调整")))
                        .unwrap_or_else(|| crate::i18n::tr("尚未选择调整对象"));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&label).strong().color(Color32::WHITE),
                        )
                        .truncate(),
                    )
                    .on_hover_text(label);
                });
            ui.small(crate::i18n::tr(
                "拖动人物、插图或特效移动，拉动四角等比缩放；Esc 取消本次拖动。",
            ));
            self.alignment_toolbar(ui, state);
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
                LayoutTarget::Mood(cid) if state["mood_overrides"][cid].is_object() => {
                    state["mood_overrides"][cid]["offset_x"] = json!(change.x);
                    state["mood_overrides"][cid]["offset_y"] = json!(change.y);
                }
                LayoutTarget::Effect { .. } => {
                    if let Some(item) = effects::item_mut(state, &change.target) {
                        item["x"] = json!(change.x);
                        item["y"] = json!(change.y);
                        let [a, b] = change.effect_scale.unwrap_or([
                            number(item, "a", 1.0) * change.scale / 100.0,
                            number(item, "b", 1.0) * change.scale / 100.0,
                        ]);
                        item["a"] = json!(a);
                        item["b"] = json!(b);
                    }
                }
                _ => (),
            }
        }
    }

    pub fn cancel_layout_drag(&mut self) {
        self.layout_drag = None;
        self.layout_edit = None;
        self.layout_command = None;
    }

    pub(super) fn layout_handles(
        &mut self,
        ui: &mut egui::Ui,
        stage: Rect,
        objects: &[LayoutObject],
        project: &Project,
        state: &Value,
    ) {
        if !self.layout_mode || !ui.is_enabled() {
            self.layout_drag = None;
            return;
        }
        let painter = ui.painter().with_clip_rect(stage);
        let stage_response = ui.interact(stage, ui.id().with("layout-stage"), egui::Sense::click());
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
            for (index, corner) in corners
                .iter()
                .enumerate()
                .filter(|_| !matches!(object.target, LayoutTarget::Mood(_)))
            {
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
                        crate::i18n::tr("拖动{corner}角缩放").replace(
                            "{corner}",
                            &crate::i18n::tr(["左上", "右上", "右下", "左下"][index]),
                        ),
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
                    crate::i18n::tr("拖动画面对象 {index}")
                        .replace("{index}", &(index + 1).to_string()),
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
                stage_response.request_focus();
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
            ui.ctx().request_repaint();
        }
        if let Some(drag) = &mut self.layout_drag {
            let point = ui
                .input(|input| input.pointer.latest_pos())
                .unwrap_or(drag.origin);
            drag.next = drag.at(point);
            if self.layout_snap
                && !ui.input(|input| input.modifiers.alt)
                && drag.resize_anchor.is_none()
                && point.distance(drag.origin) > 0.5
            {
                alignment::snap(drag, stage, &painter);
            }
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
        if self.layout_drag.is_none() {
            self.alignment_actions(ui, stage, objects, state, &stage_response);
        }
        if let Some((object, label)) = self.layout_selected.as_ref().and_then(|target| {
            let object = objects.iter().find(|object| &object.target == target)?;
            Some((object, adjustment_name(project, state, target)?))
        }) {
            let corners = object.corners();
            let anchor = corners.iter().skip(1).fold(corners[0], |point, corner| {
                Pos2::new(point.x.min(corner.x), point.y.min(corner.y))
            });
            adjustment_badge(&painter, stage, anchor, &label);
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
    fn adjustment_names_follow_the_selected_object_and_current_stage() {
        let mut project = Project::new();
        for (kind, id, name) in [
            ("character", "test.actor", "测试人物"),
            ("image", "test.first", "旧插图"),
            ("image", "test.second", "新插图"),
        ] {
            project.assets.insert(
                format!("assets/user/{kind}/{id}/content.json"),
                serde_json::to_vec(&json!({"id":id,"type":kind,"name":name})).unwrap(),
            );
        }
        let mut state = json!({
            "actors":{"player":{},"user:test.actor":{}},
            "overlays":{"main":{"image":"user:test.first"}},
            "custom_cg":{"image":"user:test.second"}
        });
        let character = LayoutTarget::Character("user:test.actor".into());
        let official = LayoutTarget::Character("player".into());
        let overlay = LayoutTarget::Overlay("main".into());
        assert!(adjustment_name(&project, &state, &character)
            .unwrap()
            .ends_with("测试人物"));
        assert!(adjustment_name(&project, &state, &official)
            .unwrap()
            .ends_with(&character_display_name(&project, "player")));
        assert!(adjustment_name(&project, &state, &overlay)
            .unwrap()
            .ends_with("旧插图"));
        assert!(adjustment_name(&project, &state, &LayoutTarget::Cg)
            .unwrap()
            .ends_with("新插图"));
        state["overlays"]["main"]["image"] = json!("user:test.second");
        assert!(adjustment_name(&project, &state, &overlay)
            .unwrap()
            .ends_with("新插图"));
        state["overlays"]["main"] = Value::Null;
        state["actors"]["user:test.actor"] = Value::Null;
        state["custom_cg"] = Value::Null;
        for target in [&overlay, &character, &LayoutTarget::Cg] {
            assert!(adjustment_name(&project, &state, target).is_none());
        }
    }

    #[test]
    fn adjustment_badges_remain_inside_the_stage_at_every_edge() {
        for size in [Vec2::new(320.0, 180.0), Vec2::new(24.0, 16.0)] {
            let stage = Rect::from_min_size(Pos2::new(31.5, 47.25), size);
            for anchor in [
                stage.left_top(),
                stage.right_top(),
                stage.right_bottom(),
                stage.left_bottom(),
                stage.center() - Vec2::splat(1000.0),
                stage.center() + Vec2::splat(1000.0),
            ] {
                for label_size in [Vec2::new(100.0, 25.0), Vec2::new(1000.0, 80.0)] {
                    let rect = badge_rect(stage, anchor, label_size);
                    assert!(stage.contains_rect(rect), "{rect:?} outside {stage:?}");
                }
            }
        }
    }

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
                effect_scale: None,
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
            effect_scale: None,
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

    #[test]
    fn effect_drag_uses_screen_down_or_world_up_and_never_clamps_to_image_limits() {
        let target = LayoutTarget::Effect {
            node: "fx".into(),
            action: Some(0),
        };
        let mut object = LayoutObject {
            target: target.clone(),
            bounds: Rect::from_center_size(Pos2::new(100.0, 100.0), Vec2::splat(50.0)),
            pivot: Pos2::new(100.0, 100.0),
            angle: 0.0,
            value: LayoutChange {
                target,
                x: 50.0,
                y: 50.0,
                scale: 100.0,
                effect_scale: None,
            },
            units: Vec2::new(10.0, -10.0),
        };
        let screen = LayoutDrag {
            object: object.clone(),
            origin: object.pivot,
            resize_anchor: None,
            next: object.value.clone(),
        }
        .at(object.pivot + Vec2::new(100.0, 50.0));
        assert_eq!((screen.x, screen.y), (60.0, 55.0));
        object.units = Vec2::splat(50.0);
        object.value.x = 200.0;
        object.value.y = -400.0;
        let world = LayoutDrag {
            object: object.clone(),
            origin: object.pivot,
            resize_anchor: None,
            next: object.value.clone(),
        }
        .at(object.pivot + Vec2::new(100.0, 50.0));
        assert_eq!((world.x, world.y), (202.0, -401.0));
    }
}
