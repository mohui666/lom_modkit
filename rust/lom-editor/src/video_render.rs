//! Native-game video jobs use the same isolated development package as playtest.
use anyhow::{ensure, Context, Result};
use lom_core::{game_tools, project::Project};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    #[default]
    Chapter,
    Project,
    Step,
}

#[derive(Clone)]
pub(crate) struct Settings {
    pub scope: Scope,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub auto_speed: String,
    pub choices: BTreeMap<String, usize>,
    pub choice_hold: f64,
    pub background: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scope: Scope::Chapter,
            width: 1920,
            height: 1080,
            fps: 30,
            auto_speed: "game".into(),
            choices: BTreeMap::new(),
            choice_hold: 1.5,
            background: true,
        }
    }
}

pub(crate) struct Prepared {
    project: Project,
    script: String,
    node: String,
    settings: Settings,
    terminals: Vec<String>,
    terminal_cards: Vec<String>,
}

pub(crate) fn prepare_request(project: &Project, request: &Value) -> Result<Prepared> {
    let mut settings = Settings::default();
    if let Some(value) = request.get("background") {
        settings.background = value.as_bool().context("background 必须是布尔值")?;
    }
    settings.scope = match request
        .get("scope")
        .map(|value| value.as_str().context("scope 必须是字符串"))
        .transpose()?
        .unwrap_or("chapter")
    {
        "chapter" => Scope::Chapter,
        "project" => Scope::Project,
        "step" => Scope::Step,
        _ => anyhow::bail!("scope 必须是 chapter、project 或 step"),
    };
    ensure!(
        request.get("width").is_some() == request.get("height").is_some(),
        "width 和 height 须同时指定"
    );
    for (key, value) in [
        ("width", &mut settings.width),
        ("height", &mut settings.height),
        ("fps", &mut settings.fps),
    ] {
        if let Some(input) = request.get(key) {
            *value = input
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .with_context(|| format!("{key} 必须是非负整数"))?;
        }
    }
    if let Some(value) = request.get("auto_speed") {
        settings.auto_speed = value
            .as_str()
            .context("auto_speed 必须是字符串")?
            .to_owned();
    }
    if let Some(value) = request.get("choice_hold") {
        settings.choice_hold = value.as_f64().context("choice_hold 必须是秒数")?;
    }
    if let Some(value) = request.get("choices") {
        settings.choices = serde_json::from_value(value.clone())
            .context("choices 必须是章节/步骤到选项序号的对象（从 0 开始）")?;
    }
    let entry = project.manifest["entry"]
        .as_str()
        .context("作品缺少开场章节")?;
    let (script, node) = match settings.scope {
        Scope::Project => {
            ensure!(
                request.get("script").is_none() && request.get("node").is_none(),
                "project 范围从作品入口开始；指定起点请使用 step 范围"
            );
            (entry, "")
        }
        Scope::Chapter => {
            ensure!(request.get("node").is_none(), "node 仅用于 step 范围");
            (
                request
                    .get("script")
                    .map(|value| value.as_str().context("script 必须是字符串"))
                    .transpose()?
                    .unwrap_or(entry),
                "",
            )
        }
        Scope::Step => (
            request["script"].as_str().context("step 范围需要 script")?,
            request["node"].as_str().context("step 范围需要 node")?,
        ),
    };
    prepare(project, script, node, settings)
}

pub(crate) fn validate_output(output: &Path, overwrite: bool) -> Result<()> {
    ensure!(
        output
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4")),
        "输出文件需要使用 .mp4 扩展名"
    );
    ensure!(
        output.parent().is_some_and(Path::is_dir),
        "视频输出目录不存在"
    );
    ensure!(!output.exists() || output.is_file(), "视频输出目标不是文件");
    ensure!(
        overwrite || !output.exists(),
        "视频输出已存在；如需替换，请明确指定 overwrite=true"
    );
    Ok(())
}

pub(crate) fn prepare(
    project: &Project,
    chapter: &str,
    selected: &str,
    settings: Settings,
) -> Result<Prepared> {
    ensure!(
        matches!(
            (settings.width, settings.height),
            (1280, 720) | (1920, 1080)
        ),
        "请选择 720p 或 1080p"
    );
    ensure!(matches!(settings.fps, 30 | 60), "请选择 30 或 60 FPS");
    ensure!(
        matches!(
            settings.auto_speed.as_str(),
            "game" | "slow" | "standard" | "fast"
        ),
        "自动播放速度无效"
    );
    ensure!(
        settings.choice_hold.is_finite() && (0.0..=30.0).contains(&settings.choice_hold),
        "选项停留时间须在 0～30 秒之间"
    );
    let script = if settings.scope == Scope::Project {
        project.manifest["entry"]
            .as_str()
            .context("作品缺少开场章节")?
    } else {
        chapter
    };
    let story = project
        .stories
        .get(script)
        .with_context(|| format!("视频起始章节不存在：{script}"))?;
    ensure!(
        story.get("vanilla_patch").is_none(),
        "原版章节补丁不支持直接生成视频，请使用结构化剧情章节"
    );
    let node = if settings.scope == Scope::Step {
        selected
    } else {
        story["start"].as_str().context("章节缺少起始步骤")?
    };
    for (location, index) in &settings.choices {
        let (chapter, node) = location.split_once('/').context("预设选项位置无效")?;
        let node = project
            .stories
            .get(chapter)
            .and_then(|story| story["nodes"].as_array())
            .and_then(|nodes| nodes.iter().find(|candidate| candidate["id"] == node))
            .with_context(|| format!("预设选项步骤已不存在：{location}"))?;
        ensure!(
            node["type"] == "choice"
                && node["options"]
                    .as_array()
                    .is_some_and(|options| *index < options.len()),
            "预设选项无效：{location}"
        );
    }
    let mut terminals = Vec::new();
    let mut terminal_cards = Vec::new();
    for (id, story) in &project.stories {
        for node in story["nodes"].as_array().into_iter().flatten() {
            let kind = node["type"].as_str().unwrap_or("");
            // Match the native ending paths emitted by lom-core::codegen.
            // A panel=endgame may continue afterwards, so it is not a terminal.
            let card =
                kind == "goto_scene" && matches!(node["scene"].as_str(), Some("End" | "GameOver"));
            let terminal = card
                || kind == "death"
                || kind == "end"
                    && ((settings.scope == Scope::Chapter && id == script)
                        || node["next_script"].as_str().is_none_or(str::is_empty));
            if terminal {
                let location = format!("{id}/{}", node["id"].as_str().unwrap_or(""));
                if card {
                    terminal_cards.push(location.clone());
                }
                terminals.push(location);
            }
        }
    }
    let preview = crate::tools_panel::prepare_preview(project, script, node)?;
    Ok(Prepared {
        project: preview,
        script: script.into(),
        node: node.into(),
        settings,
        terminals,
        terminal_cards,
    })
}

fn job_root() -> PathBuf {
    lom_core::content::default_repository_root()
        .parent()
        .unwrap()
        .join("rust/video-renders")
}

fn specification(prepared: &Prepared, encoder: &Path) -> Value {
    let settings = &prepared.settings;
    json!({"format":1,"width":settings.width,"height":settings.height,"fps":settings.fps,
        "encoder":encoder,"auto_speed":settings.auto_speed,"choice_hold":settings.choice_hold,
        "max_seconds":3600,"choices":settings.choices,"terminal_nodes":prepared.terminals,
        "terminal_cards":prepared.terminal_cards,
        "entry_story":prepared.script,"entry_node":prepared.node})
}

fn start_worker(
    game: &Path,
    runtime: &Path,
    directory: &Path,
    id: &str,
    package: &Path,
    prepared: &Prepared,
) -> Result<crate::video_worker::VideoWorker> {
    game_tools::validate_root(game)?;
    // A same-version older Host would ignore the worker argument and fail to
    // isolate saves. Background mode accepts only this editor's paired runtime.
    let bundled =
        crate::components::runtime().context("缺少内置 Host，请使用完整的 Windows 编辑器")?;
    for name in game_tools::HOST_FILES {
        ensure!(
            fs::read(runtime.join(name))? == fs::read(bundled.join(name))?,
            "后台生成需要当前编辑器配套 Host；请使用内置 runtime，或显式选择 background=false"
        );
    }
    let worker = crate::video_worker::prepare(game, runtime, directory, id)?;
    for name in game_tools::HOST_FILES {
        fs::copy(
            worker.runtime_dir().join(name),
            worker.paths().plugin.join(name),
        )?;
    }
    fs::copy(
        package,
        worker.paths().mods.join("__lom_modkit_preview.lommod"),
    )?;
    copy_dialogue_speed(game, &worker.paths().root)?;
    lom_core::project::atomic_write(
        &worker.paths().plugin.join("preview-request.json"),
        &lom_core::stable_json(&game_tools::video_preview_payload(
            &prepared.script,
            &prepared.node,
            id,
        )?)?,
    )?;
    worker
        .launch(prepared.settings.width, prepared.settings.height, false)
        .context("无法启动独立后台渲染进程")
}

fn copy_dialogue_speed(game: &Path, worker_root: &Path) -> Result<()> {
    let config = game.join("BepInEx/config/com.mohui666.mortalmodhost.cfg");
    if !config.try_exists()? {
        return Ok(());
    }
    let mut dialogue = false;
    for line in fs::read_to_string(&config)?.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            dialogue = line == "[Dialogue]";
        }
        if dialogue {
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim();
                if key.trim() == "AutoSpeed" && matches!(value, "Slow" | "Standard" | "Fast") {
                    let target = worker_root.join("BepInEx/config/com.mohui666.mortalmodhost.cfg");
                    fs::create_dir_all(target.parent().unwrap())?;
                    lom_core::project::atomic_write(
                        &target,
                        format!("[Dialogue]\nAutoSpeed = {value}\n").as_bytes(),
                    )?;
                    break;
                }
            }
        }
    }
    Ok(())
}

fn finish_worker(worker: &mut crate::video_worker::VideoWorker, directory: &Path) -> Result<()> {
    // Host publishes completion after closing audio/encoding and releasing its
    // frame buffers. Reap only this task's Job now: a private desktop need not
    // render another Update merely to quit, or delay a finished export.
    worker.stop()?;
    let root = worker.paths().root.canonicalize()?;
    ensure!(
        root.parent() == Some(directory.canonicalize()?.as_path())
            && root.file_name().is_some_and(|name| name == "worker"),
        "后台临时目录不属于本次任务，未清理"
    );
    // poll/stop have already removed the core junction and closed the worker.
    fs::remove_dir_all(root).context("无法清理本次后台渲染的临时组件与素材")
}

pub(crate) fn render(
    prepared: Prepared,
    game: &Path,
    runtime: &Path,
    output: &Path,
    overwrite: bool,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Value),
) -> Result<Value> {
    ensure!(cfg!(windows), "生成游戏原生视频需要 Windows 游戏环境");
    ensure!(
        output
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4")),
        "输出文件需要使用 .mp4 扩展名"
    );
    let components =
        crate::components::root()?.context("缺少内置视频编码器，请使用完整的 Windows 编辑器")?;
    let encoder = components.join("video-encoder/ffmpeg.exe");
    ensure!(
        encoder.is_file(),
        "内置视频编码器缺失，请使用包含视频功能的 Windows 编辑器"
    );
    let root = job_root();
    fs::create_dir_all(&root)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".render.lock"))?;
    lock.try_lock()
        .context("另一个编辑器窗口正在生成视频，请等待完成或先取消该任务")?;
    let identity = Project::new();
    let id = identity.manifest["campaign_id"]
        .as_str()
        .and_then(|value| value.strip_prefix("campaign_"))
        .context("无法创建视频任务编号")?;
    let directory = root.join(id);
    fs::create_dir_all(&directory)?;
    lom_core::project::atomic_write(
        &directory.join("request.json"),
        &lom_core::stable_json(&specification(&prepared, &encoder))?,
    )?;
    let staging = lom_core::workdir::create()?;
    let package = staging.path().join("__lom_modkit_preview.lommod");
    progress(json!({"job_id":id,"message":crate::i18n::key("video.progress.prepare")}));
    prepared.project.export(&package)?;
    ensure!(!cancel.load(Ordering::Relaxed), "视频生成已取消");
    let mut worker = if prepared.settings.background {
        let worker = start_worker(game, runtime, &directory, id, &package, &prepared)?;
        progress(
            json!({"job_id":id,"worker_pid":worker.pid(),"background":true,"message":crate::i18n::key("video.progress.background")}),
        );
        Some(worker)
    } else {
        let running = game_tools::is_game_running()?;
        let installed = game_tools::install_runtime(game, runtime)?;
        ensure!(
            !(running && installed["changed"] == true),
            "游戏 Host 已更新，请完整退出游戏后重新生成视频"
        );
        game_tools::remove_preview_packages(game)?;
        game_tools::install_mod(game, &package, true)?;
        game_tools::request_video_preview(game, &prepared.script, &prepared.node, id)?;
        game_tools::launch_game()?;
        progress(json!({"message":crate::i18n::key("video.progress.wait")}));
        None
    };
    let started = Instant::now();
    let mut observed = Value::Null;
    let mut received = false;
    let mut cancelled = false;
    let mut last_process_check = Instant::now();
    loop {
        if !cancelled && cancel.load(Ordering::Relaxed) {
            lom_core::project::atomic_write(&directory.join("cancel"), b"")?;
            cancelled = true;
            progress(json!({"message":crate::i18n::key("video.progress.cancelling")}));
        }
        let status_path = directory.join("status.json");
        if status_path.is_file() {
            let status = lom_core::load_json(&status_path)?;
            received = true;
            if status != observed {
                progress(status.clone());
                observed = status.clone();
            }
            match status["state"].as_str() {
                Some("complete") => {
                    ensure!(!cancelled, "视频生成已取消，未写入输出文件");
                    let native = directory.join("video.mp4");
                    publish(&native, output, overwrite)?;
                    let mut result = json!({"ok":true,"output":output,"frames":status["frames"],
                        "seconds":status["seconds"],
                        "job_id":id,"renderer":"native-game","script":prepared.script,"node":prepared.node,
                        "width":prepared.settings.width,"height":prepared.settings.height,"fps":prepared.settings.fps,
                        "background":prepared.settings.background,
                        "visual_review_performed":false,
                        "message":crate::i18n::key("video.progress.complete")});
                    if let Err(error) = fs::remove_file(&native) {
                        result["cleanup_warning"] =
                            json!(format!("视频已导出，临时成片未能清理：{error}"));
                    }
                    if let Some(worker) = worker.as_mut() {
                        if let Err(error) = finish_worker(worker, &directory) {
                            result["worker_cleanup_warning"] =
                                json!(format!("视频已导出，后台临时资源清理未完成：{error:#}"));
                        }
                    }
                    return Ok(result);
                }
                Some("error" | "cancelled") => anyhow::bail!(
                    "{}",
                    status["message"].as_str().unwrap_or("游戏视频生成未完成")
                ),
                Some("preparing" | "running" | "encoding") => (),
                _ => anyhow::bail!("游戏返回了未知的视频生成状态"),
            }
        }
        if let Some(worker) = worker.as_mut() {
            if let Some(code) = worker.poll()? {
                anyhow::bail!(
                    "后台渲染进程提前退出（代码 {code}），未写入输出文件；日志：{}",
                    worker.paths().player_log.display()
                );
            }
        }
        ensure!(
            received || started.elapsed() < Duration::from_secs(120),
            "等待视频任务启动超时；后台日志：{}",
            directory.join("worker-player.log").display()
        );
        if worker.is_none() && received && last_process_check.elapsed() >= Duration::from_secs(2) {
            ensure!(
                game_tools::is_game_running()?,
                "游戏在视频完成前退出，未写入输出文件"
            );
            last_process_check = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(350));
    }
}

fn publish(source: &Path, output: &Path, overwrite: bool) -> Result<()> {
    let metadata = fs::metadata(source).context("游戏报告完成，但没有生成视频文件")?;
    ensure!(
        metadata.is_file() && metadata.len() > 0,
        "游戏返回的视频文件为空"
    );
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .context("请选择视频输出目录")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut fs::File::open(source)?, &mut temporary)?;
    temporary.as_file_mut().sync_all()?;
    let result = if overwrite {
        temporary.persist(output)
    } else {
        temporary.persist_noclobber(output)
    };
    result
        .map_err(|error| error.error)
        .with_context(|| format!("无法写入视频：{}", output.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_scopes_choices_and_timing_use_the_same_native_specification() {
        let mut project = Project::new();
        project.stories.get_mut("main").unwrap()["nodes"].as_array_mut().unwrap().insert(1,
            json!({"id":"route","type":"choice","options":[{"text":"左","goto":"end1"},{"text":"右","goto":"end1"}]}));
        let mut other = project.stories["main"].clone();
        other["id"] = json!("other");
        project.stories.insert("other".into(), other);
        project.stories.get_mut("main").unwrap()["nodes"][2]["next_script"] = json!("other");
        let original = project.clone();
        let chapter = prepare_request(&project, &json!({})).unwrap();
        assert_eq!(chapter.script, "main");
        assert_eq!(chapter.node, "say1");
        assert_eq!(chapter.settings.width, 1920);
        assert_eq!(chapter.settings.fps, 30);
        assert_eq!(chapter.settings.auto_speed, "game");
        assert!(chapter.settings.background);
        assert!(chapter.terminals.contains(&"main/end1".into()));
        let whole = prepare_request(&project, &json!({"scope":"project"})).unwrap();
        assert!(!whole.terminals.contains(&"main/end1".into()));
        let step = prepare_request(
            &project,
            &json!({"scope":"step","script":"main","node":"route","choices":{"main/route":1},
            "width":1280,"height":720,"fps":60,"auto_speed":"fast","choice_hold":0.4,"background":false}),
        )
        .unwrap();
        let spec = specification(&step, Path::new("ffmpeg.exe"));
        assert_eq!(spec["entry_node"], "route");
        assert_eq!(spec["choices"]["main/route"], 1);
        assert_eq!(spec["width"], 1280);
        assert_eq!(spec["fps"], 60);
        assert_eq!(spec["auto_speed"], "fast");
        assert_eq!(spec["choice_hold"], 0.4);
        assert!(!step.settings.background);
        assert!(step.project.scenario.is_none());
        assert_eq!(project, original);
        for invalid in [
            json!({"scope":"invalid"}),
            json!({"width":1280}),
            json!({"fps":true}),
            json!({"scope":"step","node":"say1"}),
            json!({"node":"say1"}),
            json!({"scope":"project","script":"main"}),
            json!({"choices":{"main/route":2}}),
            json!({"choices":{"main/say1":0}}),
            json!({"choice_hold":-1}),
            json!({"background":"false"}),
        ] {
            assert!(prepare_request(&project, &invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn worker_copies_only_valid_dialogue_speed_without_changing_game_config() {
        let directory = tempfile::tempdir().unwrap();
        let game = directory.path().join("game");
        let worker = directory.path().join("worker");
        let config = game.join("BepInEx/config/com.mohui666.mortalmodhost.cfg");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        let original = "[General]\nEnabled = false\nAutoSpeed = Slow\n[Dialogue]\nAutoSpeed = Fast\nOther = keep\n";
        fs::write(&config, original).unwrap();
        copy_dialogue_speed(&game, &worker).unwrap();
        let target = worker.join("BepInEx/config/com.mohui666.mortalmodhost.cfg");
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "[Dialogue]\nAutoSpeed = Fast\n"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), original);

        let other_worker = directory.path().join("invalid-worker");
        fs::write(&config, "[Dialogue]\nAutoSpeed = invalid\n").unwrap();
        copy_dialogue_speed(&game, &other_worker).unwrap();
        assert!(!other_worker.exists());
    }

    #[test]
    fn chapter_boundary_keeps_source_and_stops_before_the_linked_chapter() {
        let mut project = Project::new();
        let mut other = project.stories["main"].clone();
        other["id"] = json!("other");
        project.stories.insert("other".into(), other);
        project.stories.get_mut("main").unwrap()["nodes"][1]["next_script"] = json!("other");
        let before = project.clone();
        let prepared = prepare(&project, "main", "say1", Settings::default()).unwrap();
        assert!(prepared.terminals.contains(&"main/end1".to_owned()));
        let prepared = prepare(
            &project,
            "main",
            "say1",
            Settings {
                scope: Scope::Project,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!prepared.terminals.contains(&"main/end1".to_owned()));
        assert_eq!(project, before);
    }
    #[test]
    fn invalid_choice_and_missing_video_do_not_overwrite_existing_output() {
        let project = Project::new();
        let settings = Settings {
            choices: BTreeMap::from([("main/say1".into(), 0)]),
            ..Default::default()
        };
        assert!(prepare(&project, "main", "say1", settings).is_err());
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("keep.mp4");
        fs::write(&output, b"existing").unwrap();
        assert!(publish(&directory.path().join("missing.mp4"), &output, true).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        let source = directory.path().join("video.mp4");
        fs::write(&source, b"complete video").unwrap();
        assert!(publish(&source, &output, false).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        publish(&source, &output, true).unwrap();
        assert_eq!(fs::read(output).unwrap(), b"complete video");
    }
    #[test]
    fn native_ending_cards_are_terminals_but_intermediate_panels_are_not() {
        let mut project = Project::new();
        project.stories.get_mut("main").unwrap()["nodes"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"card","type":"goto_scene","scene":"End","title":"结局"}),
                json!({"id":"over","type":"goto_scene","scene":"GameOver","key":"900001","desc":"结局"}),
                json!({"id":"death","type":"death","text":"结局","death_id":"900002"}),
                json!({"id":"panel","type":"panel","panel":"endgame","key":"key"}),
            ]);
        let prepared = prepare(&project, "main", "say1", Settings::default()).unwrap();
        assert_eq!(prepared.terminal_cards, ["main/card", "main/over"]);
        assert!(prepared.terminals.contains(&"main/death".to_owned()));
        assert!(!prepared.terminals.contains(&"main/panel".to_owned()));
    }
}
