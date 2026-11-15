use super::*;

fn positions() -> Value {
    serde_json::from_str(include_str!("../../../../data/stage_positions.json")).unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

#[test]
fn special_slot_geometry_matches_native_image_contain_and_bottom_pivot() {
    let positions = positions();
    let stage = Vec2::new(1920.0, 1080.0);
    // These expected extents follow the installed holder sizes and native
    // Image's centred preserveAspect fit, independently of the PNG thumbnail.
    for (name, top, height) in [
        ("M", 3.0, 1080.0),
        ("BC1", -572.0, 1920.0),
        ("BC2", -837.0, 1920.0),
        ("BCB2", 1580.0, 1920.0),
        ("Talk", -134.0, 1245.0),
    ] {
        let (x, y) = position(&positions, name);
        let base = Pos2::new(x as f32 * stage.x, y as f32 * stage.y);
        let rect = portrait_rect(
            slot_size(&positions, name, stage),
            base,
            Vec2::new(1060.0, 1700.0),
            1.0,
            [0.0, 0.0, 100.0],
            1.0,
            &json!({}),
        );
        close(rect.top(), top);
        close(rect.height(), height);
        close(rect.center().x, base.x);
        close(rect.bottom(), base.y);
    }
}

#[test]
fn wide_portraits_use_the_special_slot_centre_without_stretching() {
    let positions = positions();
    let stage = Vec2::new(1920.0, 1080.0);
    for (name, width, top, height, centre_y) in [
        ("BC2", 3500.0, -423.875, 1093.75, 123.0),
        ("Talk", 1920.0, 188.5, 600.0, 488.5),
    ] {
        let (x, y) = position(&positions, name);
        let rect = portrait_rect(
            slot_size(&positions, name, stage),
            Pos2::new(x as f32 * stage.x, y as f32 * stage.y),
            Vec2::new(3200.0, 1000.0),
            1.0,
            [0.0, 0.0, 100.0],
            1.0,
            &json!({}),
        );
        close(rect.width(), width);
        close(rect.height(), height);
        close(rect.top(), top);
        close(rect.center().y, centre_y);
    }
    // Ordinary and historical metadata retain the released slot dimensions.
    let mut ordinary = 0;
    for (name, entry) in positions["positions"].as_object().unwrap() {
        if entry.get("size").is_none() {
            ordinary += 1;
            assert_eq!(slot_size(&positions, name, stage * 0.5), stage * 0.5);
        }
    }
    assert_eq!(ordinary, 32);
    assert_eq!(slot_size(&json!({}), "BC2", stage), stage);
    assert_eq!(slot_size(&positions, "c", stage), stage);
}

#[test]
fn special_slots_preserve_legacy_crop_offsets_and_calibration_in_both_directions() {
    let positions = positions();
    let stage = Vec2::new(960.0, 540.0);
    let canvas = Vec2::new(1000.0, 1700.0);
    let crop = Rect::from_min_size(Pos2::new(100.0, 300.0), Vec2::new(700.0, 1400.0));
    let meta = json!({"_preview_geometry":{"canvas":[1000,1700],"image":[100,300,700,1400]}});
    for name in ["M", "BC1", "BC2", "BCB2", "Talk"] {
        let slot = slot_size(&positions, name, stage);
        let (x, y) = position(&positions, name);
        let base = Pos2::new(x as f32 * stage.x, y as f32 * stage.y);
        for sign in [-1.0, 1.0] {
            let calibration = [20.0, -15.0, 110.0];
            let full = portrait_rect(slot, base, canvas, 1.25, calibration, sign, &json!({}));
            let restored = portrait_rect(slot, base, canvas * 0.5, 1.25, calibration, sign, &meta);
            close(restored.left(), full.left());
            close(restored.top(), full.top());
            close(restored.right(), full.right());
            close(restored.bottom(), full.bottom());
            let cropped = portrait_rect(
                slot,
                base,
                crop.size() * 0.5,
                1.25,
                calibration,
                sign,
                &meta,
            );
            let factor = full.height() / canvas.y;
            let x = if sign < 0.0 {
                canvas.x - crop.right()
            } else {
                crop.left()
            };
            let expected = Rect::from_min_size(
                full.min + Vec2::new(x, crop.top()) * factor,
                crop.size() * factor,
            );
            close(cropped.left(), expected.left());
            close(cropped.top(), expected.top());
            close(cropped.right(), expected.right());
            close(cropped.bottom(), expected.bottom());
        }
    }
}
