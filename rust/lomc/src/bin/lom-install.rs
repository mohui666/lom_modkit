#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        dialog()
            .set_level(rfd::MessageLevel::Error)
            .set_description(format!("{error:#}"))
            .show();
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn dialog() -> rfd::MessageDialog {
    rfd::MessageDialog::new().set_title("活侠传 MOD 安装")
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use lom_core::{game_discovery, game_tools, release};
    use rfd::{MessageButtons, MessageDialogResult};

    let bundle = release::unpack_play_installer(&std::env::current_exe()?)?;
    let metadata = lom_core::load_json(bundle.path().join("play_bundle.json"))?;
    let name = metadata["name"].as_str().unwrap_or("MOD");
    let version = metadata["version"].as_str().unwrap_or("");
    let selection = game_discovery::installed_games();
    let games = match selection {
        Ok(found) => found,
        Err(error) => {
            dialog()
                .set_level(rfd::MessageLevel::Warning)
                .set_description(format!("自动定位失败，请手动选择游戏目录。\n\n{error:#}"))
                .show();
            Vec::new()
        }
    };
    let game = if let [found] = games.as_slice() {
        match dialog().set_buttons(MessageButtons::YesNoCancel)
            .set_description(format!("安装 {name} {version}\n\n已找到《活侠传》\n{}\n\n选择「是」开始安装，选择「否」可换目录。", found.display()))
            .show()
        {
            MessageDialogResult::Yes => Some(found.clone()),
            MessageDialogResult::No => choose_game(&games, name, version)?,
            _ => None,
        }
    } else {
        choose_game(&games, name, version)?
    };
    let Some(game) = game else { return Ok(()) };
    game_tools::validate_root(&game)?;
    let result = release::install_play_bundle(bundle.path(), &game)?;
    dialog()
        .set_description(format!(
            "{name} {version}\n\n{}",
            result["message"].as_str().unwrap_or("安装完成")
        ))
        .show();
    Ok(())
}

#[cfg(windows)]
fn choose_game(
    games: &[std::path::PathBuf],
    name: &str,
    version: &str,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    use rfd::{FileDialog, MessageButtons, MessageDialogResult};
    let picker = FileDialog::new().set_title("选择包含 Mortal.exe 的《活侠传》游戏目录");
    let picker = if let Some(first) = games.first() {
        picker.set_directory(first)
    } else {
        picker
    };
    let Some(game) = picker.pick_folder() else {
        return Ok(None);
    };
    lom_core::game_tools::validate_root(&game)?;
    let confirmed = dialog()
        .set_buttons(MessageButtons::OkCancel)
        .set_description(format!("安装 {name} {version}\n\n{}", game.display()))
        .show();
    Ok((confirmed == MessageDialogResult::Ok).then_some(game))
}

#[cfg(not(windows))]
fn main() {
    eprintln!("一键安装包请在 Windows 运行");
}
