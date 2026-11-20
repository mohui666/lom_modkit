use super::*;

pub(super) fn finish_saves(app: &mut App) {
    let ctx = egui::Context::default();
    let started = Instant::now();
    while app.saving() {
        app.poll_background_save(&ctx);
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "save worker timed out"
        );
        if app.saving() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn background_save_preserves_edits_made_after_its_snapshot_and_serializes_the_next_save() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("project");
    let mut app = app();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("第一份快照");
    assert!(app.save_at(&target, false));
    assert!(app.saving());
    assert!(app.project.source.is_none());
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("写入时继续编辑");
    finish_saves(&mut app);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.dirty());
    assert_eq!(
        Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
        "第一份快照"
    );
    assert_eq!(
        app.project.stories["main"]["nodes"][0]["text"],
        "写入时继续编辑"
    );

    assert!(app.save_at(&target, false));
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("再次保存最新修改");
    assert!(app.save_at(&target, false));
    finish_saves(&mut app);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty());
    assert_eq!(
        Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
        "再次保存最新修改"
    );
    let second = dir.path().join("second-project");
    let third = dir.path().join("third-project");
    assert!(app.save_at(&target, false));
    assert!(app.save_at(&second, false));
    assert!(app.save_at(&third, false));
    finish_saves(&mut app);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.project.source, Some(third.canonicalize().unwrap()));
    for target in [second, third] {
        assert_eq!(
            Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
            "再次保存最新修改"
        );
    }
}

#[test]
fn chapter_rename_during_first_save_keeps_the_new_name_and_prunes_the_saved_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("project");
    let mut app = app();
    assert!(app.save_at(&target, false));
    app.rename_dialog = Some(RenameDialog {
        target: RenameTarget::Chapter("main".into()),
        value: "renamed".into(),
        error: None,
        focus_name: false,
    });
    app.commit_rename().unwrap();
    assert!(app.save_at(&target, false));
    finish_saves(&mut app);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty());
    assert!(app.project.stories.contains_key("renamed"));
    assert!(!target.join("story/main.json").exists());
    let reopened = Project::open(&target).unwrap();
    assert_eq!(reopened.stories, app.project.stories);
    assert_eq!(reopened.paths, app.project.paths);
}

#[test]
fn background_save_failure_keeps_edits_and_does_not_execute_the_pending_switch() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("project");
    let mut project = Project::new();
    project.save_to(&target).unwrap();
    let mut app = app();
    app.install(Project::open(&target).unwrap());
    let mut external = Project::open(&target).unwrap();
    external.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("外部新进度");
    external.save_to(&target).unwrap();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("本窗口保留的内容");
    assert!(app.save_at(&target, false));
    app.request(Pending::New, &egui::Context::default());
    app.save_and_continue();
    assert!(app.waiting_to_continue());
    finish_saves(&mut app);
    assert!(app.error.as_ref().unwrap().contains("编辑器外修改"));
    assert!(matches!(app.pending, Some(Pending::New)));
    assert!(!app.waiting_to_continue());
    assert!(app.dirty());
    assert_eq!(
        app.project.stories["main"]["nodes"][0]["text"],
        "本窗口保留的内容"
    );
    assert_eq!(
        Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
        "外部新进度"
    );
}

#[test]
fn saved_and_discarded_project_switches_wait_for_the_active_writer() {
    for discard in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("project");
        let mut project = Project::new();
        project.save_to(&target).unwrap();
        let mut app = app();
        app.install(Project::open(&target).unwrap());
        let original_id = app.project.manifest["id"].clone();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("保存后切换");
        assert!(app.save_at(&target, false));
        let ctx = egui::Context::default();
        app.request(Pending::New, &ctx);
        if discard {
            app.discard_and_continue(&ctx);
        } else {
            app.save_and_continue();
        }
        assert!(app.waiting_to_continue());
        assert_eq!(app.project.manifest["id"], original_id);
        finish_saves(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.pending.is_none());
        assert_ne!(app.project.manifest["id"], original_id);
        assert!(app.project.source.is_none());
        assert_eq!(
            Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
            "保存后切换"
        );
    }
}

#[test]
fn cancelled_save_continuation_and_obsolete_completion_do_not_replace_the_live_project() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("project");
    let mut app = app();
    app.project.manifest["name"] = json!("原工程");
    assert!(app.save_at(&target, false));
    let ctx = egui::Context::default();
    app.request(Pending::New, &ctx);
    app.discard_and_continue(&ctx);
    app.cancel_pending_action();
    finish_saves(&mut app);
    assert_eq!(app.project.manifest["name"], "原工程");
    assert!(app.pending.is_none());

    assert!(app.save_at(&target, false));
    app.install(Project::new());
    app.project.manifest["name"] = json!("新工程继续编辑");
    app.track();
    finish_saves(&mut app);
    assert_eq!(app.project.manifest["name"], "新工程继续编辑");
    assert!(app.project.source.is_none());
    assert!(app.dirty());
}

#[test]
fn clearing_recovery_waits_until_the_active_recovery_write_has_finished() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("recovery");
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("恢复中的内容");
    app.track();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert!(app.saving());
    app.clear_recovery();
    finish_saves(&mut app);
    assert!(app.dirty());
    assert!(!app.recovery_dir.exists());
    assert!(app.recovery_paths.is_empty());
    assert!(app.recovery_assets.is_empty());
}

#[test]
fn update_preparation_leaves_unsaved_work_for_the_final_install_decision() {
    let mut app = app();
    app.project.manifest["name"] = json!("下载期间仍可继续创作");
    app.track();
    let before = snapshot(&app.project);
    let ctx = egui::Context::default();
    // No release is configured in the fixture, so this performs no download.
    app.request(Pending::EditorUpdate, &ctx);
    assert!(app.pending.is_none());
    app.request(Pending::InstallEditorUpdate, &ctx);
    assert!(matches!(app.pending, Some(Pending::InstallEditorUpdate)));
    app.cancel_pending_action();
    assert!(app.dirty());
    assert!(!app.allow_close);
    assert!(snapshot(&app.project) == before);
}

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
        app.checkpoint_base = dir.path().join("checkpoints");
        app.auto_save_project = true;
        app.auto_save_checkpoints = 1;
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
            json!("自动保存的新进度");
        app.track();
        app.last_auto_save = Instant::now() - Duration::from_secs(301);
        app.autosave();
        finish_saves(&mut app);
        assert!(app.error.is_none(), "{layout}: {:?}", app.error);
        assert!(!app.dirty(), "{layout}");
        assert!(app.pending.is_none());
        assert_eq!(
            Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
            "自动保存的新进度"
        );
        let checkpoints = checkpoints::list(&app.checkpoint_base, &target).unwrap();
        assert_eq!(checkpoints.len(), 1, "{layout}");
        assert_eq!(
            Project::open(&checkpoints[0].path).unwrap().stories,
            app.project.stories
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
        finish_saves(&mut app);
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
    finish_saves(&mut app);
    // A later frame observes the failed project write and creates recovery.
    app.autosave();
    finish_saves(&mut app);
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
    app.set_auto_save(true, 60, 4, &path).unwrap();
    let prefs = lom_core::load_json(&path).unwrap();
    assert_eq!(prefs["auto_save_project"], true);
    assert_eq!(prefs["auto_save_seconds"], 60);
    assert_eq!(prefs["auto_save_checkpoints"], 4);
    assert_eq!(app.auto_save_checkpoints, 4);
    assert_eq!(prefs["custom"], "keep");
    assert_eq!(prefs["ui_locale"], "chs");
    assert!(app.set_auto_save(true, 0, 4, &path).is_err());
    assert!(app
        .set_auto_save(true, 60, checkpoints::MAX_COUNT + 1, &path)
        .is_err());
    assert!(snapshot(&app.project) == before);
}

#[test]
fn automatic_checkpoints_rotate_restore_and_clear_only_the_current_project() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("project");
    let other = temp.path().join("another-project");
    let base = temp.path().join("checkpoints");
    let asset = "assets/revision.bin";
    let mut editor = app();
    editor.checkpoint_base = base.clone();
    editor.auto_save_project = true;
    editor.auto_save_checkpoints = 2;
    editor.recovery_dir = temp.path().join("recovery");
    assert!(editor.save_at(&source, false));
    finish_saves(&mut editor);
    assert!(checkpoints::list(&base, &source).unwrap().is_empty());

    for revision in 1..=3 {
        editor.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
            json!(format!("版本 {revision}"));
        editor.project.assets.insert(asset.into(), vec![revision]);
        editor.asset_dirty = true;
        editor.track();
        editor.last_auto_save = Instant::now() - Duration::from_secs(301);
        editor.autosave();
        finish_saves(&mut editor);
        assert!(editor.error.is_none());
    }
    let items = checkpoints::list(&base, &source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        Project::open(&items[0].path).unwrap().assets[asset],
        vec![3]
    );
    assert_eq!(
        Project::open(&items[1].path).unwrap().assets[asset],
        vec![2]
    );

    // Opening a copy is detached from both the original and the history.
    let mut copy = app();
    copy.perform(
        Pending::OpenCheckpoint(items[1].path.clone()),
        &egui::Context::default(),
    );
    assert!(copy.project.source.is_none());
    assert_eq!(copy.story()["nodes"][0]["text"], "版本 2");

    let source_before = editor.project.source.clone();
    editor.restore_checkpoint(&items[1].path);
    assert_eq!(editor.project.source, source_before);
    assert_eq!(editor.story()["nodes"][0]["text"], "版本 2");
    assert_eq!(editor.project.assets[asset], vec![2]);
    assert!(editor.dirty());
    assert_eq!(Project::open(&source).unwrap().assets[asset], vec![3]);
    editor.undo();
    assert_eq!(editor.story()["nodes"][0]["text"], "版本 3");
    editor.redo();
    assert!(editor.save_at(&source, false));
    finish_saves(&mut editor);
    assert_eq!(Project::open(&source).unwrap().assets[asset], vec![2]);
    assert_eq!(checkpoints::list(&base, &source).unwrap().len(), 2);

    let mut another = Project::new();
    another.save_to(&other).unwrap();
    checkpoints::write(&base, &other, &mut another, 2).unwrap();
    let manual = items[0].path.parent().unwrap().join("manual.txt");
    std::fs::write(&manual, "保留").unwrap();
    std::fs::create_dir_all(&editor.recovery_dir).unwrap();
    let recovery = editor.recovery_dir.join("session.json");
    std::fs::write(&recovery, "{}").unwrap();
    editor.clear_checkpoints();
    finish_saves(&mut editor);
    assert!(checkpoints::list(&base, &source).unwrap().is_empty());
    assert_eq!(checkpoints::list(&base, &other).unwrap().len(), 1);
    assert_eq!(Project::open(&source).unwrap().assets[asset], vec![2]);
    assert!(manual.is_file());
    assert!(recovery.is_file());
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
    finish_saves(&mut app);
    assert_eq!(Project::open(&root).unwrap().stories, baseline);
    app.pending = Some(Pending::Quit);
    app.last_auto_save = Instant::now() - Duration::from_secs(301);
    app.autosave();
    finish_saves(&mut app);
    assert_eq!(Project::open(&root).unwrap().stories, baseline);
    app.pending = None;
    // Recovery deliberately clears the saved baseline until the user confirms it.
    app.saved.stories.clear();
    app.last_auto_save = Instant::now() - Duration::from_secs(301);
    app.autosave();
    finish_saves(&mut app);
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
        finish_saves(&mut app);
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
    finish_saves(&mut app);
    assert!(app.recovery_dir.join("story/chapter1.json").exists());
    app.undo();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    finish_saves(&mut app);
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
    finish_saves(&mut app);
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
    finish_saves(&mut app);
    assert_eq!(app.recovery_assets, owned);
    let unknown = app.recovery_dir.join("assets/unknown.bin");
    std::fs::write(&unknown, b"unowned data").unwrap();
    lom_core::content_edit::remove(&mut app.project, &app.asset_selection).unwrap();
    app.track();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    finish_saves(&mut app);
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
fn recovery_refreshes_once_when_undo_returns_to_the_saved_dialogue() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("recovery");
    let original = app.project.stories["main"]["nodes"][0]["text"].clone();
    app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("应被撤销的对白");
    app.track();
    app.flush();
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    finish_saves(&mut app);
    assert_eq!(
        Project::open(&app.recovery_dir).unwrap().stories["main"]["nodes"][0]["text"],
        "应被撤销的对白"
    );
    let unknown = app.recovery_dir.join("assets/unknown.bin");
    std::fs::create_dir_all(unknown.parent().unwrap()).unwrap();
    std::fs::write(&unknown, b"unowned data").unwrap();

    app.undo();
    assert!(!app.dirty());
    app.needs_compile = false;
    app.last_recovery = Instant::now() - Duration::from_secs(29);
    let delay = app.idle_repaint_delay();
    assert!(delay > Duration::ZERO && delay <= Duration::from_secs(1));
    app.autosave();
    assert!(
        !app.saving(),
        "recovery still observes its 30-second interval"
    );
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    assert_eq!(app.idle_repaint_delay(), Duration::ZERO);
    app.autosave();
    assert!(app.saving());
    finish_saves(&mut app);
    assert_eq!(
        Project::open(&app.recovery_dir).unwrap().stories["main"]["nodes"][0]["text"],
        original
    );
    assert_eq!(std::fs::read(&unknown).unwrap(), b"unowned data");
    assert!(!app.dirty());
    assert_eq!(app.idle_repaint_delay(), Duration::MAX);
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    assert!(
        !app.saving(),
        "an unchanged clean project is not written again"
    );
    assert_eq!(app.idle_repaint_delay(), Duration::MAX);
}

#[test]
fn recovery_refreshes_after_cancelling_a_material_draft_in_a_clean_project() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hero.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let mut app = app();
    app.recovery_dir = dir.path().join("recovery");
    app.import_assets(vec![image], "test.cancelled", "原素材名", 0)
        .unwrap();
    assert!(app.save_at(&dir.path().join("source"), false));
    finish_saves(&mut app);
    assert!(!app.dirty());
    let key = app.asset_selection.clone();
    let mut draft: Value = serde_json::from_slice(&app.project.assets[&key]).unwrap();
    let original = draft["name"].clone();
    draft["name"] = json!("已取消的素材输入");
    app.content_panel =
        crate::content_panel::ContentPanel::pending_fixture(&app.project, &key, draft);
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    finish_saves(&mut app);
    let restored = Project::open(&app.recovery_dir).unwrap();
    let meta: Value = serde_json::from_slice(&restored.assets[&key]).unwrap();
    assert_eq!(meta["name"], "已取消的素材输入");

    app.content_panel.reset();
    assert!(!app.dirty());
    app.last_recovery = Instant::now() - Duration::from_secs(31);
    app.autosave();
    finish_saves(&mut app);
    let restored = Project::open(&app.recovery_dir).unwrap();
    let meta: Value = serde_json::from_slice(&restored.assets[&key]).unwrap();
    assert_eq!(meta["name"], original);
    assert!(!app.recovery_needs_refresh());
}

#[test]
fn save_and_recovery_capture_same_count_asset_renames_without_a_ui_dirty_flag() {
    for recovery in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let target = dir.path().join("project");
        let mut app = app();
        app.recovery_dir = if recovery {
            target.clone()
        } else {
            dir.path().join("recovery")
        };
        app.import_assets(vec![image], "test.renamed", "hero", 0)
            .unwrap();
        app.track();
        if recovery {
            app.last_recovery = Instant::now() - Duration::from_secs(31);
            app.autosave();
        } else {
            assert!(app.save_at(&target, false));
        }
        finish_saves(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);

        let key = app.asset_selection.clone();
        let mut meta: Value = serde_json::from_slice(&app.project.assets[&key]).unwrap();
        let prefix = key.strip_suffix("content.json").unwrap();
        let old = format!("{prefix}{}", meta["files"]["main"].as_str().unwrap());
        let new = format!("{prefix}renamed.png");
        let count = app.project.assets.len();
        // Exercise a direct core edit rather than the UI's reported mutation.
        let bytes = app.project.assets.remove(&old).unwrap();
        app.project.assets.insert(new.clone(), bytes);
        meta["files"]["main"] = json!("renamed.png");
        app.project
            .assets
            .insert(key.clone(), serde_json::to_vec(&meta).unwrap());
        assert_eq!(app.project.assets.len(), count);
        assert!(!app.asset_dirty);
        app.track();
        if recovery {
            app.last_recovery = Instant::now() - Duration::from_secs(31);
            app.autosave();
        } else {
            assert!(app.save_at(&target, false));
        }
        finish_saves(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        let restored = Project::open(&target).unwrap();
        assert!(restored.assets.contains_key(&new));
        assert!(!restored.assets.contains_key(&old));
        assert!(!target.join(old).exists());
        let meta: Value = serde_json::from_slice(&restored.assets[&key]).unwrap();
        assert_eq!(meta["files"]["main"], "renamed.png");
    }
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
    finish_saves(&mut app);
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
    finish_saves(&mut writer);
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
