mod asset_extraction;
mod authoring;
mod components;
mod editor_update;
mod effect_capture;
mod error_location;
mod error_ui;
mod forms;
mod game_setup;
mod graph_edges;
mod help;
mod http_download;
mod i18n;
mod initial_setup;
mod macos_window;
mod native_forms;
mod persistence;
mod play_bundle;
mod preview;
mod preview_layout;
mod route_map;
mod shell;
mod standalone_smoke;
mod tool_report;
mod tools_panel;
mod vanilla_panel;
mod workspace;

fn main() -> eframe::Result {
    if let Err(error) = components::root() {
        rfd::MessageDialog::new().set_title("无法准备编辑器组件")
            .set_description("请检查系统磁盘空间和当前账户的写入权限，然后重新打开编辑器。若仍然失败，请重新下载编辑器 exe。")
            .set_level(rfd::MessageLevel::Error).show();
        return Err(eframe::Error::AppCreation(error.into()));
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--component-task") {
        match components::task(&args[1..]) {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
        }
    }
    if args.first().is_some_and(|s| s == "--standalone-smoke") {
        let result = if args.len() == 4 {
            standalone_smoke::run(
                std::path::Path::new(&args[1]),
                std::path::Path::new(&args[2]),
                std::path::Path::new(&args[3]),
            )
        } else {
            Err(anyhow::anyhow!("需要项目、游戏目录、输出目录"))
        };
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
        }
    }
    workspace::run()
}
mod advanced;

mod audio;
mod content_panel;
mod manifest_panel;
mod scenario_panel;
mod scenario_quick;
