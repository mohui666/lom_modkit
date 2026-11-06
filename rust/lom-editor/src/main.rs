mod asset_extraction;
mod authoring;
mod editor_update;
mod effect_capture;
mod error_location;
mod forms;
mod game_setup;
mod help;
mod http_download;
mod i18n;
mod initial_setup;
mod macos_window;
mod persistence;
mod play_bundle;
mod preview;
mod preview_layout;
mod route_map;
mod shell;
mod tool_report;
mod tools_panel;
mod vanilla_panel;
mod workspace;

fn main() -> eframe::Result {
    workspace::run()
}
mod advanced;

mod audio;
mod content_panel;
mod manifest_panel;
mod scenario_panel;
mod scenario_quick;
