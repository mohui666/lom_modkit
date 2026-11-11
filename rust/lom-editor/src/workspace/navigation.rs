//! Commands, shortcuts and navigation between editor pages.
use super::*;

impl App {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let windows_settings_release = cfg!(windows)
            && ctx.input(|input| {
                if !input.focused {
                    self.settings_comma_down = false;
                    return false;
                }
                let mut requested = false;
                for event in &input.events {
                    if let egui::Event::Key {
                        key: egui::Key::Comma,
                        pressed,
                        modifiers,
                        ..
                    } = event
                    {
                        if *pressed {
                            self.settings_comma_down = true;
                        } else {
                            // Windows can deliver Ctrl+, only on release. Track
                            // every comma press, even in a modal, so a normal
                            // down/up pair never triggers the shortcut twice.
                            let had_press = std::mem::take(&mut self.settings_comma_down);
                            requested |= !had_press
                                && modifiers.ctrl
                                && modifiers.command
                                && !modifiers.alt
                                && !modifiers.shift
                                && !modifiers.mac_cmd;
                        }
                    }
                }
                requested
            });
        if self.chapter_dialog.is_some()
            || self.delete_chapter_dialog.is_some()
            || self.rename_dialog.is_some()
            || self.asset_import_dialog.is_some()
            || self.settings_open
            || self.updater.open
            || self.tools_panel.dialog_open()
            || self.error.is_some()
            || self.pending.is_some()
            || self.pending_template.is_some()
            || (!self.recovery_candidates.is_empty() && self.screenshot.is_none())
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma))
            || windows_settings_release
        {
            self.settings_open = true;
            self.settings_error = None;
            return;
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            )
        }) {
            self.save(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(false);
        }
        let editing_text = ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if !editing_text
            && ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                )
            })
        {
            self.redo();
        } else if !editing_text
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
        {
            self.undo();
        }
        if !editing_text && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y))
        {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.request(Pending::New, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F5)) {
            self.play_chapter_start();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.play_current();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F6)) {
            if self.open_center(Center::Node) {
                self.compile();
                self.view = View::Checks;
            }
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::F,
            )
        }) {
            if self.open_center(Center::Advanced) {
                self.advanced.page = 0;
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.tools_panel.open_help();
        }
        if !ctx.wants_keyboard_input()
            && matches!(self.center, Center::Node | Center::Chapter)
            && !self.is_vanilla_patch()
        {
            let (copy, paste) = ctx.input_mut(|input| {
                let mut copy = input.consume_key(egui::Modifiers::COMMAND, egui::Key::C);
                let mut paste = input.consume_key(egui::Modifiers::COMMAND, egui::Key::V);
                input.events.retain(|event| match event {
                    egui::Event::Copy => {
                        copy = true;
                        false
                    }
                    egui::Event::Paste(_) => {
                        paste = true;
                        false
                    }
                    _ => true,
                });
                (copy, paste)
            });
            if copy {
                self.copy();
            }
            if paste {
                self.paste();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                self.delete();
            }
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::O,
            )
        }) {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.request(Pending::Open(path), ctx);
            }
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("剧情、Lua 或 Mod 包", &["json", "lua", "lommod"])
                .pick_file()
            {
                self.request(Pending::Open(path), ctx);
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) {
            self.navigate(Section::Materials);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7)) {
            if self.open_center(Center::Node) {
                self.view = View::Graph;
            }
        }
    }
    pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
        let shortcut =
            |modifiers, key| ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, key));
        egui::TopBottomPanel::top("commands")
            .frame(surface(false))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    #[cfg(not(target_os = "macos"))]
                    if ui
                        .add(egui::Button::new(tr("设置")).shortcut_text("Ctrl+,"))
                        .clicked()
                    {
                        self.settings_open = true;
                        self.settings_error = None;
                    }
                    ui.menu_button(tr("文件"), |ui| {
                        ui.menu_button(crate::i18n::key("editor.workspace.new_project"), |ui| {
                            if ui.add(egui::Button::new(crate::i18n::key("editor.workspace.new_free"))
                                .shortcut_text(shortcut(egui::Modifiers::COMMAND, egui::Key::N))).clicked() {
                                ui.close();
                                self.request(Pending::New, ctx);
                            }
                            if ui.button(crate::i18n::key("editor.workspace.new_extra")).clicked() {
                                ui.close();
                                self.request(Pending::NewAddon,ctx);
                            }
                            ui.menu_button(tr("从模板新建"), |ui| {
                                let templates: Value = serde_json::from_str(include_str!(
                                    "../../data/project-templates.json"
                                ))
                                .expect("built-in templates");
                                for (key, item) in templates.as_object().unwrap() {
                                    if ui
                                        .button(item["name"].as_str().unwrap_or(key))
                                        .on_hover_text(item["description"].as_str().unwrap_or(""))
                                        .clicked()
                                    {
                                        ui.close();
                                        self.request(Pending::Template(key.clone()), ctx);
                                    }
                                }
                            });
                        });
                        ui.separator();
                        if ui.add(egui::Button::new(tr("打开目录…"))
                            .shortcut_text(shortcut(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::O))).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.request(Pending::Open(path), ctx);
                            }
                        }
                        if ui.add(egui::Button::new(tr("打开 JSON / Mod 包…"))
                            .shortcut_text(shortcut(egui::Modifiers::COMMAND, egui::Key::O))).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("剧情、Lua 或 Mod 包", &["json", "lua", "lommod"])
                                .pick_file()
                            {
                                self.request(Pending::Open(path), ctx);
                            }
                        }
                        ui.menu_button(tr("最近打开"), |ui| {
                            if self.recent.is_empty() {
                                ui.label(tr("还没有最近项目"));
                            }
                            for path in self.recent.clone() {
                                if ui.button(path.display().to_string()).clicked() {
                                    ui.close();
                                    self.request(Pending::Open(path), ctx);
                                }
                            }
                        });
                        ui.separator();
                        let save_label = tr("保存全部章节  ⌘S");
                        let save_label = if cfg!(target_os = "macos") {
                            save_label
                        } else {
                            save_label.replace("⌘S", "Ctrl+S")
                        };
                        if ui.button(save_label).clicked() {
                            ui.close();
                            self.save(false);
                        }
                        if ui.add(egui::Button::new(tr("另存项目目录…"))
                            .shortcut_text(shortcut(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S))).clicked() {
                            ui.close();
                            self.save(true);
                        }
                        ui.separator();
                        ui.menu_button(crate::i18n::key("editor.workspace.original_tools"), |ui| {
                            if ui.button(crate::i18n::key("editor.workspace.import_original")).clicked() {
                                ui.close();
                                self.request(Pending::NewVanillaPatch, ctx);
                            }
                            if ui.button(crate::i18n::key("editor.workspace.routes")).clicked() {
                                ui.close();self.open_vanilla_routes(ctx);
                            }
                            if ui.button(crate::i18n::key("editor.workspace.bookmark")).clicked() {
                                ui.close();
                                match lom_core::story_bookmarks::read(&lom_core::story_bookmarks::default_root()) {
                                    Ok(marks) => if let Some(mark)=marks.into_iter().next() {
                                        self.open_story_bookmark(mark,ctx);
                                    } else {self.error=Some("还没有游戏标记。请在原版剧情中按 Ctrl+F6 后再打开。".into());},
                                    Err(error)=>self.error=Some(crate::error_ui::capture(error)),
                                }
                            }
                        });
                        ui.menu_button(crate::i18n::key("editor.workspace.export_menu"), |ui| {
                            if ui.button(crate::i18n::key("editor.workspace.release_settings")).clicked() {
                                self.open_center(Center::Manifest);
                                ui.close();
                            }
                            if ui.button(tr("导出 .lommod…")).clicked() {
                                ui.close();
                                self.export();
                            }
                            if ui.button(crate::i18n::key("editor.workspace.export_player")).clicked() {
                                ui.close();
                                self.export_play_bundle();
                            }
                        });
                        ui.menu_button(crate::i18n::key("editor.workspace.preview_assets"), |ui| {
                            if ui.button(tr("从游戏提取预览素材…")).clicked() {
                                ui.close();
                                self.tools_panel.start_extraction();
                            }
                            if ui.button(tr("选择预览素材库…")).on_hover_text(tr("选择包含 preview_map.json 的已提取素材库；选择游戏目录则自动开始提取。")).clicked() {
                                ui.close();
                                self.choose_preview_library();
                            }
                        });
                    });

                    ui.menu_button(crate::i18n::key("editor.workspace.edit"), |ui| {
                        if ui
                            .add_enabled(
                                !self.undo.is_empty() || self.pending_history.is_some(),
                                egui::Button::new(tr("撤销"))
                                    .shortcut_text(shortcut(egui::Modifiers::COMMAND, egui::Key::Z)),
                            )
                            .clicked()
                        {
                            self.undo();
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.redo.is_empty(), egui::Button::new(tr("重做"))
                                .shortcut_text(shortcut(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z)))
                            .clicked()
                        {
                            self.redo();
                            ui.close();
                        }
                        if ui.add_enabled(matches!(self.center, Center::Node | Center::Chapter) && !self.is_vanilla_patch(), egui::Button::new(tr("复制"))).clicked() {
                            self.copy();
                            ui.close();
                        }
                        if ui.add_enabled(matches!(self.center, Center::Node | Center::Chapter) && !self.is_vanilla_patch(), egui::Button::new(tr("粘贴"))).clicked() {
                            self.paste();
                            ui.close();
                        }
                        if ui.button(tr("多语言")).clicked() {
                            self.open_center(Center::Localization);
                            ui.close();
                        }
                        ui.separator();
                        ui.menu_button(tr("高级"), |ui| {
                            if ui.button(tr("修改步骤标识…")).clicked()
                                && self.apply_pending_drafts()
                            {
                                if let Some(id) =
                                    self.story()["nodes"][self.selected]["id"].as_str()
                                {
                                    self.rename_dialog = Some(RenameDialog {
                                        target: RenameTarget::Node {
                                            story: self.current.clone(),
                                            node: id.to_owned(),
                                        },
                                        value: id.to_owned(),
                                        error: None,
                                        focus_name: true,
                                    });
                                }
                                ui.close();
                            }
                            if ui.button(tr("修改章节标识…")).clicked()
                                && self.apply_pending_drafts()
                            {
                                self.rename_dialog = Some(RenameDialog {
                                    target: RenameTarget::Chapter(self.current.clone()),
                                    value: self.current.clone(),
                                    error: None,
                                    focus_name: true,
                                });
                                ui.close();
                            }
                        });
                    });
                    ui.menu_button(tr("剧情工具"), |ui| {
                        if ui.add_enabled(cfg!(windows), egui::Button::new(crate::i18n::key("editor.play.current"))).clicked() {
                            self.play_current();
                            ui.close();
                        }
                        if ui.add_enabled(cfg!(windows), egui::Button::new(crate::i18n::key("editor.play.chapter_start"))).clicked() {
                            self.play_chapter_start();
                            ui.close();
                        }
                        ui.separator();
                        for (page, label) in [
                            "全局查找",
                            "变量管理",
                            "条件检查",
                            "路径模拟",
                            "跨章节复制",
                            "离线测试",
                        ]
                        .iter()
                        .enumerate()
                        {
                            if ui.button(tr(label)).clicked() {
                                if self.open_center(Center::Advanced) { self.advanced.page = page; }
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(tr("批量修改与模板")).clicked() {
                            self.open_tool_page(ToolPage::Editing);
                            ui.close();
                        }
                        if ui.button(tr("迁移与诊断")).clicked() {
                            self.open_tool_page(ToolPage::Maintenance);
                            ui.close();
                        }
                        if ui.button(tr("导出诊断报告…")).clicked() {
                            ui.close();
                            self.diagnostic();
                        }
                        if ui.button(tr("检查项目")).clicked() {
                            self.navigate(Section::Story);
                            self.compile();
                            self.view = View::Checks;
                            ui.close();
                        }
                        if ui.button(tr("统计")).clicked() {
                            self.navigate(Section::Story);
                            self.view = View::Statistics;
                            ui.close();
                        }
                    });
                    if ui
                        .button(crate::i18n::key("menu.help_item"))
                        .on_hover_text("F1")
                        .clicked()
                    {
                        self.tools_panel.open_help();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                cfg!(windows),
                                egui::Button::new(crate::i18n::key("toolbar.play")),
                            )
                            .on_hover_text(crate::i18n::key("editor.play.hint"))
                            .clicked()
                        {
                            self.play_current();
                        }
                        if ui
                            .add(
                                egui::Button::new(crate::i18n::key("toolbar.export"))
                                    .fill(Color32::from_rgb(213, 231, 247)),
                            )
                            .clicked()
                        {
                            self.export();
                        }
                        if ui.button(crate::i18n::key("editor.workspace.export_short")).clicked() {
                            self.export_play_bundle();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for (section, label) in [
                        (Section::Story, "剧情编辑"),
                        (Section::Materials, "素材管理"),
                        (Section::Game, "游戏与 MOD"),
                        (Section::Release, "发布"),
                    ] {
                        if ui.add_sized([112.0, 30.0], egui::Button::selectable(self.section() == section, tr(label))).clicked() {
                            self.navigate(section);
                        }
                    }
                });
            });
    }
    pub(super) fn section(&self) -> Section {
        match self.center {
            Center::Assets => Section::Materials,
            Center::Tools => match self.tools_panel.page {
                ToolPage::Previews | ToolPage::Library => Section::Materials,
                ToolPage::Game => Section::Game,
                ToolPage::Release => Section::Release,
                _ => Section::Story,
            },
            _ => Section::Story,
        }
    }
    pub(super) fn go_to_problem(
        &mut self,
        ctx: &egui::Context,
        destination: crate::error_location::Destination,
    ) {
        use crate::error_location::Destination;
        if destination == Destination::Current {
            ctx.data_mut(|data| data.remove::<String>(egui::Id::new("problem-field")));
            return;
        }
        if matches!(&destination, Destination::Asset(path) if self.center == Center::Assets && self.asset_selection == *path)
        {
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        self.auto = false;
        self.audio.stop();
        match destination {
            Destination::Story(id, node) => {
                if let Some(story) = self.project.stories.get_mut(&id) {
                    if let Some(index) = node.and_then(|node| {
                        story["nodes"]
                            .as_array()?
                            .iter()
                            .position(|n| n["id"] == node)
                    }) {
                        self.selected = index;
                        self.center = Center::Node;
                        self.view = View::Stage;
                        authoring::expand_for_node(story, index);
                    } else {
                        self.center = Center::Chapter;
                    }
                    self.current = id;
                    self.multiselect.clear();
                    self.search.clear();
                    self.needs_compile = true;
                }
            }
            Destination::Manifest => {
                self.center = Center::Manifest;
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::new("reveal-manifest-fields"), true)
                });
            }
            Destination::Asset(path) => {
                self.center = Center::Assets;
                self.asset_selection = path;
            }
            Destination::Assets => self.center = Center::Assets,
            Destination::Events => self.center = Center::Events,
            Destination::Game => {
                self.center = Center::Tools;
                self.tools_panel.page = ToolPage::Game;
            }
            Destination::PreviewLibrary => {
                self.center = Center::Tools;
                self.tools_panel.page = ToolPage::Previews;
            }
            Destination::Settings => self.settings_open = true,
            Destination::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("作品、剧情或脚本", &["json", "lua", "lommod"])
                    .pick_file()
                {
                    self.request(Pending::Open(path), ctx);
                }
            }
            Destination::Current => {}
        }
    }
    pub(super) fn open_tool_page(&mut self, page: ToolPage) {
        if self.open_center(Center::Tools) {
            self.tools_panel.page = page;
        }
    }
    pub(super) fn open_center(&mut self, center: Center) -> bool {
        if !self.apply_pending_drafts() {
            return false;
        }
        self.flush();
        self.center = center;
        if matches!(
            center,
            Center::Events | Center::Tasks | Center::Manifest | Center::Initial
        ) {
            self.auto = false;
            self.audio.stop();
        }
        true
    }
    pub(super) fn navigate(&mut self, section: Section) {
        if self.section() == section && section != Section::Story {
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        self.preview.cancel_layout_drag();
        match section {
            Section::Story => self.center = Center::Node,
            Section::Materials => self.center = Center::Assets,
            Section::Game => self.open_tool_page(ToolPage::Game),
            Section::Release => self.open_tool_page(ToolPage::Release),
        }
    }
    pub(super) fn story_panels_visible(&self) -> bool {
        self.section() == Section::Story
            && !(self.center == Center::Tools && self.tools_panel.page == ToolPage::Maintenance)
    }
    pub(super) fn material_navigation(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("素材管理"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.center == Center::Assets, tr("作品素材"))
                .clicked()
                && self.apply_pending_drafts()
            {
                self.center = Center::Assets;
            }
            for (page, label) in [
                (ToolPage::Previews, "游戏预览素材"),
                (ToolPage::Library, "共享内容库"),
            ] {
                if ui
                    .selectable_label(
                        self.center == Center::Tools && self.tools_panel.page == page,
                        tr(label),
                    )
                    .clicked()
                {
                    self.open_tool_page(page);
                }
            }
        });
        ui.separator();
    }
}
