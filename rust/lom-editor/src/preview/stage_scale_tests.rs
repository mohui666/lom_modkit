use super::*;

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

fn project() -> Project {
    let mut project = Project::new();
    let image = image::RgbaImage::from_pixel(80, 120, image::Rgba([180, 95, 60, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    project.assets.insert("assets/user/image/scaled/content.json".into(),
        serde_json::to_vec(&json!({"id":"scaled","type":"image","name":"scaled image","files":{"main":"image.png"}})).unwrap());
    project
        .assets
        .insert("assets/user/image/scaled/image.png".into(), png.clone());
    project.assets.insert(
        "assets/user/character/scaled.actor/content.json".into(),
        serde_json::to_vec(
            &json!({"id":"scaled.actor","type":"character","name":"scaled actor","scale":100,
            "files":{"main":"normal.png"},"portraits":{"normal":"normal.png"}}),
        )
        .unwrap(),
    );
    project
        .assets
        .insert("assets/user/character/scaled.actor/normal.png".into(), png);
    project
}

struct Drawing {
    image: Rect,
    clip: Rect,
    book: Option<Rect>,
    title: Option<Pos2>,
}

fn render(project: &Project, mut node: Value, scale: Option<u32>, character: bool) -> Drawing {
    if let Some(scale) = scale {
        node["scale"] = json!(scale);
    }
    let story = json!({"id":"main","start":"scaled","mood":false,"nodes":[node]});
    let ctx = egui::Context::default();
    let mut preview = Preview::new(Path::new("."));
    preview.mapping = json!({});
    let mut image_id = None;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 820.0))),
            ..Default::default()
        },
        |ctx| {
            image_id = Some(
                preview
                    .texture(
                        ctx,
                        project,
                        if character {
                            "user:scaled.actor"
                        } else {
                            "user:scaled"
                        },
                        character,
                        "normal",
                    )
                    .unwrap()
                    .0
                    .id(),
            );
            egui::CentralPanel::default().show(ctx, |ui| {
                preview.show(ui, project, &story, 0, None);
            });
        },
    );
    let image_id = image_id.unwrap();
    let (image, clip) = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Mesh(mesh) = &shape.shape {
                if mesh.texture_id == image_id {
                    let points: Vec<_> = mesh.vertices.iter().map(|vertex| vertex.pos).collect();
                    return Some((Rect::from_points(&points), shape.clip_rect));
                }
            }
            None
        })
        .expect("the source image must be drawn");
    let book = output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(220, 207, 176) => Some(rect.rect),
        _ => None,
    });
    let title = output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == "固\n定\n题\n字" => Some(text.pos),
        _ => None,
    });
    Drawing {
        image,
        clip,
        book,
        title,
    }
}

#[test]
fn show_scale_is_preserved_for_later_steps_and_playtest_and_resets_on_next_show() {
    let story = json!({"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"player","scale":150},
        {"id":"move","type":"move","character":"player","to":"L1"},
        {"id":"portrait","type":"portrait","character":"player","portrait":"happy"},
        {"id":"line","type":"say","character":"player","text":"继续"},
        {"id":"again","type":"show","character":"player"},
        {"id":"end","type":"end"} ]});
    let state = simulate(&story, "line");
    assert_eq!(state["actors"]["player"]["layout_scale"], 150);
    let prelude = build_playtest_prelude(&story, "line");
    assert_eq!(
        prelude.iter().find(|node| node["type"] == "show").unwrap()["scale"],
        150
    );
    assert_eq!(
        simulate(&story, "again")["actors"]["player"]["layout_scale"],
        100
    );
    assert!(build_playtest_prelude(&story, "end")
        .iter()
        .find(|node| node["type"] == "show")
        .unwrap()
        .get("scale")
        .is_none());

    let mut parallel = story.clone();
    parallel["nodes"][0] = json!({"id":"show","type":"parallel","actions":[
        {"type":"show","character":"player","scale":50},
        {"type":"show","character":"user:scaled.actor","scale":150} ]});
    let state = simulate(&parallel, "line");
    assert_eq!(state["actors"]["player"]["layout_scale"], 50);
    assert_eq!(state["actors"]["user:scaled.actor"]["layout_scale"], 150);
}

#[test]
fn stage_pictures_scale_the_image_without_scaling_the_book_or_title() {
    let project = project();
    for kind in ["picture", "item", "big", "event"] {
        let node = json!({"id":"scaled","type":"cg","kind":kind,"action":"show","key":"user:scaled",
            "color":2,"event_title":"固定题字"});
        let original = render(&project, node.clone(), None, false);
        let explicit = render(&project, node.clone(), Some(100), false);
        assert_eq!(
            original.image, explicit.image,
            "omitted scale keeps the released size: {kind}"
        );
        assert_eq!(original.clip, explicit.clip);
        for percent in [50, 150] {
            let scaled = render(&project, node.clone(), Some(percent), false);
            close(
                scaled.image.width(),
                original.image.width() * percent as f32 / 100.0,
            );
            close(
                scaled.image.height(),
                original.image.height() * percent as f32 / 100.0,
            );
            close(scaled.image.center().x, original.image.center().x);
            close(scaled.image.center().y, original.image.center().y);
            assert_eq!(
                scaled.clip, original.clip,
                "only event's existing mask clips the scaled image"
            );
            assert_eq!(
                scaled.book, original.book,
                "book frame stays at 100%: {kind}"
            );
            assert_eq!(
                scaled.title, original.title,
                "caption stays at 100%: {kind}"
            );
        }
        if kind == "event" {
            assert!(original.book.is_some());
            assert!(original.title.is_some());
            assert!(original.clip.width() < original.book.unwrap().width());
        }
    }
}

#[test]
fn stage_character_show_size_changes_around_the_same_feet_position() {
    let project = project();
    let node = json!({"id":"scaled","type":"show","character":"user:scaled.actor","portrait":"normal","position":"M"});
    let original = render(&project, node.clone(), None, true);
    assert_eq!(
        original.image,
        render(&project, node.clone(), Some(100), true).image
    );
    for percent in [50, 150] {
        let scaled = render(&project, node.clone(), Some(percent), true);
        close(
            scaled.image.width(),
            original.image.width() * percent as f32 / 100.0,
        );
        close(
            scaled.image.height(),
            original.image.height() * percent as f32 / 100.0,
        );
        close(scaled.image.bottom(), original.image.bottom());
        close(scaled.image.center().x, original.image.center().x);
    }
}
