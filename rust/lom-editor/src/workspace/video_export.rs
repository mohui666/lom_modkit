use super::*;
use crate::video_render::{Scope, Settings};

#[derive(Default)]
pub(super) struct Dialog {
    settings: Settings,
    output: Option<PathBuf>,
    error: Option<String>,
}

impl App {
    pub(super) fn open_video_export(&mut self) {
        if !self.apply_pending_drafts() || !self.check_before_run_or_export() {
            return;
        }
        if self.is_vanilla_patch() {
            self.error = Some(crate::i18n::key("video.patch_unavailable"));
            return;
        }
        self.tools_panel.refresh_game();
        if !self.tools_panel.ready_to_play() {
            self.open_tool_page(ToolPage::Game);
            self.status = crate::i18n::key("video.setup_required");
            return;
        }
        self.video_dialog = Some(Dialog {
            settings: Default::default(),
            output: None,
            error: None,
        });
    }

    pub(super) fn video_export_dialog(&mut self, ctx: &egui::Context) {
        if self.video_dialog.is_none() || self.error.is_some() || self.tools_panel.dialog_open() {
            return;
        }
        let (_, escape) = self.dialog_keys(ctx);
        let mut dialog = self.video_dialog.take().unwrap();
        let mut close = escape;
        let mut choose_output = false;
        let mut start = false;
        egui::Modal::new(egui::Id::new("native-video-export"))
            .frame(shell::dialog_frame_for(ctx))
            .show(ctx, |ui| {
                ui.set_width(560.0_f32.min(ctx.content_rect().width() - 60.0));
                close |= shell::dialog_header(ui, &crate::i18n::key("video.title"));
                egui::ScrollArea::vertical()
                    .id_salt("video-export-settings")
                    .max_height((ctx.content_rect().height() - 180.0).max(120.0))
                    .show(ui, |ui| {
                        ui.add_space(8.0);
                        ui.label(crate::i18n::key("video.hint"));
                        ui.checkbox(
                            &mut dialog.settings.background,
                            crate::i18n::key("video.background"),
                        );
                        ui.small(crate::i18n::key(if dialog.settings.background {
                            "video.requirements.background"
                        } else {
                            "video.requirements"
                        }));
                        ui.add_space(12.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(crate::i18n::key("video.scope"));
                            for (scope, key) in [
                                (Scope::Chapter, "video.scope.chapter"),
                                (Scope::Project, "video.scope.project"),
                                (Scope::Step, "video.scope.step"),
                            ] {
                                ui.selectable_value(
                                    &mut dialog.settings.scope,
                                    scope,
                                    crate::i18n::key(key),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::key("video.resolution"));
                            if ui
                                .selectable_label(dialog.settings.height == 1080, "1080p")
                                .clicked()
                            {
                                dialog.settings.width = 1920;
                                dialog.settings.height = 1080;
                            }
                            if ui
                                .selectable_label(dialog.settings.height == 720, "720p")
                                .clicked()
                            {
                                dialog.settings.width = 1280;
                                dialog.settings.height = 720;
                            }
                            ui.add_space(14.0);
                            ui.label(crate::i18n::key("video.fps"));
                            ui.selectable_value(&mut dialog.settings.fps, 30, "30 FPS");
                            ui.selectable_value(&mut dialog.settings.fps, 60, "60 FPS");
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label(crate::i18n::key("video.speed"));
                            for (speed, key) in [
                                ("game", "video.speed.game"),
                                ("slow", "video.speed.slow"),
                                ("standard", "video.speed.standard"),
                                ("fast", "video.speed.fast"),
                            ] {
                                ui.selectable_value(
                                    &mut dialog.settings.auto_speed,
                                    speed.into(),
                                    crate::i18n::key(key),
                                );
                            }
                        });
                        ui.small(crate::i18n::key("video.timing_hint"));
                        ui.add_space(8.0);
                        ui.collapsing(crate::i18n::key("video.choices"), |ui| {
                            ui.small(crate::i18n::key("video.choices_hint"));
                            ui.horizontal(|ui| {
                                ui.label(crate::i18n::key("video.choice_hold"));
                                ui.add(
                                    egui::DragValue::new(&mut dialog.settings.choice_hold)
                                        .range(0.0..=30.0)
                                        .speed(0.1),
                                );
                            });
                            egui::ScrollArea::vertical()
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    for (chapter, story) in &self.project.stories {
                                        if dialog.settings.scope == Scope::Chapter
                                            && chapter != &self.current
                                        {
                                            continue;
                                        }
                                        for node in story["nodes"]
                                            .as_array()
                                            .into_iter()
                                            .flatten()
                                            .filter(|node| node["type"] == "choice")
                                        {
                                            let node_id = node["id"].as_str().unwrap_or("");
                                            let location = format!("{chapter}/{node_id}");
                                            let options = node["options"]
                                                .as_array()
                                                .map(Vec::as_slice)
                                                .unwrap_or(&[]);
                                            let mut index =
                                                dialog.settings.choices.get(&location).copied();
                                            ui.label(format!(
                                                "{} · {}",
                                                story["title"].as_str().unwrap_or(chapter),
                                                node.get("_editor")
                                                    .and_then(|editor| editor["note"].as_str())
                                                    .filter(|note| !note.is_empty())
                                                    .unwrap_or(node_id)
                                            ));
                                            let label = index
                                                .and_then(|index| options.get(index))
                                                .and_then(|option| option["text"].as_str())
                                                .map(str::to_owned)
                                                .unwrap_or_else(|| {
                                                    crate::i18n::key("video.choice.first")
                                                });
                                            egui::ComboBox::from_id_salt((
                                                "video-choice",
                                                &location,
                                            ))
                                            .width(ui.available_width().min(490.0))
                                            .selected_text(label)
                                            .show_ui(
                                                ui,
                                                |ui| {
                                                    ui.selectable_value(
                                                        &mut index,
                                                        None,
                                                        crate::i18n::key("video.choice.first"),
                                                    );
                                                    for (value, option) in
                                                        options.iter().enumerate()
                                                    {
                                                        ui.selectable_value(
                                                            &mut index,
                                                            Some(value),
                                                            format!(
                                                                "{}. {}",
                                                                value + 1,
                                                                option["text"]
                                                                    .as_str()
                                                                    .unwrap_or("")
                                                            ),
                                                        );
                                                    }
                                                },
                                            );
                                            if let Some(index) = index {
                                                dialog.settings.choices.insert(location, index);
                                            } else {
                                                dialog.settings.choices.remove(&location);
                                            }
                                        }
                                    }
                                });
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::key("video.output"));
                            choose_output =
                                ui.button(crate::i18n::key("video.choose_output")).clicked();
                        });
                        if let Some(path) = &dialog.output {
                            ui.add(egui::Label::new(path.display().to_string()).wrap())
                                .on_hover_text(path.display().to_string());
                        }
                        if let Some(error) = &dialog.error {
                            crate::error_ui::show(ui, error);
                        }
                    });
                ui.add_space(10.0);
                shell::dialog_buttons(ui, |ui| {
                    start = ui
                        .add_enabled(
                            dialog.output.is_some(),
                            egui::Button::new(crate::i18n::key("video.start"))
                                .fill(ui.visuals().selection.bg_fill),
                        )
                        .clicked();
                    close |= ui.button(tr("取消")).clicked();
                });
            });
        if choose_output {
            let name = self.project.manifest["id"].as_str().unwrap_or("story");
            if let Some(path) = self
                .export_dialog()
                .add_filter("MP4", &["mp4"])
                .set_file_name(format!("{name}.mp4"))
                .save_file()
            {
                dialog.output = Some(path);
            }
        }
        if start {
            let node = self.story()["nodes"][self.selected]["id"]
                .as_str()
                .unwrap_or("");
            match crate::video_render::prepare(
                &self.project,
                &self.current,
                node,
                dialog.settings.clone(),
            ) {
                Ok(prepared) => {
                    self.stop_preview();
                    self.audio.stop();
                    self.flush();
                    self.tools_panel
                        .render_video(prepared, dialog.output.unwrap());
                    return;
                }
                Err(error) => dialog.error = Some(crate::error_ui::capture(error)),
            }
        }
        if !close {
            self.video_dialog = Some(dialog);
        }
    }
}
