use super::*;

fn released_package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lom-core/tests/fixtures/released-1.5.14-parallel-base.lommod")
}

fn identity(project: &Project) -> Value {
    json!({
        "id": project.manifest["id"],
        "campaign_id": project.manifest["campaign_id"],
        "campaign": project.manifest["campaign"]
    })
}

#[test]
fn outer_directory_opens_the_unique_child_with_the_same_source_and_identity() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("作品工程");
    let mut project = Project::new_scenario("sandbox");
    project
        .assets
        .insert("assets/kept.bin".into(), b"keep".to_vec());
    project.save_to(&folder).unwrap();
    let original_manifest = std::fs::read(folder.join("manifest.json")).unwrap();
    let original_story = std::fs::read(folder.join("story/main.json")).unwrap();
    std::fs::create_dir(temp.path().join("说明资料")).unwrap();
    std::fs::write(
        temp.path().join("说明资料/config.json"),
        br#"{"id":"unrelated"}"#,
    )
    .unwrap();

    let from_folder = open_project(temp.path()).unwrap();
    let from_file = open_project(&folder.join("manifest.json")).unwrap();
    assert_eq!(from_folder, from_file);
    assert_eq!(from_folder.source, Some(folder.canonicalize().unwrap()));
    assert_eq!(identity(&from_folder), identity(&project));
    assert_eq!(from_folder.assets, project.assets);
    assert_eq!(
        std::fs::read(folder.join("manifest.json")).unwrap(),
        original_manifest
    );
    assert_eq!(
        std::fs::read(folder.join("story/main.json")).unwrap(),
        original_story
    );
    assert!(!temp.path().join("manifest.json").exists());
}

#[test]
fn direct_project_and_loose_chapters_take_precedence_over_nested_packages() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("project");
    let mut project = Project::new();
    project.save_to(&folder).unwrap();
    std::fs::copy(released_package(), folder.join("copy.lommod")).unwrap();
    Project::new()
        .save_to(&folder.join("another-project"))
        .unwrap();
    let source = folder.canonicalize().unwrap();
    for entry in [
        &folder,
        &folder.join("story"),
        &folder.join("story/main.json"),
    ] {
        assert_eq!(project_sources(entry).unwrap(), vec![source.clone()]);
        assert_eq!(open_project(entry).unwrap().manifest, project.manifest);
    }
    let loose = temp.path().join("loose");
    std::fs::create_dir(&loose).unwrap();
    std::fs::write(
        loose.join("main.json"),
        lom_core::stable_json(&project.stories["main"]).unwrap(),
    )
    .unwrap();
    std::fs::copy(released_package(), loose.join("copy.lommod")).unwrap();
    assert_eq!(
        project_sources(&loose).unwrap(),
        vec![loose.canonicalize().unwrap()]
    );
    assert_eq!(open_project(&loose).unwrap().stories, project.stories);
}

#[test]
fn multiple_candidates_are_reported_without_loading_migrating_or_picking_one() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("project");
    std::fs::create_dir(&child).unwrap();
    // Recognition must not parse/migrate a marked project or validate a package.
    let original = br#"{"id":"original-save","invalid":"keep the diagnostic"}"#;
    std::fs::write(child.join("manifest.json"), original).unwrap();
    let package = temp.path().join("a.lommod");
    std::fs::write(&package, b"not yet opened").unwrap();
    let mut expected = vec![
        child.canonicalize().unwrap(),
        package.canonicalize().unwrap(),
    ];
    expected.sort();
    assert_eq!(project_sources(temp.path()).unwrap(), expected);
    let error = open_project(temp.path()).unwrap_err().to_string();
    assert!(error.contains("多个工程"));
    assert!(error.contains("a.lommod") && error.contains("project"));
    assert_eq!(
        std::fs::read(child.join("manifest.json")).unwrap(),
        original
    );
    assert_eq!(std::fs::read(package).unwrap(), b"not yet opened");
    assert_eq!(std::fs::read_dir(&child).unwrap().count(), 1);
    // An invalid marked project is diagnosed, never replaced by a neighbouring package.
    assert!(open_project(&child)
        .unwrap_err()
        .to_string()
        .contains("项目没有剧情章节"));
}

#[test]
fn released_package_in_outer_directory_edits_saves_exports_reopens_without_identity_changes() {
    let temp = tempfile::tempdir().unwrap();
    let download = temp.path().join("download");
    std::fs::create_dir(&download).unwrap();
    let fixture = released_package();
    let original = std::fs::read(&fixture).unwrap();
    let input = download.join("旧版作品.lommod");
    std::fs::write(&input, &original).unwrap();
    let mut project = open_project(&download).unwrap();
    assert_eq!(project, open_project(&input).unwrap());
    let before = identity(&project);
    let assets = project.assets.clone();
    let paths = project.paths.clone();
    project.stories.get_mut("main").unwrap()["title"] = json!("从外层目录打开后继续编辑");
    let saved_parent = temp.path().join("saved");
    let saved = saved_parent.join("project");
    project.save_to(&saved).unwrap();
    let reopened = open_project(&saved_parent).unwrap();
    assert_eq!(identity(&reopened), before);
    assert_eq!(reopened.stories, project.stories);
    assert_eq!(reopened.assets, assets);
    assert_eq!(reopened.paths, paths);
    let output_parent = temp.path().join("exported");
    std::fs::create_dir(&output_parent).unwrap();
    let output = output_parent.join("updated.lommod");
    reopened.export(&output).unwrap();
    let exported = open_project(&output_parent).unwrap();
    assert_eq!(identity(&exported), before);
    assert_eq!(exported.stories, reopened.stories);
    assert_eq!(exported.assets, assets);
    assert_eq!(std::fs::read(&input).unwrap(), original);
    assert_eq!(std::fs::read(&fixture).unwrap(), original);

    // Discovery never relaxes the archive's source/Lua ownership checks.
    let mut entries = lom_core::package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua')\n");
    let tampered_dir = temp.path().join("tampered");
    std::fs::create_dir(&tampered_dir).unwrap();
    lom_core::package::write_package(entries, &tampered_dir.join("bad.lommod")).unwrap();
    assert!(open_project(&tampered_dir).is_err());
}

#[test]
fn empty_or_unrelated_outer_folder_reports_the_selected_path() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("config.json"), br#"{"id":"unrelated"}"#).unwrap();
    let message = open_project(temp.path()).unwrap_err().to_string();
    assert!(message.contains("没有可打开的工程或 .lommod"));
    assert!(message.contains(
        &temp
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string()
    ));
}
