use lom_core::{gif_image, package, project::Project};
use serde_json::json;
use std::{fs, path::Path};

const GIF: &[u8] =
    include_bytes!("../../../samples/showcase3/assets/user/image/showcase.gif_lantern/lantern.gif");

#[test]
fn gif_keeps_canvas_transparency_and_per_frame_timing() {
    let animation = gif_image::decode(GIF, None).unwrap();
    assert_eq!(
        (animation.width, animation.height, animation.loops),
        (304, 196, 0)
    );
    assert_eq!(animation.frames.len(), 4);
    assert_eq!(
        animation
            .frames
            .iter()
            .map(|frame| (gif_image::frame_seconds(frame) * 1000.0).round() as u32)
            .collect::<Vec<_>>(),
        [120, 180, 90, 150]
    );
    assert_eq!(animation.frames[0].buffer().get_pixel(0, 0).0[3], 0);
    assert_ne!(animation.frames[0].buffer(), animation.frames[2].buffer());
    assert!(gif_image::validate(&GIF[..GIF.len() / 2]).is_err());
}

#[test]
fn old_package_can_add_a_gif_and_export_the_original_animation() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.4-name-base.lommod");
    let original = fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let story = project.stories.get_mut("main").unwrap();
    let first = story["start"].clone();
    story["nodes"].as_array_mut().unwrap().insert(
        0,
        json!({"id":"gif_bg","type":"background","action":"set","image":"user:test.gif","goto":first}),
    );
    story["start"] = json!("gif_bg");
    let folder = "assets/user/image/test.gif";
    let asset = format!("{folder}/lantern.GIF");
    project.assets.insert(asset.clone(), GIF.to_vec());
    project.assets.insert(
        format!("{folder}/content.json"),
        serde_json::to_vec(&json!({
            "schema":1,"id":"test.gif","type":"image","name":"动图","files":{"main":"lantern.GIF"}
        }))
        .unwrap(),
    );

    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let output = temp.path().join("animation.lommod");
    project.save_to(&source).unwrap();
    Project::open(&source).unwrap().export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.manifest["min_host_version"], "1.6.11");
    assert_eq!(reopened.assets[&asset], GIF);
    assert_eq!(reopened.stories, project.stories);
    assert_eq!(fs::read(fixture).unwrap(), original);

    let mut packed = package::read_package(&output).unwrap();
    packed.remove("package-content.sha256");
    packed
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua substitution')\n");
    let bad = temp.path().join("tampered.lommod");
    package::write_package(packed, &bad).unwrap();
    assert!(Project::open(&bad).is_err());
    project
        .assets
        .insert(asset.clone(), GIF[..GIF.len() / 2].to_vec());
    let error = project
        .export(&temp.path().join("broken.lommod"))
        .unwrap_err();
    assert!(format!("{error:#}").contains(&asset));
}
