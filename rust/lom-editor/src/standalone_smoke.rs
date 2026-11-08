//! Explicit maintainer smoke entry, using the same paths and players as the UI.
use anyhow::{ensure, Context, Result};
use serde_json::json;
use std::{
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
pub fn run(project: &Path, game: &Path, output: &Path) -> Result<()> {
    let root = crate::components::root()?.context("此检查需要内嵌组件的发布版 exe")?;
    std::fs::create_dir_all(output)?;
    ensure!(
        std::fs::read_dir(output)?.next().is_none(),
        "冒烟输出目录必须为空"
    );
    let helper = crate::components::extractor()?;
    let decoder = crate::components::decoder()?;
    let checked = std::process::Command::new(&helper)
        .arg("--self-check")
        .output()?;
    ensure!(checked.status.success(), "内置提取器自检失败");
    let check: serde_json::Value = serde_json::from_slice(&checked.stdout)?;
    ensure!(
        check["engine"] == "rust" && check["sprite_decoder_ready"] == true,
        "提取器自检结果无效"
    );
    let project = lom_core::project::Project::open(project)?;
    let package = output.join("roundtrip.lommod");
    project.export(&package)?;
    let reopened = lom_core::project::Project::open(&package)?;
    ensure!(
        reopened.stories == project.stories,
        "作品保存重开后章节不一致"
    );
    ensure!(
        reopened.assets.keys().eq(project.assets.keys()),
        "作品保存重开后素材清单不一致"
    );
    for (name, bytes) in &project.assets {
        if name.ends_with(".json") {
            let before: serde_json::Value = serde_json::from_slice(bytes)?;
            let after: serde_json::Value = serde_json::from_slice(&reopened.assets[name])?;
            let before = lom_core::content::normalize_content_metadata(&before)?;
            let after = lom_core::content::normalize_content_metadata(&after)?;
            ensure!(before == after, "素材定义不一致：{name}");
        } else {
            ensure!(*bytes == reopened.assets[name], "素材内容不一致：{name}");
        }
    }
    let runtime = root.join("runtime");
    let version = lom_core::game_tools::host_file_version(&runtime.join("MortalModHost.dll"))?;
    ensure!(
        version == env!("CARGO_PKG_VERSION"),
        "内置 Host 版本不一致：{version}"
    );
    crate::play_bundle::export(
        &project,
        &output.join("play.zip"),
        &runtime,
        &AtomicBool::new(false),
        |message, done| {
            println!(
                "{}",
                json!({"event":"play-bundle","message":message,"done":done})
            )
        },
    )?;
    let mut completed = vec![];
    for (kind, name) in [
        ("music", "普通_001"),
        ("env", "雨天_001"),
        ("sound", "子彈咻咻_001"),
        ("sound", "丟錢_002"),
        ("sound", "咻咻_001"),
        ("sound", "打火石_001"),
        ("sound", "捅人_001"),
        ("sound", "男_受傷_001"),
        ("sound", "男_受傷_002"),
        ("sound", "砍人_001"),
        ("sound", "重物掉落_002"),
        ("sound", "釣魚竿_001"),
        ("sound", "響指_001"),
    ] {
        let mut player = crate::audio::Player::default();
        player.play_official(game, kind, name, &helper, &decoder)?;
        let deadline = Instant::now() + Duration::from_secs(40);
        while player.preparing() {
            player.update()?;
            ensure!(Instant::now() < deadline, "音频准备超时：{name}");
            std::thread::sleep(Duration::from_millis(20));
        }
        ensure!(player.playing(), "音频没有启动：{name}");
        if kind == "sound" {
            while player.playing() {
                player.update()?;
                ensure!(Instant::now() < deadline, "音频播放超时：{name}");
                std::thread::sleep(Duration::from_millis(20));
            }
        } else {
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(50));
                player.update()?;
            }
        }
        player.stop();
        ensure!(!player.playing(), "音频没有停止：{name}");
        // Exercise cancellation while the Rust reader is still preparing.
        player.play_official(game, kind, name, &helper, &decoder)?;
        player.stop();
        ensure!(!player.playing(), "音频准备没有取消：{name}");
        println!(
            "{}",
            json!({"event":"audio","kind":kind,"name":name,"ok":true})
        );
        completed.push(name);
    }
    let result = json!({"event":"standalone-smoke","ok":true,"component_root":root,"engine":"rust","host":version,"project_roundtrip":true,"play_bundle":true,"audio":completed});
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}
