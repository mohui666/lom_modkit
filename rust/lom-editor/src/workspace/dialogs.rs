//! Editor dialogs and their input handling.
use super::*;

impl App {
    pub(super) fn dialog_keys(&mut self, ctx: &egui::Context) -> (bool, bool) {
        if !self.settings_open
            && !self.updater.open
            && self.chapter_dialog.is_none()
            && self.delete_chapter_dialog.is_none()
            && self.rename_dialog.is_none()
            && self.asset_import_dialog.is_none()
            && self.error.is_none()
            && self.pending.is_none()
            && self.pending_template.is_none()
            && self.recovery_candidates.is_empty()
        {
            self.dialog_ime = false;
            return (false, false);
        }
        let was_composing = self.dialog_ime;
        let events = ctx.input(|i| i.events.clone());
        let mut ime_event = false;
        for event in events {
            if let egui::Event::Ime(event) = event {
                ime_event = true;
                match event {
                    egui::ImeEvent::Enabled => {}
                    egui::ImeEvent::Preedit(text) => self.dialog_ime = !text.is_empty(),
                    egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => self.dialog_ime = false,
                }
            }
        }
        // Return confirms an IME candidate before it can confirm the dialog.
        // Popups likewise own their Return/Escape while a list is open.
        if was_composing || self.dialog_ime || ime_event || egui::Popup::is_any_open(ctx) {
            return (false, false);
        }
        ctx.input_mut(|i| {
            if !i.modifiers.is_none() {
                return (false, false);
            }
            (
                !self.settings_open && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        })
    }
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if self.tools_panel.dialog_open() {
            return;
        }
        let (return_pressed, escape_pressed) = self.dialog_keys(ctx);
        if let Some(message) = self.error.clone() {
            let mut close = false;
            let mut locate = false;
            let mut issue_location = None;
            let destination = crate::error_location::resolve(&message, &self.project);
            egui::Modal::new(egui::Id::new("operation-error"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(460.0_f32.min(ctx.content_rect().width() - 80.0));
                    close |= shell::dialog_header(ui, &crate::error_ui::title(&message));
                    egui::ScrollArea::vertical()
                        .max_height(ctx.content_rect().height() * 0.5)
                        .show(ui, |ui| {
                            crate::error_ui::show(ui, &message);
                            for issue in &self.error_issues {
                                ui.add_space(10.0);
                                ui.group(|ui| {
                                    ui.strong(crate::error_location::issue_label(
                                        issue,
                                        &self.project,
                                    ));
                                    crate::error_ui::show(ui, &issue.message);
                                    let target = crate::error_location::issue_destination(
                                        issue,
                                        &self.project,
                                    );
                                    if target != crate::error_location::Destination::Current
                                        && ui.button(target.label()).clicked()
                                    {
                                        issue_location = Some((target, issue.message.clone()));
                                    }
                                });
                            }
                            crate::error_ui::copy_button(ui, || {
                                if self.error_issues.is_empty() {
                                    message.clone()
                                } else {
                                    format!(
                                        "{message}\n\n{}",
                                        crate::error_location::issue_report(
                                            &self.error_issues,
                                            &self.project
                                        )
                                    )
                                }
                            });
                        });
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| {
                        if self.error_issues.is_empty()
                            && destination != crate::error_location::Destination::Current
                        {
                            locate = ui
                                .button(crate::error_location::action_label(&message, &destination))
                                .clicked();
                        }
                        close |= ui
                            .add_sized([80.0, 30.0], egui::Button::new(tr("关闭")))
                            .clicked();
                    });
                });
            if close || escape_pressed || locate || issue_location.is_some() {
                self.error = None;
                self.error_issues.clear();
            }
            if let Some((target, detail)) = issue_location {
                crate::error_location::focus_field(ctx, &detail);
                self.go_to_problem(ctx, target);
            } else if locate {
                crate::error_location::focus_field(ctx, &message);
                self.go_to_problem(ctx, destination);
            }
            return;
        }
        if self.updater.open
            && !self.settings_open
            && self.pending.is_none()
            && self.chapter_dialog.is_none()
            && self.delete_chapter_dialog.is_none()
            && self.rename_dialog.is_none()
            && self.asset_import_dialog.is_none()
            && self.recovery_candidates.is_empty()
        {
            if self.updater.dialog(ctx) {
                self.request(Pending::EditorUpdate, ctx);
            }
            return;
        }
        if self.settings_open {
            let mut close = false;
            let mut selected = None;
            let mut startup = self.check_updates_on_startup;
            let mut startup_changed = false;
            let mut auto_save = self.auto_save_project;
            let mut auto_save_seconds = self.auto_save_seconds;
            let mut auto_save_changed = false;
            let mut choose_library = false;
            let mut extract_library = false;
            let mut resolution_changed = false;
            egui::Modal::new(egui::Id::new("application-settings"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(520.0_f32.min(ctx.content_rect().width() - 60.0));
                    close |= shell::dialog_header(ui, &tr("设置"));
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        for (page, label) in ["常规", "素材与路径", "更新"].iter().enumerate() {
                            ui.selectable_value(&mut self.settings_page, page, tr(label));
                        }
                    });
                    ui.separator();
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 180.0).max(80.0))
                        .show(ui, |ui| {
                    match self.settings_page {
                        0 => {
                            ui.strong(tr("界面语言"));
                            for (locale, name) in crate::i18n::LOCALES.into_iter().zip(crate::i18n::NAMES) {
                                if ui.radio(crate::i18n::locale() == locale, name).clicked() { selected = Some(locale); }
                            }
                            ui.small(tr("只改变编辑器界面，不改写作品的台词与语言。"));
                            ui.add_space(12.0);
                            resolution_changed = self.shell.resolution_settings(ui);
                            ui.add_space(12.0);
                            auto_save_changed |= ui.checkbox(&mut auto_save, crate::i18n::key("editor.autosave.enabled")).changed();
                            ui.add_enabled_ui(auto_save, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for (seconds, key) in [(30,"editor.autosave.seconds30"),(60,"editor.autosave.minute1"),(300,"editor.autosave.minutes5"),(600,"editor.autosave.minutes10")] {
                                        auto_save_changed |= ui.selectable_value(&mut auto_save_seconds,seconds,crate::i18n::key(key)).changed();
                                    }
                                });
                            });
                            ui.small(crate::i18n::key("editor.autosave.hint"));
                        }
                        1 => {
                            ui.strong(crate::i18n::key("editor.workspace.preview_library"));
                            if self.preview.library.join("preview_map.json").is_file() { ui.label(self.preview.library.display().to_string()); }
                            else { ui.label(tr("尚未选择已提取的预览素材库")); }
                            choose_library = ui.button(tr("选择预览素材库…")).clicked();
                            extract_library = ui.button(tr("从游戏提取预览素材…")).clicked();
                            ui.add_space(12.0);
                            ui.strong(crate::i18n::key("library.title"));
                            let library = lom_core::content::default_repository_root();
                            ui.label(library.display().to_string());
                            if ui.button(tr("复制用户内容库路径")).clicked() { ctx.copy_text(library.display().to_string()); }
                            ui.small(tr("作品素材随项目保存；更新编辑器会保留内容库和配置。"));
                        }
                        _ => {
                            ui.strong(format!("{} {}", tr("编辑器版本"), env!("CARGO_PKG_VERSION")));
                            startup_changed = ui.checkbox(&mut startup, tr("启动时检查更新")).changed();
                            ui.small(tr("每次启动在后台检查一次，只有发现新版本才弹窗；无新版或网络失败时保持静默。"));
                            ui.add_space(8.0);
                            ui.label(tr("手动检查更新位于“游戏与 MOD”页面。"));
                        }
                    }
                    if let Some(error) = &self.settings_error { crate::error_ui::show_with_actions(ui, error); }
                        });
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| { close |= ui.add_sized([80.0,30.0],egui::Button::new(tr("完成"))).clicked(); });
                });
            let prefs = user_root().join("rust/preferences.json");
            if auto_save_changed {
                self.settings_error = self
                    .set_auto_save(auto_save, auto_save_seconds, &prefs)
                    .err()
                    .map(|error| crate::error_ui::capture(error));
            }
            if resolution_changed {
                let mut preferences = lom_core::load_json(&prefs).unwrap_or(json!({}));
                self.shell.store_resolution_preferences(&mut preferences);
                self.settings_error = lom_core::stable_json(&preferences)
                    .and_then(|bytes| lom_core::project::atomic_write(&prefs, &bytes))
                    .err()
                    .map(|error| crate::error_ui::capture(error));
            }
            if let Some(locale) = selected {
                self.settings_error = self
                    .set_interface_locale(locale, &prefs)
                    .err()
                    .map(|e| crate::error_ui::capture(e));
            }
            if startup_changed {
                self.settings_error = self
                    .set_startup_update_check(startup, &prefs)
                    .err()
                    .map(|e| crate::error_ui::capture(e));
            }
            if close || escape_pressed {
                self.settings_open = false;
            }
            if choose_library {
                self.settings_open = false;
                self.choose_preview_library();
            }
            if extract_library {
                self.settings_open = false;
                self.tools_panel.start_extraction();
            }
            return;
        }
        if let Some(draft) = &mut self.asset_import_dialog {
            let mut submit = false;
            let mut cancel = false;
            let mut choose_files = false;
            let mut choose_folder = false;
            let prefix_id = egui::Id::new("portrait-folder-prefix");
            let mut folder_prefix = ctx
                .data_mut(|data| data.get_temp::<String>(prefix_id))
                .unwrap_or_default();
            egui::Modal::new(egui::Id::new("asset-import"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    cancel |= shell::dialog_header(ui, &tr("导入素材"));
                    ui.add_space(16.0);
                    let old_kind = draft.kind;
                    ui.label(format!("{} *", tr("素材类型")));
                    if self.image_import_target.is_some() {
                        ui.label(tr("图片"));
                        draft.kind = 0;
                    } else {
                        egui::ComboBox::from_id_salt("import-kind")
                            .selected_text(tr([
                                "图片",
                                "角色",
                                "音乐",
                                "音效",
                                "配音",
                                "物品",
                                "功法",
                                "决斗援助",
                                "自创招式",
                                "消耗品",
                                "自定义天赋",
                            ][draft.kind]))
                            .show_ui(ui, |ui| {
                                for (i, kind) in [
                                    "图片",
                                    "角色",
                                    "音乐",
                                    "音效",
                                    "配音",
                                    "物品",
                                    "功法",
                                    "决斗援助",
                                    "自创招式",
                                    "消耗品",
                                    "自定义天赋",
                                ]
                                .iter()
                                .enumerate()
                                {
                                    ui.selectable_value(&mut draft.kind, i, tr(kind));
                                }
                            });
                    }
                    if old_kind != draft.kind {
                        draft.paths.clear();
                        draft.name.clear();
                        draft.error = None;
                        draft.extends.clear();
                    }
                    if draft.kind == 1 {
                        let mut target = if draft.extends.is_empty() {
                            json!({})
                        } else {
                            json!({"extends":draft.extends})
                        };
                        self.catalog.field(
                            ui,
                            &mut target,
                            "extends",
                            "扩展原版人物",
                            "official_character",
                            true,
                            &[],
                            &[],
                        );
                        draft.extends = target["extends"].as_str().unwrap_or("").to_owned();
                        ui.small(crate::i18n::key("editor.workspace.character_hint"));
                    }
                    if draft.kind != 10 {
                        choose_files = ui
                            .button(format!(
                                "{} *",
                                tr(if draft.kind == 8 {
                                    "选择 PNG 动画帧…"
                                } else if draft.kind == 1 {
                                    if draft.paths.is_empty() {
                                        "选择立绘文件…"
                                    } else {
                                        "继续添加立绘…"
                                    }
                                } else {
                                    "选择文件…"
                                })
                            ))
                            .clicked();
                    }
                    if draft.kind == 1 {
                        ui.small(tr("可按住 Ctrl 多选，Ctrl+A 全选。"));
                        ui.add(
                            egui::TextEdit::singleline(&mut folder_prefix)
                                .hint_text(tr("文件名前缀（可选）")),
                        );
                        choose_folder = ui.button(tr("从文件夹选择全部立绘…")).clicked();
                    }
                    if draft.kind == 10 || !draft.paths.is_empty() {
                        for path in &draft.paths {
                            ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                        }
                        ui.label(format!("{} *", tr("素材名称")));
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.name)
                                .id(egui::Id::new("asset-import-name"))
                                .desired_width(f32::INFINITY),
                        );
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    let can_submit = (draft.kind == 10 || !draft.paths.is_empty())
                        && !draft.name.trim().is_empty();
                    let actions = dialog_actions(ui, "导入", can_submit);
                    submit = actions.0;
                    cancel |= actions.1;
                    submit |= return_pressed && can_submit;
                });
            ctx.data_mut(|data| data.insert_temp(prefix_id, folder_prefix.clone()));
            if cancel || escape_pressed {
                self.asset_import_dialog = None;
                self.image_import_target = None;
            } else if choose_folder {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    match crate::content_panel::portrait_files(&folder, &folder_prefix) {
                        Ok(paths) => draft.select_files(paths),
                        Err(error) => draft.error = Some(crate::error_ui::capture(error)),
                    }
                }
            } else if choose_files {
                let dialog = rfd::FileDialog::new();
                let paths = if draft.kind == 8 {
                    dialog.add_filter("PNG 动画帧", &["png"]).pick_files()
                } else if draft.kind == 1 {
                    dialog
                        .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                        .pick_files()
                } else if draft.kind == 0 || draft.kind >= 5 {
                    dialog
                        .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                        .pick_file()
                        .map(|path| vec![path])
                } else {
                    dialog
                        .add_filter("WAV / OGG", &["wav", "ogg"])
                        .pick_file()
                        .map(|path| vec![path])
                };
                if let Some(paths) = paths {
                    draft.select_files(paths);
                }
            } else if submit {
                if let Err(error) = self.commit_asset_import() {
                    self.asset_import_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some(chapter) = self
            .delete_chapter_dialog
            .as_ref()
            .map(|draft| draft.chapter.clone())
        {
            let story = self.project.stories.get(&chapter);
            let name = story
                .and_then(|story| story["title"].as_str())
                .unwrap_or(&chapter)
                .to_owned();
            let steps = story
                .and_then(|story| story["nodes"].as_array())
                .map(Vec::len)
                .unwrap_or(0);
            let references = self.chapter_reference_count(&chapter);
            let mut choices: Vec<_> = self
                .project
                .stories
                .iter()
                .filter(|(id, _)| *id != &chapter)
                .map(|(id, story)| (id.clone(), story["title"].as_str().unwrap_or(id).to_owned()))
                .collect();
            crate::forms::number_duplicate_labels(&mut choices);
            let draft = self.delete_chapter_dialog.as_mut().unwrap();
            let mut submit = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("chapter-delete"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(440.0_f32.min(ctx.content_rect().width() - 60.0));
                    cancel |= shell::dialog_header(ui, &tr("删除章节"));
                    ui.add_space(12.0);
                    ui.label(format!("{}：{name}", tr("即将删除章节")));
                    ui.label(format!("{}：{steps}", tr("本章步骤数")));
                    ui.label(tr("删除本章及其全部步骤，可撤销；素材仍然保留。"));
                    if references > 0 {
                        ui.add_space(12.0);
                        ui.label(format!(
                            "{}：{references}",
                            tr("将改到替代章节的入口、跳转及事件引用")
                        ));
                        ui.label(tr("替代章节"));
                        crate::forms::SearchCombo::from_id_salt("delete-chapter-replacement")
                            .width(ui.available_width())
                            .selected_text(
                                choices
                                    .iter()
                                    .find(|(id, _)| id == &draft.replacement)
                                    .map(|(_, name)| name.as_str())
                                    .unwrap_or(""),
                            )
                            .show_ui(ui, |ui, query| {
                                for (id, name) in &choices {
                                    if !crate::forms::resource_matches(id, name, query) {
                                        continue;
                                    }
                                    if ui
                                        .selectable_value(&mut draft.replacement, id.clone(), name)
                                        .clicked()
                                    {
                                        ui.close();
                                    }
                                }
                            });
                        ui.small(tr("确认删除时，这些引用会一并改到所选章节。"));
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| {
                        let confirm = ui.add_sized(
                            [100.0, 30.0],
                            egui::Button::new(RichText::new(tr("删除章节")).color(Color32::WHITE))
                                .fill(Color32::from_rgb(170, 45, 45)),
                        );
                        submit = confirm.clicked() || (return_pressed && confirm.has_focus());
                        let dismiss = ui.add_sized([80.0, 30.0], egui::Button::new(tr("取消")));
                        cancel |= dismiss.clicked() || (return_pressed && dismiss.has_focus());
                    });
                });
            if cancel || escape_pressed {
                self.delete_chapter_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_delete_chapter() {
                    self.delete_chapter_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some(draft) = &mut self.chapter_dialog {
            let mut submit = false;
            let mut cancel = false;
            let title = match draft.action {
                ChapterAction::New => "新建章节",
                ChapterAction::Duplicate(_) => "复制章节",
                ChapterAction::NewGroup => "新建章节分组",
                ChapterAction::RenameGroup(_) => "重命名章节分组",
            };
            let group = matches!(
                draft.action,
                ChapterAction::NewGroup | ChapterAction::RenameGroup(_)
            );
            egui::Modal::new(egui::Id::new("chapter-create"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    cancel |= shell::dialog_header(ui, &tr(title));
                    ui.add_space(16.0);
                    ui.label(format!(
                        "{} *",
                        tr(if group {
                            "分组名称"
                        } else {
                            "章节名称"
                        })
                    ));
                    let name = ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .id(egui::Id::new("chapter-name"))
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut draft.focus_name) {
                        name.request_focus();
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    let can_submit = !draft.name.trim().is_empty();
                    let actions = dialog_actions(
                        ui,
                        if matches!(draft.action, ChapterAction::RenameGroup(_)) {
                            "保存"
                        } else {
                            "创建"
                        },
                        can_submit,
                    );
                    submit = actions.0;
                    cancel |= actions.1;
                    submit |= return_pressed && can_submit;
                });
            if cancel || escape_pressed {
                self.chapter_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_chapter() {
                    self.chapter_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some(draft) = &mut self.rename_dialog {
            let mut submit = false;
            let mut cancel = false;
            let title = match draft.target {
                RenameTarget::Node { .. } => "修改步骤标识",
                RenameTarget::Chapter(_) => "修改章节标识",
            };
            egui::Modal::new(egui::Id::new("advanced-rename"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    cancel |= shell::dialog_header(ui, &tr(title));
                    ui.add_space(16.0);
                    ui.label(tr("内部标识用于流程引用，修改时会同步关联内容。"));
                    ui.label(format!("{} *", tr("内部标识")));
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut draft.value)
                            .id(egui::Id::new("advanced-rename-value"))
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut draft.focus_name) {
                        field.request_focus();
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    let can_submit = !draft.value.trim().is_empty();
                    let actions = dialog_actions(ui, "应用", can_submit);
                    submit = actions.0;
                    cancel |= actions.1;
                    submit |= return_pressed && can_submit;
                });
            if cancel || escape_pressed {
                self.rename_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_rename() {
                    self.rename_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some((nodes, warnings)) = self.pending_template.clone() {
            let mut cancel = false;
            let mut insert = false;
            let structured = self.story()["vanilla_import"]["schema"] == 2;
            egui::Modal::new(egui::Id::new("template-boundary"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(460.0);
                    cancel |= shell::dialog_header(ui, &tr("模板边界引用"));
                    ui.label(crate::i18n::key(if structured {
                        "workspace.template.structured"
                    } else {
                        "workspace.template.boundary"
                    }));
                    for warning in warnings {
                        let warning = if structured {
                            warning
                                .replace(
                                    "会保留，请确认目标章节中存在该 ID",
                                    "将接回插入位置后续步骤",
                                )
                                .replace("插入后请重新绑定", "将接回插入位置后续步骤")
                        } else {
                            warning
                        };
                        ui.label(format!("• {}", crate::error_ui::text(&warning)));
                    }
                    (insert, cancel) = {
                        let (submit, footer_cancel) = dialog_actions(
                            ui,
                            if structured {
                                "接回当前流程并插入"
                            } else {
                                "保留引用并插入"
                            },
                            true,
                        );
                        (submit, cancel || footer_cancel)
                    };
                });
            if cancel || escape_pressed {
                self.pending_template = None;
            } else if insert {
                self.clipboard = nodes;
                self.paste();
                self.pending_template = None;
            }
            return;
        }
        if self.pending.is_some() {
            let mut cancel = false;
            let mut save = false;
            let mut discard = false;
            egui::Modal::new(egui::Id::new("unsaved-changes"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    cancel |= shell::dialog_header(ui, &tr("尚有未保存修改"));
                    ui.label(tr("保存整个项目后再继续，或放弃本次修改。"));
                    ui.add_space(20.0);
                    shell::dialog_buttons(ui, |ui| {
                        save = ui
                            .add_sized(
                                [108.0, 30.0],
                                egui::Button::new(
                                    RichText::new(tr("保存并继续")).color(Color32::WHITE),
                                )
                                .fill(ACCENT),
                            )
                            .clicked();
                        cancel |= ui
                            .add_sized([80.0, 30.0], egui::Button::new(tr("取消")))
                            .clicked();
                        discard = ui
                            .add_sized([96.0, 30.0], egui::Button::new(tr("放弃修改")))
                            .clicked();
                    });
                });
            if cancel || escape_pressed {
                self.pending = None;
            } else if discard || (save && self.save(false)) {
                if let Some(action) = self.pending.take() {
                    self.perform(action, ctx);
                }
            }
            return;
        }
        if !self.recovery_candidates.is_empty() && self.screenshot.is_none() {
            let mut close = false;
            egui::Modal::new(egui::Id::new("recovery-candidates"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(500.0_f32.min(ctx.content_rect().width() - 80.0));
                    close |=
                        shell::dialog_header(ui, &crate::i18n::key("editor.workspace.recover"));
                    ui.label(crate::i18n::key("workspace.recovery.hint"));
                    ui.add_space(12.0);
                    egui::ScrollArea::vertical()
                        .max_height(ctx.content_rect().height() * 0.45)
                        .show(ui, |ui| {
                            for path in self.recovery_candidates.clone() {
                                let manifest = lom_core::load_json(path.join("manifest.json"))
                                    .unwrap_or(Value::Null);
                                let title = manifest["name"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or("未命名作品");
                                egui::Frame::group(ui.style())
                                    .inner_margin(12)
                                    .show(ui, |ui| {
                                        ui.strong(title);
                                        ui.label(
                                            RichText::new(format!(
                                                "副本 {}",
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                        ui.add_space(4.0);
                                        shell::dialog_buttons(ui, |ui| {
                                            if ui.button(tr("恢复")).clicked() {
                                                self.request(Pending::Recover(path.clone()), ctx);
                                            }
                                            if ui.button(tr("移除副本")).clicked() {
                                                if recovery_owner_active(&path) {
                                                    self.error = Some(
                                                        "此副本仍有活跃的所有者，不能移除。".into(),
                                                    );
                                                    return;
                                                }
                                                match std::fs::remove_dir_all(&path) {
                                                    Ok(()) => self
                                                        .recovery_candidates
                                                        .retain(|p| p != &path),
                                                    Err(e) => {
                                                        self.error =
                                                            Some(crate::error_ui::capture(e))
                                                    }
                                                }
                                            }
                                        });
                                    });
                            }
                        });
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| {
                        close |= ui
                            .add_sized([100.0, 30.0], egui::Button::new(tr("稍后处理")))
                            .clicked();
                    });
                });
            // Dismissing hides the prompt for this session and never deletes a copy.
            if close || escape_pressed {
                self.recovery_candidates.clear();
            }
        }
    }
}
