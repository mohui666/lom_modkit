//! Chapter, node and localization property panels.
use super::*;

impl App {
    pub(super) fn center(&mut self, ctx: &egui::Context) {
        if self.center == Center::Node && self.is_vanilla_patch() {
            let mut story = self.story();
            self.vanilla.set_segment_chapters(
                self.project
                    .stories
                    .iter()
                    .filter(|(_, story)| {
                        story.get("vanilla_patch").is_none()
                            && story.get("vanilla_import").is_none()
                    })
                    .map(|(id, story)| {
                        (id.clone(), story["title"].as_str().unwrap_or(id).to_owned())
                    })
                    .collect(),
            );
            let used = self
                .project
                .stories
                .iter()
                .filter(|(id, _)| *id != &self.current)
                .filter_map(|(_, story)| lom_core::vanilla_import::target(story).map(str::to_owned))
                .collect();
            egui::CentralPanel::default()
                .frame(surface(true))
                .show(ctx, |ui| {
                    if self.vanilla.route_open() {
                        self.vanilla.show(
                            ui,
                            &mut story,
                            &self.catalog,
                            self.tools_panel.game_directory(),
                            &used,
                        );
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.vanilla.show(
                                ui,
                                &mut story,
                                &self.catalog,
                                self.tools_panel.game_directory(),
                                &used,
                            );
                        });
                    }
                });
            let mut inserted = None;
            if let Some(request) = self.vanilla.take_segment_insertion() {
                let result = self
                    .project
                    .stories
                    .get(&request.chapter)
                    .context("选择的剧情章节已不存在")
                    .and_then(|source| {
                        super::segment_insertion::insert_segment(
                            request.story,
                            source,
                            &request.anchor,
                            request.before,
                        )
                    });
                match result {
                    Ok((next, start, count)) => {
                        story = next;
                        inserted = Some((start, count));
                    }
                    Err(error) => self.vanilla.segment_error(error),
                }
            }
            let imported = self.vanilla.take_imported() || inserted.is_some();
            if imported {
                self.flush();
            }
            self.project.stories.insert(self.current.clone(), story);
            if let Some((target, chapter)) = self.vanilla.take_before_chapter() {
                if let Err(error) = self.add_before_original_event(&target, &chapter) {
                    self.vanilla.segment_error(error);
                }
            }
            if self.vanilla.take_new_segment() {
                self.begin_chapter(false);
                if let Some(draft) = &mut self.chapter_dialog {
                    draft.name = crate::i18n::key("vanilla.content_segment_name");
                }
            }
            if imported {
                self.selected = inserted.map_or(0, |(start, _)| start);
                self.multiselect.clear();
                if let Some((start, count)) = inserted {
                    self.multiselect = (start..start + count).collect();
                }
                self.view = View::Stage;
                self.auto = false;
                self.audio.stop();
                self.needs_compile = true;
                self.track();
                self.flush();
                self.status = if let Some((_, count)) = inserted {
                    crate::i18n::message("vanilla.segment_done", &[("count", count.to_string())])
                } else {
                    "已导入原版整章，可在步骤间插入内容。F5 从原版章节入口试玩。".into()
                };
            }
            if let Some(mark) = self.vanilla.take_bookmark() {
                self.open_story_bookmark(mark, ctx);
            }
            return;
        }
        egui::CentralPanel::default()
            .frame(surface(true))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(format!(
                        "properties-{:?}-{:?}",
                        self.center, self.tools_panel.page
                    ))
                    .show(ui, |ui| match self.center {
                        Center::Node => {
                            let mut story = self.story();
                            if let Some(notice) = story["lua_import"]["notice"].as_str() {
                                ui.small(notice);
                                ui.separator();
                            }
                            if let Some(target) = story["vanilla_import"]["target"].as_str() {
                                ui.small(crate::i18n::message(
                                    "editor.original.preview",
                                    &[("target", target.into())],
                                ));
                            }
                            let ids: Vec<_> = story["nodes"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|n| n["id"].as_str().map(str::to_owned))
                                .collect();
                            let stories = self.project.stories.keys().cloned().collect::<Vec<_>>();
                            if let Some(mut node) = story["nodes"].get(self.selected).cloned() {
                                crate::forms::set_preview_scope(
                                    ctx,
                                    &self.current,
                                    node["id"].as_str().unwrap_or(""),
                                );
                                let event_picture = node["type"] == "cg" && node["kind"] == "event";
                                let story_picture = node["type"] == "cg"
                                    && matches!(
                                        node["kind"].as_str(),
                                        Some("event" | "picture" | "item" | "big")
                                    );
                                if node["type"] == "cg" && node["kind"] == "big" {
                                    ui.small(crate::i18n::key("editor.form.big_picture_hint"));
                                }
                                if event_picture {
                                    ui.small(crate::i18n::key("editor.workspace.event_hint"));
                                }
                                if matches!(
                                    node["type"].as_str(),
                                    Some("background" | "custom_cg" | "overlay")
                                ) || story_picture
                                    && node["action"] == "show"
                                    && node["background_only"] != true
                                {
                                    ui.small(crate::i18n::key("editor.workspace.image_hint"));
                                    if ui
                                        .button(crate::i18n::key("editor.workspace.import_image"))
                                        .clicked()
                                    {
                                        self.image_import_target = Some((
                                            self.current.clone(),
                                            node["id"].as_str().unwrap_or("").to_owned(),
                                        ));
                                        self.asset_import_dialog = Some(AssetImportDialog {
                                            extends: String::new(),
                                            kind: 0,
                                            name: String::new(),
                                            paths: vec![],
                                            error: None,
                                        });
                                    }
                                }
                                let official = story_picture;
                                let custom = matches!(
                                    node["type"].as_str(),
                                    Some("background" | "custom_cg" | "overlay")
                                );
                                let scene =
                                    matches!(node["type"].as_str(), Some("scene" | "combat"));
                                let mut image_changed = false;
                                if (official || custom || scene)
                                    && node["background_only"] != true
                                    && !matches!(
                                        node["action"].as_str(),
                                        Some("hide" | "clear" | "fadeout")
                                    )
                                {
                                    let key = if official {
                                        "key"
                                    } else if node["type"] == "scene" {
                                        "view"
                                    } else if scene {
                                        "background"
                                    } else {
                                        "image"
                                    };
                                    let options = self.catalog.options(
                                        if official {
                                            "story_picture"
                                        } else if scene {
                                            "view"
                                        } else {
                                            "user_image"
                                        },
                                        &node,
                                        &[],
                                        &[],
                                    );
                                    ui.push_id(
                                        (
                                            &self.current,
                                            node["id"].as_str().unwrap_or("").to_owned(),
                                        ),
                                        |ui| {
                                            let browsing = crate::forms::preview_story(ctx, &story);
                                            let chosen = self.preview.image_browser(
                                                ui,
                                                &self.project,
                                                &options,
                                                browsing["nodes"][self.selected][key]
                                                    .as_str()
                                                    .unwrap_or(""),
                                                official,
                                            );
                                            if let Some(choice) = crate::forms::resource_choice(
                                                ui,
                                                &node,
                                                key,
                                                if official {
                                                    "story_picture"
                                                } else if scene {
                                                    "view"
                                                } else {
                                                    "user_image"
                                                },
                                                chosen,
                                            ) {
                                                if let Some(image) = choice {
                                                    node[key] = json!(image);
                                                } else {
                                                    node.as_object_mut().unwrap().remove(key);
                                                }
                                                image_changed = true;
                                            }
                                        },
                                    );
                                }
                                if node["type"] == "rotate" {
                                    let mut legacy = self.preview.legacy_character_rotation;
                                    if ui
                                        .checkbox(
                                            &mut legacy,
                                            crate::i18n::key("editor.workspace.legacy_rotation"),
                                        )
                                        .on_hover_text(crate::i18n::key("editor.rotation.hint"))
                                        .changed()
                                    {
                                        if let Err(error) = self.set_legacy_character_rotation(
                                            legacy,
                                            &user_root().join("rust/preferences.json"),
                                        ) {
                                            self.error = Some(crate::error_ui::capture(error));
                                        }
                                    }
                                }
                                self.catalog.stage_characters = Some(
                                    self.preview
                                        .state_before(&story, node["id"].as_str().unwrap_or(""))
                                        ["actors"]
                                        .as_object()
                                        .into_iter()
                                        .flatten()
                                        .map(|(id, _)| id.clone())
                                        .collect(),
                                );
                                let response =
                                    self.catalog.node_form(ui, &mut node, &ids, &stories);
                                self.catalog.stage_characters = None;
                                if let Some(kind) = response.help {
                                    self.tools_panel.open_node_help(&kind);
                                }
                                if matches!(node["type"].as_str(), Some("music" | "sound")) {
                                    let browsing = crate::forms::preview_story(ctx, &story);
                                    let audition = &browsing["nodes"][self.selected];
                                    ui.horizontal(|ui| {
                                        let name = audition["name"].as_str().unwrap_or("");
                                        let playable = !name.is_empty()
                                            && node["op"].as_str().unwrap_or("play") == "play";
                                        if ui
                                            .add_enabled(
                                                playable,
                                                egui::Button::new(tr("试听音频")),
                                            )
                                            .clicked()
                                        {
                                            self.stop_preview();
                                            let result = if name.starts_with("user:") {
                                                self.audio.play_content(&self.project, name)
                                            } else {
                                                let kind = if node["type"] == "music" {
                                                    "music"
                                                } else if node["kind"] == "env" {
                                                    "env"
                                                } else {
                                                    "sound"
                                                };
                                                (|| -> anyhow::Result<()> {
                                                    let helper = crate::components::extractor()?;
                                                    let decoder = crate::components::decoder()?;
                                                    self.audio.play_official(
                                                        &self.tools_panel.game_directory(),
                                                        kind,
                                                        name,
                                                        &helper,
                                                        &decoder,
                                                    )
                                                })()
                                            };
                                            if let Err(error) = result
                                                .with_context(|| format!("无法试听音频“{name}”"))
                                            {
                                                self.error = Some(crate::error_ui::capture(error));
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                self.audio.playing(),
                                                egui::Button::new(tr("停止试听")),
                                            )
                                            .clicked()
                                        {
                                            self.audio.stop();
                                        }
                                        if self.audio.preparing() {
                                            ui.spinner();
                                            ui.label(crate::i18n::key("editor.audio.preparing"));
                                        }
                                    });
                                    ui.small(crate::i18n::key("editor.audio.preview_hint"));
                                }
                                if response.changed || image_changed {
                                    story["nodes"][self.selected] = node.clone();
                                    self.project
                                        .stories
                                        .insert(self.current.clone(), story.clone());
                                }
                            } else {
                                ui.heading(tr("还没有步骤"));
                                ui.label(crate::i18n::key("editor.workspace.empty"));
                            }
                        }
                        Center::Chapter => self.chapter(ui),
                        Center::Initial => {
                            let old = self.current.clone();
                            crate::initial_setup::show(
                                ui,
                                &mut self.project,
                                &self.catalog,
                                &mut self.current,
                            );
                            if old != self.current {
                                self.selected = 0;
                                self.multiselect.clear();
                                self.needs_compile = true;
                            }
                        }
                        Center::Manifest => {
                            crate::manifest_panel::show_project(
                                ui,
                                &mut self.project,
                                &self.catalog,
                            );
                        }
                        Center::Events | Center::Tasks => {
                            crate::scenario_quick::set_game_directory(
                                ui.ctx(),
                                &self.tools_panel.game_directory(),
                            );
                            crate::scenario_panel::show(
                                ui,
                                &mut self.project,
                                &self.catalog,
                                self.center == Center::Tasks,
                                &self.scenario_selection,
                            );
                        }
                        Center::Localization => self.localization(ui),
                        Center::Assets => {
                            self.material_navigation(ui);
                            self.assets(ui);
                        }
                        Center::Tools => self.tools(ui),
                        Center::Advanced => {
                            if let Some((sid, nid)) =
                                self.advanced.show(ui, &mut self.project, &self.current)
                            {
                                self.flush();
                                if let Some(nid) = nid {
                                    if let Some(s) = self.project.stories.get(&sid) {
                                        if let Some(index) = s["nodes"]
                                            .as_array()
                                            .and_then(|ns| ns.iter().position(|n| n["id"] == nid))
                                        {
                                            self.current = sid;
                                            self.selected = index;
                                            self.multiselect.clear();
                                            self.search.clear();
                                            self.center = Center::Node;
                                            self.needs_compile = true;
                                        }
                                    }
                                } else if self.project.stories.contains_key(&sid) {
                                    self.current = sid;
                                    self.center = Center::Chapter;
                                } else {
                                    self.center = Center::Manifest;
                                }
                            }
                        }
                    });
            });
    }
    pub(super) fn chapter(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(tr("章节设置"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.project.stories.len() > 1,
                        egui::Button::new(
                            RichText::new(tr("删除章节…")).color(ui.visuals().error_fg_color),
                        ),
                    )
                    .on_hover_text(tr("删除当前章节及其步骤，可撤销；作品至少保留一个章节"))
                    .clicked()
                {
                    self.begin_delete_chapter();
                }
            });
        });
        let mut story = self.story();
        let mut title = story["title"].as_str().unwrap_or_default().to_owned();
        ui.label(tr("章节名称"));
        if ui
            .add(egui::TextEdit::singleline(&mut title).desired_width(f32::INFINITY))
            .changed()
        {
            story["title"] = json!(title);
        }
        if story.get("vanilla_patch").is_some() {
            ui.label(crate::i18n::key("editor.patch.chapter_hint"));
            self.project.stories.insert(self.current.clone(), story);
            return;
        }
        if story["vanilla_import"]["schema"] == 1 {
            ui.small(crate::i18n::key("editor.upgrade.hint"));
            if ui
                .button(crate::i18n::key("editor.upgrade.button"))
                .clicked()
            {
                let selected = story["nodes"][self.selected]["id"].clone();
                match lom_core::native_upgrade::upgrade(&story) {
                    Ok(upgraded) => {
                        self.flush();
                        story = upgraded;
                        self.selected = story["nodes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .position(|node| node["id"] == selected)
                            .unwrap_or(0);
                        self.multiselect.clear();
                        self.needs_compile = true;
                        self.status = crate::i18n::key("editor.upgrade.done");
                    }
                    Err(error) => {
                        self.error = Some(crate::i18n::message(
                            "editor.upgrade.error",
                            &[("error", crate::error_ui::capture(error))],
                        ))
                    }
                }
            }
        }
        let ids = story["nodes"].as_array().cloned().unwrap_or_default();
        let mut start = story["start"].as_str().unwrap_or("").to_owned();
        ui.label(format!("{} *", tr("入口步骤")));
        if let Some(target) = story["vanilla_import"]["target"].as_str() {
            ui.label(crate::i18n::message(
                "editor.original.target",
                &[("target", target.into())],
            ));
            ui.small(if story["vanilla_import"]["schema"] == 2 {
                crate::i18n::key("editor.original.structured_hint")
            } else {
                crate::i18n::key("editor.original.legacy_hint")
            });
        }
        ui.add_enabled_ui(story.get("vanilla_import").is_none(), |ui| {
            crate::forms::SearchCombo::from_id_salt("start")
                .selected_text(
                    ids.iter()
                        .enumerate()
                        .find(|(_, node)| node["id"] == start)
                        .map(|(index, node)| {
                            format!(
                                "{:02} {}",
                                index + 1,
                                self.catalog.label(node["type"].as_str().unwrap_or(""))
                            )
                        })
                        .unwrap_or_else(|| tr("选择入口步骤")),
                )
                .show_ui(ui, |ui, query| {
                    for (index, n) in ids.iter().enumerate() {
                        if let Some(id) = n["id"].as_str() {
                            let label = format!(
                                "{:02} {}",
                                index + 1,
                                self.catalog.label(n["type"].as_str().unwrap_or(""))
                            );
                            if !crate::forms::resource_matches(id, &label, query) {
                                continue;
                            }
                            if ui
                                .selectable_value(
                                    &mut start,
                                    id.into(),
                                    format!(
                                        "{:02} {}",
                                        index + 1,
                                        self.catalog.label(n["type"].as_str().unwrap_or(""))
                                    ),
                                )
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    }
                });
        });
        story["start"] = start.into();
        let mut mood = story["mood"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut mood, tr("显示官方心情气泡")).changed() {
            story["mood"] = mood.into();
        }
        ui.separator();
        ui.heading(tr("分区与分组"));
        ui.label(tr(
            "分区不能重叠；分组须位于所属分区内。Shift 可选择连续步骤。",
        ));
        if ui.button(tr("将选中步骤划为新分区")).clicked() {
            match authoring::add_section(
                &story,
                &self.selected_indices().into_iter().collect(),
                None,
            ) {
                Ok(next) => story = next,
                Err(e) => self.error = Some(crate::error_ui::capture(e)),
            }
        }
        ui.horizontal(|ui| {
            let sections = story["_editor"]["sections"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut section_names: Vec<_> = sections
                .iter()
                .enumerate()
                .map(|(index, section)| {
                    (
                        section["id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| index.to_string()),
                        section["title"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| crate::i18n::key("editor.workspace.section")),
                    )
                })
                .collect();
            crate::forms::number_duplicate_labels(&mut section_names);
            crate::forms::SearchCombo::from_id_salt("parent-section")
                .selected_text(
                    section_names
                        .get(self.section_parent)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_else(|| crate::i18n::key("editor.workspace.section_owner")),
                )
                .show_ui(ui, |ui, query| {
                    for (i, (id, name)) in section_names.iter().enumerate() {
                        if !crate::forms::resource_matches(id, name, query) {
                            continue;
                        }
                        if ui
                            .selectable_value(&mut self.section_parent, i, name)
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                });
            if ui
                .add_enabled(
                    !sections.is_empty(),
                    egui::Button::new(tr("将选中步骤划为分组")),
                )
                .clicked()
            {
                match authoring::add_section(
                    &story,
                    &self.selected_indices().into_iter().collect(),
                    Some(self.section_parent),
                ) {
                    Ok(next) => story = next,
                    Err(e) => self.error = Some(crate::error_ui::capture(e)),
                }
            }
        });
        if let Some(result) = authoring::section_editor(ui, &story) {
            match result {
                Ok(next) => story = next,
                Err(e) => self.error = Some(format!("分区/分组修改未应用：{e:#}")),
            }
        }
        ui.collapsing(tr("高级"), |ui| {
        if let Some(meta) = story.get_mut("_editor").and_then(Value::as_object_mut) {
            let mut other = Value::Object(
                meta.iter()
                    .filter(|(k, _)| k.as_str() != "sections")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
            if value_editor(ui, "离线测试及创作扩展数据", &mut other, 0) {
                let sections = meta.get("sections").cloned();
                *meta = other.as_object().cloned().unwrap_or_default();
                if let Some(sections) = sections {
                    meta.insert("sections".into(), sections);
                }
            }
        }
        if ui.button(tr("添加章节离线测试")).clicked() {
            if !story["_editor"].is_object() {
                story["_editor"] = json!({});
            }
            if !story["_editor"]["tests"].is_array() {
                story["_editor"]["tests"] = json!([]);
            }
            let tests = story["_editor"]["tests"].as_array_mut().unwrap();
            tests.push(json!({"name":format!("测试{}",tests.len()+1),"story":self.current,"initial":{"variables":{},"flags":{}},"actions":{"choices":[]},"assert":{"reaches_node":[]}}));
        }
        });
        self.project.stories.insert(self.current.clone(), story);
    }
    pub(super) fn localization(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("多语言对白"));
        ui.label(crate::i18n::key("editor.workspace.translation_hint"));
        let mut s = self.story();
        if !s["localization"].is_object() {
            if ui.button(tr("启用本章多语言")).clicked() {
                s["localization"] =
                    json!({"default_locale":"chs","fallback_locale":"chs","translations":{}});
                self.project.stories.insert(self.current.clone(), s);
            }
            return;
        }
        if ui.button(tr("停用本章多语言（可撤销）")).clicked() {
            s.as_object_mut().unwrap().remove("localization");
            self.project.stories.insert(self.current.clone(), s);
            return;
        }
        let mut default = s["localization"]["default_locale"]
            .as_str()
            .unwrap_or("chs")
            .to_owned();
        let mut fallback = s["localization"]["fallback_locale"]
            .as_str()
            .unwrap_or("chs")
            .to_owned();
        for (label, current) in [
            ("源语言", &mut default),
            ("回退语言", &mut fallback),
            ("正在翻译", &mut self.locale),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{}{}",
                    tr(label),
                    if label == "源语言" { " *" } else { "" }
                ));
                egui::ComboBox::from_id_salt(label)
                    .selected_text(current.as_str())
                    .show_ui(ui, |ui| {
                        for (id, name) in [
                            ("chs", "简体中文"),
                            ("cht", "繁體中文"),
                            ("ja", "日本語"),
                            ("ko", "한국어"),
                        ] {
                            ui.selectable_value(current, id.into(), name);
                        }
                    });
            });
        }
        if s["localization"]["default_locale"].as_str() != Some(&default) {
            if let Some(translations) = s["localization"]["translations"].as_object_mut() {
                translations.remove(&default);
            }
        }
        s["localization"]["default_locale"] = default.clone().into();
        s["localization"]["fallback_locale"] = fallback.into();
        let sources = lom_core::localization::iter_localizable_texts(&s);
        let translated = sources
            .iter()
            .filter(|(key, _)| {
                s["localization"]["translations"][&self.locale][key]
                    .as_str()
                    .is_some_and(|v| !v.is_empty())
            })
            .count();
        ui.label(format!(
            "{}：{} / {}",
            tr("已翻译"),
            if self.locale == default {
                sources.len()
            } else {
                translated
            },
            sources.len()
        ));
        if self.locale == default {
            ui.label(tr("当前选择为源语言，请在节点属性中修改原文。"));
        } else {
            let sources = lom_core::localization::iter_localizable_texts(&s);
            for (key, source) in sources {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new(&key).small().color(ACCENT));
                    ui.label(&source);
                    let mut translated = s["localization"]["translations"][&self.locale][&key]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut translated)
                                .hint_text(tr("未翻译，使用回退语言"))
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        if translated.is_empty() {
                            if let Some(o) =
                                s["localization"]["translations"][&self.locale].as_object_mut()
                            {
                                o.remove(&key);
                            }
                        } else {
                            s["localization"]["translations"][&self.locale][&key] =
                                translated.into();
                        }
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(tr("导出本章翻译…")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(format!("{}-translations.json", self.current))
                    .save_file()
                {
                    if let Err(e) = lom_core::project::atomic_write(
                        &path,
                        &lom_core::stable_json(&s["localization"]).unwrap_or_default(),
                    ) {
                        self.error = Some(crate::error_ui::capture(e));
                    }
                }
            }
            if ui.button(tr("导入本章翻译…")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match lom_core::load_json(path) {
                        Ok(config) => {
                            let mut candidate = s.clone();
                            candidate["localization"] = config;
                            match lom_core::localization::validate_story_localization(&candidate) {
                                Ok(()) => s = candidate,
                                Err(e) => self.error = Some(crate::error_ui::capture(e)),
                            }
                        }
                        Err(e) => self.error = Some(crate::error_ui::capture(e)),
                    }
                }
            }
        });
        self.project.stories.insert(self.current.clone(), s);
    }
}
