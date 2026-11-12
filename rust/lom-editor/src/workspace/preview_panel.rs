//! Preview transport, game playtest and the preview panel.
use super::*;

impl App {
    pub(super) fn play_current(&mut self) {
        self.play(false);
    }
    pub(super) fn play_chapter_start(&mut self) {
        self.play(true);
    }
    pub(super) fn play(&mut self, from_start: bool) {
        if !self.apply_pending_drafts() {
            return;
        }
        if !self.check_before_run_or_export() {
            return;
        }
        if cfg!(windows) {
            self.tools_panel.refresh_game();
            if !self.tools_panel.ready_to_play() {
                self.open_tool_page(ToolPage::Game);
                self.status = tr("请先完成游戏配置，再返回剧情试玩。");
                return;
            }
        }
        self.flush();
        let node = self.playtest_node(from_start);
        match self
            .tools_panel
            .play_from_current(&self.project, &self.current, &node)
        {
            Ok(status) => self.status = status,
            Err(e) => self.error = Some(crate::error_ui::capture(e)),
        }
    }
    pub(super) fn playtest_node(&self, from_start: bool) -> String {
        let story = self.story();
        (if from_start {
            &story["start"]
        } else {
            &story["nodes"][self.selected]["id"]
        })
        .as_str()
        .unwrap_or("")
        .to_owned()
    }
    pub(super) fn check_before_run_or_export(&mut self) -> bool {
        self.error_issues = lom_core::release::run_preflight(
            &self.project,
            lom_core::release::Profile::Editing,
            env!("CARGO_PKG_VERSION"),
        )
        .into_iter()
        .filter(|issue| issue.severity == "error")
        .collect();
        if self.error_issues.is_empty() {
            let game = self.tools_panel.game_directory();
            if game.join("Mortal.exe").is_file() {
                if let Err(error) =
                    lom_core::game_tools::matching_installed_mod(&game, &self.project.manifest)
                {
                    self.error = Some(crate::error_ui::capture(error));
                    self.open_center(Center::Manifest);
                    return false;
                }
            }
            return true;
        }
        self.error = Some(crate::i18n::message(
            "editor.error.issue_count",
            &[("count", self.error_issues.len().to_string())],
        ));
        false
    }
    pub(super) fn open_initial_setup(&mut self) {
        if self.is_vanilla_patch() {
            return;
        }
        if !self.open_center(Center::Initial) {
            return;
        }
        if let Some(config) = &self.project.scenario {
            if let Some(script) = config["events"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|event| event["id"] == config["entry_event"])
                .and_then(|event| event["script"].as_str())
                .filter(|id| self.project.stories.contains_key(*id))
            {
                self.current = script.to_owned();
            }
        }
        self.selected = 0;
        self.multiselect.clear();
    }
    pub(super) fn sync_preview_selection(&mut self) {
        let location = (self.current.clone(), self.selected);
        if self.preview_location != location {
            self.preview_location = location;
            self.auto = false;
            self.preview_waiting_branch = false;
            self.preview_elapsed = None;
            self.last_step = Instant::now();
        }
    }
    pub(super) fn advance_preview(&mut self) {
        if self.auto
            && !self.preview_waiting_branch
            && self.last_step.elapsed().as_secs_f64()
                >= preview::playback_duration(&self.story()["nodes"][self.selected])
        {
            let story = self.story();
            let node = &story["nodes"][self.selected];
            let branching = [
                "choice",
                "branch",
                "dice",
                "combat",
                "battle",
                "battle_result",
                "stat_check",
                "affinity_check",
                "item_check",
                "talent_check",
                "flag_check",
                "activity",
                "quest_check",
                "persistent_check",
            ];
            if branching.contains(&node["type"].as_str().unwrap_or("")) {
                self.preview_waiting_branch = true;
                self.preview_elapsed = Some(preview::playback_duration(node));
                self.status = "流程在条件或选项处分叉，等待选择后继续播放".into();
            } else {
                let targets = lom_core::analysis::successors(&story, self.selected);
                if targets.len() == 1 {
                    if let Some(index) = story["nodes"]
                        .as_array()
                        .and_then(|nodes| nodes.iter().position(|n| n["id"] == targets[0]))
                    {
                        self.selected = index;
                        self.preview_location = (self.current.clone(), index);
                        self.preview_elapsed = Some(0.0);
                    } else {
                        self.auto = false;
                        self.preview_elapsed = Some(preview::playback_duration(node));
                    }
                } else {
                    self.auto = false;
                    self.preview_elapsed = Some(preview::playback_duration(node));
                }
            }
            self.last_step = Instant::now();
        }
    }
    pub(super) fn follow_preview_target(&mut self, target: &str) {
        let story = self.story();
        if let Some(index) = story["nodes"].as_array().and_then(|nodes| {
            nodes
                .iter()
                .position(|node| node["id"].as_str() == Some(target))
        }) {
            // A choice is a playback transition, not manual editor navigation.
            // Preserve the user's play/pause state and start the target's clock.
            self.selected = index;
            self.preview_location = (self.current.clone(), index);
            self.preview_waiting_branch = false;
            self.preview_elapsed = self.auto.then_some(0.0);
            self.last_step = Instant::now();
            self.status.clear();
        }
    }
    pub(super) fn follow_scenario_link(&mut self, link: crate::scenario_panel::Navigate) {
        use crate::scenario_panel::Navigate;
        match link {
            Navigate::Story(id) => {
                if self.open_center(Center::Node) && self.project.stories.contains_key(&id) {
                    self.current = id;
                    self.selected = 0;
                    self.multiselect.clear();
                    self.needs_compile = true;
                }
            }
            Navigate::Event(id) => {
                if self.open_center(Center::Events) {
                    self.scenario_selection.event = Some(id);
                }
            }
            Navigate::Task(id) => {
                if self.open_center(Center::Tasks) {
                    self.scenario_selection.task = Some(id);
                }
            }
        }
    }
    pub(super) fn right(&mut self, ctx: &egui::Context) {
        if self.center == Center::Node && self.is_vanilla_patch() {
            if self.vanilla.route_open() {
                return;
            }
            let story = self.story();
            egui::SidePanel::right("vanilla-preview")
                .frame(surface(true))
                .default_width(340.0)
                .resizable(true)
                .show(ctx, |ui| {
                    self.vanilla.preview(ui, &story);
                });
            return;
        }
        if self.view != View::Stage {
            self.preview.cancel_layout_drag();
        }
        if matches!(
            self.center,
            Center::Events | Center::Tasks | Center::Manifest
        ) {
            self.preview.cancel_layout_drag();
            egui::SidePanel::right("scenario_context")
                .frame(surface(true))
                .default_width(300.0)
                .min_width(280.0)
                .max_width((ctx.available_rect().width() - 350.0).max(280.0))
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if self.center == Center::Manifest {
                            ui.heading(crate::i18n::key("editor.workspace.overview"));
                            ui.label(crate::i18n::key(
                                if self.project.manifest["play_mode"] == "addon" {
                                    "workspace.mode.addon_hint"
                                } else {
                                    "workspace.mode.free_hint"
                                },
                            ));
                            ui.add_space(12.0);
                            ui.label(crate::i18n::message(
                                "editor.workspace.story_count",
                                &[("count", self.project.stories.len().to_string())],
                            ));
                            if let Some(c) = &self.project.scenario {
                                ui.label(crate::i18n::message(
                                    "workspace.overview.counts",
                                    &[
                                        (
                                            "events",
                                            c["events"].as_array().map_or(0, Vec::len).to_string(),
                                        ),
                                        (
                                            "tasks",
                                            c["tasks"].as_array().map_or(0, Vec::len).to_string(),
                                        ),
                                    ],
                                ));
                            }
                        } else {
                            if let Some(link) = crate::scenario_panel::context(
                                ui,
                                &self.project,
                                self.center == Center::Tasks,
                                &self.scenario_selection,
                            ) {
                                self.follow_scenario_link(link);
                            }
                            if self.center == Center::Events {
                                if let Some(spec) = crate::scenario_test::show(
                                    ui,
                                    &self.project,
                                    self.scenario_selection.event.as_deref(),
                                    &self.catalog,
                                ) {
                                    if self.apply_pending_drafts()
                                        && self.check_before_run_or_export()
                                    {
                                        self.tools_panel.refresh_game();
                                        if self.tools_panel.ready_to_play() {
                                            match self.tools_panel.play_event(&self.project, &spec)
                                            {
                                                Ok(status) => self.status = status,
                                                Err(error) => {
                                                    self.error =
                                                        Some(crate::error_ui::capture(error))
                                                }
                                            }
                                        } else {
                                            self.open_tool_page(ToolPage::Game);
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
            return;
        }
        self.sync_preview_selection();
        egui::SidePanel::right("preview")
            .frame(surface(true))
            .default_width(shell::PREVIEW_WIDTH)
            .min_width(320.0)
            .max_width((ctx.available_rect().width() - 300.0).max(320.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (view, key) in [
                        (View::Stage, "tab.preview"),
                        (View::Portraits, "portrait.preview"),
                        (View::Graph, "tab.flow"),
                        (View::Lua, "tab.compile"),
                    ] {
                        let selected =
                            self.view == view || (view == View::Lua && self.view == View::Checks);
                        if ui
                            .add(
                                egui::Button::selectable(selected, crate::i18n::key(key))
                                    .frame(selected),
                            )
                            .clicked()
                        {
                            self.preview.cancel_layout_drag();
                            self.view = view;
                        }
                    }
                });
                ui.separator();
                let mut story = self.story();
                match self.view {
                    View::Stage => {
                        egui::TopBottomPanel::bottom("stage-transport")
                            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
                            .show_inside(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add_space(((ui.available_width() - 350.0) / 2.0).max(0.0));
                                    if ui.button(crate::i18n::key("stage.home")).clicked() {
                                        self.selected = story["nodes"]
                                            .as_array()
                                            .and_then(|a| {
                                                a.iter().position(|n| n["id"] == story["start"])
                                            })
                                            .unwrap_or(0);
                                    }
                                    if ui.button(crate::i18n::key("stage.prev")).clicked() {
                                        self.selected = self.selected.saturating_sub(1);
                                    }
                                    ui.label(format!(
                                        "{} / {}",
                                        self.selected + 1,
                                        story["nodes"].as_array().map(Vec::len).unwrap_or(0)
                                    ));
                                    if ui.button(crate::i18n::key("stage.next")).clicked() {
                                        self.selected = (self.selected + 1).min(
                                            story["nodes"]
                                                .as_array()
                                                .map(Vec::len)
                                                .unwrap_or(1)
                                                .saturating_sub(1),
                                        );
                                    }
                                    if ui
                                        .button(crate::i18n::key(if self.auto {
                                            "stage.pause"
                                        } else {
                                            "stage.play"
                                        }))
                                        .clicked()
                                    {
                                        if self.auto {
                                            self.preview_elapsed =
                                                Some(self.last_step.elapsed().as_secs_f64());
                                            self.auto = false;
                                            self.preview_waiting_branch = false;
                                        } else {
                                            self.last_step = Instant::now()
                                                - Duration::from_secs_f64(
                                                    self.preview_elapsed.unwrap_or(0.0),
                                                );
                                            self.preview_elapsed = Some(0.0);
                                            self.auto = true;
                                        }
                                    }
                                });
                                let node = &story["nodes"][self.selected];
                                if node["type"] == "transition" {
                                    ui.label(RichText::new(tr("黑幕转场预览（节奏示意）")).small());
                                } else if self
                                    .preview
                                    .state(&story, node["id"].as_str().unwrap_or(""))["blackout"]
                                    ["amount"]
                                    == 1.0
                                {
                                    ui.label(
                                        RichText::new(tr("黑幕遮挡中；播放到揭幕步骤后显示画面"))
                                            .small(),
                                    );
                                } else if node["type"] == "parallel" {
                                    ui.label(RichText::new(tr("预览人物移动与淡入淡出")).small());
                                }
                            });
                        self.sync_preview_selection();
                        if self.preview.layout_mode {
                            self.auto = false;
                            self.preview_waiting_branch = false;
                            self.preview_elapsed = None;
                        }
                        let elapsed = if self.auto && !self.preview_waiting_branch {
                            Some(self.last_step.elapsed().as_secs_f64())
                        } else {
                            self.preview_elapsed
                        };
                        if let Some(target) =
                            self.preview
                                .show(ui, &self.project, &story, self.selected, elapsed)
                        {
                            self.follow_preview_target(&target);
                        }
                        if let Some(edit) = self.preview.mood_edit.take() {
                            self.auto = false;
                            self.preview_waiting_branch = false;
                            self.preview_elapsed = None;
                            let node = &mut story["nodes"][self.selected];
                            let mood = if let Some(action) = edit.action {
                                &mut node["actions"][action]
                            } else {
                                node
                            };
                            mood["offset_x"] = json!(edit.x);
                            mood["offset_y"] = json!(edit.y);
                            self.project
                                .stories
                                .insert(self.current.clone(), story.clone());
                        }
                        if let Some(change) = self.preview.layout_edit.take() {
                            let mut next = story.clone();
                            match crate::preview_layout::apply(&mut next, self.selected, &change) {
                                Ok(selected) => {
                                    self.track();
                                    self.flush();
                                    story = next;
                                    self.project
                                        .stories
                                        .insert(self.current.clone(), story.clone());
                                    self.selected = selected;
                                    self.multiselect.clear();
                                    self.auto = false;
                                    self.preview_elapsed = None;
                                    self.track();
                                    self.flush();
                                    self.status = "已调整画面，可撤销".into();
                                }
                                Err(error) => self.error = Some(crate::error_ui::capture(error)),
                            }
                        }
                    }
                    View::Portraits => {
                        self.preview
                            .show_portrait(ui, &self.project, &story, self.selected);
                    }
                    View::Graph => {
                        if let Some(i) = preview::graph(ui, &self.project, &story, self.selected) {
                            self.selected = i;
                            self.center = Center::Node;
                        }
                    }
                    View::Lua | View::Checks => {
                        ui.horizontal(|ui| {
                            if ui.button(tr("检查项目")).clicked() {
                                self.compile();
                            }
                            if ui.button(tr("复制 Lua")).clicked() {
                                match compile_with_assets(&self.project, &story) {
                                    Ok(lua) => ctx.copy_text(lua),
                                    Err(error) => {
                                        self.error = Some(crate::error_ui::capture(error))
                                    }
                                }
                            }
                            if ui.button(tr("导出 Lua…")).clicked() {
                                match compile_with_assets(&self.project, &story) {
                                    Ok(lua) => {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .set_file_name(format!("{}.lua", self.current))
                                            .save_file()
                                        {
                                            if let Err(error) = lom_core::project::atomic_write(
                                                &path,
                                                lua.as_bytes(),
                                            ) {
                                                self.error = Some(crate::error_ui::capture(error));
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        self.error = Some(crate::error_ui::capture(error))
                                    }
                                }
                            }
                        });
                        if !self.diagnostics.is_empty() {
                            egui::ScrollArea::vertical()
                                .id_salt("compile-errors")
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    for issue in self.diagnostics.clone() {
                                        let location = if issue.node_id.is_empty() {
                                            issue.story_id.clone()
                                        } else {
                                            format!("{}/{}", issue.story_id, issue.node_id)
                                        };
                                        if ui.link(location).clicked() {
                                            if let Some(story) =
                                                self.project.stories.get_mut(&issue.story_id)
                                            {
                                                if let Some(index) =
                                                    story["nodes"].as_array().and_then(|nodes| {
                                                        nodes.iter().position(|node| {
                                                            !issue.node_id.is_empty()
                                                                && node["id"] == issue.node_id
                                                        })
                                                    })
                                                {
                                                    self.selected = index;
                                                    self.center = Center::Node;
                                                    authoring::expand_for_node(story, index);
                                                } else {
                                                    self.center = Center::Chapter;
                                                }
                                                self.current = issue.story_id;
                                                self.multiselect.clear();
                                                self.search.clear();
                                                self.needs_compile = true;
                                            }
                                        }
                                        let message = if issue.severity == "error" {
                                            format!("错误：{}", issue.message)
                                        } else {
                                            issue.message
                                        };
                                        ui.colored_label(
                                            Color32::from_rgb(179, 55, 49),
                                            crate::error_ui::text(&message),
                                        );
                                    }
                                });
                        }
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.lua)
                                    .code_editor()
                                    .interactive(false)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    }
                    View::Statistics => {
                        let analysis = lom_core::analysis::analyze_project(
                            &self.project.stories,
                            &self.project.manifest,
                        );
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            report(ui, &analysis["stats"], 0);
                        });
                    }
                }
            });
    }
}
