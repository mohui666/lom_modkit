#![cfg(windows)]
use lom_core::editor_update::{finish_job, InstallJob, PROGRAM_FILES};
use std::{
    fs,
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn parent() -> std::process::Child {
    Command::new("cmd.exe")
        .args(["/D", "/C", "set /p LOM_UPDATE_TEST_WAIT="])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn native_installer_waits_and_preserves_user_data_and_locked_targets() {
    for locked in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("local");
        let work = local.join("lom_modkit/cache/work");
        fs::create_dir_all(&work).unwrap();
        let stage = tempfile::Builder::new()
            .prefix("lom-modkit-")
            .tempdir_in(&work)
            .unwrap();
        let root = temp.path().join("Editor 中文 [test]");
        for (_, name) in PROGRAM_FILES {
            fs::create_dir_all(root.join(name).parent().unwrap()).unwrap();
            fs::write(root.join(name), b"old").unwrap();
            fs::create_dir_all(stage.path().join("bundle").join(name).parent().unwrap()).unwrap();
            fs::write(stage.path().join("bundle").join(name), b"new").unwrap();
        }
        let markers = [
            "project/story/main.json",
            "repository/assets/user/my.music/track.ogg",
            "preferences.json",
        ];
        for name in markers {
            fs::create_dir_all(root.join(name).parent().unwrap()).unwrap();
            fs::write(root.join(name), b"my data").unwrap();
        }
        let lock = locked.then(|| {
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(root.join("lom-editor.exe"))
                .unwrap()
        });
        let mut editor = parent();
        let job_path = stage.path().join("install.json");
        let job = InstallJob {
            root: root.clone(),
            stage: stage.path().into(),
            files: PROGRAM_FILES
                .iter()
                .map(|(_, path)| path.to_string())
                .collect(),
            parent_pid: editor.id(),
            project: None,
            restart: false,
            installer_pid: None,
        };
        fs::write(&job_path, serde_json::to_vec(&job).unwrap()).unwrap();
        let input = editor.stdin.take().unwrap();
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            drop(input);
            editor.wait().unwrap();
        });
        let started = Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_lomc"))
            .arg("--apply-editor-update")
            .arg(&job_path)
            .env("LOCALAPPDATA", &local)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        closer.join().unwrap();
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert_eq!(
            output.status.success(),
            !locked,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for (_, name) in PROGRAM_FILES {
            assert_eq!(
                fs::read(root.join(name)).unwrap(),
                if locked { b"old" } else { b"new" }
            );
        }
        for name in markers {
            assert_eq!(fs::read(root.join(name)).unwrap(), b"my data");
        }
        assert!(!stage.path().exists());
        drop(lock);
    }
}

#[test]
fn native_cleanup_waits_for_its_installer_before_removing_the_private_cache() {
    // Exercise this in a child test process so LOCALAPPDATA stays isolated.
    const PROBE: &str = "LOM_NATIVE_CLEANUP_PROBE";
    if let Some(path) = std::env::var_os(PROBE) {
        finish_job(std::path::Path::new(&path)).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let local = temp.path().join("local");
    let work = local.join("lom_modkit/cache/work");
    fs::create_dir_all(&work).unwrap();
    let stage = tempfile::Builder::new()
        .prefix("lom-modkit-")
        .tempdir_in(&work)
        .unwrap();
    let root = temp.path().join("Editor");
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(root.join("lomc.exe"), b"keep cli").unwrap();
    fs::write(root.join("lom-editor.exe"), b"keep editor").unwrap();
    fs::write(root.join("runtime/MortalModHost.dll"), b"keep host").unwrap();
    let mut helper = parent();
    let job_path = stage.path().join("install.json");
    let job = InstallJob {
        root: root.clone(),
        stage: stage.path().into(),
        files: vec!["lom-editor.exe".into()],
        parent_pid: helper.id(),
        project: None,
        restart: true,
        installer_pid: Some(helper.id()),
    };
    fs::write(&job_path, serde_json::to_vec(&job).unwrap()).unwrap();
    let input = helper.stdin.take().unwrap();
    let closer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        drop(input);
        helper.wait().unwrap();
    });
    let started = Instant::now();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_cleanup_waits_for_its_installer_before_removing_the_private_cache",
        ])
        .env("LOCALAPPDATA", &local)
        .env(PROBE, &job_path)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    closer.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert!(!stage.path().exists());
    assert_eq!(
        fs::read(root.join("lom-editor.exe")).unwrap(),
        b"keep editor"
    );
}
