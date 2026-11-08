//! Generate local previews with the installed game's particle renderer.
use anyhow::{ensure, Context, Result};
use lom_core::{game_tools, project::Project};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub fn cache_root() -> PathBuf {
    lom_core::content::default_repository_root()
        .parent()
        .unwrap()
        .join("effect-previews")
}

fn capture_project(request_id: &str, names: &[String]) -> Result<Project> {
    ensure!(
        request_id.len() == 32 && request_id.bytes().all(|c| c.is_ascii_hexdigit()),
        "原版特效预览编号无效"
    );
    ensure!(!names.is_empty(), "没有可提取的原版特效");
    let names_json = serde_json::to_string(names)?;
    let code = format!(
        "mod_capture_effect_previews(\"{request_id}\", {})\nwhile mod_capture_effect_previews_busy() do wait(0.1) end",
        serde_json::to_string(&names_json)?
    );
    let mut project = Project::new();
    project.manifest = json!({"format":3,"package_format":3,"story_schema":2,"content_schema":1,
        "id":game_tools::PREVIEW_ID,"campaign_id":game_tools::PREVIEW_ID,
        "name":"原版特效预览提取","version":"0.0.0-preview","author":"lom_modkit",
        "description":"在独立测试场景中生成本机原版粒子预览，不修改作者作品。",
        "entry":"main","min_host_version":"1.4.5",
        "campaign":{"new_game":true,"disable_official_events":true}});
    project.stories.insert(
        "main".into(),
        json!({"story_schema":2,"id":"main","start":"capture",
        "nodes":[{"id":"capture","type":"raw","code":code},{"id":"done","type":"end"}]}),
    );
    Ok(project)
}

pub fn remember(path: &Path) -> Result<()> {
    remember_at(&cache_root(), path)
}

fn valid_request_id(request: &str) -> bool {
    request.len() == 32 && request.bytes().all(|c| c.is_ascii_hexdigit())
}

fn confined_directory(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let path = path.canonicalize()?;
    ensure!(
        path.parent() == Some(root.as_path()),
        "特效预览必须来自本机生成目录"
    );
    Ok(path)
}

fn request_ids_at(root: &Path) -> Result<Vec<String>> {
    let pointer = root.join("current.json");
    if !pointer.try_exists()? {
        return Ok(Vec::new());
    }
    let current = lom_core::load_json(pointer)?;
    ensure!(current["format"] == 1, "原版特效缓存索引版本不支持");
    // Existing format-1 captures used a single request_id. Keep those real
    // frames when the next successful capture appends the multi-directory index.
    let requests = if let Some(values) = current.get("request_ids") {
        values
            .as_array()
            .context("原版特效缓存索引缺少请求编号列表")?
            .as_slice()
    } else {
        std::slice::from_ref(
            current
                .get("request_id")
                .context("原版特效缓存索引缺少请求编号")?,
        )
    };
    let mut ids = Vec::new();
    for value in requests {
        let id = value
            .as_str()
            .filter(|id| valid_request_id(id))
            .context("原版特效缓存请求编号不合法")?;
        confined_directory(root, &root.join(id))?;
        if !ids.iter().any(|previous| previous == id) {
            ids.push(id.to_owned());
        }
    }
    Ok(ids)
}

fn remember_at(root: &Path, path: &Path) -> Result<()> {
    let request_id = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("特效预览目录无效")?;
    ensure!(valid_request_id(request_id), "特效预览目录编号无效");
    confined_directory(root, path)?;
    ensure!(
        path.canonicalize()? == root.join(request_id).canonicalize()?,
        "特效预览必须来自本机生成目录"
    );
    crate::preview::EffectLibrary::load(path)?;
    let mut ids = request_ids_at(root)?;
    if ids.iter().any(|id| id == request_id) {
        return Ok(());
    }
    ids.push(request_id.to_owned());
    lom_core::project::atomic_write(
        &root.join("current.json"),
        &lom_core::stable_json(&json!({"format":1,"request_ids":ids}))?,
    )
}

fn requested_names(request: Option<Vec<String>>) -> Result<Vec<String>> {
    let catalog: Vec<String> = lom_core::validate::editor_data()["effects"]
        .as_array()
        .context("缺少原版特效目录")?
        .iter()
        .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
        .collect();
    let available: BTreeSet<_> = catalog.iter().map(String::as_str).collect();
    let requested = request.unwrap_or_else(|| catalog.clone());
    let mut seen = BTreeSet::new();
    let mut names = Vec::new();
    for name in requested {
        ensure!(
            available.contains(name.as_str()),
            "不在原版特效目录中的名称：{name}"
        );
        if seen.insert(name.clone()) {
            names.push(name);
        }
    }
    ensure!(!names.is_empty(), "当前没有需要提取的原版特效");
    Ok(names)
}

fn failed_capture(directory: &Path, names: &[String], message: String) -> Result<Value> {
    if directory.join("effect_map.json").try_exists()? {
        let library = crate::preview::EffectLibrary::load(directory)
            .with_context(|| format!("{message}；已完成的特效缓存无法载入"))?;
        let completed: Vec<_> = library.names().collect();
        ensure!(
            completed
                .iter()
                .all(|name| names.iter().any(|requested| requested.as_str() == *name)),
            "{message}；已完成缓存包含本次未请求的特效"
        );
        if !completed.is_empty() {
            return Ok(json!({"ok":false,"partial":true,"error":message,
                "effect_library":directory,"effects":completed.len(),"total":names.len(),
                "message":"已完成的特效缓存仍可使用；本次提取未全部完成。"}));
        }
    }
    anyhow::bail!(message)
}

pub fn capture(
    game: &Path,
    runtime: &Path,
    names: Option<Vec<String>>,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Value),
) -> Result<Value> {
    capture_at(&cache_root(), game, runtime, names, cancel, progress)
}

fn capture_at(
    root: &Path,
    game: &Path,
    runtime: &Path,
    request: Option<Vec<String>>,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Value),
) -> Result<Value> {
    let all = request.is_none();
    let mut names = requested_names(request)?;
    if all {
        let ids = request_ids_at(root)?;
        if let Some(library) = crate::preview::EffectLibrary::current_at(root)? {
            names.retain(|name| !library.contains(name));
            if names.is_empty() {
                let directories: Vec<_> = ids.iter().map(|id| root.join(id)).collect();
                return Ok(
                    json!({"ok":true,"cached":true,"effect_libraries":directories,
                    "effects":library.names().count(),"message":"所需特效预览均已缓存，直接复用；未启动游戏。"}),
                );
            }
        }
    }
    ensure!(
        cfg!(windows),
        "生成原版特效预览需要 Windows 游戏；已有缓存可离线播放"
    );
    ensure!(
        !game.as_os_str().is_empty() && !runtime.as_os_str().is_empty(),
        "请先在“游戏与 MOD”选择游戏目录并完成配置"
    );
    let identity = Project::new();
    let request_id = identity.manifest["campaign_id"]
        .as_str()
        .and_then(|s| s.strip_prefix("campaign_"))
        .context("无法生成特效预览编号")?;
    let project = capture_project(request_id, &names)?;
    let directory = root.join(request_id);
    std::fs::create_dir_all(root)?;
    let was_running = game_tools::is_game_running()?;
    let installed = game_tools::install_runtime(game, runtime)?;
    ensure!(
        !(was_running && installed["changed"] == true),
        "游戏 Host 刚刚更新，请退出游戏后重新提取特效预览"
    );
    let staging = lom_core::workdir::create()?;
    let package = staging.path().join("__lom_modkit_preview.lommod");
    project.export(&package)?;
    game_tools::remove_preview_packages(game)?;
    game_tools::install_mod(game, &package, true)?;
    game_tools::request_preview(game, "main", "capture")?;
    progress(
        json!({"message":"正在启动独立游戏场景，生成原版特效预览…","done":0,"total":names.len()}),
    );
    game_tools::launch_game()?;
    let started = Instant::now();
    let mut last_progress = Value::Null;
    loop {
        if cancel.load(Ordering::Relaxed) {
            std::fs::create_dir_all(&directory)?;
            std::fs::write(directory.join("cancel"), [])?;
            return failed_capture(&directory, &names, "原版特效预览提取已取消".into());
        }
        let status_path = directory.join("status.json");
        if status_path.is_file() {
            let mut status = lom_core::load_json(&status_path)?;
            status["done"] = status["completed"].clone();
            match status["state"].as_str() {
                Some("complete") => {
                    let library = crate::preview::EffectLibrary::load(&directory)?;
                    let count = library.names().count();
                    ensure!(
                        count == names.len() && names.iter().all(|name| library.contains(name)),
                        "原版特效预览没有完整生成：{count}/{}",
                        names.len()
                    );
                    return Ok(json!({"ok":true,"effect_library":directory,"effects":count,
                        "message":"原版特效预览已生成，已保存在本机；不会加入作品包。"}));
                }
                Some("error") => {
                    return failed_capture(
                        &directory,
                        &names,
                        format!(
                            "原版特效预览提取失败：{}",
                            status["message"].as_str().unwrap_or("游戏未提供错误信息")
                        ),
                    )
                }
                Some("cancelled") => {
                    return failed_capture(&directory, &names, "原版特效预览提取已取消".into())
                }
                Some("running") => {
                    if status != last_progress {
                        progress(status.clone());
                        last_progress = status;
                    }
                }
                _ => anyhow::bail!("游戏返回了未知的特效预览状态"),
            }
        }
        if started.elapsed() >= Duration::from_secs(600) {
            return failed_capture(
                &directory,
                &names,
                "等待游戏生成原版特效预览超时，请检查游戏窗口与 Host 日志".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(root: &Path, id: &str, names: &[String]) -> PathBuf {
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).unwrap();
        image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 0]))
            .save(directory.join("frame.png"))
            .unwrap();
        let effects: serde_json::Map<String, Value> = names
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    json!({
            "frames":[{"file":"frame.png","time":0}],"duration":1,"preview_time":0,
            "looping":false,"complete":true,"tail_frames":[],"tail_duration":0,
            "tail_sample_time":0.5,"tail_complete":true}),
                )
            })
            .collect();
        std::fs::write(
            directory.join("effect_map.json"),
            serde_json::to_vec(&json!({
            "format":1,"source":"game-render","alpha_mode":"premultiplied","width":2,"height":1,
            "world_width":19.2,"world_height":10.8,"effects":effects}))
            .unwrap(),
        )
        .unwrap();
        directory
    }

    #[test]
    fn requested_effects_are_catalog_valid_and_deduplicated_without_expanding_to_all() {
        let names = requested_names(Some(vec![
            "Hit_001".into(),
            "Glow_001".into(),
            "Hit_001".into(),
        ]))
        .unwrap();
        assert_eq!(names, ["Hit_001", "Glow_001"]);
        assert!(requested_names(Some(Vec::new())).is_err());
        assert!(requested_names(Some(vec!["not_an_official_effect".into()])).is_err());
        let project = capture_project("0123456789abcdef0123456789abcdef", &names[..1]).unwrap();
        let code = project.stories["main"]["nodes"][0]["code"]
            .as_str()
            .unwrap();
        assert!(code.contains("Hit_001"));
        assert!(!code.contains("Glow_001"));
    }

    #[test]
    fn complete_cached_all_request_reuses_directories_before_game_setup_or_launch() {
        let root = tempfile::tempdir().unwrap();
        let names = requested_names(None).unwrap();
        let first = library(
            root.path(),
            "11111111111111111111111111111111",
            &names[..names.len() / 2],
        );
        let second = library(
            root.path(),
            "22222222222222222222222222222222",
            &names[names.len() / 2..],
        );
        let frame = std::fs::read(first.join("frame.png")).unwrap();
        remember_at(root.path(), &first).unwrap();
        remember_at(root.path(), &second).unwrap();
        let report = capture_at(
            root.path(),
            Path::new(""),
            Path::new(""),
            None,
            Arc::new(AtomicBool::new(false)),
            |_| panic!("cached request must not start rendering"),
        )
        .unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["cached"], true);
        assert_eq!(report["effects"], names.len());
        assert_eq!(report["effect_libraries"], json!([first, second]));
        assert_eq!(std::fs::read(first.join("frame.png")).unwrap(), frame);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
    }

    #[test]
    fn remember_appends_unique_owned_directories_and_preserves_the_previous_index_on_error() {
        let root = tempfile::tempdir().unwrap();
        let names = ["Hit_001".into()];
        let first = library(root.path(), "11111111111111111111111111111111", &names);
        let second = library(root.path(), "22222222222222222222222222222222", &names);
        remember_at(root.path(), &first).unwrap();
        remember_at(root.path(), &second).unwrap();
        remember_at(root.path(), &first).unwrap();
        let pointer = root.path().join("current.json");
        assert_eq!(
            lom_core::load_json(&pointer).unwrap()["request_ids"],
            json!([
                "11111111111111111111111111111111",
                "22222222222222222222222222222222"
            ])
        );
        let previous = std::fs::read(&pointer).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let foreign = library(outside.path(), "33333333333333333333333333333333", &names);
        assert!(remember_at(root.path(), &foreign).is_err());
        assert!(remember_at(root.path(), &root.path().join("../escape")).is_err());
        assert_eq!(std::fs::read(&pointer).unwrap(), previous);
        std::fs::write(&pointer, r#"{"format":1,"request_ids":["../escape"]}"#).unwrap();
        assert!(remember_at(root.path(), &second).is_err());
    }

    #[test]
    fn legacy_single_pointer_preserves_existing_frames_when_appending_and_merging() {
        let root = tempfile::tempdir().unwrap();
        let first = library(
            root.path(),
            "11111111111111111111111111111111",
            &["Blood_001".into()],
        );
        let second = library(
            root.path(),
            "22222222222222222222222222222222",
            &["Glow_001".into()],
        );
        let frame = std::fs::read(first.join("frame.png")).unwrap();
        let pointer = root.path().join("current.json");
        std::fs::write(
            &pointer,
            r#"{"format":1,"request_id":"11111111111111111111111111111111"}"#,
        )
        .unwrap();
        let original = crate::preview::EffectLibrary::current_at(root.path())
            .unwrap()
            .unwrap();
        assert!(original.contains("Blood_001"));
        assert!(!original.contains("Glow_001"));
        remember_at(root.path(), &second).unwrap();
        let index = lom_core::load_json(&pointer).unwrap();
        assert_eq!(index["format"], 1);
        assert_eq!(
            index["request_ids"],
            json!([
                "11111111111111111111111111111111",
                "22222222222222222222222222222222"
            ])
        );
        assert!(index.get("request_id").is_none());
        let merged = crate::preview::EffectLibrary::current_at(root.path())
            .unwrap()
            .unwrap();
        assert!(merged.contains("Blood_001"));
        assert!(merged.contains("Glow_001"));
        assert_eq!(std::fs::read(first.join("frame.png")).unwrap(), frame);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
    }

    #[test]
    fn cancellation_keeps_completed_effects_available_without_reporting_batch_success() {
        let root = tempfile::tempdir().unwrap();
        let names = ["Hit_001".into(), "Glow_001".into()];
        let directory = library(root.path(), "11111111111111111111111111111111", &names[..1]);
        let report = failed_capture(&directory, &names, "原版特效预览提取已取消".into()).unwrap();
        assert_eq!(report["ok"], false);
        assert_eq!(report["partial"], true);
        assert_eq!(report["error"], "原版特效预览提取已取消");
        assert_eq!(report["effect_library"], json!(directory));
        assert_eq!(report["effects"], 1);
        assert_eq!(report["total"], 2);
        assert!(failed_capture(root.path(), &names, "cancelled".into()).is_err());
    }
    #[test]
    fn capture_is_an_isolated_development_package_with_no_author_assets() {
        let project = capture_project(
            "0123456789abcdef0123456789abcdef",
            &["Glow_001".into(), "Hit_001".into()],
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("__lom_modkit_preview.lommod");
        project.export(&package).unwrap();
        let inspected = lom_core::package::inspect_package(&package).unwrap();
        assert_eq!(inspected["ok"], true);
        assert_eq!(project.manifest["id"], game_tools::PREVIEW_ID);
        assert_eq!(project.manifest["min_host_version"], "1.4.5");
        assert_eq!(
            project.manifest["campaign"]["disable_official_events"],
            true
        );
        assert!(project.assets.is_empty());
        assert!(project.source.is_none());
        let code = project.stories["main"]["nodes"][0]["code"]
            .as_str()
            .unwrap();
        assert!(code.contains("mod_capture_effect_previews_busy()"));
        assert!(capture_project("../outside", &["Glow_001".into()]).is_err());
    }

    #[test]
    #[ignore = "requires Windows, LOM_EFFECT_CAPTURE_GAME and LOM_EFFECT_CAPTURE_RUNTIME; launches the installed game"]
    fn windows_live_demand_capture_incremental_merge_and_cache_reuse() -> Result<()> {
        ensure!(
            cfg!(windows),
            "此本机集成测试仅支持 Windows，且会启动实际游戏"
        );
        let game = std::env::var_os("LOM_EFFECT_CAPTURE_GAME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .context("必须设置 LOM_EFFECT_CAPTURE_GAME 为实际游戏目录")?;
        let runtime = std::env::var_os("LOM_EFFECT_CAPTURE_RUNTIME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .context("必须设置 LOM_EFFECT_CAPTURE_RUNTIME 为实际 Host 目录")?;
        let root = cache_root();
        let evidence_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?
            .join("out/verification-v1.4.5/original-demand-game-check.json");
        let save = |value: &Value| {
            lom_core::project::atomic_write(&evidence_path, &lom_core::stable_json(value)?)
        };
        let mut evidence = json!({"ok":false,"game_tested":false,
            "game_dir":game,"runtime_dir":runtime,"cache_root":root,"batches":[]});
        for (batch, names) in [
            vec!["Blood_001".to_owned()],
            [
                "Glow_001",
                "EventBubble_001",
                "Rain_001",
                "Hit_001",
                "Sword_Fight_001",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ]
        .into_iter()
        .enumerate()
        {
            let started = Instant::now();
            let result = capture(
                &game,
                &runtime,
                Some(names.clone()),
                Arc::new(AtomicBool::new(false)),
                |status| eprintln!("live effect capture progress: {status}"),
            );
            let elapsed = started.elapsed().as_secs_f64();
            let report = match &result {
                Ok(report) => report.clone(),
                Err(error) => json!({"ok":false,"error":crate::error_ui::capture(error)}),
            };
            let directory = report["effect_library"].as_str().map(PathBuf::from);
            eprintln!(
                "Live batch {}: {elapsed:.3}s; directory={directory:?}; report={report}",
                batch + 1
            );
            evidence["batches"].as_array_mut().unwrap().push(json!({
                "requested":names,"elapsed_seconds":elapsed,"directory":directory,
                "report":report,"remembered":false}));
            save(&evidence)?;
            result.with_context(|| format!("第 {} 批实际游戏捕获失败", batch + 1))?;
            ensure!(
                report["ok"] == true,
                "第 {} 批未完整捕获：{report}",
                batch + 1
            );
            ensure!(
                report["effects"].as_u64() == Some(names.len() as u64),
                "批次捕获数量与请求不一致"
            );
            evidence["game_tested"] = json!(true);
            let directory = directory.context("按需捕获必须返回本批实际目录")?;
            remember(&directory)?;
            evidence["batches"][batch]["remembered"] = json!(true);
            save(&evidence)?;
        }
        let expected = [
            "Blood_001",
            "Glow_001",
            "EventBubble_001",
            "Rain_001",
            "Hit_001",
            "Sword_Fight_001",
        ];
        let merged = crate::preview::EffectLibrary::current()?.context("实际捕获后缺少缓存索引")?;
        for name in expected {
            ensure!(merged.contains(name), "增量合并缺少实际特效：{name}");
        }
        let directories = || -> Result<BTreeSet<PathBuf>> {
            let mut paths = BTreeSet::new();
            for entry in std::fs::read_dir(&root)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    paths.insert(entry.path());
                }
            }
            Ok(paths)
        };
        let before = directories()?;
        let started = Instant::now();
        // Read the completed production library only. A full capture(None)
        // would launch an unrelated job for the rest of the official catalog.
        let cached = crate::preview::EffectLibrary::current()?.context("已完成缓存无法再次读取")?;
        let elapsed = started.elapsed().as_secs_f64();
        for name in expected {
            ensure!(cached.contains(name), "重复读取缓存缺少实际特效：{name}");
        }
        let after = directories()?;
        ensure!(before == after, "仅读取缓存不应新建捕获目录");
        evidence["cache_read"] = json!({"elapsed_seconds":elapsed,"verified_names":expected,
            "total_cached_effects":cached.names().count(),"directories_before":before,
            "directories_after":after,"created_directories":0,"capture_invocations":0});
        evidence["ok"] = json!(true);
        save(&evidence)?;
        eprintln!(
            "Completed cache read: {elapsed:.6}s; no directories created; evidence={}",
            evidence_path.display()
        );
        Ok(())
    }
}
