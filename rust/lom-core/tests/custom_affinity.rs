use lom_core::{content, package, project::Project, release};
use serde_json::{json, Value};

fn add_character(project: &mut Project) {
    let metadata = json!({"schema":1,"id":"demo.biography","type":"character","name":"讲述人",
        "files":{"main":"normal.png"},"portraits":{"normal":"normal.png"},
        "avatar":"avatar.png","initial_affinity":25,
        "biography":"状态页独立传记，随当前好感等级一起显示。",
        "intro":{"name":"讲述人","title":"故人","text":"曾与赵活同行，在此记下往事。","image":"normal.png"}});
    let directory = "assets/user/character/demo.biography";
    project.assets.insert(
        format!("{directory}/content.json"),
        serde_json::to_vec(&metadata).unwrap(),
    );
    let image = include_bytes!(
        "../../../samples/showcase3/assets/user/character/showcase.lin_deng/normal.png"
    );
    for file in ["normal.png", "avatar.png"] {
        project
            .assets
            .insert(format!("{directory}/{file}"), image.to_vec());
    }
}

#[test]
fn affinity_only_reference_packages_avatar_biography_and_requires_current_host() {
    let mut project = Project::new();
    add_character(&mut project);
    project.stories.insert("main".into(), json!({"id":"main","start":"up","nodes":[
        {"id":"up","type":"affinity","character":"user:demo.biography","delta":3},
        {"id":"down","type":"affinity","character":"user:demo.biography","delta":-2,"show_message":true},
        {"id":"check","type":"affinity_check","character":"user:demo.biography","op":">=","value":25,"success":"end","failure":"end"},
        {"id":"end","type":"end"}]}));
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("affinity.lommod");
    project.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
    let metadata: Value = serde_json::from_slice(
        &reopened.assets["assets/user/character/demo.biography/content.json"],
    )
    .unwrap();
    assert_eq!(metadata["avatar"], "avatar.png");
    assert_eq!(metadata["initial_affinity"], 25);
    assert_eq!(
        metadata["biography"],
        "状态页独立传记，随当前好感等级一起显示。"
    );
    assert_eq!(metadata["intro"]["text"], "曾与赵活同行，在此记下往事。");
    assert!(reopened
        .assets
        .contains_key("assets/user/character/demo.biography/avatar.png"));
    let entries = package::read_package(&output).unwrap();
    let lua = std::str::from_utf8(&entries["lua/main.lua"]).unwrap();
    assert!(lua.contains("statmodifymanager.Character(\"user:demo.biography\", -2, 1)"));
    assert!(lua.contains("mod_affinity_value(\"user:demo.biography\")"));
    let issues = release::run_preflight(&project, release::Profile::Release, "1.5.10");
    assert!(
        !issues
            .iter()
            .any(|issue| issue.severity == "error" || issue.code == "unused_content"),
        "{issues:?}"
    );
}

#[test]
fn released_154_project_accepts_custom_character_edit_save_export_reopen_and_rejects_lua_swap() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.4-name-base.lommod");
    let original = std::fs::read(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    add_character(&mut project);
    project.stories.get_mut("main").unwrap()["nodes"].as_array_mut().unwrap().insert(1,
        json!({"id":"custom_affinity","type":"affinity","character":"user:demo.biography","delta":-5}));
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("edited");
    project.save_to(&source).unwrap();
    let edited = Project::open(&source).unwrap();
    let output = directory.path().join("edited.lommod");
    edited.export(&output).unwrap();
    let reopened = Project::open(&output).unwrap();
    assert_eq!(reopened.stories, edited.stories);
    assert_eq!(reopened.manifest["min_host_version"], "1.5.10");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
    let mut entries = package::read_package(&output).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua replacement')\n");
    let tampered = directory.path().join("tampered.lommod");
    package::write_package(entries, &tampered).unwrap();
    assert!(Project::open(&tampered).is_err());
}

#[test]
fn unsafe_avatar_path_is_not_packaged() {
    let metadata = json!({"schema":1,"id":"demo.biography","type":"character","name":"讲述人",
        "files":{"main":"normal.png"},"avatar":"../elsewhere.png"});
    assert!(content::normalize_content_metadata(&metadata).is_err());
    let invalid_reference = json!({"id":"main","start":"bad","nodes":[
        {"id":"bad","type":"affinity","character":"user:../other.character","delta":1},
        {"id":"end","type":"end"} ]});
    assert!(lom_core::validate::validate_story(&invalid_reference).is_err());
}

#[test]
fn status_biography_without_a_stage_reference_is_packaged_and_preserved() {
    let mut project = Project::new();
    add_character(&mut project);
    let folder = tempfile::tempdir().unwrap();
    let package = folder.path().join("status-person.lommod");
    project.export(&package).unwrap();
    let mut reopened = Project::open(&package).unwrap();
    let key = "assets/user/character/demo.biography/content.json";
    let mut metadata: Value = serde_json::from_slice(&reopened.assets[key]).unwrap();
    assert_eq!(
        metadata["biography"],
        "状态页独立传记，随当前好感等级一起显示。"
    );
    assert_eq!(metadata["intro"]["text"], "曾与赵活同行，在此记下往事。");
    metadata["biography"] = json!("修改后的状态传记。\n第二段。");
    reopened
        .assets
        .insert(key.into(), serde_json::to_vec(&metadata).unwrap());
    reopened.save_to(&folder.path().join("project")).unwrap();
    let saved = Project::open(&folder.path().join("project")).unwrap();
    let exported = folder.path().join("edited.lommod");
    saved.export(&exported).unwrap();
    let final_project = Project::open(&exported).unwrap();
    let final_metadata: Value = serde_json::from_slice(&final_project.assets[key]).unwrap();
    assert_eq!(final_metadata["biography"], metadata["biography"]);
    assert_eq!(
        final_metadata["intro"]["text"],
        "曾与赵活同行，在此记下往事。"
    );
    let issues = release::run_preflight(&project, release::Profile::Release, "1.5.10");
    assert!(
        !issues
            .iter()
            .any(|issue| issue.severity == "error" || issue.code == "unused_content"),
        "{issues:?}"
    );
    let mut malformed = metadata;
    malformed["biography"] = json!(7);
    assert!(content::normalize_content_metadata(&malformed).is_err());
}
