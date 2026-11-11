use super::*;

#[test]
fn project_autosave_writes_directory_json_and_package_without_a_dialog() {
    for layout in ["directory", "json", "package"] {
        let dir = tempfile::tempdir().unwrap();
        let target = match layout {
            "json" => dir.path().join("main.json"),
            "package" => dir.path().join("work.lommod"),
            _ => dir.path().join("project"),
        };
        let mut project = Project::new();
        project.save_to(&target).unwrap();
        let mut app = app();
        app.install(Project::open(&target).unwrap());
        app.recovery_dir = dir.path().join("recovery");
        app.auto_save_project = true;
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
            json!("自动保存的新进度");
        app.track();
        app.last_auto_save = Instant::now() - Duration::from_secs(301);
        app.autosave();
        assert!(app.error.is_none(), "{layout}: {:?}", app.error);
        assert!(!app.dirty(), "{layout}");
        assert!(app.pending.is_none());
        assert_eq!(
            Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
            "自动保存的新进度"
        );
        assert!(app.status.contains(&target.display().to_string()));
    }
}

#[test]
fn project_autosave_keeps_new_and_disabled_projects_in_recovery() {
    for has_source in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app();
        if has_source {
            app.project.save_to(&dir.path().join("project")).unwrap();
            app.last = snapshot(&app.project);
            app.mark_saved();
        }
        let original = app
            .project
            .source
            .as_deref()
            .map(|path| Project::open(path).unwrap());
        app.auto_save_project = !has_source;
        app.recovery_dir = dir.path().join("recovery");
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
            json!("恢复副本中的新进度");
        app.track();
        app.last_auto_save = Instant::now() - Duration::from_secs(301);
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        assert!(app.dirty());
        assert!(app.pending.is_none());
        assert_eq!(
            Project::open(&app.recovery_dir).unwrap().stories["main"]["nodes"][0]["text"],
            "恢复副本中的新进度"
        );
        if let Some(original) = original {
            assert_eq!(
                Project::open(original.source.as_ref().unwrap())
                    .unwrap()
                    .stories,
                original.stories
            );
        }
    }
}

#[test]
fn project_autosave_preserves_newer_external_progress_and_keeps_its_own_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let mut app = app();
    app.project.save_to(&root).unwrap();
    app.last = snapshot(&app.project);
    app.mark_saved();
    let mut other = Project::open(&root).unwrap();
    other.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("另一个窗口的新进度");
    other.save_to(&root).unwrap();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("本窗口的修改");
    app.track();
    app.auto_save_project = true;
    app.recovery_dir = dir.path().join("recovery");
    app.last_auto_save = Instant::now() - Duration::from_secs(301);
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert!(app.dirty());
    assert!(app.error.as_ref().unwrap().contains("编辑器外修改"));
    assert_eq!(
        Project::open(&root).unwrap().stories["main"]["nodes"][0]["text"],
        "另一个窗口的新进度"
    );
    assert_eq!(
        Project::open(&app.recovery_dir).unwrap().stories["main"]["nodes"][0]["text"],
        "本窗口的修改"
    );
}

#[test]
fn project_autosave_preferences_preserve_other_settings_and_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    std::fs::write(&path, r#"{"ui_locale":"chs","custom":"keep"}"#).unwrap();
    let mut app = app();
    let before = snapshot(&app.project);
    assert_eq!(app.auto_save_seconds, 300);
    app.set_auto_save(true, 60, &path).unwrap();
    let prefs = lom_core::load_json(&path).unwrap();
    assert_eq!(prefs["auto_save_project"], true);
    assert_eq!(prefs["auto_save_seconds"], 60);
    assert_eq!(prefs["custom"], "keep");
    assert_eq!(prefs["ui_locale"], "chs");
    assert!(app.set_auto_save(true, 0, &path).is_err());
    assert!(snapshot(&app.project) == before);
}

#[test]
fn project_autosave_waits_for_the_interval_and_respects_pending_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let mut app = app();
    app.project.save_to(&root).unwrap();
    app.last = snapshot(&app.project);
    app.mark_saved();
    let baseline = app.project.stories.clone();
    app.auto_save_project = true;
    app.recovery_dir = dir.path().join("recovery");
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("待保存的新进度");
    app.track();
    app.last_auto_save = Instant::now();
    app.autosave();
    assert_eq!(Project::open(&root).unwrap().stories, baseline);
    app.pending = Some(Pending::Quit);
    app.last_auto_save = Instant::now() - Duration::from_secs(301);
    app.autosave();
    assert_eq!(Project::open(&root).unwrap().stories, baseline);
    app.pending = None;
    // Recovery deliberately clears the saved baseline until the user confirms it.
    app.saved.stories.clear();
    app.last_auto_save = Instant::now() - Duration::from_secs(301);
    app.autosave();
    assert_eq!(Project::open(&root).unwrap().stories, baseline);
    assert!(app.dirty());
}
#[test]
fn idle_frames_sleep_until_compile_history_or_recovery_is_due() {
    let mut app = app();
    app.needs_compile = false;
    assert_eq!(app.idle_repaint_delay(), Duration::MAX);
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("修改");
    app.track();
    assert!(app.idle_repaint_delay() <= Duration::from_millis(400));
    app.needs_compile = false;
    assert!(app.idle_repaint_delay() <= Duration::from_millis(650));
    app.flush();
    assert!(app.idle_repaint_delay() <= Duration::from_secs(30));
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    assert_eq!(app.idle_repaint_delay(), Duration::ZERO);
    app.mark_saved();
    assert_eq!(app.idle_repaint_delay(), Duration::MAX);
    app.screenshot = Some(PathBuf::from("unused-screenshot-test.png"));
    assert!(app.idle_repaint_delay() <= Duration::from_millis(70));
}
#[test]
fn repeated_recovery_updates_only_owned_files_without_polluting_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("owned-recovery");
    app.project.source = Some(dir.path().join("original-project"));
    app.project
        .paths
        .insert("main".into(), "authored-name.json".into());
    app.project.story_subdir = false;
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("first");
    app.track();
    let original_source = app.project.source.clone();
    let original_paths = app.project.paths.clone();
    for text in ["first", "second"] {
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!(text);
        app.track();
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        let restored = Project::open(&app.recovery_dir).unwrap();
        assert_eq!(restored.stories["main"]["nodes"][0]["text"], text);
        assert_eq!(app.project.source, original_source);
        assert_eq!(app.project.paths, original_paths);
        assert!(!app.project.story_subdir);
        assert_eq!(app.recovery_paths["main"], PathBuf::from("main.json"));
        let session = lom_core::load_json(app.recovery_dir.join("session.json")).unwrap();
        assert_eq!(session["paths"]["main"], "authored-name.json");
        assert_eq!(session["story_subdir"], false);
    }
    app.begin_chapter(false);
    app.commit_chapter().unwrap();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert!(app.recovery_dir.join("story/chapter1.json").exists());
    app.undo();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert!(!app.recovery_dir.join("story/chapter1.json").exists());
    assert_eq!(Project::open(&app.recovery_dir).unwrap().stories.len(), 1);
    app.clear_recovery();
    assert!(app.recovery_paths.is_empty());
    assert!(!app.recovery_dir.exists());
}
#[test]
fn recovery_never_claims_a_foreign_collision_as_owned() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("foreign-recovery");
    std::fs::create_dir_all(app.recovery_dir.join("story")).unwrap();
    let collision = app.recovery_dir.join("story/main.json");
    std::fs::write(&collision, b"unowned data").unwrap();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("unsaved");
    app.track();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert_eq!(std::fs::read(&collision).unwrap(), b"unowned data");
    assert!(app.recovery_paths.is_empty());
    assert!(app.status.starts_with("自动恢复副本写入失败"));
    assert!(app.project.source.is_none());
    assert!(app.project.paths.is_empty());
}
#[test]
fn recovery_removes_deleted_owned_assets_without_removing_unknown_files() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hero.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("asset-recovery");
    app.import_assets(vec![image], "test.hero", "hero", 0)
        .unwrap();
    app.track();
    let owned: BTreeSet<_> = app.project.assets.keys().cloned().collect();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert_eq!(app.recovery_assets, owned);
    let unknown = app.recovery_dir.join("assets/unknown.bin");
    std::fs::write(&unknown, b"unowned data").unwrap();
    lom_core::content_edit::remove(&mut app.project, &app.asset_selection).unwrap();
    app.track();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    let restored = Project::open(&app.recovery_dir).unwrap();
    for name in owned {
        assert!(!restored.assets.contains_key(&name));
        assert!(!app.recovery_dir.join(name).exists());
    }
    assert_eq!(std::fs::read(&unknown).unwrap(), b"unowned data");
    assert!(app.recovery_assets.is_empty());
    app.clear_recovery();
    assert!(app.recovery_assets.is_empty());
}
#[test]
fn recovery_includes_valid_pending_content_without_changing_live_input() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hero.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("draft-recovery");
    app.import_assets(vec![image], "test.hero", "hero", 0)
        .unwrap();
    app.track();
    let before = snapshot(&app.project);
    let mut draft: Value =
        serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
    draft["name"] = json!("new title ");
    app.content_panel = crate::content_panel::ContentPanel::pending_fixture(
        &app.project,
        &app.asset_selection,
        draft,
    );
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    let restored = Project::open(&app.recovery_dir).unwrap();
    let meta: Value = serde_json::from_slice(&restored.assets[&app.asset_selection]).unwrap();
    assert_eq!(meta["name"], "new title");
    assert!(snapshot(&app.project) == before);
    assert!(app.content_panel.has_pending());
}
#[test]
fn dirty_project_requires_a_decision_before_recovery() {
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("unsaved");
    app.track();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    app.request(Pending::Recover(PathBuf::from("/missing")), &ctx);
    assert!(matches!(app.pending, Some(Pending::Recover(_))));
    assert!(snapshot(&app.project) == before);
}
#[test]
fn closing_dialogs_cancels_only_the_dialog_and_keeps_work_and_recovery_files() {
    let root = std::env::temp_dir().join(format!(
        "lom-dialog-close-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let marker = root.join("session.json");
    std::fs::write(&marker, b"retained recovery").unwrap();
    for kind in ["chapter", "pending", "error", "recovery"] {
        for escape in [false, true] {
            let mut app = app();
            app.project.manifest["name"] = json!("unsaved work");
            match kind {
                "chapter" => app.begin_chapter(false),
                "pending" => app.pending = Some(Pending::New),
                "error" => app.error = Some("example failure".into()),
                _ => app.recovery_candidates = vec![root.clone()],
            }
            let before = snapshot(&app.project);
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let frame = |app: &mut App, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(900.0, 560.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |_ui| {});
                        app.dialogs(ctx);
                    },
                )
            };
            let output = frame(&mut app, vec![]);
            if kind == "recovery" {
                let nodes = &output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes;
                let bounds = |label: &str| {
                    nodes
                        .iter()
                        .find(|(_, n)| n.label().or_else(|| n.value()) == Some(tr(label).as_str()))
                        .and_then(|(_, n)| n.bounds())
                        .unwrap()
                };
                assert!(
                    bounds("稍后处理").y1 - bounds("恢复创作副本").y0 < 300.0,
                    "a single recovery card must not expand to fill the scroll area"
                );
            }
            let event = if escape {
                key_event(egui::Key::Escape, egui::Modifiers::NONE)
            } else {
                click_named_option(&output, &tr("关闭"))
            };
            let _ = frame(&mut app, vec![event]);
            assert!(
                app.chapter_dialog.is_none()
                    && app.pending.is_none()
                    && app.error.is_none()
                    && app.recovery_candidates.is_empty(),
                "{kind}, escape={escape}"
            );
            assert!(
                snapshot(&app.project) == before,
                "closing {kind} must preserve unsaved work"
            );
            assert_eq!(std::fs::read(&marker).unwrap(), b"retained recovery");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn active_instance_recovery_is_never_a_candidate() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert!(recovery_owner_active(&PathBuf::from(format!(
        "{}-{stamp}",
        std::process::id()
    ))));
    assert!(recovery_owner_active(Path::new("unknown-owner")));
}
#[cfg(windows)]
#[test]
fn windows_recovery_distinguishes_live_exited_and_reused_processes() {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut child = Command::new("cmd.exe")
        .args(["/D", "/C", "set /p recovery_test="])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .unwrap();
    let pid = child.id();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let path = PathBuf::from(format!("{pid}-{stamp}"));
    let live_active = recovery_owner_active(&path);
    let reused_active = recovery_owner_active(Path::new(&format!("{pid}-1")));
    // Close only our child's input, allowing it to exit before assertions.
    drop(child.stdin.take());
    child.wait().unwrap();
    let exited_active = recovery_owner_active(&path);
    drop(child);
    let removed_active = recovery_owner_active(&path);

    assert!(live_active);
    assert!(!reused_active);
    assert!(!exited_active);
    assert!(!removed_active);
    assert!(!recovery_owner_active(Path::new(&format!(
        "{}-1",
        std::process::id()
    ))));
    assert!(recovery_owner_active(Path::new("0-1")));
    assert!(recovery_owner_active(Path::new(&format!("{pid}-invalid"))));
}
#[cfg(windows)]
#[test]
fn windows_recovery_restores_chapters_assets_and_original_source_after_exit() {
    use std::os::windows::process::CommandExt;

    let mut child = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "exit", "0"])
        .creation_flags(0x0800_0000)
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    drop(child);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("原项目 source");
    let image = dir.path().join("恢复 素材.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut writer = app();
    writer.project.save_to(&original).unwrap();
    writer.saved = snapshot(&writer.project);
    writer.recovery_dir = dir.path().join(format!("{pid}-{stamp}"));
    writer.begin_chapter(false);
    writer.chapter_dialog.as_mut().unwrap().name = "未保存章节".into();
    writer.commit_chapter().unwrap();
    writer
        .import_assets(vec![image], "test.recovered", "恢复素材", 0)
        .unwrap();
    writer.track();
    writer.last_recovery = Instant::now() - Duration::from_secs(31);
    writer.autosave();
    assert!(writer.recovery_dir.join("session.json").is_file());
    assert!(!recovery_owner_active(&writer.recovery_dir));

    let mut restored = app();
    restored.recover(&writer.recovery_dir);
    assert!(restored.error.is_none(), "{:?}", restored.error);
    assert_eq!(restored.project.stories, writer.project.stories);
    assert_eq!(restored.project.assets, writer.project.assets);
    assert_eq!(restored.project.source, writer.project.source);
    assert_eq!(restored.project.paths, writer.project.paths);
    assert_eq!(restored.current, writer.current);
    assert!(restored.dirty());
    let unchanged = Project::open(&original).unwrap();
    assert_eq!(unchanged.stories.len(), 1);
    assert!(unchanged.assets.is_empty());

    let active = dir.path().join(format!("{}-{stamp}", std::process::id()));
    std::fs::create_dir(&active).unwrap();
    let before = snapshot(&restored.project);
    restored.recover(&active);
    assert!(restored.error.as_ref().unwrap().contains("仍在运行"));
    assert!(snapshot(&restored.project) == before);
    assert!(active.exists());
}
