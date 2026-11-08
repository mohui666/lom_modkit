//! Headless operations that share the editor's preview and game-tool entrypoints.
use anyhow::{ensure, Context, Result};
use lom_core::{load_json, project::Project};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::AtomicBool, Arc},
};

pub fn schema() -> Value {
    json!({"request":"COMMAND request.json --json; paths are relative to request.json",
        "extract-assets":{"required":["game","output"],"result":"private preview library under output"},
        "extract-story":{"required":["game"],"optional":["target"],"result":"script/translations, or chapter list if target omitted"},
        "extract-routes":{"required":["game"],"result":"official chapter routes"},
        "extract-audio":{"required":["game","kind","name","output"],"kind":["music","sound","env"],"result":"WAV file"},
        "capture-effects":{"required":["game"],"optional":["runtime","names"],"side_effect":"launches game, captures local particle previews"},
        "preview":{"required":["project","script","node"],"result":"offline final preview state, no GUI or game"},
        "open-editor":{"required":["project"],"optional":["output","script","node"],"side_effect":"imports optionally to output directory, opens a new editor window at script/node"},
        "preview-adjust":{"required":["project","script","node","target","x","y","output"],"optional":["scale","effect_scale"],
            "target":{"kind":["character","mood","overlay","cg","effect"],"id":"character/slot ID","node":"effect source node","action":"optional parallel child index"}},
        "prepare-playtest":{"required":["project","script","output"],"optional":["node"],"result":"isolated presentation-only .lommod, no game writes"},
        "playtest":{"required":["project","script","game"],"optional":["node","runtime"],"side_effect":"installs preview/Host and starts or requests game playback"}})
}

pub fn run(args: &[String]) -> Option<Result<Value>> {
    let command = args.iter().find(|a| a.as_str() != "--json")?.as_str();
    if command == "automation-schema" {
        return Some(Ok(schema()));
    }
    if !schema().get(command).is_some_and(Value::is_object) {
        return None;
    }
    Some((|| {
        let rest: Vec<_> = args.iter().filter(|arg| arg.as_str() != "--json").collect();
        ensure!(rest.len() == 2, "{command} 需要一个请求 JSON 文件");
        let path = Path::new(rest[1]).canonicalize()?;
        let request = load_json(&path)?;
        execute(command, &request, path.parent().unwrap())
    })())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("{key} 必须是字符串"))
}
fn file(base: &Path, value: &Value, key: &str) -> Result<PathBuf> {
    Ok(base.join(text(value, key)?))
}
fn runtime(base: &Path, value: &Value) -> Result<PathBuf> {
    if value.get("runtime").is_some() {
        file(base, value, "runtime")
    } else {
        crate::components::runtime().context("当前构建未提供内置 Host；请指定 runtime")
    }
}
fn worker(game: &Path, args: &[String]) -> Result<Value> {
    let mut command = Command::new(crate::components::extractor()?);
    command.arg("--game-dir").arg(game).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output()?;
    let result = String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| matches!(event["event"].as_str(), Some("result" | "error")))
        .last()
        .context("资源组件未返回结果")?;
    ensure!(
        output.status.success() && result["event"] != "error",
        "{}",
        result["message"].as_str().unwrap_or("资源读取失败")
    );
    Ok(result)
}

pub(crate) fn execute(command: &str, request: &Value, base: &Path) -> Result<Value> {
    let contract = schema();
    let spec = contract.get(command).context("未知自动化命令")?;
    for key in request.as_object().context("请求必须是对象")?.keys() {
        ensure!(
            ["required", "optional"].iter().any(|kind| spec[*kind]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == key))),
            "{command} 不支持字段 {key}"
        );
    }
    for key in spec["required"].as_array().unwrap() {
        ensure!(
            request.get(key.as_str().unwrap()).is_some(),
            "缺少字段 {key}"
        );
    }
    match command {
        "open-editor" => {
            let (path, project) = prepare_editor_open(request, base)?;
            let mut args = vec![path.clone().into_os_string()];
            for (key, flag) in [("script", "--chapter"), ("node", "--node")] {
                if request.get(key).is_some() {
                    args.push(flag.into());
                    args.push(text(request, key)?.into());
                }
            }
            let pid = spawn_editor_window(&args)?;
            Ok(json!({"ok":true,"output":path,"pid":pid,"chapters":project.stories.len()}))
        }
        "extract-assets" => crate::asset_extraction::extract(
            &crate::components::extractor()?,
            &file(base, request, "game")?,
            &file(base, request, "output")?,
            Arc::new(AtomicBool::new(false)),
            |event| eprintln!("{event}"),
        ),
        "extract-story" | "extract-routes" => {
            let args = if command == "extract-routes" {
                vec!["--story-routes".into()]
            } else if request.get("target").is_some() {
                vec!["--story-script".into(), text(request, "target")?.into()]
            } else {
                vec!["--story-list".into()]
            };
            worker(&file(base, request, "game")?, &args)
        }
        "extract-audio" => {
            let output = file(base, request, "output")?;
            ensure!(
                output.extension().and_then(|s| s.to_str()) == Some("wav") && !output.exists(),
                "音频输出须为尚不存在的 .wav 文件"
            );
            let temp = lom_core::workdir::create()?;
            let kind = text(request, "kind")?;
            ensure!(
                ["music", "sound", "env"].contains(&kind),
                "音频 kind 必须是 music/sound/env"
            );
            let result = worker(
                &file(base, request, "game")?,
                &[
                    "--audio-kind".into(),
                    kind.into(),
                    "--audio-name".into(),
                    text(request, "name")?.into(),
                    "--audio-output".into(),
                    temp.path().join("preview.wem").to_string_lossy().into(),
                ],
            )?;
            let input = if temp.path().join("preview.txtp").exists() {
                temp.path().join("preview.txtp")
            } else {
                temp.path().join("preview.wem")
            };
            let decoded = temp.path().join("preview.wav");
            let mut decoder = Command::new(crate::components::decoder()?);
            decoder.args(["-i", "-o"]).arg(&decoded).arg(input);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                decoder.creation_flags(0x08000000);
            }
            let decoded_result = decoder.output()?;
            ensure!(
                decoded_result.status.success() && decoded.is_file(),
                "音频解码失败：{}",
                String::from_utf8_lossy(&decoded_result.stderr)
            );
            lom_core::project::atomic_write(&output, &std::fs::read(decoded)?)?;
            Ok(json!({"ok":true,"output":output,"source":result}))
        }
        "capture-effects" => crate::effect_capture::capture(
            &file(base, request, "game")?,
            &runtime(base, request)?,
            request
                .get("names")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?,
            Arc::new(AtomicBool::new(false)),
            |event| eprintln!("{event}"),
        ),
        _ => {
            let mut project = Project::open(&file(base, request, "project")?)?;
            let script = text(request, "script")?;
            let story = project.stories.get(script).context("章节不存在")?;
            let node = request
                .get("node")
                .map(|v| v.as_str().context("node 必须是字符串"))
                .transpose()?
                .unwrap_or(story["start"].as_str().context("章节缺少 start")?);
            let index = story["nodes"]
                .as_array()
                .context("章节缺少 nodes")?
                .iter()
                .position(|n| n["id"] == node)
                .context("节点不存在")?;
            match command {
                "preview" => Ok(
                    json!({"ok":true,"state":crate::preview::simulate(story,node),"game_tested":false}),
                ),
                "preview-adjust" => {
                    use crate::preview_layout::{Change, Target};
                    let target = &request["target"];
                    let get_id = || text(target, "id").map(str::to_owned);
                    let target = match text(target, "kind")? {
                        "character" => Target::Character(get_id()?),
                        "mood" => Target::Mood(get_id()?),
                        "overlay" => Target::Overlay(get_id()?),
                        "cg" => Target::Cg,
                        "effect" => Target::Effect {
                            node: text(target, "node")?.into(),
                            action: target
                                .get("action")
                                .map(|v| {
                                    v.as_u64()
                                        .map(|v| v as usize)
                                        .context("action 必须是非负整数")
                                })
                                .transpose()?,
                        },
                        _ => anyhow::bail!("未知预览目标类型"),
                    };
                    let number = |key| {
                        request[key]
                            .as_f64()
                            .with_context(|| format!("{key} 必须是数值"))
                    };
                    let change = Change {
                        target,
                        x: number("x")?,
                        y: number("y")?,
                        scale: request
                            .get("scale")
                            .map(|v| v.as_f64().context("scale 必须是数值"))
                            .transpose()?
                            .unwrap_or(100.0),
                        effect_scale: request
                            .get("effect_scale")
                            .map(|v| serde_json::from_value(v.clone()))
                            .transpose()?,
                    };
                    let selected = crate::preview_layout::apply(
                        project.stories.get_mut(script).unwrap(),
                        index,
                        &change,
                    )?;
                    let issues = lom_core::release::run_preflight(
                        &project,
                        lom_core::release::Profile::Editing,
                        env!("CARGO_PKG_VERSION"),
                    );
                    if issues.iter().any(|i| i.severity == "error") {
                        return Ok(json!({"ok":false,"issues":issues}));
                    }
                    project.save_to(&file(base, request, "output")?)?;
                    Ok(
                        json!({"ok":true,"output":file(base,request,"output")?,"node":project.stories[script]["nodes"][selected]["id"]}),
                    )
                }
                "prepare-playtest" => {
                    let preview = crate::tools_panel::prepare_preview(&project, script, node)?;
                    let output = preview.export(&file(base, request, "output")?)?;
                    Ok(
                        json!({"ok":true,"output":output,"script":script,"node":node,"game_modified":false}),
                    )
                }
                "playtest" => {
                    let message = crate::tools_panel::play_project(
                        &project,
                        script,
                        node,
                        &file(base, request, "game")?,
                        &runtime(base, request)?,
                    )?;
                    Ok(json!({"ok":true,"message":message}))
                }
                _ => anyhow::bail!("未知自动化操作"),
            }
        }
    }
}

#[cfg(not(windows))]
fn spawn_editor_window(args: &[std::ffi::OsString]) -> Result<u32> {
    let child = Command::new(std::env::current_exe()?)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(child.id())
}

#[cfg(windows)]
fn spawn_editor_window(args: &[std::ffi::OsString]) -> Result<u32> {
    // Do not inherit ANY handles: Rust Command's redirected stdio can still
    // leave the caller's capture pipe open in this long-lived GUI process.
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr};
    #[repr(C)]
    struct StartupInfo {
        size: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        chars_x: u32,
        chars_y: u32,
        fill: u32,
        flags: u32,
        show: u16,
        reserved_len: u16,
        reserved_bytes: *mut u8,
        input: *mut c_void,
        output: *mut c_void,
        error: *mut c_void,
    }
    #[repr(C)]
    struct ProcessInfo {
        process: *mut c_void,
        thread: *mut c_void,
        pid: u32,
        tid: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateProcessW(
            app: *const u16,
            command: *mut u16,
            process_attributes: *mut c_void,
            thread_attributes: *mut c_void,
            inherit: i32,
            flags: u32,
            environment: *mut c_void,
            directory: *const u16,
            startup: *mut StartupInfo,
            process: *mut ProcessInfo,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    let exe = std::env::current_exe()?;
    let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut command = Vec::new();
    for argument in std::iter::once(exe.into_os_string()).chain(args.iter().cloned()) {
        if !command.is_empty() {
            command.push(32);
        }
        command.push(34);
        let mut slashes = 0;
        for value in argument.encode_wide() {
            ensure!(value != 0, "启动参数不能包含空字符");
            if value == 92 {
                slashes += 1;
                continue;
            }
            command.extend(std::iter::repeat_n(
                92,
                if value == 34 {
                    slashes * 2 + 1
                } else {
                    slashes
                },
            ));
            command.push(value);
            slashes = 0;
        }
        command.extend(std::iter::repeat_n(92, slashes * 2));
        command.push(34);
    }
    command.push(0);
    let mut startup: StartupInfo = unsafe { std::mem::zeroed() };
    startup.size = std::mem::size_of::<StartupInfo>() as u32;
    let mut process: ProcessInfo = unsafe { std::mem::zeroed() };
    let started = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            0x208,
            ptr::null_mut(),
            ptr::null(),
            &mut startup,
            &mut process,
        )
    };
    ensure!(
        started != 0,
        "无法打开编辑器界面：{}",
        std::io::Error::last_os_error()
    );
    unsafe {
        CloseHandle(process.thread);
        CloseHandle(process.process);
    }
    Ok(process.pid)
}

fn prepare_editor_open(request: &Value, base: &Path) -> Result<(PathBuf, Project)> {
    let path = file(base, request, "project")?.canonicalize()?;
    let mut project = Project::open(&path)?;
    let script = request
        .get("script")
        .map(|_| text(request, "script"))
        .transpose()?
        .unwrap_or_else(|| project.manifest["entry"].as_str().unwrap_or(""));
    if request.get("script").is_some() || request.get("node").is_some() {
        let story = project.stories.get(script).context("要打开的章节不存在")?;
        if request.get("node").is_some() {
            ensure!(
                story["nodes"].as_array().is_some_and(|nodes| nodes
                    .iter()
                    .any(|n| n["id"].as_str() == request["node"].as_str())),
                "要打开的节点不存在"
            );
        }
    }
    let output = if request.get("output").is_some() {
        let output = file(base, request, "output")?;
        ensure!(
            !output.is_file()
                && !matches!(
                    output.extension().and_then(|s| s.to_str()),
                    Some("json" | "lommod")
                ),
            "导入目标必须是工程目录"
        );
        project.save_to(&output)?;
        output.canonicalize()?
    } else {
        path
    };
    Ok((output, project))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headless_mood_adjustment_and_playtest_use_the_editor_contracts() {
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::new();
        project.stories.insert(
            "main".into(),
            json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"player","position":"M"},
            {"id":"group","type":"parallel","actions":[
                {"type":"shock","character":"player","duration":0.5},
                {"type":"mood","character":"player","enabled":true,"icon":"shock"}]},
            {"id":"end","type":"end"}]}),
        );
        project.save_to(&temp.path().join("original")).unwrap();
        let original = std::fs::read(temp.path().join("original/story/main.json")).unwrap();
        let result = execute(
            "preview",
            &json!({"project":"original","script":"main","node":"group"}),
            temp.path(),
        )
        .unwrap();
        assert_eq!(result["state"]["mood_overrides"]["player"]["icon"], "shock");
        execute("preview-adjust",&json!({"project":"original","script":"main","node":"group","target":{"kind":"mood","id":"player"},"x":42.5,"y":23,"output":"adjusted"}),temp.path()).unwrap();
        let adjusted = Project::open(&temp.path().join("adjusted")).unwrap();
        assert_eq!(
            adjusted.stories["main"]["nodes"][1]["actions"][1]["offset_x"],
            42.5
        );
        assert_eq!(
            adjusted.stories["main"]["nodes"][1]["actions"][0],
            project.stories["main"]["nodes"][1]["actions"][0]
        );
        let result = execute(
            "prepare-playtest",
            &json!({"project":"adjusted","script":"main","node":"group","output":"preview.lommod"}),
            temp.path(),
        )
        .unwrap();
        assert_eq!(result["game_modified"], false);
        let preview = Project::open(&temp.path().join("preview.lommod")).unwrap();
        assert_eq!(preview.manifest["id"], lom_core::game_tools::PREVIEW_ID);
        assert!(preview.scenario.is_none());
        assert_eq!(
            std::fs::read(temp.path().join("original/story/main.json")).unwrap(),
            original
        );
    }
    #[test]
    fn unknown_operation_fields_fail_before_output() {
        let temp = tempfile::tempdir().unwrap();
        let error = execute(
            "extract-assets",
            &json!({"game":"absent","output":"output","typo":true}),
            temp.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("typo"));
        assert!(!temp.path().join("output").exists());
    }
    #[test]
    fn editor_import_preserves_source_and_validates_requested_location() {
        let temp = tempfile::tempdir().unwrap();
        let project = Project::new();
        let package = project.export(&temp.path().join("source.lommod")).unwrap();
        let before = std::fs::read(&package).unwrap();
        let (output, opened) = prepare_editor_open(
            &json!({"project":"source.lommod","output":"imported","script":"main","node":"say1"}),
            temp.path(),
        )
        .unwrap();
        assert_eq!(Project::open(&output).unwrap().stories, project.stories);
        assert_eq!(
            opened.manifest["campaign_id"],
            project.manifest["campaign_id"]
        );
        assert_eq!(std::fs::read(package).unwrap(), before);
        assert!(prepare_editor_open(
            &json!({"project":"source.lommod","output":"invalid","script":"missing"}),
            temp.path()
        )
        .is_err());
        assert!(!temp.path().join("invalid").exists());
    }
}
