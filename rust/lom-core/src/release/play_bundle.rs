//! Portable player bundles contain our runtime and pinned loader distributions.
//! Official game binaries and the editor's extracted preview cache are never read.
use super::{build_release, text, version_greater, PreflightIssue};
use crate::{game_tools, package, project::Project, stable_json};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct PlayBundleResult {
    pub output: PathBuf,
    pub package_name: String,
    pub runtime_version: String,
    pub warnings: Vec<PreflightIssue>,
    pub message: String,
}

pub fn validate_loader_archive(architecture: &str, path: &Path) -> Result<Vec<u8>> {
    let distribution = game_tools::bepinex_distribution(architecture)?;
    ensure!(
        fs::metadata(path)?.len() <= 32 * 1024 * 1024,
        "BepInEx 压缩包过大"
    );
    let bytes = fs::read(path)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == distribution.sha256,
        "BepInEx 压缩包版本或位数不符：需要 {}",
        distribution.filename
    );
    Ok(bytes)
}

pub fn build_play_bundle(
    project: &Project,
    output: &Path,
    runtime_dir: &Path,
    installer: &Path,
    loader_archives: &BTreeMap<String, PathBuf>,
    runtime_version: &str,
) -> Result<PlayBundleResult> {
    ensure!(
        installer.is_file(),
        "缺少 lomc.exe 安装助手，请使用完整的 Windows 编辑器目录"
    );
    let mut files = BTreeMap::new();
    for name in game_tools::HOST_FILES {
        let path = runtime_dir.join(name);
        ensure!(path.is_file(), "运行时依赖缺失：{name}");
        files.insert(format!("runtime/{name}"), fs::read(path)?);
    }
    for architecture in ["x86", "x64"] {
        let archive = loader_archives
            .get(architecture)
            .with_context(|| format!("缺少 {architecture} BepInEx 加载组件"))?;
        files.insert(
            format!("loaders/{architecture}.zip"),
            validate_loader_archive(architecture, archive)?,
        );
    }
    files.insert("lomc.exe".into(), fs::read(installer)?);
    files.insert(
        "LICENSE.txt".into(),
        include_bytes!("../../../../LICENSE").to_vec(),
    );
    files.insert(
        "THIRD_PARTY_NOTICES.md".into(),
        include_bytes!("../../../../runtime/MortalModHost/THIRD_PARTY_NOTICES.md").to_vec(),
    );
    files.insert(
        "LICENSE.UnityDoorstop".into(),
        include_bytes!("../../../../editor/assets/doorstop/LICENSE.UnityDoorstop").to_vec(),
    );
    write_bundle(project, output, files, runtime_version)
}

fn write_bundle(
    project: &Project,
    output: &Path,
    mut files: BTreeMap<String, Vec<u8>>,
    runtime_version: &str,
) -> Result<PlayBundleResult> {
    let mut output = output.to_path_buf();
    output.set_extension("zip");
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    // Release validation and compilation capture every project-owned asset into
    // one portable .lommod, using the same path as ordinary release export.
    let built = build_release(
        project,
        &staging.path().join("work.lommod"),
        runtime_version,
    )?;
    let package_name = format!("{}.lommod", text(&project.manifest, "id"));
    files.insert(
        format!("mods/{package_name}"),
        fs::read(&built.package_path)?,
    );
    files.insert(
        "play_bundle.json".into(),
        stable_json(&json!({"format":1,"package":package_name,"runtime_version":runtime_version}))?,
    );
    files.insert("安装MOD.cmd".into(), b"@echo off\r\nchcp 65001 >nul\r\n\"%~dp0lomc.exe\" install-play-bundle \"%~dp0.\" %*\r\npause\r\n".to_vec());
    let readme = format!("{} {}\r\n作者：{}\r\n\r\n{}\r\n\r\n这是 Windows 完整游玩包。请先安装正版《活侠传》，退出游戏。\r\n1. 将整个 ZIP 解压到一个普通文件夹。\r\n2. 双击“安装MOD.cmd”；自动找到单个 Steam 游戏目录时会直接安装，否则按提示输入包含 Mortal.exe 的目录。\r\n3. 从 Steam 启动游戏，选择原版＋MOD 或纯 MOD 中对应的作品。\r\n\r\n本包包含作品 .lommod、自有图片和音频、MortalModHost {}、NVorbis、x86/x64 BepInEx 加载组件和安装助手。玩家无需安装编辑器或另找美术素材，无需联网。原版游戏及原版立绘仍由玩家本机游戏提供。\r\n\r\n已有其它 MOD 和存档由原安装规则保留；更新同一作品时保留其启用/停用状态。安装不会启动游戏。\r\n命令行：lomc.exe install-play-bundle <本包目录> --game <游戏目录>\r\n",text(&project.manifest,"name"),text(&project.manifest,"version"),text(&project.manifest,"author"),text(&project.manifest,"description"),runtime_version);
    files.insert("游玩说明.txt".into(), readme.into_bytes());
    let archive = staging.path().join("play-bundle.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        package::canonical_archive_name(&name)?;
        zip.start_file(name, options)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?.sync_all()?;
    fs::rename(archive, &output)?;
    Ok(PlayBundleResult {
        output: output.canonicalize()?,
        package_name,
        runtime_version: runtime_version.into(),
        warnings: built.warnings,
        message: "完整游玩包已导出；将 ZIP 发给玩家，解压后双击“安装MOD.cmd”。".into(),
    })
}

/// Uses the same bounded installers as the editor; never copy an unpacked tree
/// over the game or replace an unrelated work's package.
pub fn install_play_bundle(bundle: &Path, game: &Path) -> Result<Value> {
    ensure!(cfg!(windows), "游玩包安装请在 Windows 执行");
    game_tools::validate_root(game)?;
    ensure!(!game_tools::is_game_running()?, "请先退出游戏，再安装 MOD");
    let metadata: Value = crate::load_json(bundle.join("play_bundle.json"))?;
    ensure!(metadata["format"] == 1, "不支持的游玩包格式");
    let name = metadata["package"]
        .as_str()
        .context("游玩包缺少作品包文件名")?;
    ensure!(
        package::canonical_archive_name(name)? == name && !name.contains('/'),
        "游玩包文件名不安全"
    );
    let source = package::resolve_confined_file(bundle, &bundle.join("mods").join(name))?;
    let staging = crate::workdir::create()?;
    let snapshot = staging.path().join("source.lommod");
    ensure!(
        fs::metadata(&source)?.len() <= package::MAX_PACKAGE_BYTES,
        "Mod 包超过 160 MiB 上限"
    );
    fs::copy(&source, &snapshot)?;
    let entries = package::read_package(&snapshot)?;
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"])?;
    let runtime = bundle.join("runtime");
    for name in game_tools::HOST_FILES {
        package::resolve_confined_file(bundle, &runtime.join(name))?;
    }
    let host_version = game_tools::host_file_version(&runtime.join("MortalModHost.dll"))?;
    ensure!(
        !version_greater(text(&manifest, "min_host_version"), &host_version),
        "随包 Host {host_version} 低于作品要求，请重新导出完整游玩包"
    );
    let architecture = game_tools::game_architecture(game)?;
    let loader = package::resolve_confined_file(
        bundle,
        &bundle.join(format!("loaders/{architecture}.zip")),
    )?;
    validate_loader_archive(&architecture, &loader)?;
    let installed = game_tools::matching_installed_mod(game, &manifest)?;
    let (filename, enabled) = if let Some(row) = &installed {
        (
            Path::new(row["path"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_owned(),
            row["enabled"] == true,
        )
    } else {
        (std::ffi::OsString::from(name), true)
    };
    let captured = staging.path().join(filename);
    if captured != snapshot {
        fs::rename(&snapshot, &captured)?;
    }
    let windows = PathBuf::from(std::env::var_os("SystemRoot").context("缺少 SystemRoot")?);
    let system_version = game_tools::system_version_dll(&windows, &architecture)?;
    let doorstop = staging.path().join("doorstop.dll");
    fs::write(&doorstop, game_tools::bundled_doorstop(&architecture)?)?;
    game_tools::install_bepinex_archive(game, &loader)?;
    game_tools::apply_steam_launch_fix(game, Some(&doorstop), Some(&system_version))?;
    game_tools::install_runtime(game, &runtime)?;
    game_tools::apply_installation_doctor_fixes(game, &runtime)?;
    let target = game_tools::install_mod(game, &captured, enabled)?;
    Ok(
        json!({"ok":true,"path":target,"enabled":enabled,"message":if enabled {"作品已安装，请从 Steam 启动《活侠传》。"} else {"作品已更新，仍保持停用；可在 MOD 管理中启用。"},"game_tested":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn portable_package_embeds_project_art_and_does_not_walk_external_folders() {
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::new();
        project.manifest["id"] = json!("play_demo");
        project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("开场。");
        project.stories.get_mut("main").unwrap()["nodes"].as_array_mut().unwrap()
            .insert(1, json!({"id":"background1","type":"background","action":"show","image":"user:demo.art","fade":0}));
        project.assets.insert("assets/user/image/demo.art/content.json".into(), stable_json(&json!({"content_schema":1,"id":"demo.art","type":"image","name":"自有图片","files":{"main":"image.png"}})).unwrap());
        project
            .assets
            .insert("assets/user/image/demo.art/image.png".into(), vec![1, 2, 3]);
        fs::write(temp.path().join("private-preview.png"), b"private").unwrap();
        let output = temp.path().join("play.zip");
        let result = write_bundle(
            &project,
            &output,
            BTreeMap::from([("runtime/MortalModHost.dll".into(), b"fixture".to_vec())]),
            "1.4.2",
        )
        .unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&output).unwrap()).unwrap();
        let mut bytes = vec![];
        zip.by_name("mods/play_demo.lommod")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        let mod_path = temp.path().join("embedded.lommod");
        fs::write(&mod_path, bytes).unwrap();
        let entries = package::read_package(&mod_path).unwrap();
        assert_eq!(entries["assets/user/image/demo.art/image.png"], [1, 2, 3]);
        assert!(zip.by_name("安装MOD.cmd").is_ok());
        assert!(zip.by_name("游玩说明.txt").is_ok());
        assert!(zip.by_name("private-preview.png").is_err());
        assert_eq!(result.package_name, "play_demo.lommod");
        project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("在这里填写对白");
        let before = fs::read(&output).unwrap();
        assert!(write_bundle(&project, &output, BTreeMap::new(), "1.4.2").is_err());
        assert_eq!(fs::read(output).unwrap(), before);
    }

    #[test]
    fn invalid_loader_is_rejected_before_export() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("loader.zip");
        fs::write(&path, b"unrelated zip").unwrap();
        assert!(validate_loader_archive("x64", &path)
            .unwrap_err()
            .to_string()
            .contains("版本或位数不符"));
    }
}
