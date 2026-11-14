//! Launch the bundled extractor without requiring an end-user Python install.
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub fn helper_path() -> Result<PathBuf> {
    crate::components::extractor()
}

pub fn validate_game(root: &Path) -> Result<()> {
    for file in ["sharedassets2.assets", "StreamingAssets/aa/catalog.json"] {
        ensure!(
            root.join("Mortal_Data").join(file).is_file(),
            "请选择包含 Mortal_Data 的《活侠传》安装目录；缺少 {file}"
        );
    }
    ensure!(
        ["StandaloneWindows64", "StandaloneWindows"]
            .iter()
            .any(|platform| root
                .join("Mortal_Data/StreamingAssets/aa")
                .join(platform)
                .is_dir()),
        "游戏目录缺少图片素材包（StandaloneWindows64 / StandaloneWindows）"
    );
    Ok(())
}

/// New images stay in a private staging directory. Incremental extraction only
/// publishes their references after the complete mapping has been validated.
pub fn extract(
    helper: &Path,
    game: &Path,
    cache: &Path,
    existing: Option<&Path>,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Value) + Send + 'static,
) -> Result<Value> {
    validate_game(game)?;
    let existing = existing.map(Path::canonicalize).transpose()?;
    let output = if let Some(root) = &existing {
        lom_core::package::resolve_confined_file(root, &root.join("preview_map.json"))?;
        let assets = root.join("assets");
        std::fs::create_dir_all(&assets)?;
        ensure!(
            assets.canonicalize()?.starts_with(root),
            "素材缓存目录指向已有素材库之外"
        );
        tempfile::Builder::new()
            .prefix("supplement-")
            .tempdir_in(assets)?
    } else {
        std::fs::create_dir_all(cache)?;
        tempfile::Builder::new()
            .prefix("extracted-")
            .tempdir_in(cache)?
    };
    ensure!(
        !output
            .path()
            .canonicalize()?
            .starts_with(game.canonicalize()?),
        "素材缓存必须位于游戏目录之外"
    );
    let mut command = Command::new(helper);
    command
        .args(["--game-dir"])
        .arg(game)
        .arg("--output-dir")
        .arg(output.path())
        .arg("--max-size")
        .arg("1024")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(root) = &existing {
        command.arg("--existing-library").arg(root);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().context("无法启动素材提取组件")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut result = None;
        for line in BufReader::new(stdout).lines() {
            let line = line?;
            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                match event["event"].as_str() {
                    Some("result") | Some("error") => result = Some(event),
                    Some("progress") => progress(event),
                    _ => {}
                }
            }
        }
        Ok::<_, std::io::Error>(result)
    });
    // Always drain stderr so a verbose dependency cannot deadlock the child.
    let errors = std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        let mut stream = BufReader::new(stderr);
        let _ = stream.by_ref().take(16_384).read_to_string(&mut text);
        let _ = std::io::copy(&mut stream, &mut std::io::sink());
        text
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let event = reader
        .join()
        .map_err(|_| anyhow::anyhow!("素材提取结果读取中断"))??;
    let stderr = errors.join().unwrap_or_default();
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "提取已取消，原素材库保持不变"
    );
    ensure!(
        status.success(),
        "{}",
        event
            .as_ref()
            .and_then(|v| v["message"].as_str())
            .unwrap_or(if stderr.is_empty() {
                "素材提取组件意外退出，请重新安装提取组件或检查游戏版本"
            } else {
                &stderr
            })
    );
    let mut report = event
        .filter(|v| v["event"] == "result")
        .context("提取组件没有返回完成结果，原素材库保持不变")?;
    ensure!(
        report["exported"].as_u64().unwrap_or(0) + report["reused"].as_u64().unwrap_or(0) > 0,
        "未能提取任何游戏图片"
    );
    let mut mapping = lom_core::load_json(output.path().join("preview_map.json"))?;
    if let Some(root) = existing {
        let details = report["failures"]
            .as_array()
            .into_iter()
            .flatten()
            .take(3)
            .map(|failure| {
                format!(
                    "{}：{}",
                    failure["address"].as_str().unwrap_or("图片"),
                    failure["error"].as_str().unwrap_or("提取失败")
                )
            })
            .collect::<Vec<_>>()
            .join("；");
        ensure!(
            report["failed"].as_u64().unwrap_or(0) == 0,
            "本次补充有图片提取失败，原素材库保持不变：{details}"
        );
        let has_new_files = publish_incremental(&root, output.path(), &mut mapping, &cancel)?;
        if has_new_files {
            let _ = output.keep();
        }
        report["output_dir"] = serde_json::json!(root);
    } else {
        validate_mapping(output.path(), &mapping)?;
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "提取已取消，原素材库保持不变"
        );
        report["output_dir"] = serde_json::json!(output.keep());
    }
    Ok(report)
}

fn mapping_files(mapping: &Value) -> Result<Vec<&Value>> {
    ensure!(
        mapping["characters"].is_object() && mapping["views"].is_object(),
        "提取映射不完整"
    );
    let mut files = Vec::new();
    for character in mapping["characters"].as_object().unwrap().values() {
        let portraits = character["portraits"]
            .as_object()
            .context("人物立绘映射不完整")?;
        ensure!(
            character["first"]
                .as_str()
                .is_some_and(|s| portraits.contains_key(s)),
            "人物缺少默认立绘"
        );
        files.extend(portraits.values());
    }
    files.extend(mapping["views"].as_object().unwrap().values());
    if let Some(pictures) = mapping.get("pictures") {
        files.extend(pictures.as_object().context("插图映射必须是对象")?.values());
    }
    if let Some(book) = mapping.get("event_book") {
        files.push(book);
    }
    if let Some(moods) = mapping.get("moods") {
        for mood in moods.as_object().context("气泡映射必须是对象")?.values() {
            for facing in ["left", "right"] {
                let animation = &mood[facing];
                let frames = animation["frames"].as_array().context("气泡缺少动画帧")?;
                ensure!(
                    !frames.is_empty() && frames.len() <= 240,
                    "气泡动画帧数量无效"
                );
                ensure!(
                    animation["duration"].as_f64().is_some_and(|v| v > 0.0),
                    "气泡动画时长无效"
                );
                let mut previous = 0.0;
                for frame in frames {
                    let time = frame["time"].as_f64().context("气泡帧缺少时间")?;
                    ensure!(
                        time >= previous && time <= animation["duration"].as_f64().unwrap(),
                        "气泡帧时间无效"
                    );
                    previous = time;
                    files.push(&frame["file"]);
                }
            }
        }
    }
    Ok(files)
}

pub(crate) fn validate_mapping(root: &Path, mapping: &Value) -> Result<()> {
    for value in mapping_files(mapping)? {
        let name = value.as_str().context("图片路径格式不正确")?;
        lom_core::package::canonical_archive_name(name)?;
        ensure!(name.starts_with("assets/"), "图片路径超出素材目录");
        lom_core::package::resolve_confined_file(root, &root.join(name))?;
        ensure!(root.join(name).is_file(), "提取结果缺少图片：{name}");
    }
    Ok(())
}

fn publish_incremental(
    root: &Path,
    staging: &Path,
    mapping: &mut Value,
    cancel: &AtomicBool,
) -> Result<bool> {
    let prefix = staging
        .strip_prefix(root)?
        .to_str()
        .context("素材缓存路径无效")?
        .replace('\\', "/");
    lom_core::package::canonical_archive_name(&prefix)?;
    ensure!(
        prefix.starts_with("assets/"),
        "新增图片必须保存在已有素材库中"
    );
    let previous = lom_core::load_json(root.join("preview_map.json"))?;
    let mut paths = std::collections::BTreeMap::new();
    for value in mapping_files(mapping)? {
        let name = value.as_str().context("图片路径格式不正确")?;
        lom_core::package::canonical_archive_name(name)?;
        ensure!(name.starts_with("assets/"), "图片路径超出素材目录");
        if staging.join(name).try_exists()? {
            lom_core::package::resolve_confined_file(staging, &staging.join(name))?;
            paths.insert(name.to_owned(), format!("{prefix}/{name}"));
        } else {
            lom_core::package::resolve_confined_file(root, &root.join(name))?;
        }
    }
    let mut changed_files = 0;
    let mut remap = |value: &mut Value, old: &Value| {
        // A generated file may use the same conventional name as a cached
        // alias. Retain that alias's original reference and rebase only the
        // new/missing entry that actually needs the staged image.
        if &*value == old && value.as_str().is_some_and(|name| root.join(name).is_file()) {
            return;
        }
        if let Some(rebased) = value.as_str().and_then(|name| paths.get(name)) {
            *value = Value::String(rebased.clone());
            changed_files += 1;
        }
    };
    for (id, character) in mapping["characters"].as_object_mut().unwrap() {
        for (key, file) in character["portraits"].as_object_mut().unwrap() {
            remap(file, &previous["characters"][id]["portraits"][key]);
        }
    }
    for group in ["views", "pictures"] {
        if let Some(entries) = mapping.get_mut(group).and_then(Value::as_object_mut) {
            for (key, file) in entries {
                remap(file, &previous[group][key]);
            }
        }
    }
    if let Some(file) = mapping.get_mut("event_book") {
        remap(file, &previous["event_book"]);
    }
    if let Some(moods) = mapping.get_mut("moods").and_then(Value::as_object_mut) {
        for (key, mood) in moods {
            for facing in ["left", "right"] {
                for (index, frame) in mood[facing]["frames"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .enumerate()
                {
                    remap(
                        &mut frame["file"],
                        &previous["moods"][key][facing]["frames"][index]["file"],
                    );
                }
            }
        }
    }
    validate_mapping(root, mapping)?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "提取已取消，原素材库保持不变"
    );
    // This is the sole publication point. Until here the old mapping and every
    // referenced old image remain untouched; TempDir removes new files on error.
    lom_core::project::atomic_write(
        &root.join("preview_map.json"),
        &lom_core::stable_json(mapping)?,
    )?;
    Ok(changed_files > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn incremental_fixture(root: &Path) -> Value {
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/old.png"), b"keep original image").unwrap();
        let mapping = json!({"portrait_canvas":"sprite_rect","characters":{"player":{"name":"assets/new.png","first":"normal","portraits":{"normal":"assets/old.png"}}},"views":{"old":"assets/old.png"},"pictures":{}});
        std::fs::write(
            root.join("preview_map.json"),
            lom_core::stable_json(&mapping).unwrap(),
        )
        .unwrap();
        mapping
    }

    #[test]
    fn incremental_publish_reuses_old_images_and_rebases_only_new_file_references() {
        let root = tempfile::tempdir().unwrap();
        let mut mapping = incremental_fixture(root.path());
        let staging = tempfile::Builder::new()
            .prefix("supplement-")
            .tempdir_in(root.path().join("assets"))
            .unwrap();
        std::fs::create_dir(staging.path().join("assets")).unwrap();
        std::fs::write(staging.path().join("assets/new.png"), b"new image").unwrap();
        std::fs::write(
            staging.path().join("assets/old.png"),
            b"new conventional name",
        )
        .unwrap();
        mapping["pictures"]["new"] = json!("assets/new.png");
        mapping["pictures"]["same_file_name"] = json!("assets/old.png");
        mapping["event_book"] = json!("assets/new.png");
        let animation = json!({"frames":[{"time":0.0,"file":"assets/new.png"}],"duration":1.0});
        mapping["moods"] = json!({"happy":{"left":animation,"right":animation}});
        assert!(publish_incremental(
            root.path(),
            staging.path(),
            &mut mapping,
            &AtomicBool::new(false)
        )
        .unwrap());
        validate_mapping(root.path(), &mapping).unwrap();
        assert_eq!(
            mapping["characters"]["player"]["portraits"]["normal"],
            "assets/old.png"
        );
        assert_eq!(mapping["characters"]["player"]["name"], "assets/new.png");
        let new = mapping["pictures"]["new"].as_str().unwrap();
        assert!(new.starts_with("assets/supplement-"));
        assert_eq!(mapping["views"]["old"], "assets/old.png");
        assert_ne!(mapping["pictures"]["same_file_name"], "assets/old.png");
        assert_eq!(mapping["event_book"], new);
        assert_eq!(mapping["moods"]["happy"]["left"]["frames"][0]["file"], new);
        assert_eq!(
            std::fs::read(root.path().join("assets/old.png")).unwrap(),
            b"keep original image"
        );
        assert_eq!(
            lom_core::load_json(root.path().join("preview_map.json")).unwrap(),
            mapping
        );
    }

    #[test]
    fn incremental_failure_and_cancellation_keep_old_mapping_and_remove_staging() {
        let root = tempfile::tempdir().unwrap();
        let original = incremental_fixture(root.path());
        let original_bytes = std::fs::read(root.path().join("preview_map.json")).unwrap();
        for cancelled in [false, true] {
            let staging = tempfile::Builder::new()
                .prefix("supplement-")
                .tempdir_in(root.path().join("assets"))
                .unwrap();
            let stage_path = staging.path().to_path_buf();
            std::fs::create_dir(staging.path().join("assets")).unwrap();
            std::fs::write(staging.path().join("assets/new.png"), b"new image").unwrap();
            let mut mapping = original.clone();
            mapping["pictures"]["new"] = json!("assets/new.png");
            if !cancelled {
                mapping["views"]["missing"] = json!("assets/unavailable.png");
            }
            assert!(publish_incremental(
                root.path(),
                staging.path(),
                &mut mapping,
                &AtomicBool::new(cancelled)
            )
            .is_err());
            assert_eq!(
                std::fs::read(root.path().join("preview_map.json")).unwrap(),
                original_bytes
            );
            assert_eq!(
                std::fs::read(root.path().join("assets/old.png")).unwrap(),
                b"keep original image"
            );
            drop(staging);
            assert!(!stage_path.exists());
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("assets"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn incremental_complete_cache_needs_no_new_image_directory() {
        let root = tempfile::tempdir().unwrap();
        let mut mapping = incremental_fixture(root.path());
        mapping["pictures"]["new_alias"] = json!("assets/old.png");
        let staging = tempfile::Builder::new()
            .prefix("supplement-")
            .tempdir_in(root.path().join("assets"))
            .unwrap();
        assert!(!publish_incremental(
            root.path(),
            staging.path(),
            &mut mapping,
            &AtomicBool::new(false)
        )
        .unwrap());
        drop(staging);
        validate_mapping(root.path(), &mapping).unwrap();
        assert_eq!(
            std::fs::read_dir(root.path().join("assets"))
                .unwrap()
                .count(),
            1
        );
    }
    #[test]
    fn accepts_both_installed_game_platform_layouts() {
        for platform in ["StandaloneWindows64", "StandaloneWindows"] {
            let root = tempfile::tempdir().unwrap();
            let aa = root.path().join("Mortal_Data/StreamingAssets/aa");
            std::fs::create_dir_all(aa.join(platform)).unwrap();
            std::fs::write(aa.join("catalog.json"), "{}").unwrap();
            std::fs::write(
                root.path().join("Mortal_Data/sharedassets2.assets"),
                "fixture",
            )
            .unwrap();
            validate_game(root.path()).unwrap();
        }
    }
    #[test]
    fn rejects_incomplete_or_escaping_preview_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let mut map = json!({"characters":{"player":{"first":"normal","portraits":{"normal":"assets/normal.png"}}},"views":{}});
        assert!(validate_mapping(dir.path(), &map).is_err());
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/normal.png"), b"fixture").unwrap();
        validate_mapping(dir.path(), &map).unwrap();
        map["views"]["bad"] = json!("../outside.png");
        assert!(validate_mapping(dir.path(), &map).is_err());
        map["views"] = json!({});
        map["pictures"] = json!({"valid":"assets/normal.png"});
        validate_mapping(dir.path(), &map).unwrap();
        map["pictures"]["escape"] = json!("../outside.png");
        assert!(validate_mapping(dir.path(), &map).is_err());
    }
    #[cfg(unix)]
    fn fake_game_and_helper(root: &Path, script: &str) -> (PathBuf, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let game = root.join("游戏 安装");
        let aa = game.join("Mortal_Data/StreamingAssets/aa");
        std::fs::create_dir_all(aa.join("StandaloneWindows")).unwrap();
        std::fs::write(aa.join("catalog.json"), "{}").unwrap();
        std::fs::write(game.join("Mortal_Data/sharedassets2.assets"), "fixture").unwrap();
        let helper = root.join("fake-helper");
        std::fs::write(&helper, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        (game, helper, root.join("素材 缓存"))
    }
    #[test]
    #[cfg(unix)]
    fn helper_protocol_publishes_only_a_complete_library_and_keeps_source_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let (game, helper, cache) = fake_game_and_helper(
            root.path(),
            r#"
mkdir -p "$4/assets"
printf '%s' fixture > "$4/assets/image.png"
printf '%s' '{"characters":{},"views":{"black":"assets/image.png"}}' > "$4/preview_map.json"
printf '%s\n' '{"event":"progress","done":1,"total":1}' '{"event":"result","exported":1,"failed":0}'
"#,
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let report = extract(
            &helper,
            &game,
            &cache,
            None,
            Arc::new(AtomicBool::new(false)),
            move |v| {
                tx.send(v).unwrap();
            },
        )
        .unwrap();
        assert_eq!(rx.recv().unwrap()["done"], 1);
        assert!(Path::new(report["output_dir"].as_str().unwrap())
            .join("preview_map.json")
            .is_file());
        assert_eq!(
            std::fs::read_to_string(game.join("Mortal_Data/sharedassets2.assets")).unwrap(),
            "fixture"
        );
    }
    #[test]
    #[cfg(unix)]
    fn helper_failure_and_cancellation_do_not_publish_or_leave_staging_cache() {
        let root = tempfile::tempdir().unwrap();
        let (game, helper, cache) = fake_game_and_helper(
            root.path(),
            "printf '%s\\n' '{\"event\":\"error\",\"message\":\"fixture failure\"}'\nexit 1",
        );
        let error = extract(
            &helper,
            &game,
            &cache,
            None,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("fixture failure"));
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
        std::fs::write(&helper, "#!/bin/sh\nexec sleep 10\n").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            signal.store(true, Ordering::Relaxed);
        });
        let error = extract(&helper, &game, &cache, None, cancel, |_| {}).unwrap_err();
        thread.join().unwrap();
        assert!(error.to_string().contains("取消"));
        assert_eq!(std::fs::read_dir(cache).unwrap().count(), 0);
    }
}
