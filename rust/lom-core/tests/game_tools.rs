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
    assert!(g::install_mod(&root, &source, true).is_err());
    g::set_enabled(&root, &disabled, true).unwrap();
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
