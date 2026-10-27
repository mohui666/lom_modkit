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
    let exe = std::env::current_exe()?;
    let folder = exe.parent().context("无法确定编辑器位置")?;
    let name = if cfg!(windows) {
        "lom-asset-extractor.exe"
    } else {
        "lom-asset-extractor"
    };
    [
        folder.join("asset-extractor").join(name),
        folder.join("../Resources/asset-extractor").join(name),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .context("安装包缺少素材提取组件，请使用包含 asset-extractor 的完整编辑器安装包")
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

/// Output is a fresh private cache. A failure/cancellation never replaces the
/// active preview library; only a finished mapping can be selected by the UI.
pub fn extract(
    helper: &Path,
    game: &Path,
    cache: &Path,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Value) + Send + 'static,
) -> Result<Value> {
    validate_game(game)?;
    std::fs::create_dir_all(cache)?;
    let output = tempfile::Builder::new()
        .prefix("extracted-")
        .tempdir_in(cache)?;
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
    let mapping = lom_core::load_json(output.path().join("preview_map.json"))?;
    validate_mapping(output.path(), &mapping)?;
    ensure!(
        report["exported"].as_u64().unwrap_or(0) > 0,
        "未能提取任何游戏图片"
    );
    report["output_dir"] = serde_json::json!(output.keep());
    Ok(report)
}

pub(crate) fn validate_mapping(root: &Path, mapping: &Value) -> Result<()> {
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
    for value in files {
        let name = value.as_str().context("图片路径格式不正确")?;
        lom_core::package::canonical_archive_name(name)?;
        ensure!(name.starts_with("assets/"), "图片路径超出素材目录");
        lom_core::package::resolve_confined_file(root, &root.join(name))?;
        ensure!(root.join(name).is_file(), "提取结果缺少图片：{name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
        let error = extract(&helper, &game, &cache, cancel, |_| {}).unwrap_err();
        thread.join().unwrap();
        assert!(error.to_string().contains("取消"));
        assert_eq!(std::fs::read_dir(cache).unwrap().count(), 0);
    }
}
