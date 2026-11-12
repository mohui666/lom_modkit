//! Viewing-only zoom. No project values or stage layout handles are edited here.
use super::*;

const VIEWER_REQUEST: &str = "preview-detail-window";

#[derive(Clone)]
pub(super) struct Request {
    pub(super) lookup: String,
    pub(super) character: bool,
    pub(super) portrait: String,
    pub(super) label: String,
}

impl Request {
    fn identity(&self) -> String {
        format!("{}:{}:{}", self.character, self.lookup, self.portrait)
    }
}

#[derive(Clone)]
struct DetailImage {
    source: String,
    thumbnail: egui::TextureId,
    texture: TextureHandle,
    source_pixels: Vec2,
    limited: bool,
}

#[derive(Clone, Debug)]
struct ViewState {
    source: String,
    /// None continuously fits the viewport. Some(1) maps one source pixel to
    /// one display pixel, including on screens with a non-unit DPI scale.
    zoom: Option<f32>,
    pan: Vec2,
}

impl ViewState {
    fn new(source: String) -> Self {
        Self {
            source,
            zoom: None,
            pan: Vec2::ZERO,
        }
    }

    fn fit_zoom(pixels: Vec2, viewport: Rect, pixels_per_point: f32) -> f32 {
        (viewport.width() / pixels.x).min(viewport.height() / pixels.y) * pixels_per_point
    }

    fn scale(&self, pixels: Vec2, viewport: Rect, pixels_per_point: f32) -> f32 {
        self.zoom
            .unwrap_or_else(|| Self::fit_zoom(pixels, viewport, pixels_per_point))
    }

    fn image_rect(&self, pixels: Vec2, viewport: Rect, pixels_per_point: f32) -> Rect {
        Rect::from_center_size(
            viewport.center() + self.pan,
            pixels * (self.scale(pixels, viewport, pixels_per_point) / pixels_per_point),
        )
    }

    fn clamp_pan(&mut self, pixels: Vec2, viewport: Rect, pixels_per_point: f32) {
        let size = self.image_rect(pixels, viewport, pixels_per_point).size();
        let limit = ((size - viewport.size()) * 0.5).max(Vec2::ZERO);
        self.pan = self.pan.clamp(-limit, limit);
    }

    fn zoom_at(
        &mut self,
        factor: f32,
        anchor: Pos2,
        pixels: Vec2,
        viewport: Rect,
        pixels_per_point: f32,
    ) {
        let old = self.scale(pixels, viewport, pixels_per_point);
        let fit = Self::fit_zoom(pixels, viewport, pixels_per_point);
        let next = (old * factor).clamp(fit.min(0.01), fit.max(32.0));
        // Keep the source pixel under the cursor fixed while zooming.
        self.pan =
            anchor - viewport.center() - (anchor - viewport.center() - self.pan) * (next / old);
        self.zoom = Some(next);
        self.clamp_pan(pixels, viewport, pixels_per_point);
    }

    fn fit(&mut self) {
        self.zoom = None;
        self.pan = Vec2::ZERO;
    }
}

fn viewer_text(name: &str) -> String {
    crate::i18n::key(&format!("preview.viewer.{name}"))
}

impl Preview {
    pub(super) fn open_image_viewer(&self, ctx: &egui::Context, request: Request) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new(VIEWER_REQUEST), request));
        ctx.request_repaint();
    }

    /// The ordinary preview texture is deliberately capped at 1600 pixels.
    /// A detail view keeps just its current full-resolution image, invalidated
    /// by that existing texture cache's content identity.
    fn detail_image(
        &self,
        ctx: &egui::Context,
        project: &Project,
        request: &Request,
        thumbnail: &TextureHandle,
        slot: egui::Id,
    ) -> anyhow::Result<DetailImage> {
        let source = request.identity();
        let cached = ctx.data_mut(|data| data.get_temp::<DetailImage>(slot));
        if let Some(cached) =
            cached.filter(|image| image.source == source && image.thumbnail == thumbnail.id())
        {
            return Ok(cached);
        }
        let (_, bytes, _) = self
            .asset(
                project,
                &request.lookup,
                request.character,
                &request.portrait,
            )
            .ok_or_else(|| anyhow::anyhow!(viewer_text("unavailable")))?;
        let bytes = match bytes {
            AssetSource::Project(bytes) => std::borrow::Cow::Borrowed(bytes),
            AssetSource::Library(path) => std::borrow::Cow::Owned(std::fs::read(path)?),
        };
        anyhow::ensure!(
            bytes.len() <= 32 * 1024 * 1024,
            "{}",
            viewer_text("too_large")
        );
        let image = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()?
            .decode()?;
        let source_pixels = Vec2::new(image.width() as f32, image.height() as f32);
        let limit = ctx.input(|input| input.max_texture_side) as u32;
        let limited = image.width() > limit || image.height() > limit;
        let image = if limited {
            image.thumbnail(limit, limit)
        } else {
            image
        }
        .into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let texture =
            if thumbnail.size() == size && self.camera_filter == filters::Settings::default() {
                // Images already cached at source resolution need no duplicate
                // upload. Keep their shared texture identity when switching actors.
                thumbnail.clone()
            } else {
                let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                ctx.load_texture(
                    format!("detail:{source}"),
                    pixels,
                    egui::TextureOptions::LINEAR,
                )
            };
        let loaded = DetailImage {
            source,
            thumbnail: thumbnail.id(),
            texture,
            source_pixels,
            limited,
        };
        ctx.data_mut(|data| data.insert_temp(slot, loaded.clone()));
        Ok(loaded)
    }

    pub(super) fn portrait_image_viewer(
        &self,
        ui: &mut egui::Ui,
        project: &Project,
        request: Request,
        thumbnail: TextureHandle,
    ) {
        let slot = ui.id().with("portrait-detail-texture");
        match self.detail_image(ui.ctx(), project, &request, &thumbnail, slot) {
            Ok(image) => {
                let expand = image_view(ui, &image, "portrait-detail-canvas", true);
                if expand {
                    self.open_image_viewer(ui.ctx(), request);
                }
            }
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, crate::error_ui::capture(error));
            }
        }
    }

    pub(super) fn image_viewer_window(&mut self, ctx: &egui::Context, project: &Project) {
        let request_id = egui::Id::new(VIEWER_REQUEST);
        let Some(request) = ctx.data_mut(|data| data.get_temp::<Request>(request_id)) else {
            return;
        };
        // Portraits and the thumbnail browser can both be visible in one frame.
        let frame_id = request_id.with("drawn-pass");
        let frame = ctx.cumulative_pass_nr();
        if ctx.data_mut(|data| data.get_temp::<u64>(frame_id)) == Some(frame) {
            return;
        }
        ctx.data_mut(|data| data.insert_temp(frame_id, frame));
        let mut open = true;
        let max_size = (ctx.content_rect().size() - Vec2::splat(40.0)).max(Vec2::new(300.0, 240.0));
        egui::Window::new(format!("{} · {}", viewer_text("title"), request.label))
            .id(request_id)
            .open(&mut open)
            .collapsible(false)
            .default_size(Vec2::new(860.0, 650.0).min(max_size))
            .min_size(Vec2::new(300.0, 240.0))
            .max_size(max_size)
            .show(ctx, |ui| {
                ui.weak(&request.lookup);
                if let Some((thumbnail, _)) = self.texture(
                    ctx,
                    project,
                    &request.lookup,
                    request.character,
                    &request.portrait,
                ) {
                    let slot = request_id.with("detail-texture");
                    match self.detail_image(ctx, project, &request, &thumbnail, slot) {
                        Ok(image) => {
                            image_view(ui, &image, "detail-window-canvas", false);
                        }
                        Err(error) => {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                crate::error_ui::capture(error),
                            );
                        }
                    }
                } else {
                    ui.label(viewer_text("unavailable"));
                }
            });
        if !open {
            ctx.data_mut(|data| {
                data.remove::<Request>(request_id);
                data.remove::<DetailImage>(request_id.with("detail-texture"));
            });
        }
    }
}

fn image_view(ui: &mut egui::Ui, image: &DetailImage, name: &str, allow_expand: bool) -> bool {
    let state_id = ui.id().with(name);
    let mut state = ui
        .data_mut(|data| data.get_temp::<ViewState>(state_id))
        .filter(|state| state.source == image.source)
        .unwrap_or_else(|| ViewState::new(image.source.clone()));
    let pixels_per_point = ui.ctx().pixels_per_point();
    let mut factor = 1.0;
    let mut expand = false;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("−")
            .on_hover_text(viewer_text("zoom_out"))
            .clicked()
        {
            factor = 1.0 / 1.25;
        }
        if ui
            .button("+")
            .on_hover_text(viewer_text("zoom_in"))
            .clicked()
        {
            factor = 1.25;
        }
        if ui.button(viewer_text("fit")).clicked() {
            state.fit();
        }
        if ui
            .button("100%")
            .on_hover_text(viewer_text("actual_pixels"))
            .clicked()
        {
            state.zoom = Some(1.0);
            state.pan = Vec2::ZERO;
        }
        if allow_expand && ui.button(viewer_text("open")).clicked() {
            expand = true;
        }
        let zoom = state
            .zoom
            .map(|zoom| format!("{:.0}%", zoom * 100.0))
            .unwrap_or_else(|| viewer_text("fit"));
        ui.small(format!(
            "{zoom} · {:.0} × {:.0} px",
            image.source_pixels.x, image.source_pixels.y
        ));
    });
    ui.small(viewer_text("hint"));
    if image.limited {
        ui.colored_label(ui.visuals().warn_fg_color, viewer_text("texture_limit"));
    }
    let size = ui.available_size().max(Vec2::splat(1.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    state.clamp_pan(image.source_pixels, rect, pixels_per_point);
    if factor != 1.0 {
        state.zoom_at(
            factor,
            rect.center(),
            image.source_pixels,
            rect,
            pixels_per_point,
        );
        ui.ctx().request_repaint();
    }
    // Consume scroll only over this canvas, so forms and thumbnail lists retain
    // their normal scrolling everywhere else.
    if response.hovered() {
        let scroll = ui.input_mut(|input| {
            let scroll = input.smooth_scroll_delta.y;
            input.smooth_scroll_delta.y = 0.0;
            input.raw_scroll_delta.y = 0.0;
            scroll
        });
        if scroll != 0.0 {
            let anchor = response.hover_pos().unwrap_or(rect.center());
            state.zoom_at(
                (scroll * 0.003).exp(),
                anchor,
                image.source_pixels,
                rect,
                pixels_per_point,
            );
            ui.ctx().request_repaint();
        }
    }
    if response.dragged_by(egui::PointerButton::Primary) {
        state.pan += response.drag_delta();
        state.clamp_pan(image.source_pixels, rect, pixels_per_point);
    }
    let image_rect = state.image_rect(image.source_pixels, rect, pixels_per_point);
    if image_rect.width() > rect.width() || image_rect.height() > rect.height() {
        let cursor = if response.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        };
        response.on_hover_cursor(cursor);
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 21, 31));
    painter.image(image.texture.id(), image_rect, uv(), Color32::WHITE);
    ui.data_mut(|data| data.insert_temp(state_id, state));
    expand
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "{a} != {b}");
    }

    #[test]
    fn actual_pixels_honor_display_dpi_and_fit_resets_pan() {
        let pixels = Vec2::new(800.0, 600.0);
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let mut state = ViewState::new("image".into());
        state.zoom = Some(1.0);
        let actual = state.image_rect(pixels, viewport, 2.0);
        assert_eq!(actual.size(), Vec2::new(400.0, 300.0));
        state.pan = Vec2::new(22.0, 33.0);
        state.fit();
        let fitted = state.image_rect(pixels, viewport, 2.0);
        close(fitted.height(), viewport.height());
        assert_eq!(fitted.center(), viewport.center());
    }

    #[test]
    fn cursor_zoom_keeps_the_same_source_pixel_and_pan_stops_at_edges() {
        let pixels = Vec2::new(1000.0, 800.0);
        let viewport = Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(400.0, 300.0));
        let mut state = ViewState::new("image".into());
        state.zoom = Some(1.0);
        let cursor = viewport.center() + Vec2::new(35.0, -23.0);
        let before = (cursor - state.image_rect(pixels, viewport, 1.0).min)
            / state.scale(pixels, viewport, 1.0);
        state.zoom_at(2.0, cursor, pixels, viewport, 1.0);
        let after = (cursor - state.image_rect(pixels, viewport, 1.0).min)
            / state.scale(pixels, viewport, 1.0);
        close(before.x, after.x);
        close(before.y, after.y);
        state.pan = Vec2::new(10000.0, -10000.0);
        state.clamp_pan(pixels, viewport, 1.0);
        let drawn = state.image_rect(pixels, viewport, 1.0);
        close(drawn.left(), viewport.left());
        close(drawn.bottom(), viewport.bottom());
    }

    fn synthetic_project(size: [u32; 2]) -> Project {
        let mut project = Project::new();
        let image = image::RgbaImage::from_fn(size[0], size[1], |x, y| {
            image::Rgba([(x % 251) as u8, (y % 251) as u8, 190, 255])
        });
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        project.assets.insert(
            "assets/user/image/view/content.json".into(),
            serde_json::to_vec(
                &json!({"id":"view","type":"image","name":"detail","files":{"main":"image.png"}}),
            )
            .unwrap(),
        );
        project
            .assets
            .insert("assets/user/image/view/image.png".into(), png.into_inner());
        project
    }

    #[test]
    fn detail_loading_keeps_pixels_beyond_the_thumbnail_limit_without_editing_source() {
        let ctx = egui::Context::default();
        let project = synthetic_project([2100, 1200]);
        let before = (
            project.manifest.clone(),
            project.stories.clone(),
            project.assets.clone(),
        );
        let mut preview = Preview::new(Path::new("."));
        let request = Request {
            lookup: "user:view".into(),
            character: false,
            portrait: String::new(),
            label: "detail".into(),
        };
        let _ = ctx.run(
            egui::RawInput {
                max_texture_side: Some(4096),
                ..Default::default()
            },
            |ctx| {
                let (thumbnail, _) = preview
                    .texture(ctx, &project, &request.lookup, false, "")
                    .unwrap();
                assert_eq!(thumbnail.size()[0], 1600);
                let detail = preview
                    .detail_image(
                        ctx,
                        &project,
                        &request,
                        &thumbnail,
                        egui::Id::new("detail-test"),
                    )
                    .unwrap();
                assert_eq!(detail.texture.size(), [2100, 1200]);
                assert_eq!(detail.source_pixels, Vec2::new(2100.0, 1200.0));
                assert!(!detail.limited);
            },
        );
        assert_eq!(
            (
                project.manifest.clone(),
                project.stories.clone(),
                project.assets.clone()
            ),
            before
        );
        assert!(preview.layout_edit.is_none());
        assert!(preview.mood_edit.is_none());
    }

    #[test]
    fn opening_a_thumbnail_detail_does_not_choose_the_asset() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let project = synthetic_project([240, 180]);
        let before = (
            project.manifest.clone(),
            project.stories.clone(),
            project.assets.clone(),
        );
        let mut preview = Preview::new(Path::new("."));
        let options = vec![("user:view".to_owned(), "detail".to_owned())];
        let mut chosen = None;
        let mut frame = |events| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        chosen = preview.image_browser(ui, &project, &options, "", false);
                    });
                },
            );
            (output, chosen.clone())
        };
        let (output, _) = frame(vec![]);
        let header = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| {
                node.label() == Some(crate::i18n::key("editor.preview.browse_images").as_str())
            })
            .map(|(id, _)| *id)
            .unwrap();
        let (output, _) = frame(vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target: header,
                data: None,
            },
        )]);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let target = nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(viewer_text("open").as_str())
            })
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("thumbnail detail button missing: {nodes:?}"));
        let (_, chosen) = frame(vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target,
                data: None,
            },
        )]);
        assert!(chosen.is_none());
        // egui measures a new floating window in its first sizing pass.
        let (opened, _) = frame(vec![]);
        let image = ctx
            .data_mut(|data| {
                data.get_temp::<DetailImage>(egui::Id::new(VIEWER_REQUEST).with("detail-texture"))
            })
            .unwrap();
        assert!(opened.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Mesh(mesh) if mesh.texture_id == image.texture.id())));
        assert_eq!(
            ctx.data_mut(|data| data.get_temp::<Request>(egui::Id::new(VIEWER_REQUEST)))
                .unwrap()
                .lookup,
            "user:view"
        );
        assert_eq!(
            (
                project.manifest.clone(),
                project.stories.clone(),
                project.assets.clone()
            ),
            before
        );
    }

    #[test]
    fn wheel_zoom_is_local_to_the_canvas_and_drag_only_changes_view_state() {
        let ctx = egui::Context::default();
        let pixels = egui::ColorImage::filled([800, 600], Color32::LIGHT_BLUE);
        let texture = ctx.load_texture("synthetic-view", pixels, egui::TextureOptions::LINEAR);
        let image = DetailImage {
            source: "synthetic".into(),
            thumbnail: texture.id(),
            texture,
            source_pixels: Vec2::new(800.0, 600.0),
            limited: false,
        };
        let mut state_id = egui::Id::NULL;
        let mut canvas = Rect::NOTHING;
        let mut form = Rect::NOTHING;
        let mut form_offset = 0.0;
        let mut frame = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 450.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.columns(2, |columns| {
                            let ui = &mut columns[0];
                            state_id = ui.id().with("test-canvas");
                            image_view(ui, &image, "test-canvas", false);
                            canvas = ui.min_rect();
                            let ui = &mut columns[1];
                            let output = egui::ScrollArea::vertical()
                                .id_salt("ordinary-form")
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for n in 0..80 {
                                        ui.label(format!("field {n}"));
                                    }
                                });
                            form = output.inner_rect;
                            form_offset = output.state.offset.y;
                        });
                    });
                },
            );
            (state_id, canvas, form, form_offset)
        };
        let (_, canvas, form, _) = frame(vec![]);
        let wheel = |y| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, y),
            modifiers: egui::Modifiers::NONE,
        };
        let (id, _, _, _) = frame(vec![egui::Event::PointerMoved(canvas.center()), wheel(7.0)]);
        let zoomed = ctx.data_mut(|data| data.get_temp::<ViewState>(id)).unwrap();
        assert!(zoomed.zoom.is_some());
        let (_, _, _, offset) = frame(vec![egui::Event::PointerMoved(form.center()), wheel(-7.0)]);
        assert!(offset > 0.0, "ordinary form should scroll");
        let after = ctx.data_mut(|data| data.get_temp::<ViewState>(id)).unwrap();
        assert_eq!(
            after.zoom, zoomed.zoom,
            "scrolling the form must not zoom the image"
        );
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![
            egui::Event::PointerMoved(canvas.center()),
            pointer(canvas.center(), true),
        ]);
        frame(vec![egui::Event::PointerMoved(
            canvas.center() + Vec2::new(30.0, 20.0),
        )]);
        frame(vec![pointer(
            canvas.center() + Vec2::new(30.0, 20.0),
            false,
        )]);
        let dragged = ctx.data_mut(|data| data.get_temp::<ViewState>(id)).unwrap();
        assert_ne!(dragged.pan, Vec2::ZERO);
    }
}
