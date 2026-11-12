//! Synthetic filesystem installs; no game process is launched.
use lom_core::{game_tools as g, project::Project};
use std::{fs, path::Path};
#[path = "fixtures/host_version.rs"]
mod host_version;
use host_version::host_fixture;
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("Mortal_Data/Managed")).unwrap();
    fs::write(
        root.join("Mortal.exe"),
        b"synthetic fixture, not executable",
    )
    .unwrap();
    for f in g::BEPINEX_FILES {
        fs::create_dir_all(root.join(f).parent().unwrap()).unwrap();
        fs::write(root.join(f), b"fixture").unwrap();
    }
}

#[test]
fn event_preview_requests_are_complete_before_publication_and_invalid_inputs_preserve_them() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let event = serde_json::json!({"id":"visit","trigger":"story_entry","target":"section_01_free_03","from":"section_01_free_start_02"});
    let path =
        g::request_event_preview(dir.path(), "event_test_setup_1", "notice", &event).unwrap();
    let request = lom_core::load_json(&path).unwrap();
    assert_eq!(request["fresh_campaign"], true);
    assert_eq!(request["event_id"], "visit");
    assert_eq!(request["target"], event["target"]);
    assert_eq!(request["from"], event["from"]);
    assert_eq!(request["mod_id"], g::PREVIEW_ID);
    let before = fs::read(&path).unwrap();
    let mut invalid = event.clone();
    invalid["from"] = serde_json::json!("../unowned");
    assert!(
        g::request_event_preview(dir.path(), "event_test_setup_1", "notice", &invalid).is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    g::request_preview(dir.path(), "main", "say1").unwrap();
    assert!(lom_core::load_json(&path)
        .unwrap()
        .get("fresh_campaign")
        .is_none());
}
#[test]
fn host_install_and_verified_rollback_preserve_other_plugins() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let bundle = t.path().join("host");
    fs::create_dir(&bundle).unwrap();
    for name in g::HOST_FILES {
        fs::write(bundle.join(name), host_fixture(1, 4, 8)).unwrap();
    }
    g::install_runtime(&root, &bundle).unwrap();
    fs::write(g::plugin_dir(&root).join("user-note.txt"), b"keep").unwrap();
    for name in g::HOST_FILES {
        fs::write(bundle.join(name), host_fixture(1, 5, 8)).unwrap();
    }
    g::install_runtime(&root, &bundle).unwrap();
    g::restore_runtime(&root).unwrap();
    for name in g::HOST_FILES {
        assert_eq!(
            fs::read(g::plugin_dir(&root).join(name)).unwrap(),
            host_fixture(1, 4, 8)
        );
    }
    assert_eq!(
        fs::read(g::plugin_dir(&root).join("user-note.txt")).unwrap(),
        b"keep"
    );
}
#[cfg(windows)]
#[test]
fn old_host_cannot_overwrite_newer_host_or_dependencies() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let bundle = t.path().join("host");
    fs::create_dir(&bundle).unwrap();
    for name in g::HOST_FILES {
        fs::write(bundle.join(name), host_fixture(1, 5, 8)).unwrap();
    }
    assert_eq!(
        g::host_file_version(&bundle.join("MortalModHost.dll")).unwrap(),
        "1.5.8"
    );
    g::install_runtime(&root, &bundle).unwrap();
    for name in g::HOST_FILES {
        fs::write(bundle.join(name), host_fixture(1, 4, 8)).unwrap();
    }
    assert!(g::install_runtime(&root, &bundle)
        .unwrap_err()
        .to_string()
        .contains("拒绝降级"));
    assert_eq!(
        g::sync_installed_runtime(&root, &bundle).unwrap()["changed"],
        false
    );
    for name in g::HOST_FILES {
        assert_eq!(
            fs::read(g::plugin_dir(&root).join(name)).unwrap(),
            host_fixture(1, 5, 8)
        );
    }
    assert!(!g::plugin_dir(&root).join(".runtime_rollback").exists());
}
#[test]
fn mod_toggle_is_confined_and_collision_safe() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let source = t.path().join("test.lommod");
    Project::new().export(&source).unwrap();
    let installed = g::install_mod(&root, &source, true).unwrap();
    assert_eq!(g::list_mods(&root).unwrap().as_array().unwrap().len(), 1);
    assert!(g::set_enabled(&root, &source, false).is_err());
    let disabled = g::set_enabled(&root, &installed, false).unwrap();
    assert!(!installed.exists());
    assert_eq!(g::install_mod(&root, &source, true).unwrap(), disabled);
    g::set_enabled(&root, &disabled, true).unwrap();
}
#[test]
fn mod_updates_follow_identity_and_reject_another_work_reusing_the_id() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let mut first = Project::new();
    first.manifest["id"] = serde_json::json!("my_mod");
    let source = t.path().join("first.lommod");
    first.export(&source).unwrap();
    let installed = g::install_mod(&root, &source, true).unwrap();
    let original = fs::read(&installed).unwrap();
    let mut second = Project::new();
    second.manifest["id"] = serde_json::json!("my_mod");
    second.manifest["name"] = serde_json::json!("另一作品");
    let other = t.path().join("other-directory/different-name.lommod");
    second.export(&other).unwrap();
    let error = g::install_mod(&root, &other, true).unwrap_err().to_string();
    assert!(error.contains("作品编号") && error.contains("作品设置"));
    assert_eq!(fs::read(&installed).unwrap(), original);
    let disabled = g::set_enabled(&root, &installed, false).unwrap();
    first.manifest["name"] = serde_json::json!("修改后的名称");
    first.stories.get_mut("main").unwrap()["nodes"][0]["text"] = serde_json::json!("已更新");
    let updated = t.path().join("renamed-update.lommod");
    first.export(&updated).unwrap();
    assert_eq!(g::install_mod(&root, &updated, true).unwrap(), disabled);
    assert_eq!(Project::open(&disabled).unwrap().stories, first.stories);
    assert_eq!(g::list_mods(&root).unwrap().as_array().unwrap().len(), 1);
}

#[test]
fn export_detects_a_different_work_reusing_the_id_before_writing() {
    let directory = tempfile::tempdir().unwrap();
    let mut first = Project::new();
    first.manifest["id"] = serde_json::json!("my_mod");
    let original = directory.path().join("first.lommod");
    first.export(&original).unwrap();
    let bytes = fs::read(&original).unwrap();
    let mut second = Project::new();
    second.manifest["id"] = first.manifest["id"].clone();
    second.manifest["name"] = serde_json::json!("完全不同的名字");
    let output = directory.path().join("another-file.lommod");
    let error = second.export(&output).unwrap_err().to_string();
    assert!(error.contains("作品编号") && error.contains("作品设置"));
    assert!(!output.exists());
    assert_eq!(fs::read(&original).unwrap(), bytes);
    first.export(&output).unwrap();
    assert_eq!(
        Project::open(&output).unwrap().manifest["campaign_id"],
        first.manifest["campaign_id"]
    );
}

#[test]
fn deleting_a_mod_keeps_sources_saves_and_unrelated_files() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let source = t.path().join("source.lommod");
    Project::new().export(&source).unwrap();
    let installed = g::install_mod(&root, &source, true).unwrap();
    let save = g::plugin_dir(&root).join("saves/keep.dat");
    fs::create_dir_all(save.parent().unwrap()).unwrap();
    fs::write(&save, b"save").unwrap();
    let note = installed.parent().unwrap().join("keep.txt");
    fs::write(&note, b"keep").unwrap();
    assert!(g::remove_mod(&root, &source).is_err());
    assert!(g::remove_mod(&root, &note).is_err());
    g::remove_mod(&root, &installed).unwrap();
    assert!(!installed.exists());
    assert!(source.exists());
    assert_eq!(fs::read(&save).unwrap(), b"save");
    assert_eq!(fs::read(&note).unwrap(), b"keep");
}
#[test]
fn incomplete_runtime_and_bad_loader_do_not_mutate_install() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let bundle = t.path().join("host");
    fs::create_dir(&bundle).unwrap();
    fs::write(bundle.join("MortalModHost.dll"), b"fixture").unwrap();
    assert!(g::install_runtime(&root, &bundle).is_err());
    assert!(!g::plugin_dir(&root).exists());
    let bad = t.path().join("bad.zip");
    fs::write(&bad, b"untrusted").unwrap();
    assert!(g::install_bepinex_archive(&root, &bad).is_err());
    assert!(!root.join("winhttp.dll").exists());
}
#[cfg(unix)]
#[test]
fn symlink_install_escape_rejected() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("fake-game");
    fixture(&root);
    let outside = t.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::create_dir_all(root.join("BepInEx/plugins")).unwrap();
    std::os::unix::fs::symlink(&outside, g::plugin_dir(&root)).unwrap();
    let source = t.path().join("test.lommod");
    Project::new().export(&source).unwrap();
    assert!(g::install_mod(&root, &source, true).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
