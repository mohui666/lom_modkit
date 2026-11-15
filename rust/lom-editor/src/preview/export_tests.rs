use super::*;

fn paint(preview: &mut Preview, story: Value) -> egui::FullOutput {
    let ctx = egui::Context::default();
    let mut project = Project::new();
    project.stories.insert("main".into(), story);
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let story = &project.stories["main"];
                    preview.show(
                        ui,
                        &project,
                        story,
                        story["nodes"].as_array().unwrap().len() - 1,
                        None,
                    );
                });
        },
    )
}

#[test]
fn stage_export_preserves_dimming_and_excludes_death_editor_explanations() {
    let mut preview = Preview::new(Path::new(""));
    preview.stage_only = true;
    let _ = paint(
        &mut preview,
        json!({"id":"main","start":"show","nodes":[
        {"id":"show","type":"show","character":"player","position":"L1"},
        {"id":"dim","type":"dim","character":"player","mode":"manual","dimmed":true},
        {"id":"move","type":"move","character":"player","from":"L1","to":"R1","duration":1}]}),
    );
    assert_eq!(preview.stage_snapshot["actors"]["player"]["dimmed"], true);
    let output = paint(
        &mut preview,
        json!({"id":"main","start":"death","nodes":[
        {"id":"death","type":"death","death_id":"900001","title":"TITLE","text":"BODY"}]}),
    );
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("BODY"));
    assert!(
        !text.contains("900001"),
        "export leaked editor-only death instructions"
    );
}

#[test]
fn stage_export_samples_mood_animation_at_the_requested_time() {
    let root = tempfile::tempdir().unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(root.path().join("a.png"))
        .unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255]))
        .save(root.path().join("b.png"))
        .unwrap();
    let mut preview = Preview::new(root.path());
    preview.library = root.path().to_path_buf();
    preview.stage_only = true;
    preview.mapping["moods"] = json!({"shock":{"left":{"duration":1.0,"frames":[
        {"time":0,"file":"a.png"},{"time":0.5,"file":"b.png"}]}}});
    let ctx = egui::Context::default();
    let project = Project::new();
    let mut ids = Vec::new();
    for time in [0.1, 0.7] {
        preview.stage_time = Some(time);
        let _ = ctx.run(Default::default(), |ctx| {
            ids.push(
                preview
                    .mood_texture(ctx, &project, "player", "shock", false)
                    .unwrap()
                    .0
                    .id(),
            );
        });
    }
    assert_ne!(
        ids[0], ids[1],
        "time was ignored while exporting a bubble animation"
    );
}
