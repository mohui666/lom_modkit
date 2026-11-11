//! 2D preview of the installed StoryCamera presets. RGB multipliers come from
//! the local VolumeProfiles; blur and vignette illustrate, rather than run, URP.
use eframe::egui::{self, Color32, ColorImage, Pos2, Rect};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    color: [f32; 3],
    color_max: [f32; 3],
    color_weight: f32,
    desaturation: f32,
    blur: f32,
    blur_max: f32,
    vignette: Option<([f32; 3], f32, f32)>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            color_max: [1.0; 3],
            color_weight: 0.0,
            desaturation: 0.0,
            blur: 0.0,
            blur_max: 0.0,
            vignette: None,
        }
    }
}
impl Settings {
    pub fn from_state(state: &Value) -> Self {
        let mut settings = Self::default();
        for (name, enabled) in state["camera_filters"].as_object().into_iter().flatten() {
            let weight = state["camera_filter_weights"][name]
                .as_f64()
                .unwrap_or(if enabled == true { 1.0 } else { 0.0 })
                .clamp(0.0, 1.0) as f32;
            if weight <= 0.0 {
                continue;
            }
            let mut preset = Self::default();
            match name.as_str() {
                "stage-gray-100" => {
                    settings.desaturation = weight;
                    continue;
                }
                "stage-memory" => preset.color = [1.0592737, 0.948355, 0.6433285],
                "stage-fire" => preset.color = [2.0, 1.0267676, 0.9716982],
                "stage-dream" => {
                    preset.color = [1.1892071; 3];
                    preset.blur = 1.5;
                    preset.vignette = Some(([0.2924528; 3], 0.653, 0.23));
                }
                "stage-blurdim" => {
                    preset.color = [0.6415094; 3];
                    preset.blur = 2.0;
                    preset.vignette = Some(([0.0; 3], 0.35, 1.0));
                }
                _ => (),
            }
            settings.color = preset.color.map(|color| 1.0 + (color - 1.0) * weight);
            settings.color_max = preset.color;
            settings.color_weight = weight;
            settings.blur = preset.blur * weight;
            settings.blur_max = preset.blur;
            settings.vignette = preset
                .vignette
                .map(|(color, intensity, smoothness)| (color, intensity * weight, smoothness));
        }
        settings
    }
    pub fn cache_key(&self, source: &str) -> String {
        if *self == Self::default() {
            source.to_owned()
        } else {
            // One mutable scene-filter texture per source. Animated weights must
            // not fill the LRU with a newly decoded image on every frame.
            format!("{source}:camera")
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
            let [r, g, b] = [r * self.color[0], g * self.color[1], b * self.color[2]];
            let luminance = r * 0.2126 + g * 0.7152 + b * 0.0722;
            let gray = |channel| channel + (luminance - channel) * self.desaturation;
            *target = image::Rgba([gray(r), gray(g), gray(b), a]);
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

/// Keep decoded start/end images and a reusable output buffer for a fade.
/// Intermediate frames crossfade premultiplied RGBA, including additive RGB at
/// alpha=0. Static endpoints retain the full linear-color/blur calculation.
pub struct FilterImage {
    source: ColorImage,
    tint: Option<(Settings, ColorImage)>,
    gray: Option<ColorImage>,
    tint_gray: Option<ColorImage>,
    output: std::sync::Arc<ColorImage>,
    rendered: Option<Settings>,
}
impl FilterImage {
    pub fn new(pixels: ColorImage) -> Self {
        Self {
            source: pixels.clone(),
            tint: None,
            gray: None,
            tint_gray: None,
            output: std::sync::Arc::new(pixels),
            rendered: None,
        }
    }
    pub fn bytes(&self) -> usize {
        (2 + usize::from(self.tint.is_some())
            + usize::from(self.gray.is_some())
            + usize::from(self.tint_gray.is_some()))
            * self.source.pixels.len()
            * std::mem::size_of::<Color32>()
    }
    pub fn pixels(&self) -> std::sync::Arc<ColorImage> {
        self.output.clone()
    }
    pub fn update(&mut self, settings: &Settings) -> bool {
        if self.rendered.as_ref().is_some_and(|old| {
            old.color == settings.color
                && old.desaturation == settings.desaturation
                && old.blur == settings.blur
                && old.blur_max == settings.blur_max
        }) {
            return false;
        }
        let tint_weight = (settings.color_weight * 256.0 + 0.5).clamp(0.0, 256.0) as u16;
        let gray_weight = (settings.desaturation * 256.0 + 0.5).clamp(0.0, 256.0) as u16;
        let endpoint = Settings {
            color: settings.color_max,
            blur: settings.blur_max,
            ..Settings::default()
        };
        if tint_weight > 0 && self.tint.as_ref().is_none_or(|(old, _)| *old != endpoint) {
            let mut pixels = self.source.clone();
            endpoint.apply(&mut pixels);
            self.tint = Some((endpoint.clone(), pixels));
            self.tint_gray = None;
        }
        if gray_weight > 0 && self.gray.is_none() {
            let mut pixels = self.source.clone();
            Settings {
                desaturation: 1.0,
                ..Settings::default()
            }
            .apply(&mut pixels);
            self.gray = Some(pixels);
        }
        if tint_weight > 0 && gray_weight > 0 && self.tint_gray.is_none() {
            let mut pixels = self.source.clone();
            Settings {
                desaturation: 1.0,
                ..endpoint
            }
            .apply(&mut pixels);
            self.tint_gray = Some(pixels);
        }
        let output = std::sync::Arc::make_mut(&mut self.output);
        match (tint_weight, gray_weight) {
            (0, 0) => output.pixels.copy_from_slice(&self.source.pixels),
            (weight, 0) => blend(output, &self.source, &self.tint.as_ref().unwrap().1, weight),
            (0, weight) => blend(output, &self.source, self.gray.as_ref().unwrap(), weight),
            (tint_weight, gray_weight) => {
                let tint = &self.tint.as_ref().unwrap().1;
                let gray = self.gray.as_ref().unwrap();
                let tint_gray = self.tint_gray.as_ref().unwrap();
                for (i, target) in output.pixels.iter_mut().enumerate() {
                    *target = mix(
                        mix(self.source.pixels[i], tint.pixels[i], tint_weight),
                        mix(gray.pixels[i], tint_gray.pixels[i], tint_weight),
                        gray_weight,
                    );
                }
            }
        }
        self.rendered = Some(settings.clone());
        true
    }
}

fn mix(from: Color32, to: Color32, weight: u16) -> Color32 {
    let from = from.to_array();
    let to = to.to_array();
    let channel = |i| ((from[i] as u16 * (256 - weight) + to[i] as u16 * weight + 128) >> 8) as u8;
    Color32::from_rgba_premultiplied(channel(0), channel(1), channel(2), channel(3))
}
fn blend(output: &mut ColorImage, from: &ColorImage, to: &ColorImage, weight: u16) {
    for ((target, from), to) in output.pixels.iter_mut().zip(&from.pixels).zip(&to.pixels) {
        *target = mix(*from, *to, weight);
    }
}

pub fn excludes(state: &Value, character: &str) -> bool {
    state["camera_filter_exclusions"]
        .as_object()
        .into_iter()
        .flatten()
        .any(|(name, ids)| {
            state["camera_filters"][name] == true
                && ids
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == character))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cached_endpoints_match_static_pixels_for_all_presets_and_combined_gray() {
        let original = ColorImage::new(
            [3, 2],
            vec![
                Color32::from_rgb(23, 128, 220),
                Color32::from_rgba_unmultiplied(230, 40, 80, 71),
                Color32::from_rgba_premultiplied(40, 70, 10, 0),
                Color32::TRANSPARENT,
                Color32::WHITE,
                Color32::BLACK,
            ],
        );
        for preset in [
            "stage-memory",
            "stage-fire",
            "stage-dream",
            "stage-blurdim",
            "stage-gray-100",
        ] {
            for gray in [false, true] {
                let mut state = json!({"camera_filters":{preset:true}});
                if gray {
                    state["camera_filters"]["stage-gray-100"] = json!(true);
                }
                let settings = Settings::from_state(&state);
                let mut expected = original.clone();
                settings.apply(&mut expected);
                let mut cached = FilterImage::new(original.clone());
                assert!(cached.update(&settings));
                assert_eq!(*cached.pixels(), expected, "{preset} gray={gray}");
                assert!(
                    !cached.update(&settings),
                    "static pixels must not upload again"
                );
            }
        }
    }

    #[test]
    fn fade_interpolates_same_source_alpha_additive_and_combined_filter_endpoints() {
        let source = ColorImage::new(
            [3, 1],
            vec![
                Color32::from_rgba_unmultiplied(210, 50, 20, 73),
                Color32::from_rgba_premultiplied(80, 20, 10, 0),
                Color32::TRANSPARENT,
            ],
        );
        let tint = Settings::from_state(&json!({"camera_filters":{"stage-memory":true}}));
        let gray = Settings::from_state(&json!({"camera_filters":{"stage-gray-100":true}}));
        let both = Settings::from_state(
            &json!({"camera_filters":{"stage-memory":true,"stage-gray-100":true}}),
        );
        let mut end_tint = source.clone();
        tint.apply(&mut end_tint);
        let mut end_gray = source.clone();
        gray.apply(&mut end_gray);
        let mut end_both = source.clone();
        both.apply(&mut end_both);
        let settings = Settings::from_state(&json!({
            "camera_filters":{"stage-memory":true,"stage-gray-100":true},
            "camera_filter_weights":{"stage-memory":0.5,"stage-gray-100":0.25}
        }));
        let mut cached = FilterImage::new(source.clone());
        cached.update(&settings);
        let result = cached.pixels();
        for index in 0..3 {
            assert_eq!(
                result.pixels[index],
                mix(
                    mix(source.pixels[index], end_tint.pixels[index], 128),
                    mix(end_gray.pixels[index], end_both.pixels[index], 128),
                    64
                )
            );
            assert_eq!(result.pixels[index].a(), source.pixels[index].a());
        }
        assert!(result.pixels[1].r() > 0, "alpha=0 additive light survives");
        let buffer = result.pixels.as_ptr();
        drop(result);
        cached.update(&tint);
        assert_eq!(
            cached.pixels().pixels.as_ptr(),
            buffer,
            "reuse the output allocation"
        );
    }

    #[test]
    fn blur_fade_interpolates_alpha_once_instead_of_source_over() {
        let mut source = ColorImage::new([9, 9], vec![Color32::TRANSPARENT; 81]);
        source.pixels[40] = Color32::from_rgb(40, 110, 220);
        let full = Settings::from_state(&json!({"camera_filters":{"stage-dream":true}}));
        let half = Settings::from_state(
            &json!({"camera_filters":{"stage-dream":true},"camera_filter_weights":{"stage-dream":0.5}}),
        );
        let mut endpoint = source.clone();
        full.apply(&mut endpoint);
        let mut cached = FilterImage::new(source.clone());
        cached.update(&half);
        let result = cached.pixels();
        for index in 0..81 {
            assert_eq!(
                result.pixels[index],
                mix(source.pixels[index], endpoint.pixels[index], 128)
            );
        }
        assert!(result.pixels[40].a() < 255);
        assert!(result.pixels[39].a() > 0);
    }

    #[test]
    fn grayscale_matches_full_desaturation_fades_and_preserves_alpha() {
        let original = Color32::from_rgb(30, 140, 220);
        let full = Settings::from_state(&json!({"camera_filters":{"stage-gray-100":true}}));
        let mut image = ColorImage::new([1, 1], vec![original]);
        full.apply(&mut image);
        assert_eq!(image.pixels[0].r(), image.pixels[0].g());
        assert_eq!(image.pixels[0].g(), image.pixels[0].b());
        assert_eq!(image.pixels[0].a(), original.a());
        let half = Settings::from_state(
            &json!({"camera_filters":{"stage-gray-100":true},"camera_filter_weights":{"stage-gray-100":0.5}}),
        );
        let mut image = ColorImage::new([1, 1], vec![original]);
        half.apply(&mut image);
        assert!(image.pixels[0].b() > image.pixels[0].r());
        assert!(image.pixels[0].b() - image.pixels[0].r() < original.b() - original.r());
        let mut additive = ColorImage::new(
            [1, 1],
            vec![Color32::from_rgba_premultiplied(20, 50, 90, 0)],
        );
        full.apply(&mut additive);
        assert_eq!(additive.pixels[0].a(), 0);
        assert_eq!(additive.pixels[0].r(), additive.pixels[0].b());
        assert!(additive.pixels[0].r() > 0);
        assert_eq!(
            Settings::from_state(&json!({"camera_filters":{"stage-gray-100":false}})),
            Settings::default()
        );
    }

    #[test]
    fn intermediate_filter_weight_interpolates_tint_and_blur() {
        let half = Settings::from_state(
            &json!({"camera_filters":{"stage-dream":true},"camera_filter_weights":{"stage-dream":0.5}}),
        );
        let full = Settings::from_state(&json!({"camera_filters":{"stage-dream":true}}));
        assert!(half.color[0] > 1.0 && half.color[0] < full.color[0]);
        assert_eq!(half.blur, full.blur * 0.5);
        let clear = Settings::from_state(
            &json!({"camera_filters":{"stage-dream":true},"camera_filter_weights":{"stage-dream":0}}),
        );
        assert_eq!(clear, Settings::default());
    }

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
