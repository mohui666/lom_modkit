mod authoring;
mod forms;
mod i18n;
mod macos_glass;
mod preview;
mod tools_panel;
mod workspace;

fn main() -> eframe::Result {
    workspace::run()
}
