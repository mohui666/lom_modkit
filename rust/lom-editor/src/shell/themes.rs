use super::{egui, Color32, Vec2, ACCENT, TEXT};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Theme {
    #[default]
    Classic,
    Light,
    Dark,
    WarmPaper,
    Bamboo,
}
impl Theme {
    pub const ALL: [Self; 5] = [
        Self::Classic,
        Self::Light,
        Self::Dark,
        Self::WarmPaper,
        Self::Bamboo,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::WarmPaper => "warm-paper",
            Self::Bamboo => "bamboo",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Classic => "经典",
            Self::Light => "浅色",
            Self::Dark => "深色",
            Self::WarmPaper => "暖纸色",
            Self::Bamboo => "竹青色",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.id() == id)
            .unwrap_or_default()
    }

    pub fn clear_color(self) -> [f32; 4] {
        if self == Self::Classic {
            [0.94, 0.94, 0.92, 1.0]
        } else {
            self.palette().canvas.to_normalized_gamma_f32()
        }
    }

    fn palette(self) -> Palette {
        let rgb = Color32::from_rgb;
        match self {
            Self::Classic | Self::Light => Palette {
                canvas: rgb(238, 242, 246),
                surface: rgb(255, 255, 255),
                text: rgb(36, 49, 61),
                muted: rgb(90, 106, 120),
                input: rgb(255, 255, 255),
                faint: rgb(244, 247, 250),
                button: rgb(244, 247, 251),
                hover: rgb(230, 240, 250),
                active: rgb(210, 230, 246),
                border: rgb(179, 193, 206),
                hover_border: rgb(57, 123, 183),
                accent: rgb(36, 110, 168),
                selection: rgb(205, 223, 243),
                selection_text: rgb(20, 76, 119),
                warning: rgb(142, 94, 12),
                error: rgb(181, 51, 51),
            },
            Self::Dark => Palette {
                canvas: rgb(23, 28, 36),
                surface: rgb(32, 40, 50),
                text: rgb(231, 238, 245),
                muted: rgb(173, 185, 200),
                input: rgb(20, 26, 34),
                faint: rgb(42, 51, 64),
                button: rgb(42, 53, 67),
                hover: rgb(52, 75, 99),
                active: rgb(53, 95, 131),
                border: rgb(95, 115, 139),
                hover_border: rgb(126, 177, 227),
                accent: rgb(141, 196, 242),
                selection: rgb(50, 90, 126),
                selection_text: rgb(238, 248, 255),
                warning: rgb(245, 199, 133),
                error: rgb(255, 155, 155),
            },
            Self::WarmPaper => Palette {
                canvas: rgb(233, 226, 211),
                surface: rgb(250, 244, 231),
                text: rgb(79, 68, 54),
                muted: rgb(120, 105, 84),
                input: rgb(255, 249, 238),
                faint: rgb(242, 233, 216),
                button: rgb(245, 237, 223),
                hover: rgb(238, 224, 197),
                active: rgb(230, 209, 173),
                border: rgb(182, 169, 144),
                hover_border: rgb(167, 131, 77),
                accent: rgb(138, 99, 47),
                selection: rgb(232, 212, 173),
                selection_text: rgb(95, 64, 34),
                warning: rgb(143, 90, 21),
                error: rgb(165, 66, 50),
            },
            Self::Bamboo => Palette {
                canvas: rgb(224, 235, 229),
                surface: rgb(242, 248, 242),
                text: rgb(40, 71, 59),
                muted: rgb(87, 113, 101),
                input: rgb(250, 255, 250),
                faint: rgb(231, 241, 233),
                button: rgb(234, 243, 235),
                hover: rgb(217, 234, 221),
                active: rgb(198, 223, 204),
                border: rgb(155, 184, 168),
                hover_border: rgb(92, 143, 118),
                accent: rgb(44, 117, 85),
                selection: rgb(197, 222, 204),
                selection_text: rgb(37, 78, 54),
                warning: rgb(138, 100, 30),
                error: rgb(164, 69, 59),
            },
        }
    }
}

struct Palette {
    canvas: Color32,
    surface: Color32,
    text: Color32,
    muted: Color32,
    input: Color32,
    faint: Color32,
    button: Color32,
    hover: Color32,
    active: Color32,
    border: Color32,
    hover_border: Color32,
    accent: Color32,
    selection: Color32,
    selection_text: Color32,
    warning: Color32,
    error: Color32,
}

fn classic_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = Color32::from_rgb(249, 249, 247);
    visuals.extreme_bg_color = Color32::from_white_alpha(195);
    visuals.faint_bg_color = Color32::from_black_alpha(3);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(218, 232, 244);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(39, 89, 137));
    visuals.widgets.active.bg_fill = Color32::from_rgb(204, 224, 244);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(204, 224, 244);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(226, 238, 249);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(226, 238, 249);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(246, 249, 252);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(246, 249, 252);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = egui::CornerRadius::same(5);
        widget.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(22));
        widget.fg_stroke.color = TEXT;
    }
    visuals.widgets.inactive.bg_stroke.color = Color32::from_rgb(150, 164, 178);
    visuals.widgets.hovered.bg_stroke.color = Color32::from_rgb(65, 118, 169);
    visuals.widgets.active.bg_stroke.color = ACCENT;
    visuals.widgets.noninteractive.bg_stroke.color = Color32::from_black_alpha(17);
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(22));
    visuals
}

fn themed_visuals(theme: Theme) -> egui::Visuals {
    if theme == Theme::Classic {
        return classic_visuals();
    }
    let palette = theme.palette();
    let mut visuals = if theme == Theme::Dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.canvas;
    visuals.window_fill = palette.surface;
    visuals.override_text_color = Some(palette.text);
    visuals.weak_text_color = Some(palette.muted);
    visuals.extreme_bg_color = palette.input;
    visuals.text_edit_bg_color = Some(palette.input);
    visuals.faint_bg_color = palette.faint;
    visuals.code_bg_color = palette.faint;
    visuals.hyperlink_color = palette.accent;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.error;
    visuals.selection.bg_fill = palette.selection;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, palette.selection_text);
    visuals.text_cursor.stroke.color = palette.accent;
    for (widget, fill, stroke) in [
        (
            &mut visuals.widgets.noninteractive,
            palette.surface,
            palette.border,
        ),
        (
            &mut visuals.widgets.inactive,
            palette.button,
            palette.border,
        ),
        (
            &mut visuals.widgets.hovered,
            palette.hover,
            palette.hover_border,
        ),
        (&mut visuals.widgets.active, palette.active, palette.accent),
        (&mut visuals.widgets.open, palette.active, palette.accent),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = egui::Stroke::new(1.0_f32, stroke);
        widget.fg_stroke.color = palette.text;
        widget.corner_radius = egui::CornerRadius::same(5);
    }
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, palette.border);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if theme == Theme::Dark { 90 } else { 36 }),
    };
    visuals
}

/// Switch colors without reloading the installed fonts. Each context owns its
/// visuals; another editor or a parallel test retains its own selected theme.
pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let egui_theme = if theme == Theme::Dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(egui_theme);
    let mut style = (*ctx.style()).clone();
    style.visuals = themed_visuals(theme);
    style.spacing.item_spacing = Vec2::new(12.0, 8.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.text_edit_width = 180.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.icon_width = 14.0;
    style.spacing.icon_width_inner = 8.0;
    style.spacing.button_padding = Vec2::new(11.0, 7.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(19.0));
    ctx.set_style_of(egui_theme, style);
    ctx.request_repaint();
}

pub fn accent_for(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.widgets.active.bg_stroke.color
}

pub fn surface_for(ctx: &egui::Context, content: bool) -> egui::Frame {
    let visuals = &ctx.style().visuals;
    if visuals.panel_fill == Color32::TRANSPARENT {
        return super::surface(content);
    }
    super::surface(content).fill(if content {
        visuals.window_fill
    } else {
        visuals.panel_fill
    })
}

pub fn dialog_frame_for(ctx: &egui::Context) -> egui::Frame {
    let visuals = &ctx.style().visuals;
    if visuals.panel_fill == Color32::TRANSPARENT {
        return super::dialog_frame();
    }
    super::dialog_frame()
        .fill(visuals.window_fill)
        .stroke(visuals.window_stroke)
        .shadow(visuals.window_shadow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(foreground: Color32, background: Color32) -> f32 {
        let luminance = |color: Color32| {
            let channel = |value: u8| {
                let value = value as f32 / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            let [r, g, b, _] = color.to_array();
            0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
        };
        let a = luminance(foreground);
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn stable_ids_round_trip_and_unknown_preferences_keep_the_classic_theme() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_id(theme.id()), theme);
            assert!(!theme.label().is_empty());
        }
        assert_eq!(Theme::from_id(""), Theme::Classic);
        assert_eq!(Theme::from_id("future-theme"), Theme::Classic);
    }

    #[test]
    fn theme_switches_update_surfaces_widgets_and_text_with_readable_contrast() {
        let ctx = egui::Context::default();
        for theme in Theme::ALL {
            apply_theme(&ctx, theme);
            let style = ctx.style();
            let visuals = &style.visuals;
            assert_eq!(visuals.dark_mode, theme == Theme::Dark);
            for background in [
                visuals.window_fill,
                visuals.widgets.inactive.bg_fill,
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.active.bg_fill,
            ] {
                assert!(
                    contrast(visuals.text_color(), background) >= 4.5,
                    "{theme:?}: {background:?}"
                );
            }
            assert!(contrast(visuals.selection.stroke.color, visuals.selection.bg_fill) >= 4.5);
            if theme == Theme::Classic {
                assert_eq!(
                    surface_for(&ctx, true).fill,
                    super::super::surface(true).fill
                );
                assert_eq!(
                    dialog_frame_for(&ctx).fill,
                    super::super::dialog_frame().fill
                );
                assert_eq!(
                    visuals.widgets.inactive.bg_stroke.color,
                    Color32::from_rgb(150, 164, 178)
                );
            } else {
                assert_eq!(surface_for(&ctx, true).fill, visuals.window_fill);
                assert_eq!(surface_for(&ctx, false).fill, visuals.panel_fill);
                assert_eq!(dialog_frame_for(&ctx).fill, visuals.window_fill);
                assert_eq!(
                    theme.clear_color(),
                    visuals.panel_fill.to_normalized_gamma_f32()
                );
            }
        }
    }

    #[test]
    fn themes_remain_context_local_and_system_theme_input_does_not_override_selection() {
        let dark = egui::Context::default();
        let classic = egui::Context::default();
        apply_theme(&dark, Theme::Dark);
        apply_theme(&classic, Theme::Classic);
        let classic_fill = classic.style().visuals.window_fill;
        for theme in [Theme::Dark, Theme::WarmPaper, Theme::Bamboo, Theme::Light] {
            apply_theme(&dark, theme);
            let expected = dark.style().visuals.window_fill;
            let _ = dark.run(
                egui::RawInput {
                    system_theme: Some(egui::Theme::Light),
                    ..Default::default()
                },
                |_| {},
            );
            assert_eq!(dark.style().visuals.window_fill, expected);
            assert_eq!(classic.style().visuals.window_fill, classic_fill);
            assert!(!classic.style().visuals.dark_mode);
        }
    }
}
