//! Shared Rust window shell. Platform chrome is isolated from authoring state.
use eframe::egui::{self, Color32, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(41, 101, 158);
const TEXT: Color32 = Color32::from_rgb(45, 51, 59);
pub const NAVIGATION_WIDTH: f32 = 280.0;
pub const PREVIEW_WIDTH: f32 = 560.0;

#[derive(Default)]
pub struct Shell {
    backend: Option<String>,
    title: String,
}
impl Shell {
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
pub fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 760.0])
        .with_min_inner_size([900.0, 560.0])
        .with_transparent(false)
        .with_decorations(true)
        .with_fullsize_content_view(false)
        .with_title_shown(true)
        .with_titlebar_buttons_shown(true)
        .with_titlebar_shown(true)
        .with_title("活侠传剧情编辑器")
}
pub fn style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "C:/Windows/Fonts/msyh.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("chinese".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("chinese".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("chinese".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
    let mut s = (*ctx.style()).clone();
    s.visuals = egui::Visuals::light();
    s.visuals.panel_fill = Color32::TRANSPARENT;
    s.visuals.window_fill = Color32::from_rgb(249, 249, 247);
    s.visuals.extreme_bg_color = Color32::from_white_alpha(195);
    s.visuals.faint_bg_color = Color32::from_black_alpha(3);
    s.visuals.override_text_color = Some(TEXT);
    s.visuals.selection.bg_fill = Color32::from_rgb(218, 232, 244);
    s.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(39, 89, 137));
    s.visuals.widgets.active.bg_fill = Color32::from_rgb(222, 231, 240);
    s.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(222, 231, 240);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 239, 243);
    s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(235, 239, 243);
    s.visuals.widgets.inactive.bg_fill = Color32::from_white_alpha(195);
    s.visuals.widgets.inactive.weak_bg_fill = Color32::from_white_alpha(80);
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
    s.visuals.widgets.active.bg_stroke.color = Color32::from_rgb(92, 142, 188);
    s.visuals.window_corner_radius = egui::CornerRadius::same(12);
    s.visuals.window_stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(22));
    s.spacing.item_spacing = Vec2::new(8.0, 6.0);
    s.spacing.interact_size.y = 26.0;
    s.spacing.text_edit_width = 180.0;
    s.spacing.icon_width = 14.0;
    s.spacing.icon_width_inner = 8.0;
    s.visuals.widgets.noninteractive.bg_stroke.color = Color32::from_black_alpha(17);
    s.spacing.button_padding = Vec2::new(9.0, 5.0);
    s.text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    s.text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    s.text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(19.0));
    ctx.set_style(s);
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
