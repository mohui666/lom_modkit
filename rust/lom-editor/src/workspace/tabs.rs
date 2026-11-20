//! Each open project owns its editor history, drafts and widget memory.
use super::*;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Location {
    source: Option<PathBuf>,
    chapter: String,
    node: String,
    selected: usize,
    center: Center,
    view: View,
    tool_page: ToolPage,
}

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Session {
    tabs: Vec<Location>,
    active: usize,
}

struct Tab {
    id: u64,
    app: App,
    memory: Option<egui::Memory>,
    // Keep unavailable paths in the session so a missing drive is actionable.
    reopen: Option<PathBuf>,
}

struct Closing {
    approved: BTreeSet<usize>,
    waiting: Option<usize>,
    action: Pending,
    origin: usize,
}

pub(super) struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    visible_tab: Option<u64>,
    session_path: Option<PathBuf>,
    observed: Session,
    session_changed: Option<Instant>,
    closing: Option<Closing>,
    allow_close: bool,
}

const TAB_HEIGHT: f32 = 38.0;

fn tab_bar_color(ctx: &egui::Context) -> Color32 {
    let dark = ctx.style().visuals.dark_mode;
    ctx.style().visuals.window_fill.lerp_to_gamma(
        if dark { Color32::WHITE } else { Color32::BLACK },
        if dark { 0.07 } else { 0.08 },
    )
}

fn project_tab(ui: &mut egui::Ui, tab: &Tab, selected: bool, reveal: bool) -> (bool, bool) {
    let name = tab.app.project.manifest["name"]
        .as_str()
        .unwrap_or("新作品");
    let dirty = tab.app.dirty();
    let color = if selected {
        ui.visuals().text_color()
    } else {
        ui.visuals()
            .text_color()
            .lerp_to_gamma(tab_bar_color(ui.ctx()), 0.2)
    };
    let font = egui::FontId::proportional(13.0);
    let full_label = ui
        .painter()
        .layout_no_wrap(name.to_owned(), font.clone(), color);
    let width = (full_label.size().x + 70.0).clamp(144.0, 240.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, TAB_HEIGHT), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            name,
        )
    });
    if reveal {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    let close_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 18.0, rect.center().y),
        egui::vec2(24.0, 24.0),
    );
    let close = ui.interact(close_rect, response.id.with("close"), egui::Sense::click());
    close.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("{} · {name}", crate::i18n::key("workspace.tabs.close")),
        )
    });
    let hovered = response.hovered() || close.hovered();
    let foreground = if ui.is_enabled() {
        color
    } else {
        ui.visuals().weak_text_color()
    };
    let active_fill = ui.visuals().window_fill;
    let fill = if selected {
        active_fill
    } else if hovered {
        tab_bar_color(ui.ctx()).lerp_to_gamma(active_fill, 0.65)
    } else {
        tab_bar_color(ui.ctx())
    };
    let border = ui.visuals().widgets.inactive.bg_stroke;
    let accent = shell::accent_for(ui.ctx());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, fill);
        painter.vline(rect.right() - 0.5, rect.y_range(), border);
        if selected {
            painter.rect_filled(
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 2.5)),
                0.0,
                accent,
            );
        }
        // A small project folder, aligned with the label instead of a separate button.
        let icon = egui::pos2(rect.left() + 19.0, rect.center().y);
        painter.add(egui::Shape::closed_line(
            vec![
                icon + egui::vec2(-7.0, -5.0),
                icon + egui::vec2(-2.0, -5.0),
                icon + egui::vec2(0.0, -2.0),
                icon + egui::vec2(7.0, -2.0),
                icon + egui::vec2(7.0, 6.0),
                icon + egui::vec2(-7.0, 6.0),
            ],
            egui::Stroke::new(1.2_f32, if selected { accent } else { foreground }),
        ));
        let mut label = egui::text::LayoutJob::simple_singleline(name.to_owned(), font, foreground);
        label.wrap = egui::text::TextWrapping::truncate_at_width(width - 66.0);
        let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
        painter.galley(
            egui::pos2(rect.left() + 34.0, rect.center().y - label.size().y / 2.0),
            label,
            foreground,
        );
        if dirty && !hovered && !close.has_focus() {
            painter.circle_filled(close_rect.center(), 3.0, foreground);
        } else if selected || hovered || close.has_focus() {
            if close.hovered() || close.has_focus() {
                painter.rect_filled(
                    close_rect.shrink(2.0),
                    3.0,
                    fill.lerp_to_gamma(foreground, 0.12),
                );
            }
            let center = close_rect.center();
            for sign in [-1.0, 1.0] {
                painter.line_segment(
                    [
                        center + egui::vec2(-3.5, -3.5 * sign),
                        center + egui::vec2(3.5, 3.5 * sign),
                    ],
                    egui::Stroke::new(1.2_f32, foreground),
                );
            }
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect.shrink(3.0),
                0.0,
                egui::Stroke::new(1.0_f32, accent),
                egui::StrokeKind::Inside,
            );
        }
    }
    let closed = close.clicked() || response.clicked_by(egui::PointerButton::Middle);
    let clicked = response.clicked() && !closed;
    let path = tab.app.project.source.as_ref().or(tab.reopen.as_ref());
    let description = path.map_or_else(
        || crate::i18n::key("workspace.tabs.unsaved"),
        |path| path.display().to_string(),
    );
    let description = if dirty {
        format!("{name} · {}\n{description}", tr("未保存"))
    } else {
        format!("{name}\n{description}")
    };
    response.on_hover_text(description);
    close.on_hover_text(crate::i18n::key("workspace.tabs.close"));
    (clicked, closed)
}

impl Tab {
    fn location(&self) -> Location {
        Location {
            source: self
                .app
                .project
                .source
                .clone()
                .or_else(|| self.reopen.clone()),
            chapter: self.app.current.clone(),
            node: self.app.story()["nodes"][self.app.selected]["id"]
                .as_str()
                .unwrap_or("")
                .into(),
            selected: self.app.selected,
            center: self.app.center,
            view: self.app.view,
            tool_page: self.app.tools_panel.page,
        }
    }

    fn restore(&mut self, location: &Location) {
        self.reopen = location.source.clone();
        if let Some(path) = &location.source {
            match open_project(path) {
                Ok(project) => self.app.install(project),
                Err(error) => {
                    self.app.error =
                        Some(format!("无法恢复工程标签：{}\n{error:#}", path.display()))
                }
            }
        }
        if self.app.project.stories.contains_key(&location.chapter) {
            self.app.current = location.chapter.clone();
        }
        let nodes = self.app.story()["nodes"].as_array();
        self.app.selected = nodes
            .and_then(|nodes| nodes.iter().position(|node| node["id"] == location.node))
            .unwrap_or(location.selected)
            .min(nodes.map_or(0, |nodes| nodes.len().saturating_sub(1)));
        self.app.center = location.center;
        self.app.view = location.view;
        self.app.tools_panel.page = location.tool_page;
    }
}

impl Workspace {
    pub(super) fn new(ctx: &egui::Context, mut app: App, capture_tabs: bool) -> Self {
        let screenshot = app.screenshot.clone();
        let session_path =
            (screenshot.is_none() || capture_tabs).then(|| user_root().join("rust/workspace.json"));
        app.managed_tab = true;
        let mut workspace = Self::with_session(ctx, app, session_path);
        if let Some(path) = screenshot {
            for tab in &mut workspace.tabs {
                tab.app.screenshot = None;
            }
            workspace.tabs[workspace.active].app.screenshot = Some(path);
            workspace.session_path = None;
        }
        workspace
    }

    fn with_session(ctx: &egui::Context, mut app: App, session_path: Option<PathBuf>) -> Self {
        let explicit_source = app.project.source.clone();
        let explicit_open = explicit_source.is_some() || app.project_choice.is_some();
        app.managed_tab = true;
        let mut workspace = Self {
            tabs: vec![Tab {
                id: 0,
                app,
                memory: None,
                reopen: None,
            }],
            active: 0,
            next_id: 1,
            visible_tab: None,
            session_path,
            observed: Session::default(),
            session_changed: None,
            closing: None,
            allow_close: false,
        };
        if let Some(path) = workspace.session_path.clone().filter(|path| path.exists()) {
            match lom_core::load_json(&path)
                .and_then(|value| Ok(serde_json::from_value::<Session>(value)?))
            {
                Ok(session) => {
                    for (index, location) in session.tabs.iter().enumerate() {
                        if explicit_source.is_some() && location.source == explicit_source {
                            continue;
                        }
                        if index == 0 && !explicit_open {
                            workspace.tabs[0].restore(location);
                        } else {
                            let mut tab = workspace.new_tab(ctx);
                            tab.restore(location);
                            workspace.tabs.push(tab);
                        }
                    }
                    if !explicit_open {
                        workspace.switch(ctx, session.active.min(workspace.tabs.len() - 1));
                    }
                }
                Err(error) => {
                    workspace.tabs[0].app.error =
                        Some(format!("工程标签恢复失败：{}\n{error:#}", path.display()))
                }
            }
        }
        workspace.observed = workspace.session();
        workspace
    }

    fn new_tab(&mut self, ctx: &egui::Context) -> Tab {
        // TextEdit undo and unfinished form input must never leak to another project.
        let mut fresh = egui::Memory::default();
        fresh.options = ctx.memory(|memory| memory.options.clone());
        let previous = ctx.memory_mut(|memory| std::mem::replace(memory, fresh));
        let preview = self.tabs[self.active]
            .app
            .preview
            .for_another_project(&repository());
        let mut app = App::from_context_with_preview(ctx, None, None, Some(preview));
        app.managed_tab = true;
        app.recovery_candidates.clear();
        let memory = ctx.memory_mut(|memory| std::mem::replace(memory, previous));
        let id = self.next_id;
        self.next_id += 1;
        Tab {
            id,
            app,
            memory: Some(memory),
            reopen: None,
        }
    }

    fn session(&self) -> Session {
        Session {
            tabs: self.tabs.iter().map(Tab::location).collect(),
            active: self.active,
        }
    }

    fn persist(&mut self) -> bool {
        self.observed = self.session();
        self.session_changed = None;
        let Some(path) = &self.session_path else {
            return true;
        };
        match serde_json::to_value(&self.observed)
            .map_err(anyhow::Error::from)
            .and_then(|value| lom_core::stable_json(&value))
            .and_then(|bytes| lom_core::project::atomic_write(path, &bytes))
        {
            Ok(()) => true,
            Err(error) => {
                self.tabs[self.active].app.error = Some(format!(
                    "无法保存工程标签和编辑位置：{}\n{error:#}",
                    path.display()
                ));
                false
            }
        }
    }

    fn switch(&mut self, ctx: &egui::Context, index: usize) {
        if index == self.active || index >= self.tabs.len() {
            return;
        }
        let old = &mut self.tabs[self.active].app;
        old.track();
        old.flush();
        old.stop_preview();
        old.audio.stop();
        old.node_paste_requested = false;
        old.preview.cancel_layout_drag();
        let theme = old.theme;
        let shell = old.shell.clone();
        let usability = old.usability.clone();
        let auto_save = (
            old.auto_save_project,
            old.auto_save_seconds,
            old.auto_save_checkpoints,
        );
        let legacy_rotation = old.preview.legacy_character_rotation;
        let preview_resolution = old.preview.resolution;
        let startup = old.check_updates_on_startup;
        let library = old.preview.library.clone();
        let recent = old.recent.clone();
        let templates = old.templates.clone();
        let updater = std::mem::take(&mut old.updater);
        let next_memory = self.tabs[index].memory.take().unwrap_or_default();
        let memory = ctx.memory_mut(|memory| {
            if let Some(focus) = memory.focused() {
                memory.surrender_focus(focus);
            }
            std::mem::replace(memory, next_memory)
        });
        self.tabs[self.active].memory = Some(memory);
        self.active = index;
        let next = &mut self.tabs[index].app;
        next.theme = theme;
        next.shell = shell;
        next.usability = usability;
        (
            next.auto_save_project,
            next.auto_save_seconds,
            next.auto_save_checkpoints,
        ) = auto_save;
        next.preview.legacy_character_rotation = legacy_rotation;
        next.preview.resolution = preview_resolution;
        next.check_updates_on_startup = startup;
        next.recent = recent;
        next.templates = templates;
        next.updater = updater;
        if next.preview.library != library {
            if let Err(error) = next.preview.set_library(&library) {
                next.error = Some(crate::error_ui::capture(error));
            }
        }
        if let Err(error) = next
            .catalog
            .configure_parameter_defaults(user_root().join("rust/node-defaults.json"))
        {
            next.error = Some(crate::error_ui::capture(error));
        }
        shell::apply_theme(ctx, theme);
        ctx.request_repaint();
    }

    fn blocked(&self) -> bool {
        let app = &self.tabs[self.active].app;
        self.closing.is_some()
            || app.pending.is_some()
            || app.error.is_some()
            || app.settings_open
            || app.updater.open
            || app.tools_panel.dialog_open()
            || app.chapter_dialog.is_some()
            || app.project_choice.is_some()
            || app.delete_chapter_dialog.is_some()
            || app.rename_dialog.is_some()
            || app.asset_import_dialog.is_some()
            || app.video_dialog.is_some()
            || app.font_conversion_dialog.is_some()
            || app.feedback_dialog.is_some()
            || app.pending_template.is_some()
            || !app.recovery_candidates.is_empty()
    }

    fn strip(&mut self, ctx: &egui::Context) {
        let mut select = None;
        let mut close = None;
        let mut create = false;
        let blocked = self.blocked();
        let active_id = self.tabs[self.active].id;
        let reveal_active = self.visible_tab != Some(active_id);
        let border = ctx.style().visuals.widgets.inactive.bg_stroke;
        let panel = egui::TopBottomPanel::top("project-tabs")
            .frame(egui::Frame::NONE.fill(tab_bar_color(ctx)))
            .exact_height(TAB_HEIGHT)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_enabled_ui(!blocked, |ui| {
                    ui.horizontal(|ui| {
                        egui::ScrollArea::horizontal()
                            .id_salt("open-project-tabs")
                            .max_width((ui.available_width() - 72.0).max(80.0))
                            .max_height(TAB_HEIGHT)
                            .min_scrolled_height(TAB_HEIGHT)
                            .auto_shrink([false, false])
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            )
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for (index, tab) in self.tabs.iter().enumerate() {
                                        ui.push_id(tab.id, |ui| {
                                            let (clicked, closed) = project_tab(
                                                ui,
                                                tab,
                                                index == self.active,
                                                index == self.active && reveal_active,
                                            );
                                            if clicked {
                                                select = Some(index);
                                            }
                                            if closed {
                                                close = Some(index);
                                            }
                                        });
                                    }
                                });
                            });
                        ui.scope(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
                            ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                            let response =
                                ui.add_sized([36.0, 30.0], egui::Button::new("").frame(false));
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    crate::i18n::key("workspace.tabs.new"),
                                )
                            });
                            let center = response.rect.center();
                            let stroke = egui::Stroke::new(1.3_f32, ui.visuals().text_color());
                            ui.painter().line_segment(
                                [center - egui::vec2(4.5, 0.0), center + egui::vec2(4.5, 0.0)],
                                stroke,
                            );
                            ui.painter().line_segment(
                                [center - egui::vec2(0.0, 4.5), center + egui::vec2(0.0, 4.5)],
                                stroke,
                            );
                            create = response
                                .on_hover_text(crate::i18n::key("workspace.tabs.new"))
                                .clicked();
                            let menu = ui.menu_button("", |ui| {
                                ui.set_min_width(240.0);
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                                for (index, tab) in self.tabs.iter().enumerate() {
                                    let name = tab.app.project.manifest["name"]
                                        .as_str()
                                        .unwrap_or("新作品");
                                    let label = format!(
                                        "{}{name}",
                                        if tab.app.dirty() { "● " } else { "" }
                                    );
                                    if ui
                                        .selectable_label(index == self.active, label)
                                        .on_hover_text(tab.location().source.map_or_else(
                                            || crate::i18n::key("workspace.tabs.unsaved"),
                                            |path| path.display().to_string(),
                                        ))
                                        .clicked()
                                    {
                                        select = Some(index);
                                        ui.close();
                                    }
                                }
                            });
                            let center = menu.response.rect.center();
                            ui.painter().add(egui::Shape::line(
                                vec![
                                    center + egui::vec2(-4.0, -2.0),
                                    center + egui::vec2(0.0, 2.0),
                                    center + egui::vec2(4.0, -2.0),
                                ],
                                stroke,
                            ));
                            menu.response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    crate::i18n::key("workspace.tabs.select"),
                                )
                            });
                            menu.response
                                .on_hover_text(crate::i18n::key("workspace.tabs.select"));
                        });
                    });
                });
            });
        ctx.layer_painter(panel.response.layer_id).hline(
            panel.response.rect.x_range(),
            panel.response.rect.bottom() - 0.5,
            border,
        );
        self.visible_tab = Some(active_id);
        if !blocked && !egui::Popup::is_any_open(ctx) {
            if ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                    egui::Key::Tab,
                )
            }) {
                select = Some((self.active + self.tabs.len() - 1) % self.tabs.len());
            } else if ctx
                .input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Tab))
            {
                select = Some((self.active + 1) % self.tabs.len());
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)) {
                close = Some(self.active);
            }
        }
        if let Some(index) = select {
            self.switch(ctx, index);
        }
        if create {
            self.open(Pending::New, ctx);
        }
        if let Some(index) = close {
            self.switch(ctx, index);
            self.tabs[index].app.request(Pending::CloseTab, ctx);
        }
    }

    fn open(&mut self, action: Pending, ctx: &egui::Context) {
        let action = if let Pending::Open(path) = action {
            let Some(source) = self.tabs[self.active].app.resolve_project_source(&path) else {
                return;
            };
            if let Some(index) = self
                .tabs
                .iter()
                .position(|tab| tab.app.project.source.as_ref() == Some(&source))
            {
                self.switch(ctx, index);
                return;
            }
            Pending::Open(source)
        } else {
            action
        };
        let mut tab = self.new_tab(ctx);
        tab.app.perform(action, ctx);
        self.tabs.push(tab);
        self.switch(ctx, self.tabs.len() - 1);
    }

    fn begin_close(&mut self, action: Pending) {
        if self.closing.is_none() {
            self.closing = Some(Closing {
                approved: BTreeSet::new(),
                waiting: None,
                action,
                origin: self.active,
            });
        }
    }

    fn continue_close(&mut self, ctx: &egui::Context) {
        let Some(closing) = &self.closing else { return };
        if let Some(index) = closing.waiting {
            if self.tabs[index].app.pending.is_none() {
                self.closing = None; // The user cancelled, or a save still needs attention.
            }
            return;
        }
        if let Some(index) = (0..self.tabs.len()).find(|index| !closing.approved.contains(index)) {
            self.switch(ctx, index);
            self.closing.as_mut().unwrap().waiting = Some(index);
            self.tabs[index].app.request(Pending::CloseTab, ctx);
            ctx.request_repaint();
            return;
        }
        let closing = self.closing.take().unwrap();
        self.switch(ctx, closing.origin);
        if !self.persist() {
            return;
        }
        self.tabs[self.active].app.perform(closing.action, ctx);
        self.allow_close = self.tabs[self.active].app.allow_close;
        if self.allow_close {
            for tab in &mut self.tabs {
                tab.app.clear_recovery();
            }
        }
    }

    fn actions(&mut self, ctx: &egui::Context) {
        for index in 0..self.tabs.len() {
            let Some(action) = self.tabs[index].app.tab_action.take() else {
                continue;
            };
            match action {
                Pending::CloseTab => {
                    if let Some(closing) = &mut self.closing {
                        closing.approved.insert(index);
                        closing.waiting = None;
                    } else {
                        self.remove(index, ctx);
                        return;
                    }
                }
                Pending::Quit | Pending::InstallEditorUpdate => {
                    self.switch(ctx, index);
                    self.begin_close(action);
                }
                action => {
                    self.open(action, ctx);
                    return;
                }
            }
        }
        self.continue_close(ctx);
    }

    fn remove(&mut self, index: usize, ctx: &egui::Context) {
        self.tabs[index].app.clear_recovery();
        if self.tabs.len() == 1 {
            self.open(Pending::New, ctx);
        } else if self.active == index {
            self.switch(ctx, if index == 0 { 1 } else { index - 1 });
        }
        self.tabs[index].app.updater.shutdown();
        self.tabs.remove(index);
        if self.active > index {
            self.active -= 1;
        }
        self.persist();
    }
}

impl eframe::App for Workspace {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.tabs[self.active].app.clear_color(visuals)
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        for tab in &mut self.tabs {
            tab.app.on_exit(gl);
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested())
            && !self.allow_close
            && !self.tabs[self.active].app.allow_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.begin_close(Pending::Quit);
        }
        self.tabs[self.active].app.shell.apply_layout(ctx);
        self.tabs[self.active].app.menu_toolbar(ctx);
        self.strip(ctx);
        self.tabs[self.active].app.section_toolbar(ctx);
        let checkpoint_limit = self.tabs[self.active].app.auto_save_checkpoints;
        for index in 0..self.tabs.len() {
            if index != self.active {
                let app = &mut self.tabs[index].app;
                app.auto_save_checkpoints = checkpoint_limit;
                app.tools_panel.poll(ctx);
                app.poll_background_save(ctx);
                app.autosave();
                let delay = app.idle_save_delay();
                if delay != Duration::MAX {
                    ctx.request_repaint_after(delay);
                }
            }
        }
        self.tabs[self.active].app.update(ctx, frame);
        self.actions(ctx);
        let session = self.session();
        if session != self.observed {
            self.observed = session;
            self.session_changed = Some(Instant::now());
        }
        if let Some(changed) = self.session_changed {
            let delay = Duration::from_millis(500).saturating_sub(changed.elapsed());
            if delay.is_zero() {
                self.persist();
            } else {
                ctx.request_repaint_after(delay);
            }
        }
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
