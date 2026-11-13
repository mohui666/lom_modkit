//! Shared Rust window shell. Platform chrome is isolated from authoring state.
use eframe::egui::{self, Color32, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(41, 101, 158);
const TEXT: Color32 = Color32::from_rgb(45, 51, 59);
pub const NAVIGATION_WIDTH: f32 = 280.0;
pub const PREVIEW_WIDTH: f32 = 560.0;

pub struct Shell {
    backend: Option<String>,
    title: String,
    auto_scale: bool,
    ui_scale_percent: u16,
}
impl Default for Shell {
    fn default() -> Self {
        Self {
            backend: None,
            title: String::new(),
            auto_scale: true,
            ui_scale_percent: 100,
        }
    }
}
impl Shell {
    pub fn from_preferences(preferences: &serde_json::Value) -> Self {
        Self {
            auto_scale: preferences["ui_auto_scale"].as_bool().unwrap_or(true),
            ui_scale_percent: preferences["ui_scale_percent"]
                .as_u64()
                .map(|value| value.clamp(50, 200) as u16)
                .unwrap_or(100),
            ..Self::default()
        }
    }

    /// Adapt in UI zoom units, leaving the operating system's DPI scale intact.
    pub fn apply_layout(&mut self, ctx: &egui::Context) {
        // content_rect is already divided by the current zoom. Multiplying it
        // back gives the same native logical size after the next egui pass,
        // rather than repeatedly shrinking and enlarging a fixed-size window.
        let native_size = ctx.content_rect().size() * ctx.zoom_factor();
        let target = if self.auto_scale {
            fitted_zoom(native_size)
        } else {
            self.ui_scale_percent as f32 / 100.0
        };
        if (ctx.zoom_factor() - target).abs() > 0.005 {
            ctx.set_zoom_factor(target);
        }
    }

    pub fn resolution_settings(&mut self, ui: &mut egui::Ui) -> bool {
        ui.strong(crate::i18n::key("common.shell.scale"));
        let mut changed = ui
            .checkbox(
                &mut self.auto_scale,
                crate::i18n::key("common.shell.auto_scale"),
            )
            .changed();
        if !self.auto_scale {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::key("common.shell.manual_scale"));
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.ui_scale_percent)
                            .range(50..=200)
                            .suffix("%"),
                    )
                    .changed();
            });
        }
        ui.small(crate::i18n::message(
            "common.shell.scale_info",
            &[("percent", format!("{:.0}", ui.ctx().zoom_factor() * 100.0))],
        ));
        if ui
            .small_button(crate::i18n::key("common.shell.restore_scale"))
            .clicked()
        {
            self.auto_scale = true;
            self.ui_scale_percent = 100;
            changed = true;
        }
        changed
    }

    pub fn store_resolution_preferences(&self, preferences: &mut serde_json::Value) {
        preferences["ui_auto_scale"] = self.auto_scale.into();
        preferences["ui_scale_percent"] = self.ui_scale_percent.into();
    }

    pub fn install(&mut self, frame: &eframe::Frame) {
        if self.backend.is_none() {
            let backend = crate::macos_window::install(frame).unwrap_or_else(|error| {
                eprintln!("Native window appearance unavailable: {error}");
                "portable".into()
            });
            println!("Native window backend: {backend}");
            self.backend = Some(backend);
        }
    }
    pub fn set_title(&mut self, ctx: &egui::Context, title: String) {
        if self.title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
    pub fn clear_color(&self) -> [f32; 4] {
        [0.94, 0.94, 0.92, 1.0]
    }
}

fn fitted_zoom(native_size: Vec2) -> f32 {
    (native_size.x / 1280.0)
        .min(native_size.y / 760.0)
        .clamp(0.5, 1.0)
}
/// Shared chrome for in-app dialogs. These controls are drawn by egui, not AppKit.
pub fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(250, 250, 249))
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(218, 222, 226)))
        .corner_radius(12)
        .inner_margin(20)
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(36),
        })
}

pub fn dialog_header(ui: &mut egui::Ui, title: &str) -> bool {
    let close = ui
        .horizontal(|ui| {
            ui.add(egui::Label::new(egui::RichText::new(title).size(19.0).strong()).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = ui
                    .add_sized(
                        [28.0, 28.0],
                        egui::Button::new(egui::RichText::new("×").size(22.0)).frame(false),
                    )
                    .on_hover_text(crate::i18n::tr("关闭"));
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        crate::i18n::tr("关闭"),
                    )
                });
                response.clicked()
            })
            .inner
        })
        .inner;
    ui.add_space(8.0);
    close
}

/// A compact trailing button row, including inside scrollable dialog content.
pub fn dialog_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 30.0),
        egui::Layout::right_to_left(egui::Align::Center),
        add,
    )
    .inner
}

pub fn pin_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    pinned: bool,
    label: &str,
) -> egui::Response {
    let response = ui
        .put(
            rect,
            egui::Button::new("")
                .small()
                .frame(false)
                .min_size(rect.size()),
        )
        .on_hover_text(label);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), pinned, label)
    });
    let color = if pinned {
        ACCENT
    } else {
        ui.style().interact(&response).fg_stroke.color
    };
    let center = response.rect.center();
    let point = |x, y| center + egui::vec2(x, y);
    if pinned {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(point(-5.0, -6.0), point(5.0, -4.0)),
            0,
            color,
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_max(point(-2.0, -4.0), point(2.0, 0.0)),
            0,
            color,
        );
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                point(-2.0, 0.0),
                point(2.0, 0.0),
                point(5.0, 4.0),
                point(-5.0, 4.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
    }
    let stroke = egui::Stroke::new(1.4_f32, color);
    ui.painter().add(egui::Shape::closed_line(
        vec![
            point(-5.0, -6.0),
            point(5.0, -6.0),
            point(5.0, -4.0),
            point(2.0, -4.0),
            point(2.0, 0.0),
            point(5.0, 4.0),
            point(-5.0, 4.0),
            point(-2.0, 0.0),
            point(-2.0, -4.0),
            point(-5.0, -4.0),
        ],
        stroke,
    ));
    ui.painter()
        .line_segment([point(0.0, 4.0), point(0.0, 8.0)], stroke);
    response
}

pub fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 760.0])
        .with_min_inner_size([640.0, 400.0])
        .with_clamp_size_to_monitor_size(true)
        .with_transparent(false)
        .with_decorations(true)
        .with_fullsize_content_view(false)
        .with_title_shown(true)
        .with_titlebar_buttons_shown(true)
        .with_titlebar_shown(true)
        .with_title(crate::i18n::key("app.title"))
}
fn append_system_font(fonts: &mut egui::FontDefinitions, name: &str, paths: &[&str]) {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            // Keep egui's Latin fonts first; these fonts supply missing scripts only.
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.into());
            }
            break;
        }
    }
}
fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(target_os = "macos")]
    {
        append_system_font(
            &mut fonts,
            "system-chinese",
            &[
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/STHeiti Light.ttc",
                "/System/Library/Fonts/STHeiti Medium.ttc",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
            ],
        );
        append_system_font(
            &mut fonts,
            "system-japanese",
            &["/System/Library/Fonts/Hiragino Sans GB.ttc"],
        );
        append_system_font(
            &mut fonts,
            "system-korean",
            &[
                "/System/Library/Fonts/AppleSDGothicNeo.ttc",
                "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
            ],
        );
    }
    #[cfg(target_os = "windows")]
    {
        append_system_font(
            &mut fonts,
            "system-chinese",
            &["C:/Windows/Fonts/msyh.ttc", "C:/Windows/Fonts/msyh.ttf"],
        );
        append_system_font(
            &mut fonts,
            "system-japanese",
            &[
                "C:/Windows/Fonts/YuGothM.ttc",
                "C:/Windows/Fonts/meiryo.ttc",
            ],
        );
        append_system_font(
            &mut fonts,
            "system-korean",
            &["C:/Windows/Fonts/malgun.ttf", "C:/Windows/Fonts/gulim.ttc"],
        );
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        append_system_font(
            &mut fonts,
            "system-cjk",
            &[
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            ],
        );
        append_system_font(
            &mut fonts,
            "system-korean",
            &["/usr/share/fonts/truetype/nanum/NanumGothic.ttf"],
        );
    }
    fonts
}
pub fn style(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    let mut s = (*ctx.style()).clone();
    s.visuals = egui::Visuals::light();
    s.visuals.panel_fill = Color32::TRANSPARENT;
    s.visuals.window_fill = Color32::from_rgb(249, 249, 247);
    s.visuals.extreme_bg_color = Color32::from_white_alpha(195);
    s.visuals.faint_bg_color = Color32::from_black_alpha(3);
    s.visuals.override_text_color = Some(TEXT);
    s.visuals.selection.bg_fill = Color32::from_rgb(218, 232, 244);
    s.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(39, 89, 137));
    s.visuals.widgets.active.bg_fill = Color32::from_rgb(204, 224, 244);
    s.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(204, 224, 244);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(226, 238, 249);
    s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(226, 238, 249);
    s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(246, 249, 252);
    s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(246, 249, 252);
    for widget in [
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
        &mut s.visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = egui::CornerRadius::same(5);
        widget.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(22));
        widget.fg_stroke.color = TEXT;
    }
    s.visuals.widgets.inactive.bg_stroke.color = Color32::from_rgb(150, 164, 178);
    s.visuals.widgets.hovered.bg_stroke.color = Color32::from_rgb(65, 118, 169);
    s.visuals.widgets.active.bg_stroke.color = ACCENT;
    s.visuals.window_corner_radius = egui::CornerRadius::same(12);
    s.visuals.window_stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(22));
    s.spacing.item_spacing = Vec2::new(12.0, 8.0);
    s.spacing.interact_size.y = 30.0;
    s.spacing.text_edit_width = 180.0;
    s.spacing.scroll = egui::style::ScrollStyle::solid();
    s.spacing.icon_width = 14.0;
    s.spacing.icon_width_inner = 8.0;
    s.visuals.widgets.noninteractive.bg_stroke.color = Color32::from_black_alpha(17);
    s.spacing.button_padding = Vec2::new(11.0, 7.0);
    s.text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    s.text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    s.text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(19.0));
    // Choose the app's light theme explicitly: set_style alone can configure the
    // fallback dark theme before Windows supplies its first system-theme input.
    ctx.set_theme(egui::Theme::Light);
    ctx.set_style_of(egui::Theme::Light, s);
}

pub fn surface(content: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(if content {
            Color32::from_rgba_unmultiplied(252, 252, 249, 20)
        } else {
            Color32::from_rgba_unmultiplied(247, 248, 247, 32)
        })
        .inner_margin(if content {
            egui::Margin::symmetric(14, 12)
        } else {
            egui::Margin::symmetric(14, 5)
        })
}

#[cfg(test)]
mod button_style_tests {
    use super::*;

    #[test]
    fn nested_scrollbars_reserve_separate_content_lanes() {
        for width in [400.0, 760.0] {
            let ctx = egui::Context::default();
            style(&ctx);
            let mut rectangles = None;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let outer = egui::ScrollArea::vertical()
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let inner = egui::ScrollArea::vertical()
                                    .max_height(120.0)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                    )
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.add_space(600.0);
                                    });
                                ui.add_space(600.0);
                                inner.inner_rect
                            });
                        rectangles = Some((outer.inner_rect, outer.inner));
                    });
                },
            );
            let (outer, inner) = rectangles.unwrap();
            let scroll = &ctx.style().spacing.scroll;
            assert!(!scroll.floating);
            assert!(
                inner.right() + scroll.bar_width + scroll.bar_inner_margin <= outer.right() + 0.1
            );
        }
    }

    #[test]
    fn buttons_keep_app_style_after_first_system_theme_input() {
        let ctx = egui::Context::default();
        style(&ctx);
        for system_theme in [egui::Theme::Light, egui::Theme::Dark] {
            let mut button_size = Vec2::ZERO;
            let mut border = egui::Stroke::NONE;
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: Some(system_theme),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        button_size = ui.button("Install MOD").rect.size();
                        border = ui.visuals().widgets.inactive.bg_stroke;
                    });
                },
            );
            println!("System theme {system_theme:?}: button={button_size:?}, border={border:?}");
            assert!(button_size.y >= 30.0, "App button spacing was replaced");
            assert_eq!(border.color, Color32::from_rgb(150, 164, 178));
        }
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;

    #[test]
    fn small_windows_and_high_dpi_keep_room_for_the_editor_without_zoom_feedback() {
        for (physical_size, dpi) in [
            (Vec2::new(1280.0, 720.0), 1.25),
            (Vec2::new(1366.0, 768.0), 1.5),
            (Vec2::new(1920.0, 1080.0), 1.5),
            (Vec2::new(3840.0, 2160.0), 2.0),
            (Vec2::new(640.0, 400.0), 1.0),
        ] {
            let ctx = egui::Context::default();
            let mut shell = Shell::default();
            let mut last_zoom: Option<f32> = None;
            for pass in 0..8 {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        physical_size / dpi / ctx.zoom_factor(),
                    )),
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(dpi);
                let _ = ctx.run(input, |ctx| {
                    shell.apply_layout(ctx);
                    egui::SidePanel::left("navigation")
                        .default_width(NAVIGATION_WIDTH)
                        .show(ctx, |ui| {
                            ui.label("Chapters");
                        });
                    egui::SidePanel::left("steps")
                        .default_width(168.0)
                        .show(ctx, |ui| {
                            ui.label("Steps");
                        });
                    egui::SidePanel::right("preview")
                        .default_width(PREVIEW_WIDTH)
                        .min_width(320.0)
                        .max_width((ctx.available_rect().width() - 300.0).max(320.0))
                        .show(ctx, |ui| {
                            ui.label("Preview");
                        });
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if pass >= 2 {
                            assert!(
                                ui.max_rect().width() >= 280.0,
                                "Form clipped at {physical_size:?}, DPI {dpi}"
                            );
                            assert!(ctx.content_rect().height() >= 750.0);
                        }
                    });
                });
                if pass >= 2 {
                    let zoom = ctx.zoom_factor();
                    if let Some(previous) = last_zoom {
                        assert!(
                            (zoom - previous).abs() < 0.005,
                            "Zoom oscillates at {physical_size:?}, DPI {dpi}"
                        );
                    }
                    last_zoom = Some(zoom);
                    assert_eq!(ctx.native_pixels_per_point(), Some(dpi));
                    assert!((ctx.pixels_per_point() - dpi * zoom).abs() < 0.001);
                }
            }
        }
    }

    #[test]
    fn manual_scale_survives_preference_roundtrip_and_preserves_other_settings() {
        let mut prefs = serde_json::json!({
            "ui_locale":"chs", "custom":"keep", "ui_auto_scale":false,
            "ui_scale_percent":150
        });
        let shell = Shell::from_preferences(&prefs);
        shell.store_resolution_preferences(&mut prefs);
        assert_eq!(prefs["ui_locale"], "chs");
        assert_eq!(prefs["custom"], "keep");
        let mut restored = Shell::from_preferences(&prefs);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |ctx| restored.apply_layout(ctx));
        }
        assert_eq!(ctx.zoom_factor(), 1.5);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn system_fallbacks_render_every_supported_language_without_replacing_latin_fonts() {
        let original = egui::Context::default();
        let configured = egui::Context::default();
        style(&configured);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let font = egui::FontId::new(14.0, family);
            let mut original_widths = Vec::new();
            let _ = original.run(Default::default(), |ctx| {
                ctx.fonts_mut(|fonts| {
                    original_widths = "LoM Modkit 0123 / JSON"
                        .chars()
                        .map(|c| fonts.glyph_width(&font, c))
                        .collect();
                });
            });
            let _ = configured.run(Default::default(), |ctx| {
                ctx.fonts_mut(|fonts| {
                    for text in [
                        "简体中文编辑器",
                        "繁體中文編輯器",
                        "日本語ひらがなカタカナ",
                        "한국어",
                    ] {
                        for character in text.chars() {
                            assert!(
                                fonts.has_glyph(&font, character),
                                "Missing {character:?} from {:?}",
                                font.family
                            );
                        }
                        // Exercise font selection and glyph rasterization, not just file loading.
                        let galley =
                            fonts.layout_no_wrap(text.into(), font.clone(), Color32::BLACK);
                        assert!(!galley.is_empty());
                    }
                    let actual_widths: Vec<_> = "LoM Modkit 0123 / JSON"
                        .chars()
                        .map(|c| fonts.glyph_width(&font, c))
                        .collect();
                    assert_eq!(
                        actual_widths, original_widths,
                        "System fallback must not replace Latin glyphs"
                    );
                });
            });
        }
    }
}
