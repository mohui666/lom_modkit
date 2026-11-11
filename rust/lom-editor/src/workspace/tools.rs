//! Templates and auxiliary authoring tools.
use super::*;

impl App {
    pub(super) fn persist_templates(&mut self) {
        match lom_core::stable_json(&self.templates).and_then(|b| {
            lom_core::project::atomic_write(&user_root().join("rust/node-templates.json"), &b)
        }) {
            Ok(()) => self.status = "节点模板已保存到本机创作数据目录".into(),
            Err(e) => self.error = Some(crate::error_ui::capture(e)),
        }
    }
    pub(super) fn save_selected_template(&mut self) {
        let name = self.template_name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            self.error = Some("模板名称须为 1–80 个非控制字符".into());
            return;
        }
        if self.templates.get(&name).is_some() {
            self.error = Some("模板名称已存在，请先删除旧模板或更换名称".into());
            return;
        }
        match authoring::selection_block(
            &self.story(),
            &self.selected_indices().into_iter().collect(),
        ) {
            Ok((nodes, warnings)) => {
                self.templates[&name] =
                    json!({"schema":1,"name":name,"nodes":nodes,"warnings":warnings});
                self.persist_templates();
                self.template_name.clear();
            }
            Err(e) => self.error = Some(crate::error_ui::capture(e)),
        }
    }
    pub(super) fn tools(&mut self, ui: &mut egui::Ui) {
        let page = self.tools_panel.page;
        if matches!(page, ToolPage::Previews | ToolPage::Library) {
            self.material_navigation(ui);
        } else {
            ui.heading(tr(page.title()));
        }
        if page == ToolPage::Game && !cfg!(windows) {
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("游戏预览素材")).clicked() {
                    self.open_tool_page(ToolPage::Previews);
                }
                if ui.button(tr("前往发布与导出")).clicked() {
                    self.open_tool_page(ToolPage::Release);
                }
            });
            ui.add_space(8.0);
        }
        self.tools_panel.show(ui, &mut self.project);
        if self.tools_panel.take_export_request() {
            self.export();
        }
        if page != ToolPage::Editing {
            return;
        }
        ui.collapsing(tr("批量修改选中步骤字段"), |ui| {
            let story = self.story();
            let indices: BTreeSet<usize> = self.selected_indices().into_iter().collect();
            let selected: Vec<Value> = indices
                .iter()
                .filter_map(|i| story["nodes"].get(*i).cloned())
                .collect();
            ui.label(crate::i18n::message(
                "workspace.bulk.selected",
                &[("count", selected.len().to_string())],
            ));
            let fields = authoring::bulk_fields(&self.catalog.schema, &selected);
            if selected.len() < 2 || fields.is_empty() {
                ui.label(tr("至少选择两个具有相同标量字段类型的步骤。"));
                return;
            }
            let old = self.bulk_key.clone();
            egui::ComboBox::from_id_salt("bulk-field")
                .selected_text(
                    fields
                        .iter()
                        .find(|f| f.key == self.bulk_key)
                        .map(|f| f.label.as_str())
                        .unwrap_or("选择共同字段"),
                )
                .show_ui(ui, |ui| {
                    for f in &fields {
                        ui.selectable_value(
                            &mut self.bulk_key,
                            f.key.clone(),
                            format!("{} · {}", f.label, f.key),
                        );
                    }
                });
            if let Some(field) = fields.iter().find(|f| f.key == self.bulk_key) {
                if old != self.bulk_key || !self.bulk_draft.is_object() {
                    self.bulk_draft = selected[0].clone();
                }
                self.catalog.field(
                    ui,
                    &mut self.bulk_draft,
                    &field.key,
                    &field.label,
                    &field.kind,
                    false,
                    &[],
                    &[],
                );
                ui.label(tr("应用前按节点规则校验整章。任何错误都会保留所有原值。"));
                if ui.button(tr("应用到全部选中步骤")).clicked() {
                    match authoring::bulk_edit_with_catalog(
                        &story,
                        &indices,
                        &self.catalog.schema,
                        &field.key,
                        &self.bulk_draft[&field.key],
                        &self.catalog.data,
                    ) {
                        Ok(next) => {
                            self.flush();
                            self.project.stories.insert(self.current.clone(), next);
                            self.status = format!("已批改 {} 个步骤，可撤销", selected.len());
                        }
                        Err(e) => self.error = Some(crate::error_ui::capture(e)),
                    }
                }
            }
        });
        ui.collapsing(tr("批量修改对白"), |ui| {
            ui.label(crate::i18n::key("workspace.bulk.text_hint"));
            ui.horizontal(|ui| {
                ui.label(tr("查找"));
                ui.text_edit_singleline(&mut self.search);
            });
            ui.horizontal(|ui| {
                ui.label(tr("替换为"));
                ui.text_edit_singleline(&mut self.replace);
            });
            if ui
                .add_enabled(
                    !self.search.is_empty(),
                    egui::Button::new(tr("替换所有章节中的匹配文本")),
                )
                .clicked()
            {
                self.flush();
                let mut count = 0;
                for s in self.project.stories.values_mut() {
                    count += replace_prose(s, &self.search, &self.replace);
                }
                self.status = format!("已替换 {count} 处文本，可撤销");
            }
        });
        ui.collapsing(crate::i18n::key("editor.workspace.templates"), |ui| {
            ui.label(tr(
                "模板保存连续选中的步骤，并在插入时生成新 ID、重连内部跳转。",
            ));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.template_name)
                        .hint_text(format!("{} *", tr("模板名称"))),
                );
                if ui.button(tr("保存选中范围")).clicked() {
                    self.save_selected_template();
                }
            });
            let list = self.templates.as_object().cloned().unwrap_or_default();
            let mut remove = None;
            for (name, template) in list {
                let nodes = template["nodes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| vec![template.clone()]);
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::message(
                        "editor.workspace.steps",
                        &[("name", name.clone()), ("count", nodes.len().to_string())],
                    ));
                    if ui.small_button(tr("插入")).clicked() {
                        match authoring::validate_template(&nodes) {
                            Ok(()) => {
                                let mut warnings = authoring::template_warnings(&nodes);
                                warnings.extend(
                                    template["warnings"]
                                        .as_array()
                                        .into_iter()
                                        .flatten()
                                        .filter_map(|v| v.as_str().map(str::to_owned)),
                                );
                                warnings.sort();
                                warnings.dedup();
                                if warnings.is_empty() {
                                    self.clipboard = nodes.clone();
                                    self.paste();
                                } else {
                                    self.pending_template = Some((nodes.clone(), warnings));
                                }
                            }
                            Err(e) => self.error = Some(crate::error_ui::capture(e)),
                        }
                    }
                    if ui.small_button(tr("移除模板")).clicked() {
                        remove = Some(name.clone());
                    }
                });
            }
            if let Some(key) = remove {
                self.templates.as_object_mut().unwrap().remove(&key);
                self.persist_templates();
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui|{
            if ui.button(tr("变量 / 条件 / 引用分析")).clicked(){self.tools_report=lom_core::analysis::analyze_project(&self.project.stories,&self.project.manifest);self.tools_panel.present("变量 / 条件 / 引用分析", self.tools_report.clone());}
            if ui.button(tr("运行项目离线测试")).clicked(){let cases:Vec<Value>=self.project.stories.values().flat_map(|s|s["_editor"]["tests"].as_array().cloned().unwrap_or_default()).collect();if cases.is_empty(){self.error=Some("还没有定义离线测试。请打开章节设置，添加测试并填写初始变量、选项和断言。".into());}else{match lom_core::analysis::run_story_tests(&self.project.stories,&Value::Array(cases)){Ok(result)=>{self.tools_report=result;self.tools_panel.present("运行项目离线测试", self.tools_report.clone());},Err(e)=>self.error=Some(crate::error_ui::capture(e))}}}
            if ui.button(tr("从入口模拟当前章节")).clicked(){let case=json!([{"name":"入口路径模拟","story":self.current,"initial":{"variables":{},"flags":{}},"actions":{"choices":[]},"assert":{}}]);match lom_core::analysis::run_story_tests(&self.project.stories,&case){Ok(r)=>{self.tools_report=r;self.tools_panel.present("从入口模拟当前章节", self.tools_report.clone());},Err(e)=>self.error=Some(crate::error_ui::capture(e))}}
        });
        ui.label(
            RichText::new(tr("离线模拟无法确定的游戏状态会标为“需要在游戏中确认”。"))
                .small()
                .color(Color32::GRAY),
        );
        ui.separator();
    }
    pub(super) fn diagnostic(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("lom-editor-diagnostics.zip")
            .save_file()
        {
            let issues = lom_core::release::run_preflight(
                &self.project,
                lom_core::release::Profile::Editing,
                "1.1.3",
            );
            match lom_core::release::export_diagnostic_bundle(
                &path,
                &self.project,
                &issues,
                &self.tools_panel.diagnostic_options(),
            ) {
                Ok(saved) => self.status = format!("脱敏诊断包已保存：{}", saved.display()),
                Err(e) => self.error = Some(crate::error_ui::capture(e)),
            }
        }
    }
    pub(super) fn choose_preview_library(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr(
                "选择已提取的素材库（含 preview_map.json），或选择游戏安装目录进行提取",
            ))
            .pick_folder()
        else {
            return;
        };
        if path.join("Mortal_Data").is_dir()
            || path.file_name().is_some_and(|name| name == "Mortal_Data")
        {
            self.tools_panel.start_extraction_from(path);
        } else {
            match self.preview.set_library(&path) {
                Ok(()) => {
                    self.status = format!("预览素材库：{}", path.display());
                    let preference_path = user_root().join("rust/preferences.json");
                    let mut prefs = lom_core::load_json(&preference_path).unwrap_or(json!({}));
                    prefs["preview_library_dir"] = json!(path);
                    if let Err(error) = lom_core::stable_json(&prefs)
                        .and_then(|bytes| lom_core::project::atomic_write(&preference_path, &bytes))
                    {
                        self.error = Some(format!("素材已载入，但无法记住素材位置：{error:#}"));
                    }
                }
                Err(error) => self.error = Some(crate::error_ui::capture(error)),
            }
        }
    }
}
