use super::*;

#[derive(Clone, Debug)]
pub(super) struct Preferences {
    pub widths: [f32; 2],
    pub auto_hide_palette: bool,
    pub export_bundle: bool,
    pub export_directory: Option<PathBuf>,
}
impl Preferences {
    pub fn load(prefs: &Value) -> Self {
        let mut widths = [shell::NAVIGATION_WIDTH, shell::PREVIEW_WIDTH];
        // Older preferences stored one pair per layout. Keep the pair the
        // author was using; subsequent saves use the single draggable layout.
        let stored = if prefs["workspace_widths"][0].is_array() {
            let selected = prefs["workspace_layout"].as_u64().unwrap_or(0).min(2) as usize;
            &prefs["workspace_widths"][selected]
        } else {
            &prefs["workspace_widths"]
        };
        for (panel, width) in widths.iter_mut().enumerate() {
            if let Some(saved) = stored[panel]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 160.0 && *n <= 1600.0)
            {
                *width = saved as f32;
            }
        }
        Self {
            widths,
            auto_hide_palette: prefs["palette_auto_hide"].as_bool().unwrap_or(false),
            export_bundle: prefs["export_bundle"].as_bool().unwrap_or(false),
            export_directory: prefs["export_directory"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        }
    }
}

impl App {
    pub(super) fn save_usability_preferences(&mut self) {
        if cfg!(test) {
            return;
        }
        let path = user_root().join("rust/preferences.json");
        let result = (|| -> anyhow::Result<()> {
            let mut value = if path.exists() {
                lom_core::load_json(&path)?
            } else {
                json!({})
            };
            anyhow::ensure!(value.is_object(), "个人设置格式无效");
            value.as_object_mut().unwrap().remove("workspace_layout");
            value["workspace_widths"] = json!(self.usability.widths);
            value["palette_auto_hide"] = json!(self.usability.auto_hide_palette);
            value["export_bundle"] = json!(self.usability.export_bundle);
            value["export_directory"] = json!(self.usability.export_directory);
            lom_core::project::atomic_write(&path, &lom_core::stable_json(&value)?)
        })();
        if let Err(error) = result {
            self.error = Some(crate::error_ui::capture(error));
        }
    }
    pub(super) fn remember_panel_width(&mut self, ctx: &egui::Context, panel: usize, width: f32) {
        if ctx.input(|i| i.pointer.any_released())
            && (self.usability.widths[panel] - width).abs() > 1.0
        {
            self.usability.widths[panel] = width;
            self.save_usability_preferences();
        }
    }
    pub(super) fn search_control(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = crate::i18n::key("usability.search.button");
            let response = ui.add_sized([30.0, 28.0], egui::Button::new(""));
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
            });
            let color = ui.style().interact(&response).fg_stroke.color;
            let center = response.rect.center() - egui::vec2(2.0, 2.0);
            let stroke = egui::Stroke::new(1.5_f32, color);
            ui.painter().circle_stroke(center, 5.0, stroke);
            ui.painter().line_segment(
                [center + egui::vec2(3.5, 3.5), center + egui::vec2(8.0, 8.0)],
                stroke,
            );
            if response
                .on_hover_text(crate::i18n::key("usability.search.hint"))
                .clicked()
            {
                self.open_global_search(ui.ctx());
            }
        });
    }
    pub(super) fn open_name_authoring(&mut self) {
        if self.is_vanilla_patch() {
            self.status = crate::i18n::key("usability.authoring.name_chapter");
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        let count = self.story()["nodes"].as_array().map_or(0, Vec::len);
        self.add_node("character_title");
        if self.story()["nodes"].as_array().map_or(0, Vec::len) == count + 1 {
            let node =
                &mut self.project.stories.get_mut(&self.current).unwrap()["nodes"][self.selected];
            node["name_action"] = "set".into();
            node["action"] = "keep".into();
        }
    }
    pub(super) fn open_custom_talent(&mut self) {
        if self.apply_pending_drafts() && self.open_center(Center::Assets) {
            self.image_import_target = None;
            self.asset_import_dialog = Some(AssetImportDialog {
                extends: String::new(),
                kind: 10,
                name: String::new(),
                paths: vec![],
                error: None,
            });
        }
    }
    pub(super) fn request_export(&mut self, ctx: &egui::Context) {
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("guided-export-open"), true);
            data.insert_temp(
                egui::Id::new("guided-export-bundle"),
                self.usability.export_bundle,
            );
        });
    }
    pub(super) fn guided_export(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("guided-export-open");
        if !ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false)) {
            return;
        }
        let mut close = false;
        let mut export = false;
        let choice_id = egui::Id::new("guided-export-bundle");
        let mut bundle = ctx
            .data(|data| data.get_temp::<bool>(choice_id))
            .unwrap_or(self.usability.export_bundle);
        let response = egui::Modal::new(egui::Id::new("guided-export")).show(ctx, |ui| {
            ui.set_width(440.0_f32.min(ctx.content_rect().width() - 64.0));
            close |= shell::dialog_header(ui, &crate::i18n::key("usability.export.title"));
            ui.label(crate::i18n::key("usability.export.recipient"));
            ui.radio_value(
                &mut bundle,
                false,
                crate::i18n::key("usability.export.configured"),
            );
            ui.small(crate::i18n::key("usability.export.configured_hint"));
            ui.add_space(8.0);
            ui.radio_value(
                &mut bundle,
                true,
                crate::i18n::key("usability.export.first_time"),
            );
            ui.small(crate::i18n::key("usability.export.first_time_hint"));
            let (submit, cancel) = dialog_actions(ui, "导出", true);
            export = submit;
            close |= cancel;
        });
        close |= response.should_close();
        ctx.data_mut(|data| data.insert_temp(choice_id, bundle));
        if export || close {
            ctx.data_mut(|data| {
                data.remove::<bool>(id);
                data.remove::<bool>(choice_id);
            });
        }
        if export {
            self.usability.export_bundle = bundle;
            self.save_usability_preferences();
            if bundle {
                self.export_play_bundle();
            } else {
                self.export();
            }
        }
    }
    pub(super) fn remember_export_directory(&mut self, path: &Path) {
        self.usability.export_directory = path.parent().map(Path::to_path_buf);
        self.save_usability_preferences();
    }
    pub(super) fn refresh_full_problems(&mut self) {
        self.preflight_tracked = true;
        self.diagnostics = lom_core::release::run_preflight(
            &self.project,
            lom_core::release::Profile::Editing,
            env!("CARGO_PKG_VERSION"),
        );
        self.error_issues = self
            .diagnostics
            .iter()
            .filter(|issue| issue.severity == "error")
            .cloned()
            .collect();
    }
    pub(super) fn problems_panel(&mut self, ctx: &egui::Context) {
        if !self.problems_open {
            return;
        }
        let mut location = None;
        egui::TopBottomPanel::bottom("project-problems")
            .frame(surface(true))
            .default_height(170.0)
            .height_range(85.0..=350.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(crate::i18n::message(
                        "usability.problems.title",
                        &[("count", self.diagnostics.len().to_string())],
                    ));
                    if ui
                        .small_button(crate::i18n::key("usability.problems.check"))
                        .clicked()
                    {
                        self.refresh_full_problems();
                    }
                    crate::error_ui::copy_button(ui, || {
                        crate::error_location::issue_report(&self.diagnostics, &self.project)
                    });
                    if self.needs_compile {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            self.problems_open = false;
                        }
                    });
                });
                ui.small(crate::i18n::key("usability.problems.severity_hint"));
                egui::ScrollArea::vertical()
                    .id_salt("persistent-problem-list")
                    .show(ui, |ui| {
                        if self.diagnostics.is_empty() {
                            ui.label(crate::i18n::key("usability.problems.none"));
                        }
                        for issue in &self.diagnostics {
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(
                                    if issue.severity == "error" {
                                        ui.visuals().error_fg_color
                                    } else {
                                        ui.visuals().warn_fg_color
                                    },
                                    crate::i18n::key(if issue.severity == "error" {
                                        "usability.problems.error"
                                    } else {
                                        "usability.problems.warning"
                                    }),
                                );
                                if ui
                                    .link(crate::error_location::issue_label(issue, &self.project))
                                    .clicked()
                                {
                                    location = Some((
                                        crate::error_location::issue_destination(
                                            issue,
                                            &self.project,
                                        ),
                                        issue.message.clone(),
                                    ));
                                }
                                ui.label(&issue.message);
                            });
                        }
                    });
            });
        if let Some((destination, message)) = location {
            crate::error_location::focus_field(ctx, &message);
            self.go_to_problem(ctx, destination);
        }
    }
}
