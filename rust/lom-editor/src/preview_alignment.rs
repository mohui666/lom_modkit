use super::*;

#[derive(Clone, Copy)]
pub(in crate::preview) enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}
pub(in crate::preview) enum Command {
    Align(Align),
    Copy,
    Paste,
}
pub(in crate::preview) struct Clipboard {
    value: LayoutChange,
    screen: Option<bool>,
}

fn screen_space(state: &Value, target: &LayoutTarget) -> Option<bool> {
    effects::item(state, target).map(|item| item["space"] == "screen")
}

impl LayoutObject {
    fn visible_bounds(&self) -> Rect {
        self.corners()
            .into_iter()
            .fold(Rect::NOTHING, |mut bounds, point| {
                bounds.extend_with(point);
                bounds
            })
    }
    fn translated(&self, value: &LayoutChange, delta: Vec2) -> LayoutChange {
        let delta = if matches!(self.target, LayoutTarget::Mood(_)) {
            egui::emath::Rot2::from_angle(-self.angle.to_radians()) * delta
        } else {
            delta
        };
        let mut next = value.clone();
        next.x += (delta.x / self.units.x) as f64;
        next.y -= (delta.y / self.units.y) as f64;
        next
    }
    fn displacement(&self, value: &LayoutChange) -> Vec2 {
        let delta = Vec2::new(
            (value.x - self.value.x) as f32 * self.units.x,
            -(value.y - self.value.y) as f32 * self.units.y,
        );
        if matches!(self.target, LayoutTarget::Mood(_)) {
            egui::emath::Rot2::from_angle(self.angle.to_radians()) * delta
        } else {
            delta
        }
    }
    fn aligned(&self, stage: Rect, alignment: Align) -> LayoutChange {
        let bounds = self.visible_bounds();
        let delta = match alignment {
            Align::Left => Vec2::new(stage.left() - bounds.left(), 0.0),
            Align::CenterX => Vec2::new(stage.center().x - bounds.center().x, 0.0),
            Align::Right => Vec2::new(stage.right() - bounds.right(), 0.0),
            Align::Top => Vec2::new(0.0, stage.top() - bounds.top()),
            Align::CenterY => Vec2::new(0.0, stage.center().y - bounds.center().y),
            Align::Bottom => Vec2::new(0.0, stage.bottom() - bounds.bottom()),
        };
        self.translated(&self.value, delta)
    }
}

fn snap_delta(bounds: Rect, stage: Rect) -> (Vec2, [Option<f32>; 2]) {
    let mut delta = Vec2::ZERO;
    let mut guides = [None, None];
    for axis in 0..2 {
        let mut nearest = 6.0;
        let points = if axis == 0 {
            [bounds.left(), bounds.center().x, bounds.right()]
        } else {
            [bounds.top(), bounds.center().y, bounds.bottom()]
        };
        let targets = if axis == 0 {
            [stage.left(), stage.center().x, stage.right()]
        } else {
            [stage.top(), stage.center().y, stage.bottom()]
        };
        for (point, target) in points.into_iter().zip(targets) {
            let distance = target - point;
            if distance.abs() <= nearest {
                nearest = distance.abs();
                delta[axis] = distance;
                guides[axis] = Some(target);
            }
        }
    }
    (delta, guides)
}

pub(super) fn snap(drag: &mut LayoutDrag, stage: Rect, painter: &egui::Painter) {
    let bounds = drag
        .object
        .visible_bounds()
        .translate(drag.object.displacement(&drag.next));
    let (delta, guides) = snap_delta(bounds, stage);
    drag.next = drag.object.translated(&drag.next, delta);
    if matches!(
        drag.next.target,
        LayoutTarget::Overlay(_) | LayoutTarget::Cg
    ) {
        drag.next.x = drag.next.x.clamp(-100.0, 100.0);
        drag.next.y = drag.next.y.clamp(-100.0, 100.0);
    }
    let actual = drag
        .object
        .visible_bounds()
        .translate(drag.object.displacement(&drag.next));
    let expected = bounds.translate(delta);
    for (axis, guide) in guides.into_iter().enumerate() {
        if let Some(guide) = guide.filter(|_| (actual.min[axis] - expected.min[axis]).abs() < 0.1) {
            let line = if axis == 0 {
                [
                    Pos2::new(guide, stage.top()),
                    Pos2::new(guide, stage.bottom()),
                ]
            } else {
                [
                    Pos2::new(stage.left(), guide),
                    Pos2::new(stage.right(), guide),
                ]
            };
            painter.line_segment(line, Stroke::new(1.0_f32, Color32::from_rgb(220, 105, 25)));
        }
    }
}

impl Preview {
    pub(in crate::preview) fn alignment_toolbar(&mut self, ui: &mut egui::Ui, state: &Value) {
        use crate::i18n::tr;
        ui.add_enabled_ui(self.layout_selected.is_some(), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (alignment, label) in [
                    (Align::Left, "靠左"),
                    (Align::CenterX, "水平居中"),
                    (Align::Right, "靠右"),
                    (Align::Top, "靠上"),
                    (Align::CenterY, "垂直居中"),
                    (Align::Bottom, "贴底"),
                ] {
                    if ui
                        .small_button(tr(label))
                        .on_hover_text(tr("按可见边缘对齐到画面，忽略透明留白；可撤销。"))
                        .clicked()
                    {
                        self.layout_command = Some(Command::Align(alignment));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(tr("复制布局")).clicked() {
                    self.layout_command = Some(Command::Copy);
                }
                let compatible = self
                    .layout_selected
                    .as_ref()
                    .zip(self.layout_clipboard.as_ref())
                    .is_some_and(|(target, clipboard)| {
                        std::mem::discriminant(target)
                            == std::mem::discriminant(&clipboard.value.target)
                            && screen_space(state, target) == clipboard.screen
                    });
                if ui
                    .add_enabled(compatible, egui::Button::new(tr("粘贴布局")).small())
                    .on_hover_text(tr("粘贴同类对象的位置与缩放；特效需使用相同坐标模式。"))
                    .clicked()
                {
                    self.layout_command = Some(Command::Paste);
                }
                ui.checkbox(&mut self.layout_snap, tr("吸附参考线"));
            });
        });
        ui.small(tr(
            "点击画面对象后用方向键微调 1 个舞台像素，Shift 为 10；拖动时按 Alt 暂停吸附。",
        ));
    }

    pub(in crate::preview) fn alignment_actions(
        &mut self,
        ui: &mut egui::Ui,
        stage: Rect,
        objects: &[LayoutObject],
        state: &Value,
        response: &egui::Response,
    ) {
        let Some(object) = objects
            .iter()
            .find(|o| Some(&o.target) == self.layout_selected.as_ref())
        else {
            self.layout_command = None;
            return;
        };
        if let Some(command) = self.layout_command.take() {
            match command {
                Command::Align(alignment) => {
                    self.layout_edit = Some(object.aligned(stage, alignment))
                }
                Command::Copy => {
                    let mut value = object.value.clone();
                    value.effect_scale = effects::item(state, &object.target)
                        .map(|item| [number(item, "a", 1.0), number(item, "b", 1.0)]);
                    self.layout_clipboard = Some(Clipboard {
                        value,
                        screen: screen_space(state, &object.target),
                    });
                }
                Command::Paste => {
                    if let Some(clipboard) = &self.layout_clipboard {
                        if std::mem::discriminant(&object.target)
                            == std::mem::discriminant(&clipboard.value.target)
                            && screen_space(state, &object.target) == clipboard.screen
                        {
                            let mut change = clipboard.value.clone();
                            change.target = object.target.clone();
                            self.layout_edit = Some(change);
                        }
                    }
                }
            }
            response.request_focus();
        }
        if response.has_focus() && self.layout_edit.is_none() {
            let delta = ui.input_mut(|input| {
                if input.modifiers.alt || input.modifiers.ctrl || input.modifiers.command {
                    return Vec2::ZERO;
                }
                let modifiers = if input.modifiers.shift {
                    egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::NONE
                };
                let step = if input.modifiers.shift { 10.0 } else { 1.0 };
                let mut delta = Vec2::ZERO;
                for (key, direction) in [
                    (egui::Key::ArrowLeft, Vec2::new(-1.0, 0.0)),
                    (egui::Key::ArrowRight, Vec2::new(1.0, 0.0)),
                    (egui::Key::ArrowUp, Vec2::new(0.0, -1.0)),
                    (egui::Key::ArrowDown, Vec2::new(0.0, 1.0)),
                ] {
                    if input.consume_key(modifiers, key) {
                        delta += direction * step;
                    }
                }
                delta
            });
            if delta != Vec2::ZERO {
                let mut change = object.translated(
                    &object.value,
                    Vec2::new(
                        delta.x * stage.width() / 1920.0,
                        delta.y * stage.height() / 1080.0,
                    ),
                );
                if matches!(change.target, LayoutTarget::Overlay(_) | LayoutTarget::Cg) {
                    change.x = change.x.clamp(-100.0, 100.0);
                    change.y = change.y.clamp(-100.0, 100.0);
                }
                self.layout_edit = Some(change);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calibrated_expressions_keep_their_visible_center_and_baseline_when_facing_either_way() {
        let mut project = Project::new();
        let root = "assets/user/character/test.poses";
        let mut files = std::collections::BTreeMap::new();
        for (name, width, height, body) in [
            ("normal.png", 100, 160, [30, 10, 80, 125]),
            ("happy.png", 200, 80, [60, 5, 170, 65]),
        ] {
            let mut image = image::RgbaImage::new(width, height);
            for y in body[1]..body[3] {
                for x in body[0]..body[2] {
                    image.put_pixel(x, y, image::Rgba([150, 20, 220, 255]));
                }
            }
            let mut png = std::io::Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png).unwrap();
            files.insert(name.to_owned(), png.into_inner());
        }
        let mut metadata = json!({"id":"test.poses","type":"character","scale":82,"portrait_reference":"normal","portraits":{"normal":"normal.png","happy":"happy.png"}});
        lom_core::portrait_alignment::auto_align(&mut metadata, &files).unwrap();
        project.assets.insert(
            format!("{root}/content.json"),
            serde_json::to_vec(&metadata).unwrap(),
        );
        for (name, bytes) in files {
            project.assets.insert(format!("{root}/{name}"), bytes);
        }
        for facing in ["left", "right"] {
            let story = json!({"id":"main","start":"show","nodes":[{"id":"show","type":"show","character":"user:test.poses","position":"M","portrait":"normal","facing":facing},{"id":"expression","type":"portrait","character":"user:test.poses","portrait":"happy"}]});
            let ctx = egui::Context::default();
            let mut preview = Preview::new(Path::new("/unused"));
            let mut positions = Vec::new();
            for index in 0..2 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1000.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            preview.show(ui, &project, &story, index, None);
                        });
                    },
                );
                let (stage, bounds) = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        let egui::Shape::Mesh(mesh) = &shape.shape else {
                            return None;
                        };
                        let texture = preview
                            .cache
                            .values()
                            .find(|cached| cached.texture.id() == mesh.texture_id)?;
                        Some((
                            shape.clip_rect,
                            preview.opaque_rect(
                                &texture.texture,
                                mesh.calc_bounds(),
                                facing == "right",
                            ),
                        ))
                    })
                    .unwrap();
                positions.push((
                    (bounds.center().x - stage.left()) / stage.width(),
                    (bounds.bottom() - stage.top()) / stage.height(),
                ));
            }
            assert!(
                (positions[0].0 - positions[1].0).abs() < 0.001,
                "{facing}: {positions:?}"
            );
            assert!(
                (positions[0].1 - positions[1].1).abs() < 0.001,
                "{facing}: {positions:?}"
            );
        }
    }
    #[test]
    fn half_body_with_transparent_padding_can_be_bottom_aligned_and_nudged_in_the_real_preview() {
        let mut project = Project::new();
        let root = "assets/user/character/test.half";
        project.assets.insert(
            format!("{root}/content.json"),
            serde_json::to_vec(
                &json!({"id":"test.half","type":"character","portraits":{"normal":"normal.png"}}),
            )
            .unwrap(),
        );
        let mut image = image::RgbaImage::new(200, 80);
        for y in 5..60 {
            for x in 60..150 {
                image.put_pixel(x, y, image::Rgba([150, 20, 220, 255]));
            }
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        project
            .assets
            .insert(format!("{root}/normal.png"), png.into_inner());
        let mut story = json!({"id":"main","start":"show","nodes":[{"id":"show","type":"show","character":"user:test.half","position":"M","facing":"left"}]});
        let mut preview = Preview::new(Path::new("/unused"));
        preview.layout_mode = true;
        let ctx = egui::Context::default();
        let frame = |preview: &mut Preview, story: &Value, index, events, modifiers, focus| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
                    events,
                    modifiers,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if focus {
                            ui.memory_mut(|memory| {
                                memory.request_focus(ui.id().with("layout-stage"))
                            });
                        }
                        preview.show(ui, &project, story, index, None);
                    });
                },
            )
        };
        let initial = frame(
            &mut preview,
            &story,
            0,
            vec![],
            egui::Modifiers::NONE,
            false,
        );
        let bounds = |preview: &Preview, output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Mesh(mesh) = &shape.shape {
                        let texture = preview
                            .cache
                            .values()
                            .find(|cached| cached.texture.id() == mesh.texture_id)?;
                        Some((
                            shape.clip_rect,
                            preview.opaque_rect(&texture.texture, mesh.calc_bounds(), false),
                        ))
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        let (stage, body) = bounds(&preview, &initial);
        assert!(body.bottom() < stage.bottom() - 10.0);
        preview.layout_selected = Some(LayoutTarget::Character("user:test.half".into()));
        preview.layout_command = Some(Command::Align(Align::Bottom));
        frame(
            &mut preview,
            &story,
            0,
            vec![],
            egui::Modifiers::NONE,
            false,
        );
        let change = preview
            .layout_edit
            .take()
            .expect("bottom alignment must produce a saved edit");
        let index = crate::preview_layout::apply(&mut story, 0, &change).unwrap();
        let aligned = frame(
            &mut preview,
            &story,
            index,
            vec![],
            egui::Modifiers::NONE,
            true,
        );
        let (stage, body) = bounds(&preview, &aligned);
        assert!((body.bottom() - stage.bottom()).abs() < 0.01);
        for (key, modifiers, dx, dy) in [
            (egui::Key::ArrowRight, egui::Modifiers::NONE, 1.0, 0.0),
            (egui::Key::ArrowDown, egui::Modifiers::SHIFT, 0.0, -10.0),
        ] {
            frame(
                &mut preview,
                &story,
                index,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                true,
            );
            let changed = preview
                .layout_edit
                .take()
                .expect("arrow keys must nudge the selected object");
            assert!((changed.x - change.x - dx).abs() < 0.001);
            assert!((changed.y - change.y - dy).abs() < 0.001);
        }
    }
    fn object(target: LayoutTarget) -> LayoutObject {
        LayoutObject {
            target: target.clone(),
            bounds: Rect::from_min_max(Pos2::new(30.0, 45.0), Pos2::new(130.0, 125.0)),
            pivot: Pos2::new(90.0, 100.0),
            angle: 0.0,
            value: LayoutChange {
                target,
                x: 0.0,
                y: 0.0,
                scale: 100.0,
                effect_scale: None,
            },
            units: Vec2::splat(0.5),
        }
    }
    #[test]
    fn alignment_uses_rotated_visible_bounds_and_the_correct_mirrored_bubble_axes() {
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 135.0));
        for target in [
            LayoutTarget::Character("actor".into()),
            LayoutTarget::Mood("actor".into()),
            LayoutTarget::Cg,
        ] {
            let mut object = object(target);
            object.angle = 32.0;
            if matches!(object.target, LayoutTarget::Mood(_)) {
                object.units.x = -0.5;
            }
            let bottom = object.aligned(stage, Align::Bottom);
            let bounds = object
                .visible_bounds()
                .translate(object.displacement(&bottom));
            assert!((bounds.bottom() - stage.bottom()).abs() < 0.001);
            assert!((bounds.left() - object.visible_bounds().left()).abs() < 0.001);
            let centered = object.aligned(stage, Align::CenterX);
            let bounds = object
                .visible_bounds()
                .translate(object.displacement(&centered));
            assert!((bounds.center().x - stage.center().x).abs() < 0.001);
        }
    }
    #[test]
    fn snapping_only_applies_near_the_corresponding_edge_or_center() {
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 135.0));
        let (delta, guides) = snap_delta(
            Rect::from_min_max(Pos2::new(3.0, 30.0), Pos2::new(80.0, 132.0)),
            stage,
        );
        assert_eq!(delta, Vec2::new(-3.0, 3.0));
        assert_eq!(guides, [Some(0.0), Some(135.0)]);
        assert_eq!(
            snap_delta(
                Rect::from_min_max(Pos2::new(20.0, 20.0), Pos2::new(85.0, 95.0)),
                stage
            )
            .0,
            Vec2::ZERO
        );
    }
}
