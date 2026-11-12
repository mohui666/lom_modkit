use lom_core::{content_edit, package, project::Project, release};
use serde_json::json;
use std::path::Path;

fn picture_project(content_type: &str, picture_kind: &str) -> Project {
    let mut project = Project::new();
    project.manifest["min_host_version"] = json!("1.4.1");
    let mut meta = json!({"schema":1,"content_schema":1,"id":"demo.art","type":content_type,
        "name":"自带图片","files":{"main":"icon.png"}});
    if content_type == "martial" {
        meta["levels"] = json!([{"points":10}]);
    }
    let prefix = format!("assets/user/{content_type}/demo.art");
    project.assets.insert(
        format!("{prefix}/content.json"),
        serde_json::to_vec(&meta).unwrap(),
    );
    project.assets.insert(
        format!("{prefix}/icon.png"),
        include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
        )
        .to_vec(),
    );
    project.stories.insert(
        "main".into(),
        json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"cg","kind":picture_kind,"action":"show","key":"user:demo.art"},
            {"id":"hide","type":"cg","kind":picture_kind,"action":"hide"},
            {"id":"end","type":"end"}
        ]}),
    );
    project
}

#[test]
fn every_story_picture_packages_and_protects_images_and_item_manual_icons() {
    let root = tempfile::tempdir().unwrap();
    for content_type in ["image", "item", "martial"] {
        for kind in ["picture", "item", "event", "big"] {
            let mut project = picture_project(content_type, kind);
            let key = format!("assets/user/{content_type}/demo.art/content.json");
            assert!(content_edit::remove(&mut project, &key).is_err());
            let diagnostics = release::run_preflight(&project, release::Profile::Editing, "1.5.13");
            assert!(
                !diagnostics
                    .iter()
                    .any(|issue| issue.severity == "error" || issue.code == "unused_content"),
                "{diagnostics:?}"
            );
            let output = root.path().join(format!("{content_type}-{kind}.lommod"));
            project.export(&output).unwrap();
            let entries = package::read_package(&output).unwrap();
            assert!(entries.contains_key(&format!("assets/user/{content_type}/demo.art/icon.png")));
            let reopened = Project::open(&output).unwrap();
            assert_eq!(reopened.stories, project.stories);
            assert_eq!(
                reopened.manifest["min_host_version"],
                if content_type == "image" && kind == "big" {
                    "1.5.9"
                } else if content_type == "image" && kind == "event" {
                    "1.4.2"
                } else {
                    "1.5.13"
                }
            );
            assert!(release::unused_asset_paths(
                &project.stories,
                &project.assets.keys().cloned().collect::<Vec<_>>()
            )
            .is_empty());
        }
    }
}

#[test]
fn item_icon_and_gameplay_reference_can_share_the_same_definition() {
    let root = tempfile::tempdir().unwrap();
    let mut project = picture_project("item", "item");
    project.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(
            1,
            json!({"id":"give","type":"item","kind":"misc","item":"user:demo.art","count":1}),
        );
    project.export(&root.path().join("item.lommod")).unwrap();
    project.stories.get_mut("main").unwrap()["nodes"][1]["kind"] = json!("special");
    assert!(project
        .export(&root.path().join("wrong-category.lommod"))
        .is_err());
    project.assets.clear();
    let diagnostics = release::run_preflight(&project, release::Profile::Editing, "1.5.13");
    assert!(diagnostics.iter().any(|issue| issue.code == "missing_image"
        && issue.story_id == "main"
        && issue.node_id == "show"));
}

#[test]
fn released_picture_package_can_be_edited_saved_exported_and_reopened() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.8-big-picture.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    // Keep its actual historical source/assets; change the formerly unsupported display channel.
    let story = project.stories.get_mut("main").unwrap();
    story["nodes"].as_array_mut().unwrap().drain(..2);
    story["start"] = json!("big");
    story["nodes"][0]["kind"] = json!("item");
    story["nodes"][0].as_object_mut().unwrap().remove("fade_in");
    for node in story["nodes"].as_array_mut().unwrap() {
        if node["id"] == "hide_big" {
            node["kind"] = json!("item");
        }
    }
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("edited");
    project.save_to(&source).unwrap();
    let saved = Project::open(&source).unwrap();
    let output = root.path().join("edited.lommod");
    saved.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, saved.stories);
    assert_eq!(reopened.assets, saved.assets);
    assert_eq!(std::fs::read(fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    assert!(String::from_utf8_lossy(&entries["lua/main.lua"])
        .contains("ShowItemPicture(\"user:test.image\")"));
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated replacement')\n");
    let tampered = root.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}
