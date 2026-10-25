//! Guided setup only composes the existing, bounded file installers.
//! It never starts the game or loads an official game assembly.
use anyhow::{ensure, Context, Result};
use lom_core::game_tools;
use serde_json::{json, Value};
use std::path::Path;

pub fn install_project(game: &Path, project: &lom_core::project::Project) -> Result<Value> {
    let temp = lom_core::workdir::create()?;
    lom_core::validate::validate_manifest(&project.manifest)?;
    let id = project.manifest["id"].as_str().unwrap();
    let rows = game_tools::list_mods(game)?;
    let installed: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["manifest"]["id"] == id)
        .collect();
    ensure!(
        installed.len() <= 1,
        "同一作品已有多个安装包，请先在 MOD 列表中核对"
    );
    let (filename, enabled) = if let Some(row) = installed.first() {
        (
            Path::new(row["path"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_owned(),
            row["enabled"] == true,
        )
    } else {
        (std::ffi::OsString::from(format!("{id}.lommod")), true)
    };
    let package = temp.path().join(filename);
    project.export(&package)?;
    let path = game_tools::install_mod(game, &package, enabled)?;
    Ok(
        json!({"id":id,"path":path,"enabled":enabled,"message":if enabled {"作品已安装，下次启动游戏后生效。"} else {"作品已更新，仍保持停用；可在 MOD 列表中启用。"}}),
    )
}

pub fn diagnose(game: &Path, runtime: &Path) -> Result<Value> {
    let mut report = game_tools::diagnose_installation(game, runtime)?;
    let (ready, detail) = match game_tools::steam_launch_fix_applied(game) {
        Ok(ready) => (ready, String::new()),
        Err(error) => (false, format!("{error:#}")),
    };
    report["steam_ready"] = json!(ready);
    report["findings"].as_array_mut().unwrap().push(json!({
        "code":if ready {"steam_launch_ready"} else {"steam_launch_required"},
        "severity":if ready {"ok"} else {"error"},
        "fixable":!ready,"detail":detail,
    }));
    report["healthy"] = json!(report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["severity"] == "ok"));
    Ok(report)
}

pub fn validate_inputs(game: &Path, runtime: &Path) -> Result<()> {
    game_tools::validate_root(game)?;
    game_tools::game_architecture(game)?;
    for name in game_tools::HOST_FILES {
        ensure!(
            runtime.join(name).is_file(),
            "安装包缺少 {name}，请在高级维护中选择完整的宿主目录"
        );
    }
    Ok(())
}

pub fn configure(
    game: &Path,
    runtime: &Path,
    archive: Option<&Path>,
    mut progress: impl FnMut(&str, u64),
) -> Result<Value> {
    ensure!(cfg!(windows), "游戏配置请在 Windows 版编辑器中执行");
    validate_inputs(game, runtime)?;
    ensure!(!game_tools::is_game_running()?, "请先退出游戏，再进行配置");
    let architecture = game_tools::game_architecture(game)?;
    let distribution = game_tools::bepinex_distribution(&architecture)?;
    let workspace = lom_core::workdir::create()?;
    let downloaded = workspace.path().join(distribution.filename);
    let archive = if let Some(archive) = archive {
        progress("正在校验加载组件…", 0);
        archive
    } else {
        progress("正在下载官方加载组件…", 0);
        download(distribution.url, &downloaded)?;
        &downloaded
    };
    // The existing installer authenticates the exact pinned archive and bitness
    // before writing any files, and keeps existing mods and configuration.
    game_tools::install_bepinex_archive(game, archive)
        .context("加载组件安装失败；可在高级维护中选择官方离线压缩包后重新配置")?;
    progress("正在配置 Steam 启动…", 1);
    super::tools_panel::steam_launch_fix(game)?;
    progress("正在安装 MOD 支持组件…", 2);
    game_tools::install_runtime(game, runtime)?;
    game_tools::apply_installation_doctor_fixes(game, runtime)?;
    progress("正在检查配置结果…", 3);
    let mut report = diagnose(game, runtime)?;
    report["message"] = json!(if report["healthy"] == true {
        "游戏配置完成，可以安装 MOD 或返回剧情试玩。"
    } else {
        "配置操作已完成，仍有检查项需要处理，请查看下方结果。"
    });
    report["game_tested"] = json!(false);
    progress("配置检查完成", 4);
    Ok(report)
}

#[cfg(windows)]
fn download(url: &str, destination: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // Pass paths as environment values instead of interpolating shell code.
    // Bound both network waits and the download size, including chunked replies.
    let script = r#"
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$request = [Net.HttpWebRequest]::Create($env:LOM_SETUP_URL)
$request.Timeout = 120000
$request.ReadWriteTimeout = 120000
$response = $null; $inputStream = $null; $outputStream = $null
try {
    $response = $request.GetResponse()
    if ($response.ContentLength -gt 33554432) { throw 'Download exceeds 32 MiB' }
    $inputStream = $response.GetResponseStream()
    $outputStream = [IO.File]::Open($env:LOM_SETUP_ARCHIVE, [IO.FileMode]::CreateNew)
    $buffer = New-Object byte[] 65536
    $total = 0
    while (($count = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
        $total += $count
        if ($total -gt 33554432) { throw 'Download exceeds 32 MiB' }
        $outputStream.Write($buffer, 0, $count)
    }
} finally {
    if ($outputStream) { $outputStream.Dispose() }
    if ($inputStream) { $inputStream.Dispose() }
    if ($response) { $response.Close() }
}
"#;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("LOM_SETUP_URL", url)
        .env("LOM_SETUP_ARCHIVE", destination)
        .creation_flags(0x08000000)
        .output()
        .context("无法启动官方组件下载，请在高级维护中使用离线压缩包")?;
    ensure!(
        output.status.success(),
        "官方组件下载失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        std::fs::metadata(destination)?.len() > 0,
        "下载的压缩包为空"
    );
    Ok(())
}

#[cfg(not(windows))]
fn download(_url: &str, _destination: &Path) -> Result<()> {
    anyhow::bail!("游戏配置请在 Windows 版编辑器中执行")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_host_is_rejected_before_download_or_game_writes() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        std::fs::create_dir_all(game.join("Mortal_Data/Managed")).unwrap();
        std::fs::write(
            game.join("Mortal.exe"),
            game_tools::bundled_doorstop("x64").unwrap(),
        )
        .unwrap();
        let error = validate_inputs(&game, &temp.path().join("missing-runtime")).unwrap_err();
        assert!(error.to_string().contains("MortalModHost.dll"));
        assert!(!game.join("BepInEx").exists());
    }

    #[test]
    fn installing_two_projects_keeps_their_identities_and_originals() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        std::fs::create_dir_all(game.join("Mortal_Data/Managed")).unwrap();
        std::fs::write(game.join("Mortal.exe"), b"fixture").unwrap();
        let mut project = lom_core::project::Project::new();
        project.manifest["id"] = json!("first_work");
        let original = project.stories.clone();
        let first = install_project(&game, &project).unwrap();
        project.manifest["id"] = json!("second_work");
        let second = install_project(&game, &project).unwrap();
        assert_ne!(first["path"], second["path"]);
        assert_eq!(
            game_tools::list_mods(&game)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(project.stories, original);
        let row = game_tools::list_mods(&game).unwrap()[0].clone();
        let path = Path::new(row["path"].as_str().unwrap());
        game_tools::set_enabled(&game, path, false).unwrap();
        project.manifest["id"] = row["manifest"]["id"].clone();
        install_project(&game, &project).unwrap();
        let rows = game_tools::list_mods(&game).unwrap();
        assert_eq!(
            rows.as_array().unwrap().len(),
            2,
            "updates must not add a duplicate package"
        );
        assert_eq!(
            rows.as_array()
                .unwrap()
                .iter()
                .find(|r| r["manifest"]["id"] == project.manifest["id"])
                .unwrap()["enabled"],
            false,
            "updating must preserve disabled state"
        );
        project.manifest["id"] = json!("../escape");
        assert!(install_project(&game, &project).is_err());
        assert!(!temp.path().join("escape.lommod").exists());
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_never_downloads_or_writes_game_files() {
        let temp = tempfile::tempdir().unwrap();
        let result = configure(temp.path(), temp.path(), None, |_, _| {
            panic!("no setup on Mac")
        });
        assert!(result.unwrap_err().to_string().contains("Windows"));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
