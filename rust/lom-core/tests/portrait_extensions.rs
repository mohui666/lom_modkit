use lom_core::{content, content_edit, package, project::Project, release, validate};
use serde_json::{json, Value};

fn project() -> Project {
    let mut project = Project::new();
    let meta = json!({"schema":1,"id":"demo.xia","type":"character","name":"Extra portraits",
        "extends":"special825","files":{"main":"tired.png"},
        "portraits":{"tired":"tired.png","sleep":"sleep.png"},"scale":280.5});
    let root = "assets/user/character/demo.xia";
    project.assets.insert(
        format!("{root}/content.json"),
        serde_json::to_vec(&meta).unwrap(),
    );
    let image = include_bytes!(
        "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
    );
    for name in ["tired.png", "sleep.png"] {
        project
            .assets
            .insert(format!("{root}/{name}"), image.to_vec());
    }
    project.stories.insert("main".into(), json!({"id":"main","start":"show1","nodes":[
        {"id":"show1","type":"show","character":"special825","portrait":"tired","position":"M"},
        {"id":"say1","type":"say","character":"special825","portrait":"sleep","text":"Extra expression"},
        {"id":"normal","type":"portrait","character":"special825","portrait":"normal"},
        {"id":"end1","type":"end"}]}));
    project
}

#[test]
fn added_expressions_compile_travel_with_the_package_and_preserve_official_ones() {
    let project = project();
    // A preview mapping alone must never authorize a missing game resource.
    assert!(validate::validate_story(&project.stories["main"]).is_err());
    let catalog = validate::catalog_with_assets(&project.assets).unwrap();
    validate::validate_story_with_catalog(&project.stories["main"], &catalog).unwrap();
    let issues = release::run_preflight(&project, release::Profile::Release, "1.4.3");
    assert!(
        !issues.iter().any(|i| i.severity == "error"
            || i.code == "unused_content"
            || i.code == "unused_critical_asset"),
        "{issues:?}"
    );
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("portable.lommod");
    project.export(&path).unwrap();
    package::inspect_package(&path).unwrap();
    let loaded = Project::open(&path).unwrap();
    assert_eq!(loaded.manifest["min_host_version"], "1.4.3");
    assert_eq!(loaded.assets.len(), project.assets.len());
    for (key, bytes) in &project.assets {
        if !key.ends_with("content.json") {
            assert!(
                loaded.assets.get(key) == Some(bytes),
                "image changed: {key}"
            );
        }
    }
    assert_eq!(
        loaded.stories["main"]["nodes"][0]["character"],
        "special825"
    );
    assert!(!String::from_utf8(
        loaded.assets["assets/user/character/demo.xia/content.json"].clone()
    )
    .unwrap()
    .contains("normal"));
}

#[test]
fn extensions_do_not_relax_validation_for_other_people_or_undefined_expressions() {
    let project = project();
    let catalog = validate::catalog_with_assets(&project.assets).unwrap();
    for (character, portrait) in [("special825", "missing"), ("player", "tired")] {
        let mut story = project.stories["main"].clone();
        story["nodes"][0]["character"] = json!(character);
        story["nodes"][0]["portrait"] = json!(portrait);
        assert!(validate::validate_story_with_catalog(&story, &catalog).is_err());
    }
    let mut project = project;
    assert!(
        content_edit::remove(&mut project, "assets/user/character/demo.xia/content.json").is_err()
    );
    let mut duplicate: Value =
        serde_json::from_slice(&project.assets["assets/user/character/demo.xia/content.json"])
            .unwrap();
    duplicate["id"] = json!("demo.duplicate");
    project.assets.insert(
        "assets/user/character/demo.duplicate/content.json".into(),
        serde_json::to_vec(&duplicate).unwrap(),
    );
    assert!(validate::catalog_with_assets(&project.assets)
        .unwrap_err()
        .to_string()
        .contains("多份扩展"));
}

#[test]
fn scales_preserve_small_large_and_fractional_values_and_reject_invalid_input() {
    for scale in [0.01, 25.5, 280.5, 2500.0] {
        let mut meta: Value = serde_json::from_slice(
            &project().assets["assets/user/character/demo.xia/content.json"],
        )
        .unwrap();
        meta["scale"] = json!(scale);
        let normalized = content::normalize_content_metadata(&meta).unwrap();
        assert_eq!(normalized["scale"].as_f64(), Some(scale));
        assert!(normalized["portraits"].get("normal").is_none());
        assert_eq!(
            content::content_metadata_payload(&normalized)["extends"],
            "special825"
        );
        for node in [
            json!({"id":"scale","type":"overlay","action":"show","slot":"art","image":"user:demo.image","scale":scale}),
            json!({"id":"scale","type":"custom_cg","action":"show","image":"user:demo.image","scale":scale}),
        ] {
            validate::validate_story(
                &json!({"id":"main","start":"scale","nodes":[node,{"id":"end1","type":"end"}]}),
            )
            .unwrap();
        }
    }
    for value in [json!(0), json!(-1), json!(true), json!("NaN"), json!(1e100)] {
        assert!(content::positive_scale(&value, "scale").is_err());
    }
}
