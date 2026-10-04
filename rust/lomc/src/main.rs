use anyhow::{bail, Context, Result};
use lom_core::{
    codegen, load_json, localization, package, project::Project, stable_json, validate,
};
use serde_json::json;
use std::{env, fs, path::PathBuf};

fn run() -> Result<i32> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|s| s == "--help" || s == "-h") {
        println!("lomc 1.2.0 — Rust 活侠传剧情工具\n\n用法: lomc <check|build|compile|pack|inspect|new-story> <路径> [-o 输出] [--json] [--locale chs|cht|ja|ko]\n水印: lomc detect-watermark <图片> [--json]\n      lomc detect-watermark-video <视频> [--ffmpeg 路径] [--interval 秒] [--max-frames 数量] [--json]\n\ncheck 校验剧情；build/compile 生成 Lua；pack 生成 v3 .lommod；inspect 验证完整性与源码/Lua一致性。\n扩展: analyze/test/statistics/preflight <项目> [--profile editing|release]\n      release <项目> -o <输出目录>\n      migrate <JSON> [--kind story|manifest|content]\n      content-inspect/content-import <.lomcontent> [--library 目录]\n      author <请求.json> [-o 输出.json]\n      edit <story.json> --operations <操作数组.json> [-o 输出.json]");
        return Ok(0);
    }
    if args[0] == "--version" {
        println!("lomc {} (Rust)", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    let as_json = args.iter().any(|s| s == "--json");
    args.retain(|s| s != "--json");
    let command = args.remove(0);
    let path = PathBuf::from(args.first().context("缺少输入路径")?);
    args.remove(0);
    let mut output = None;
    let mut title = None;
    let mut kind = "story".to_owned();
    let mut profile = "editing".to_owned();
    let mut library = lom_core::content::default_repository_root();
    let mut locale = None;
    let mut index = 0;
    let mut ffmpeg = None;
    let mut interval = 2.0;
    let mut max_frames = 12;
    let mut operations = None;
    while index < args.len() {
        match args[index].as_str() {
            "--operations" => {
                index += 1;
                operations = Some(PathBuf::from(args.get(index).context("缺少操作文件")?));
            }
            "--kind" => {
                index += 1;
                kind = args.get(index).context("缺少迁移类型")?.clone();
            }
            "--profile" => {
                index += 1;
                profile = args.get(index).context("缺少检查模式")?.clone();
            }
            "--library" => {
                index += 1;
                library = PathBuf::from(args.get(index).context("缺少内容库目录")?);
            }
            "--title" if command == "new-story" => {
                index += 1;
                title = Some(args.get(index).context("缺少标题")?.clone());
            }
            "-o" | "--output" => {
                index += 1;
                output = Some(PathBuf::from(args.get(index).context("缺少输出路径")?));
            }
            "--locale" => {
                index += 1;
                locale = Some(args.get(index).context("缺少 locale")?.clone());
            }
            "--ffmpeg" => {
                index += 1;
                ffmpeg = Some(PathBuf::from(args.get(index).context("缺少 FFmpeg 路径")?));
            }
            "--interval" => {
                index += 1;
                interval = args.get(index).context("缺少 interval")?.parse()?;
            }
            "--max-frames" => {
                index += 1;
                max_frames = args.get(index).context("缺少 max-frames")?.parse()?;
            }
            other => bail!("未知参数: {other}"),
        }
        index += 1;
    }
    let result = match command.as_str() {
        "author" => {
            let request = load_json(&path)?;
            let response = lom_core::story_api::execute(
                request["op"].as_str().context("缺少 op")?,
                request.get("params").unwrap_or(&request),
            )?;
            if let Some(output) = output {
                lom_core::project::atomic_write(&output, &stable_json(&response)?)?;
            }
            json!({"ok":true,"result":response["result"],"after":response["after"]})
        }
        "edit" => {
            let story = load_json(&path)?;
            let ops = load_json(operations.context("需要 --operations JSON 文件")?)?;
            let next = lom_core::story_api::apply(&story, &ops)?;
            let target = output.unwrap_or(path.clone());
            lom_core::project::atomic_write(&target, &stable_json(&next)?)?;
            json!({"ok":true,"output":target,"story":next})
        }
        "analyze" => {
            let project = Project::open(&path)?;
            lom_core::analysis::analyze_project(&project.stories, &project.manifest)
        }
        "test" => {
            let project = Project::open(&path)?;
            let tests = project
                .stories
                .values()
                .flat_map(|s| {
                    s["_editor"]["tests"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            anyhow::ensure!(!tests.is_empty(), "项目尚未定义离线测试");
            let results = lom_core::analysis::run_story_tests(&project.stories, &json!(tests))?;
            json!({"ok":results.as_array().is_some_and(|rows|rows.iter().all(|row|row["status"]=="pass")),"results":results})
        }
        "statistics" => {
            let project = Project::open(&path)?;
            let assets = project.assets.keys().cloned().collect::<Vec<_>>();
            json!({"statistics":lom_core::release::calculate_project_statistics(&project.stories,Some(&assets)),"voice":lom_core::release::calculate_voice_coverage(&project.stories)})
        }
        "preflight" => {
            let project = Project::open(&path)?;
            anyhow::ensure!(
                ["editing", "release"].contains(&profile.as_str()),
                "profile 必须是 editing 或 release"
            );
            let issues = lom_core::release::run_preflight(
                &project,
                if profile == "release" {
                    lom_core::release::Profile::Release
                } else {
                    lom_core::release::Profile::Editing
                },
                "1.1.2",
            );
            json!({"ok":!issues.iter().any(|i|i.severity=="error"),"issues":issues})
        }
        "release" => {
            let project = Project::open(&path)?;
            let target = output.context("发布构建需要 -o 输出目录")?;
            serde_json::to_value(lom_core::release::build_release_directory(
                &project, &target, "1.1.2",
            )?)?
        }
        "migrate" => {
            let (result, backup) = lom_core::migration::migrate_json_file(&path, &kind, None)?;
            json!({"ok":true,"changed":result.changed,"steps":result.steps,"backup":backup})
        }
        "content-inspect" => serde_json::to_value(
            lom_core::content_library::inspect_content_pack(&library, &path)?,
        )?,
        "content-import" => serde_json::to_value(lom_core::content_library::import_content_pack(
            &library, &path,
        )?)?,
        "check" | "build" | "compile" => {
            let mut story = load_json(&path)?;
            if let Some(locale) = locale {
                story = localization::apply_story_locale(&story, &locale)?;
            }
            let warnings = validate::validate_story(&story)?;
            if command == "check" {
                json!({"ok":true,"errors":[],"warnings":warnings})
            } else {
                let source = path.to_string_lossy();
                let story_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
                let content_root =
                    if story_dir.file_name().and_then(|s| s.to_str()) == Some("story") {
                        story_dir.parent().unwrap_or(story_dir)
                    } else {
                        story_dir
                    };
                let lua = codegen::compile_story(&story, None, Some(&source), Some(content_root))?;
                let target = output.unwrap_or_else(|| path.with_extension("lua"));
                lom_core::project::atomic_write(&target, lua.as_bytes())?;
                json!({"ok":true,"output":target,"warnings":warnings})
            }
        }
        "pack" => {
            let target = package::pack_mod(&path, output.as_deref())?;
            json!({"ok":true,"output":target})
        }
        "inspect" => {
            let entries = package::read_package(&path)?;
            let manifest: serde_json::Value = serde_json::from_slice(&entries["manifest.json"])?;
            json!({"ok":true,"manifest":manifest,"content_hash":package::content_hash(&entries),"entries":entries.iter().map(|(n,b)|json!({"path":n,"size":b.len()})).collect::<Vec<_>>()})
        }
        "detect-watermark" => {
            lom_core::watermark::detect_image(&path, lom_core::watermark::DEFAULT_SCALE_FACTORS)?
        }
        "detect-watermark-video" => lom_core::watermark::detect_video(
            &path,
            ffmpeg.as_deref(),
            interval,
            max_frames,
            lom_core::watermark::DEFAULT_SCALE_FACTORS,
        )?,
        "new-story" => {
            let mut project = Project::new();
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
            anyhow::ensure!(
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
                "剧情脚本 id 非法: {id}（规则 [a-zA-Z0-9_-]{{1,64}}）"
            );
            let mut story = project.stories.remove("main").unwrap();
            story["id"] = json!(id);
            if let Some(title) = title {
                story["title"] = json!(title);
            }
            let target = output.unwrap_or(path.clone());
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            lom_core::project::atomic_write(&target, &stable_json(&story)?)?;
            json!({"ok":true,"output":target})
        }
        _ => bail!("未知命令: {command}"),
    };
    if as_json {
        println!("{}", serde_json::to_string(&result)?);
    } else if let Some(output) = result.get("output") {
        println!("已生成 {}", output.as_str().unwrap_or(""));
    } else {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(
        if result.get("ok") == Some(&json!(false)) || result.get("passed") == Some(&json!(false)) {
            1
        } else if result.get("detected") == Some(&json!(false)) {
            2
        } else {
            0
        },
    )
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if env::args().any(|s| s == "--json") {
                println!(
                    "{}",
                    json!({"ok":false,"errors":[format!("{error:#}")],"warnings":[]})
                );
            } else {
                eprintln!("错误: {error:#}");
            }
            std::process::exit(1);
        }
    }
}
