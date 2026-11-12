//! Chapter sidebar and the step palette.
use super::*;

impl App {
    pub(super) fn step_palette(&mut self, ctx: &egui::Context) {
        let story = self.project.stories.get(&self.current);
        let imported = story.is_some_and(|story| story.get("vanilla_import").is_some());
        let structured = story.is_some_and(|story| story["vanilla_import"]["schema"] == 2);
        if story.is_some_and(|story| story.get("vanilla_patch").is_some()) {
            return;
        }
        if !self.steps_panel_open || !matches!(self.center, Center::Node | Center::Chapter) {
            return;
        }
        egui::SidePanel::left("step-palette")
            .frame(surface(true))
            .default_width(168.0)
            .min_width(140.0)
            .max_width(260.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(crate::i18n::key("editor.workspace.add_steps"));
                    if ui
                        .small_button("×")
                        .on_hover_text(crate::i18n::key("editor.workspace.collapse_steps"))
                        .clicked()
                    {
                        self.steps_panel_open = false;
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.node_search)
                        .desired_width(f32::INFINITY)
                        .hint_text(crate::i18n::key("editor.workspace.search_steps")),
                );
                ui.small(crate::i18n::key("editor.workspace.insert_hint"));
                if ui
                    .checkbox(
                        &mut self.usability.auto_hide_palette,
                        crate::i18n::key("usability.palette.auto_hide"),
                    )
                    .changed()
                {
                    self.save_usability_preferences();
                }
                if ui
                    .button(crate::i18n::key("editor.workspace.parallel"))
                    .on_hover_text(crate::i18n::key("editor.workspace.parallel_hint"))
                    .clicked()
                {
                    self.add_node("parallel");
                }
                ui.separator();
                let groups = self.catalog.schema["NODE_GROUPS_SRC"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let search = self.node_search.to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("step-palette-list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (group_i, group) in groups.iter().enumerate() {
                            let kinds: Vec<_> = group[1]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .filter(|kind| {
                                    structured
                                        || !matches!(*kind, "native_action" | "native_branch")
                                })
                                .filter(|kind| {
                                    self.project.scenario.is_none()
                                        || imported
                                        || lom_core::scenario::performance_node(kind)
                                })
                                .filter(|kind| {
                                    search.is_empty()
                                        || format!("{} {kind}", self.catalog.label(kind))
                                            .to_lowercase()
                                            .contains(&search)
                                })
                                .collect();
                            if kinds.is_empty() {
                                continue;
                            }
                            egui::CollapsingHeader::new(crate::i18n::key(
                                [
                                    "workspace.palette.stage",
                                    "workspace.palette.stats",
                                    "workspace.palette.gameplay",
                                    "workspace.palette.flow",
                                ][group_i.min(3)],
                            ))
                            .id_salt(("palette-group", group_i))
                            .default_open(true)
                            .show(ui, |ui| {
                                for kind in kinds {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 26.0],
                                            egui::Button::new(self.catalog.label(kind)),
                                        )
                                        .on_hover_text(kind)
                                        .clicked()
                                    {
                                        self.add_node(kind);
                                    }
                                }
                            });
                        }
                    });
            });
    }
    pub(super) fn sidebar(&mut self, ctx: &egui::Context) {
        let panel = egui::SidePanel::left(egui::Id::new(("chapters", self.usability.layout)))
            .frame(surface(true))
            .default_width(self.usability.widths[self.usability.layout][0])
            .min_width(180.0)
            .max_width((ctx.available_rect().width() - 620.0).max(180.0))
            .resizable(true)
            .show(ctx, |ui| {
                if self.screenshot.is_some() && self.frame == 12 {
                    println!("Layout navigation width: {}", ui.max_rect().width());
                }
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                {
                    ui.strong(
                        self.project.manifest["name"]
                            .as_str()
                            .unwrap_or("未命名作品"),
                    );
                    ui.horizontal(|ui| {
                        ui.small(crate::i18n::key(
                            if self.project.manifest["play_mode"] == "addon" {
                                "workspace.mode.addon"
                            } else {
                                "workspace.mode.free"
                            },
                        ));
                        if ui
                            .small_button(crate::i18n::key("editor.workspace.release_settings"))
                            .clicked()
                        {
                            self.open_center(Center::Manifest);
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .selectable_label(
                                matches!(self.center, Center::Node | Center::Chapter),
                                crate::i18n::key("editor.workspace.story_content"),
                            )
                            .clicked()
                        {
                            self.open_center(Center::Node);
                        }
                        if self.project.scenario.is_some()
                            && ui
                                .selectable_label(
                                    matches!(self.center, Center::Events | Center::Tasks),
                                    crate::i18n::key("editor.workspace.schedule"),
                                )
                                .clicked()
                        {
                            self.open_center(Center::Events);
                        }
                        if ui
                            .selectable_label(
                                self.center == Center::Initial,
                                crate::i18n::key("editor.workspace.initial"),
                            )
                            .clicked()
                        {
                            self.open_initial_setup();
                        }
                    });
                    ui.separator();
                    if matches!(self.center, Center::Events | Center::Tasks) {
                        let task_page = self.center == Center::Tasks;
                        egui::ScrollArea::vertical()
                            .id_salt("scenario_navigation")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if let Some(tasks) = crate::scenario_panel::navigation(
                                    ui,
                                    &mut self.project,
                                    task_page,
                                    &mut self.scenario_selection,
                                ) {
                                    self.open_center(if tasks {
                                        Center::Tasks
                                    } else {
                                        Center::Events
                                    });
                                }
                            });
                        return;
                    }
                    if self.center == Center::Initial {
                        ui.strong(crate::i18n::key("editor.workspace.opening"));
                        ui.small(crate::i18n::key("editor.workspace.opening_hint"));
                        return;
                    }
                    if self.center == Center::Manifest {
                        ui.strong(tr("作品设置"));
                        ui.small(crate::i18n::key("editor.workspace.settings_hint"));
                        return;
                    }
                    ui.small(crate::i18n::key("editor.workspace.story_hint"));
                }
                egui::TopBottomPanel::bottom("add-step-bottom")
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
                    .show_inside(ui, |ui| {
                        ui.toggle_value(&mut self.steps_panel_open, tr("＋ 添加步骤"))
                            .on_hover_text(crate::i18n::key("editor.workspace.panel_toggle"));
                        ui.small(crate::i18n::key("editor.workspace.panel_resize"));
                    });
                let old = self.current.clone();
                let mut chapter_names: Vec<_> = self
                    .project
                    .stories
                    .iter()
                    .map(|(id, story)| {
                        (id.clone(), story["title"].as_str().unwrap_or(id).to_owned())
                    })
                    .collect();
                crate::forms::number_duplicate_labels(&mut chapter_names);
                ui.label(crate::i18n::key("nav.story"));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("chapters")
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .truncate()
                        .width((ui.available_width() - 112.0).max(75.0))
                        .selected_text(
                            chapter_names
                                .iter()
                                .find(|(id, _)| id == &self.current)
                                .map(|(_, name)| name.as_str())
                                .unwrap_or(&self.current),
                        )
                        .show_ui(ui, |ui| {
                            let mut groups = BTreeMap::<&str, Vec<(&String, &String)>>::new();
                            for (id, name) in &chapter_names {
                                groups
                                    .entry(chapter_group(&self.project.stories[id]))
                                    .or_default()
                                    .push((id, name));
                            }
                            let grouped = groups.keys().any(|group| !group.is_empty());
                            for (group, chapters) in groups {
                                if grouped {
                                    egui::CollapsingHeader::new(if group.is_empty() {
                                        tr("未分组")
                                    } else {
                                        group.to_owned()
                                    })
                                    .id_salt(("chapter-group", group))
                                    .default_open(
                                        chapters.iter().any(|(id, _)| *id == &self.current),
                                    )
                                    .show(ui, |ui| {
                                        for (id, name) in chapters {
                                            if ui
                                                .selectable_value(
                                                    &mut self.current,
                                                    id.clone(),
                                                    name,
                                                )
                                                .clicked()
                                            {
                                                ui.close_kind(egui::UiKind::Menu);
                                            }
                                        }
                                    });
                                } else {
                                    for (id, name) in chapters {
                                        if ui
                                            .selectable_value(&mut self.current, id.clone(), name)
                                            .clicked()
                                        {
                                            ui.close_kind(egui::UiKind::Menu);
                                        }
                                    }
                                }
                            }
                        });
                    ui.menu_button("＋", |ui| {
                        if ui.button(tr("新建章节…")).clicked() {
                            self.begin_chapter(false);
                            ui.close();
                        }
                        if ui.button(tr("复制当前章节…")).clicked() {
                            self.begin_chapter(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(tr("新建章节分组…")).clicked() {
                            self.begin_chapter_group(false);
                            ui.close();
                        }
                        let current_group = chapter_group(&self.story()).to_owned();
                        if ui
                            .add_enabled(
                                !current_group.is_empty(),
                                egui::Button::new(tr("重命名当前分组…")),
                            )
                            .clicked()
                        {
                            self.begin_chapter_group(true);
                            ui.close();
                        }
                        ui.menu_button(tr("移入分组"), |ui| {
                            let groups: BTreeSet<_> = self
                                .project
                                .stories
                                .values()
                                .map(chapter_group)
                                .filter(|group| !group.is_empty())
                                .map(str::to_owned)
                                .collect();
                            for group in std::iter::once(String::new()).chain(groups) {
                                if ui
                                    .selectable_label(
                                        current_group == group,
                                        if group.is_empty() {
                                            tr("未分组")
                                        } else {
                                            group.clone()
                                        },
                                    )
                                    .clicked()
                                {
                                    if let Err(error) = self.move_chapter_to_group(&group) {
                                        self.error = Some(crate::error_ui::capture(error));
                                    }
                                    ui.close();
                                }
                            }
                        });
                    });

                    ui.menu_button("…", |ui| {
                        if ui.button(tr("复制")).clicked() {
                            self.copy();
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.clipboard.is_empty(), egui::Button::new(tr("粘贴")))
                            .clicked()
                        {
                            self.paste();
                            ui.close();
                        }
                        if ui.button(tr("上移")).clicked() && self.selected > 0 {
                            self.move_node(self.selected - 1);
                            ui.close();
                        }
                        if ui.button(tr("下移")).clicked() {
                            self.move_node(self.selected + 1);
                            ui.close();
                        }
                        if ui.button(tr("移到最前")).clicked() {
                            self.move_steps(&self.selected_indices(), 0);
                            ui.close();
                        }
                        if ui.button(tr("移到最后")).clicked() {
                            let len = self.story()["nodes"].as_array().map_or(0, Vec::len);
                            self.move_steps(&self.selected_indices(), len);
                            ui.close();
                        }
                        if ui.button(tr("全选步骤")).clicked() {
                            self.multiselect =
                                (0..self.story()["nodes"].as_array().map_or(0, Vec::len)).collect();
                            ui.close();
                        }
                        if ui.button(tr("删除")).clicked() {
                            self.delete();
                            ui.close();
                        }
                        if ui.button(tr("存为模板")).clicked() {
                            self.open_tool_page(ToolPage::Editing);
                            ui.close();
                        }
                        ui.separator();
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text(tr("搜索所有章节 · ID / 台词 / 角色")),
                        );
                    });
                });
                if old != self.current {
                    self.flush();
                    self.selected = 0;
                    self.multiselect.clear();
                    self.needs_compile = true;
                    self.center = Center::Node;
                }
                ui.add_space(4.0);
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new(tr("章节设置")).selected(self.center == Center::Chapter),
                    )
                    .on_hover_text(tr("修改章节名称、入口步骤，或删除当前章节"))
                    .clicked()
                {
                    self.open_center(Center::Chapter);
                }
                ui.separator();
                if !self.search.is_empty() {
                    ui.horizontal(|ui| {
                        if ui.small_button("×").clicked() {
                            self.search.clear();
                        }
                        ui.add(
                            egui::Label::new(format!("{}: {}", tr("查找"), self.search)).truncate(),
                        )
                        .on_hover_text(&self.search);
                    });
                }
                let story = self.story();
                if let Some(patch) = story.get("vanilla_patch") {
                    ui.strong(crate::i18n::key("editor.patch.title"));
                    ui.label(crate::i18n::message(
                        "editor.patch.chapter",
                        &[("target", patch["target"].as_str().unwrap_or("").into())],
                    ));
                    ui.label(crate::i18n::key("editor.patch.insert_hint"));
                    let used: BTreeSet<_> = self
                        .project
                        .stories
                        .values()
                        .filter_map(|story| {
                            lom_core::vanilla_import::target(story).map(str::to_owned)
                        })
                        .collect();
                    let next = self.vanilla.next_target(&used);
                    if ui
                        .add_enabled(
                            next.is_some(),
                            egui::Button::new(crate::i18n::key("editor.patch.add_chapter")),
                        )
                        .clicked()
                    {
                        let target = next.unwrap();
                        let id = (1..)
                            .map(|i| format!("patch{i}"))
                            .find(|id| !self.project.stories.contains_key(id))
                            .unwrap();
                        match lom_core::vanilla_patch::new_story(&id, &target) {
                            Ok(story) => {
                                self.flush();
                                self.project.stories.insert(id.clone(), story);
                                self.current = id;
                                self.selected = 0;
                                self.multiselect.clear();
                            }
                            Err(error) => self.error = Some(crate::error_ui::capture(error)),
                        }
                    }
                    return;
                }
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                if !self.search.is_empty() {
                    egui::ScrollArea::vertical()
                        .id_salt("step-search")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let query = self.search.to_lowercase();
                            for (sid, s) in self.project.stories.clone() {
                                for (index, node) in
                                    s["nodes"].as_array().into_iter().flatten().enumerate()
                                {
                                    if node.to_string().to_lowercase().contains(&query)
                                        && ui
                                            .selectable_label(
                                                self.current == sid && self.selected == index,
                                                format!(
                                                    "{} · {:02} {} · {}",
                                                    s["title"].as_str().unwrap_or(&sid),
                                                    index + 1,
                                                    self.catalog
                                                        .label(node["type"].as_str().unwrap_or("")),
                                                    short(node["text"].as_str().unwrap_or(""), 22)
                                                ),
                                            )
                                            .on_hover_text(format!(
                                                "{sid} / {}",
                                                node["id"].as_str().unwrap_or("")
                                            ))
                                            .clicked()
                                    {
                                        self.current = sid.clone();
                                        self.selected = index;
                                        self.center = Center::Node;
                                        self.needs_compile = true;
                                        if let Some(s) = self.project.stories.get_mut(&sid) {
                                            authoring::expand_for_node(s, index);
                                        }
                                    }
                                }
                            }
                        });
                    return;
                }
                if let Err(error) = authoring::validate_sections(&story) {
                    crate::error_ui::show_with_actions(ui, &crate::error_ui::capture(error));
                }
                let nodes = story["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let rows = authoring::section_rows(&story);
                ui.horizontal(|ui| {
                    ui.small(crate::i18n::message(
                        "workspace.steps.selected",
                        &[("count", self.selected_indices().len().to_string())],
                    ));
                    if self.multiselect.len() > 1 && ui.small_button(tr("取消多选")).clicked() {
                        self.multiselect.clear();
                    }
                });
                ui.small(tr("勾选可多选 · Shift 连选 · 拖动可批量移动"))
                    .on_hover_text(tr(
                        "也可用 Ctrl 点选。拖到列表上下边缘可自动滚动；… 菜单可移到最前或最后。",
                    ));
                let row_height = 44.0;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("steps")
                    .auto_shrink([false, false]);
                if !egui::DragAndDrop::has_payload_of_type::<StepDrag>(ctx)
                    && self.scroll_selection != (self.current.clone(), self.selected)
                {
                    if let Some(row) = rows.iter().position(|row| {
                        matches!(row, authoring::Row::Node { index, .. } if *index == self.selected)
                    }) {
                        let offset = row as f32 * (row_height + ui.spacing().item_spacing.y)
                            - (ui.available_height() - row_height) * 0.5;
                        scroll = scroll.vertical_scroll_offset(offset.max(0.0));
                        self.scroll_selection = (self.current.clone(), self.selected);
                    }
                }
                let mut output = scroll.show_rows(ui, row_height, rows.len(), |ui, visible| {
                    let mut drop_to = None;
                    for row in rows.into_iter().skip(visible.start).take(visible.len()) {
                        let (index, depth) = match row {
                            authoring::Row::Header {
                                section,
                                group,
                                depth,
                                title,
                                collapsed,
                            } => {
                                let response = ui
                                    .horizontal(|ui| {
                                        ui.set_min_height(row_height);
                                        ui.add_space(depth as f32 * 10.0);
                                        ui.selectable_label(
                                            false,
                                            format!(
                                                "{} {}",
                                                if collapsed { "▶" } else { "▼" },
                                                title
                                            ),
                                        )
                                    })
                                    .inner;
                                if response.clicked() {
                                    let s = self.project.stories.get_mut(&self.current).unwrap();
                                    let item = if let Some(g) = group {
                                        &mut s["_editor"]["sections"][section]["groups"][g]
                                    } else {
                                        &mut s["_editor"]["sections"][section]
                                    };
                                    item["collapsed"] = (!collapsed).into();
                                }
                                continue;
                            }
                            authoring::Row::Node { index, depth } => (index, depth),
                        };
                        let node = &nodes[index];
                        let summary = if matches!(
                            node["type"].as_str(),
                            Some("native_action" | "native_branch")
                        ) {
                            crate::native_forms::summary(node)
                        } else if node["type"] == "parallel" {
                            format!(
                                "{} {}",
                                node["actions"].as_array().map_or(0, Vec::len),
                                tr("个同时动作")
                            )
                        } else if node["type"] == "end" {
                            if let Some(next) =
                                node["next_script"].as_str().filter(|s| !s.is_empty())
                            {
                                format!(
                                    "{} {}",
                                    tr("转到章节"),
                                    self.project
                                        .stories
                                        .get(next)
                                        .and_then(|s| s["title"].as_str())
                                        .unwrap_or(next)
                                )
                            } else {
                                tr("返回自由模式")
                            }
                        } else if let Some(text) = node["text"].as_str().filter(|v| !v.is_empty()) {
                            text.to_owned()
                        } else if let Some(character) =
                            node["character"].as_str().filter(|v| !v.is_empty())
                        {
                            let name = self.catalog.data["characters"]
                                .as_array()
                                .and_then(|a| a.iter().find(|v| v["id"] == character))
                                .and_then(|v| v["name"].as_str())
                                .unwrap_or(character);
                            let name = crate::i18n::term("characters", character, name);
                            let position = node["position"]
                                .as_str()
                                .or_else(|| node["to"].as_str())
                                .unwrap_or("");
                            if position.is_empty() {
                                name
                            } else {
                                format!("{name} · {position}")
                            }
                        } else {
                            node["title"]
                                .as_str()
                                .or_else(|| node["view"].as_str())
                                .or_else(|| node["next_script"].as_str())
                                .unwrap_or("")
                                .to_owned()
                        };
                        let response = ui
                            .horizontal(|ui| {
                                ui.add_space(depth as f32 * 9.0);
                                let mut checked = self.selected_indices().contains(&index);
                                let checkbox =
                                    ui.checkbox(&mut checked, "").on_hover_text(tr("选择步骤"));
                                ui.painter().rect_stroke(
                                    egui::Rect::from_center_size(
                                        checkbox.rect.center(),
                                        Vec2::splat(ui.spacing().icon_width),
                                    ),
                                    3.0,
                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 140, 158)),
                                    egui::StrokeKind::Inside,
                                );
                                checkbox.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Checkbox,
                                        ui.is_enabled(),
                                        checked,
                                        format!("{} {}", tr("选择步骤"), index + 1),
                                    )
                                });
                                if checkbox.changed() {
                                    self.select_step(index, ui.input(|i| i.modifiers.shift), true);
                                }
                                let selected =
                                    self.selected == index || self.multiselect.contains(&index);
                                let primary = egui::TextFormat {
                                    font_id: egui::FontId::proportional(14.0),
                                    color: ui.visuals().text_color(),
                                    ..Default::default()
                                };
                                let secondary = egui::TextFormat {
                                    font_id: egui::FontId::proportional(12.0),
                                    color: if selected {
                                        Color32::from_rgb(70, 96, 120)
                                    } else {
                                        Color32::from_rgb(103, 113, 125)
                                    },
                                    ..Default::default()
                                };
                                let mut label = egui::text::LayoutJob::default();
                                label.wrap.max_width = (ui.available_width() - 24.0).max(40.0);
                                label.wrap.max_rows = 2;
                                label.wrap.break_anywhere = true;
                                label.append(
                                    &format!("{:02}  ", index + 1),
                                    0.0,
                                    secondary.clone(),
                                );
                                label.append(
                                    &self.catalog.label(node["type"].as_str().unwrap_or("")),
                                    0.0,
                                    primary,
                                );
                                let remark = node["_editor"]["note"]
                                    .as_str()
                                    .filter(|note| !note.trim().is_empty())
                                    .or_else(|| node["id"].as_str())
                                    .unwrap_or("");
                                let preview = if summary.is_empty() {
                                    remark.to_owned()
                                } else {
                                    format!("{remark} · {summary}")
                                };
                                let preview =
                                    preview.split_whitespace().collect::<Vec<_>>().join(" ");
                                if !preview.is_empty() {
                                    label.append("\n", 0.0, secondary.clone());
                                    label.append(&short(&preview, 26), 23.0, secondary);
                                }
                                let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
                                ui.add_sized(
                                    [ui.available_width(), row_height],
                                    egui::Button::new(label)
                                        .right_text("")
                                        .selected(
                                            self.selected == index
                                                || self.multiselect.contains(&index),
                                        )
                                        .sense(egui::Sense::click_and_drag()),
                                )
                            })
                            .inner
                            .on_hover_text(format!(
                                "{}\n{}",
                                node["id"].as_str().unwrap_or(""),
                                summary
                            ));
                        if response.clicked() {
                            let modifiers = ui.input(|i| i.modifiers);
                            self.select_step(
                                index,
                                modifiers.shift,
                                modifiers.command || modifiers.ctrl,
                            );
                        }
                        response.context_menu(|ui| {
                            if !self.selected_indices().contains(&index) {
                                self.select_step(index, false, false);
                            }
                            if ui.button(tr("复制")).clicked() {
                                self.copy();
                                ui.close();
                            }
                            if ui.button(tr("粘贴")).clicked() {
                                self.paste();
                                ui.close();
                            }
                            if ui.button(tr("删除")).clicked() {
                                self.delete();
                                ui.close();
                            }
                        });
                        if response.drag_started() {
                            if !self.selected_indices().contains(&index) {
                                self.select_step(index, false, false);
                            }
                            response.dnd_set_drag_payload(StepDrag {
                                chapter: self.current.clone(),
                                indices: self.selected_indices(),
                            });
                        }
                        if let Some(drag) = response.dnd_hover_payload::<StepDrag>() {
                            if drag.chapter == self.current {
                                let after = ui
                                    .input(|i| i.pointer.hover_pos())
                                    .is_some_and(|p| p.y >= response.rect.center().y);
                                let y = if after {
                                    response.rect.bottom()
                                } else {
                                    response.rect.top()
                                };
                                ui.painter().hline(
                                    response.rect.x_range(),
                                    y,
                                    egui::Stroke::new(2.0_f32, crate::shell::ACCENT),
                                );
                            }
                        }
                        if let Some(drag) = response.dnd_release_payload::<StepDrag>() {
                            if drag.chapter == self.current {
                                let after = ui
                                    .input(|i| i.pointer.hover_pos())
                                    .is_some_and(|p| p.y >= response.rect.center().y);
                                drop_to = Some((drag.indices.clone(), index + usize::from(after)));
                            }
                        }
                    }
                    if let Some((indices, boundary)) = drop_to {
                        self.move_steps(&indices, boundary);
                    }
                });
                if egui::DragAndDrop::payload::<StepDrag>(ctx)
                    .is_some_and(|drag| drag.chapter == self.current)
                    && ui.input(|i| i.pointer.primary_down())
                {
                    let speed = ui
                        .input(|i| i.pointer.hover_pos())
                        .map_or(0.0, |pointer| step_edge_scroll(output.inner_rect, pointer));
                    if speed != 0.0 {
                        let dt = ui.input(|i| i.stable_dt).min(0.05);
                        let max_offset =
                            (output.content_size.y - output.inner_rect.height()).max(0.0);
                        output.state.offset.y =
                            (output.state.offset.y + speed * dt).clamp(0.0, max_offset);
                        output.state.store(ctx, output.id);
                        ctx.request_repaint();
                    }
                }
            });
        self.remember_panel_width(ctx, 0, panel.response.rect.width());
    }
}
