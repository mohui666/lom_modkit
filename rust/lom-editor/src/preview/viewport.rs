use super::*;

pub(super) fn fit(outer: Rect, resolution: [u32; 2]) -> Rect {
    let pixels = Vec2::new(resolution[0].max(1) as f32, resolution[1].max(1) as f32);
    let factor = (outer.width() / pixels.x).min(outer.height() / pixels.y);
    Rect::from_center_size(outer.center(), pixels * factor)
}

/// Story's UI Canvas uses Shrink and all named slots have top-centre anchors.
/// Keep its reference units isotropic; a wide viewport crops the lower canvas.
pub(super) fn character_canvas(viewport: Rect) -> Rect {
    let unit = (viewport.width() / 1920.0).max(viewport.height() / 1080.0);
    let size = Vec2::new(1920.0, 1080.0) * unit;
    Rect::from_min_size(
        Pos2::new(viewport.center().x - size.x / 2.0, viewport.top()),
        size,
    )
}

pub(super) fn background(viewport: Rect, image: Vec2, custom: bool) -> Rect {
    let size = if custom {
        // CustomImageRuntime.ShowBackground uses EnvelopeParent.
        image * (viewport.width() / image.x).max(viewport.height() / image.y)
    } else {
        // BackCamera keeps orthographicSize=5.4: native 1080px-high views
        // preserve their height and can be horizontally cropped or pillarboxed.
        image * (viewport.height() / image.y)
    };
    Rect::from_center_size(viewport.center(), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
    }

    #[test]
    fn chosen_resolution_fits_the_panel_without_changing_its_aspect() {
        let outer = Rect::from_min_size(Pos2::new(50.0, 20.0), Vec2::new(900.0, 600.0));
        for resolution in [[1920, 1080], [1280, 1024], [1024, 768], [1080, 1920]] {
            let rect = fit(outer, resolution);
            close(
                rect.aspect_ratio(),
                resolution[0] as f32 / resolution[1] as f32,
            );
            assert!(outer.contains_rect(rect));
            assert_eq!(rect.center(), outer.center());
        }
    }

    #[test]
    fn native_camera_and_top_anchored_canvas_match_tall_and_wide_viewports() {
        let positions: Value =
            serde_json::from_str(include_str!("../../../../data/stage_positions.json")).unwrap();
        for (size, left, ui_height) in [
            (Vec2::new(1280.0, 1024.0), -270.2222, 1024.0),
            (Vec2::new(1024.0, 768.0), -170.6667, 768.0),
            (Vec2::new(2560.0, 1080.0), 320.0, 1440.0),
        ] {
            let viewport = Rect::from_min_size(Pos2::ZERO, size);
            let bg = background(viewport, Vec2::new(1920.0, 1080.0), false);
            close(bg.left(), left);
            close(bg.height(), size.y);
            let canvas = character_canvas(viewport);
            close(canvas.height(), ui_height);
            close(canvas.top(), 0.0);
            let (x, y) = position(&positions, "M");
            let pivot = canvas.min + canvas.size() * Vec2::new(x as f32, y as f32);
            let bounds = portrait_rect(
                slot_size(&positions, "M", canvas.size()),
                pivot,
                Vec2::new(1920.0, 1080.0),
                1.0,
                [0.0, 0.0, 100.0],
                1.0,
                &json!({}),
            );
            close(bounds.center().y, (y as f32 - 0.5) * ui_height);
        }
        let default = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
        assert_eq!(character_canvas(default), default);
    }

    #[test]
    fn custom_background_keeps_its_shape_while_covering_the_viewport() {
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
        let bounds = background(viewport, Vec2::splat(1000.0), true);
        assert!(bounds.contains_rect(viewport));
        close(bounds.width(), bounds.height());
        assert_eq!(bounds.center(), viewport.center());
    }
}
