//! 2D preview of the installed StoryCamera presets. RGB multipliers come from
//! the local VolumeProfiles; blur and vignette illustrate, rather than run, URP.
use eframe::egui::{self, Color32, ColorImage, Pos2, Rect};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    color: [f32; 3],
    blur: f32,
    vignette: Option<([f32; 3], f32, f32)>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            blur: 0.0,
            vignette: None,
        }
    }
}
impl Settings {
    pub fn from_state(state: &Value) -> Self {
        let mut settings = Self::default();
        for (name, enabled) in state["camera_filters"].as_object().into_iter().flatten() {
            if enabled != true {
                continue;
            }
            match name.as_str() {
                "stage-memory" => settings.color = [1.0592737, 0.948355, 0.6433285],
                "stage-fire" => settings.color = [2.0, 1.0267676, 0.9716982],
                "stage-dream" => {
                    settings.color = [1.1892071; 3];
                    settings.blur = 1.5;
                    settings.vignette = Some(([0.2924528; 3], 0.653, 0.23));
                }
                "stage-blurdim" => {
                    settings.color = [0.6415094; 3];
                    settings.blur = 2.0;
                    settings.vignette = Some(([0.0; 3], 0.35, 1.0));
                }
                _ => (),
            }
        }
        settings
    }
    pub fn cache_key(&self, source: &str) -> String {
        if *self == Self::default() {
            source.to_owned()
        } else {
            format!("{source}:camera:{self:?}")
        }
    }
    pub fn apply(&self, image: &mut ColorImage) {
        if *self == Self::default() {
            return;
        }
        let [width, height] = image.size;
        let mut linear = image::Rgba32FImage::new(width as u32, height as u32);
        for (target, source) in linear.pixels_mut().zip(&image.pixels) {
            let [r, g, b, a] = egui::Rgba::from(*source).to_array();
            *target = image::Rgba([r * self.color[0], g * self.color[1], b * self.color[2], a]);
        }
        let linear = if self.blur > 0.0 {
            image::imageops::blur(&linear, self.blur)
        } else {
            linear
        };
        for (target, source) in image.pixels.iter_mut().zip(linear.pixels()) {
            let [r, g, b, a] = source.0;
            *target = egui::Rgba::from_rgba_premultiplied(r, g, b, a).into();
        }
    }
    pub fn paint_vignette(&self, painter: &egui::Painter, rect: Rect) {
        let Some((color, intensity, smoothness)) = self.vignette else {
            return;
        };
        let mut mesh = egui::Mesh::default();
        const SECTORS: usize = 32;
        const RINGS: usize = 8;
        for ring in 0..=RINGS {
            let radius = ring as f32 / RINGS as f32;
            let alpha = ((radius - 0.25) / 0.75)
                .clamp(0.0, 1.0)
                .powf(1.0 + smoothness * 2.0)
                * intensity;
            let tint: Color32 = egui::Rgba::from_rgba_premultiplied(
                color[0] * alpha,
                color[1] * alpha,
                color[2] * alpha,
                alpha,
            )
            .into();
            for sector in 0..SECTORS {
                let phase = sector as f32 / SECTORS as f32 * std::f32::consts::TAU;
                mesh.colored_vertex(
                    Pos2::new(
                        rect.center().x + phase.cos() * rect.width() * radius * 0.72,
                        rect.center().y + phase.sin() * rect.height() * radius * 0.72,
                    ),
                    tint,
                );
            }
        }
        for ring in 0..RINGS {
            for sector in 0..SECTORS {
                let next = (sector + 1) % SECTORS;
                let a = (ring * SECTORS + sector) as u32;
                let b = (ring * SECTORS + next) as u32;
                let c = ((ring + 1) * SECTORS + sector) as u32;
                let d = ((ring + 1) * SECTORS + next) as u32;
                mesh.add_triangle(a, b, c);
                mesh.add_triangle(b, d, c);
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn filter_changes_pixels_and_cache_identity_without_losing_additive_alpha() {
        let state = json!({"camera_filters":{"stage-fire":true}});
        let settings = Settings::from_state(&state);
        let mut image = ColorImage::new([1, 1], vec![Color32::from_rgb(90, 90, 90)]);
        settings.apply(&mut image);
        assert!(image.pixels[0].r() > image.pixels[0].g());
        let mut light = ColorImage::new(
            [1, 1],
            vec![Color32::from_rgba_premultiplied(90, 40, 10, 0)],
        );
        settings.apply(&mut light);
        assert_eq!(light.pixels[0].a(), 0);
        assert!(light.pixels[0].r() > 0);
        assert_ne!(
            settings.cache_key("asset"),
            Settings::default().cache_key("asset")
        );
        assert_eq!(
            Settings::from_state(&json!({"camera_filters":{"stage-fire":false}})),
            Settings::default()
        );
    }
}
