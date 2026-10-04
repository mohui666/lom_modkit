//! Advanced authoring workflows with explicit navigation and transactional edits.
use crate::i18n::tr;
use eframe::egui;
use lom_core::{analysis, editing, project::Project};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(PartialEq, Eq)]
struct CopySelection {
    source: String,
    target: String,
    first: usize,
    last: usize,
    insertion: usize,
}

#[derive(Default)]
pub struct Advanced {
    pub page: usize,
    query: String,
    category: String,
    symbol_kind: String,
    references: Option<Vec<editing::SearchHit>>,
    source: String,
    target: String,
    first: usize,
    last: usize,
    insertion: usize,
    proposal: Option<(CopySelection, BTreeMap<String, Value>, editing::Transfer)>,
    tests_story: String,
    tests: String,
    tests_baseline: String,
    tests_source: Option<Value>,
    results: Value,
    error: String,
}
pub type Location = (String, Option<String>);
fn rows(
    ui: &mut egui::Ui,
    hits: &[editing::SearchHit],
) -> (Option<Location>, Option<editing::SearchHit>) {
    let mut nav = None;
    let mut refs = None;
    for (i, h) in hits.iter().enumerate() {
        ui.push_id(i, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 12.0).max(100.0));
                ui.horizontal_wrapped(|ui| {
                    let label = format!(
                        "{} / {}",
                        h.story_id,
                        h.node_id.as_deref().unwrap_or("章节设置")
                    );
                    if ui.link(label).clicked() {
                        nav = Some((
                            if h.field.starts_with("manifest.") {
                                String::new()
                            } else {
                                h.story_id.clone()
                            },
                            h.node_id.clone(),
                        ));
                    }
                    if ["story", "node", "content", "character", "variable", "flag"]
                        .contains(&ref_kind(h))
                        && ui.small_button(tr("查找引用")).clicked()
                    {
                        refs = Some(h.clone());
                    }
                });
                ui.label(egui::RichText::new(display_label(&h.category)).small());
                ui.add(egui::Label::new(&h.preview).wrap());
            });
        });
    }
    (nav, refs)
}
fn ref_kind(h: &editing::SearchHit) -> &str {
    match h.category.as_str() {
        "story" => "story",
        "node" => "node",
        "goto" if h.field == "next_script" => "story",
        "goto" => "node",
        "voice" | "image" | "content_ref" => "content",
        other => other,
    }
}
impl Advanced {
    pub fn has_pending(&self) -> bool {
        !self.tests_story.is_empty() && self.tests != self.tests_baseline
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn apply_pending(&mut self, project: &mut Project) -> anyhow::Result<bool> {
        if !self.has_pending() {
            return Ok(false);
        }
        let cases: Value = serde_json::from_str(&self.tests)?;
        anyhow::ensure!(cases.is_array(), "测试定义必须为数组");
        let story = project
            .stories
            .get_mut(&self.tests_story)
            .ok_or_else(|| anyhow::anyhow!("测试所属章节已不存在，请撤销未完成的输入"))?;
        anyhow::ensure!(
            story["_editor"].get("tests") == self.tests_source.as_ref(),
            "测试内容已被其他操作修改；请撤销未完成的输入后重试"
        );
        let changed = self.tests_source.as_ref() != Some(&cases);
        if changed {
            if !story["_editor"].is_object() {
                story["_editor"] = json!({});
            }
            story["_editor"]["tests"] = cases.clone();
        }
        self.tests_source = Some(cases);
        self.tests_baseline = self.tests.clone();
        self.results = Value::Null;
        self.error.clear();
        Ok(changed)
    }

    fn select_tests(&mut self, project: &mut Project, sid: &str) -> bool {
        if self.tests_story != sid {
            if let Err(error) = self.apply_pending(project) {
                self.error = format!("测试输入尚未完成：{error}");
                return false;
            }
            self.load_tests(project, sid);
        } else if project
            .stories
            .get(sid)
            .and_then(|s| s["_editor"].get("tests"))
            != self.tests_source.as_ref()
        {
            if self.has_pending() {
                self.error = "测试内容已被其他操作修改；请撤销未完成的输入后重试".into();
                return false;
            }
            self.load_tests(project, sid);
        }
        true
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        current: &str,
    ) -> Option<Location> {
        let mut nav = None;
        let pages = [
            "全局查找",
            "变量管理",
            "条件检查",
            "路径模拟",
            "跨章节复制",
            "离线测试",
        ];
        ui.horizontal(|ui| {
            ui.heading(tr("创作工具"));
            egui::ComboBox::from_id_salt("authoring-page")
                .selected_text(tr(pages[self.page.min(pages.len() - 1)]))
                .show_ui(ui, |ui| {
                    for (i, name) in pages.iter().enumerate() {
                        if ui.selectable_value(&mut self.page, i, tr(name)).clicked() {
                            self.references = None;
                        }
                    }
                });
        });
        ui.add_space(6.0);
        match self.page {
            0 => {
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .desired_width(f32::INFINITY)
                        .hint_text(tr("查找台词、人物或节点")),
                );
                egui::ComboBox::from_id_salt("search-category")
                    .selected_text(if self.category.is_empty() {
                        tr("全部类型")
                    } else {
                        display_label(&self.category)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.category, String::new(), tr("全部类型"));
                        for category in editing::CATEGORIES {
                            ui.selectable_value(
                                &mut self.category,
                                category.to_string(),
                                display_label(category),
                            );
                        }
                    });
                let hits = editing::search(
                    &editing::index_project(&project.stories),
                    &self.query,
                    &self.category,
                );
                ui.label(format!("{} 个匹配", hits.len()));
                let (location, refs) = rows(ui, &hits);
                nav = location;
                if let Some(h) = refs {
                    self.references = Some(editing::references(
                        &project.stories,
                        &project.manifest,
                        ref_kind(&h),
                        if h.category == "story" {
                            &h.story_id
                        } else {
                            &h.value
                        },
                        Some(&h.story_id),
                    ));
                }
            }
            1 => {
                egui::ComboBox::from_id_salt("symbol-kind")
                    .selected_text(if self.symbol_kind.is_empty() {
                        tr("全部变量")
                    } else {
                        display_label(&self.symbol_kind)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.symbol_kind, String::new(), tr("全部变量"));
                        for k in [
                            "mod_flag",
                            "game_flag",
                            "checkpoint",
                            "condition",
                            "flow_variable",
                            "stat",
                        ] {
                            ui.selectable_value(&mut self.symbol_kind, k.into(), display_label(k));
                        }
                    });
                let report = analysis::analyze_project(&project.stories, &project.manifest);
                for s in report["symbols"].as_array().into_iter().flatten() {
                    if !self.symbol_kind.is_empty() && s["kind"] != self.symbol_kind {
                        continue;
                    }
                    ui.push_id(s.to_string(), |ui| {
                        ui.strong(format!(
                            "{} · {}",
                            display_label(s["kind"].as_str().unwrap_or("")),
                            s["name"].as_str().unwrap_or("")
                        ));
                        ui.label(format!(
                            "读取 {} · 写入 {} · 未使用 {} · 可能先读后写 {}",
                            s["reads"],
                            s["writes"],
                            tri(&s["unused"]),
                            tri(&s["possibly_read_before_write"])
                        ));
                        for u in s["uses"].as_array().into_iter().flatten() {
                            let sid = u["story"].as_str().unwrap_or("");
                            let nid = u["node"].as_str();
                            if ui
                                .link(format!(
                                    "{sid}/{} · {}",
                                    nid.unwrap_or("作品设置"),
                                    display_label(u["access"].as_str().unwrap_or(""))
                                ))
                                .clicked()
                            {
                                nav = Some((sid.into(), nid.map(str::to_owned)));
                            }
                        }
                        if ui.small_button(tr("查找全部引用")).clicked() {
                            let kind = if s["kind"] == "flow_variable" || s["kind"] == "stat" {
                                "variable"
                            } else {
                                "flag"
                            };
                            self.references = Some(editing::references(
                                &project.stories,
                                &project.manifest,
                                kind,
                                s["name"].as_str().unwrap_or(""),
                                None,
                            ));
                        }
                        ui.separator();
                    });
                }
            }
            2 | 3 => {
                let report = analysis::analyze_project(&project.stories, &project.manifest);
                let collection = if self.page == 2 {
                    "conditions"
                } else {
                    "issues"
                };
                for row in report[collection].as_array().into_iter().flatten() {
                    ui.push_id(row.to_string(), |ui| {
                        let sid = row["story"].as_str().unwrap_or("");
                        let nid = row["node"].as_str();
                        if ui
                            .link(format!("{sid}/{}", nid.unwrap_or("章节入口")))
                            .clicked()
                        {
                            nav = Some((sid.into(), nid.map(str::to_owned)));
                        }
                        if self.page == 2 {
                            ui.label(format!(
                                "{} · {} · {}",
                                display_label(row["source"].as_str().unwrap_or("")),
                                row["subject"].as_str().unwrap_or(""),
                                row["reason"].as_str().unwrap_or("")
                            ));
                            if let Some(s) = project.stories.get(sid) {
                                if let Some(i) = s["nodes"].as_array().and_then(|n| {
                                    n.iter().position(|n| n["id"] == nid.unwrap_or(""))
                                }) {
                                    for target in analysis::successors(s, i) {
                                        if ui.link(format!("→ {target}")).clicked() {
                                            nav = Some((sid.into(), Some(target)));
                                        }
                                    }
                                }
                            }
                            for case in row["cases"].as_array().into_iter().flatten() {
                                ui.label(case.to_string());
                            }
                        } else {
                            ui.label(format!(
                                "{} · {}",
                                display_label(row["severity"].as_str().unwrap_or("")),
                                row["detail"].as_str().unwrap_or("")
                            ));
                        }
                        ui.separator();
                    });
                }
                if report[collection].as_array().is_some_and(Vec::is_empty) {
                    ui.label(tr("没有发现问题"));
                }
                ui.label(tr(
                    "离线分析保留未知游戏状态；不会猜测战斗、原始 Lua 或随机结果。",
                ));
            }
            4 => {
                if !project.stories.contains_key(&self.source) {
                    self.source = current.into();
                }
                if !project.stories.contains_key(&self.target) || self.target == self.source {
                    self.target = project
                        .stories
                        .keys()
                        .find(|k| **k != self.source)
                        .cloned()
                        .unwrap_or_default();
                }
                for (label, selected) in [
                    ("来源章节", &mut self.source),
                    ("目标章节", &mut self.target),
                ] {
                    egui::ComboBox::from_id_salt(label)
                        .selected_text(selected.as_str())
                        .show_ui(ui, |ui| {
                            for id in project.stories.keys() {
                                ui.selectable_value(selected, id.clone(), id);
                            }
                        });
                }
                let count = project
                    .stories
                    .get(&self.source)
                    .and_then(|s| s["nodes"].as_array())
                    .map_or(0, Vec::len);
                let target_count = project
                    .stories
                    .get(&self.target)
                    .and_then(|s| s["nodes"].as_array())
                    .map_or(0, Vec::len);
                for (label, value) in [
                    ("首步（从 1 开始）", &mut self.first),
                    ("末步（包含）", &mut self.last),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(tr(label));
                        *value = (*value).clamp(1, count.max(1));
                        ui.add(egui::DragValue::new(value).range(1..=count.max(1)));
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(tr("插入到第几步之后（0 为开头）"));
                    ui.add(egui::DragValue::new(&mut self.insertion).range(0..=target_count));
                });
                let selection = self.copy_selection();
                if self
                    .proposal
                    .as_ref()
                    .is_some_and(|(previous, _, _)| previous != &selection)
                {
                    // A preview only authorizes the exact options it was generated from.
                    self.proposal = None;
                    self.error.clear();
                }
                if ui
                    .add_enabled(
                        count > 0 && self.source != self.target,
                        egui::Button::new(tr("预览复制与边界引用")),
                    )
                    .clicked()
                {
                    match editing::transfer(
                        &project.stories,
                        &self.source,
                        self.first - 1,
                        self.last - 1,
                        &self.target,
                        self.insertion,
                    ) {
                        Ok(t) => {
                            self.proposal = Some((selection, project.stories.clone(), t));
                            self.error.clear();
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
                if let Some((_, before, plan)) = &self.proposal {
                    ui.label(format!(
                        "{} → {} · {} 个节点",
                        plan.source_story, plan.target_story, plan.count
                    ));
                    for w in &plan.warnings {
                        ui.colored_label(ui.visuals().warn_fg_color, w);
                    }
                    let valid = before == &project.stories;
                    if !valid {
                        ui.label(tr("项目已变化，请重新预览"));
                    }
                    if ui
                        .add_enabled(valid, egui::Button::new(tr("确认复制（可撤销）")))
                        .clicked()
                    {
                        project
                            .stories
                            .insert(plan.target_story.clone(), plan.after.clone());
                        nav = Some((
                            plan.target_story.clone(),
                            plan.after["nodes"][plan.first_index]["id"]
                                .as_str()
                                .map(str::to_owned),
                        ));
                        self.proposal = None;
                    }
                }
            }
            5 => {
                let mut selected = if self.tests_story.is_empty() {
                    current.into()
                } else {
                    self.tests_story.clone()
                };
                self.select_tests(project, &selected);
                egui::ComboBox::from_id_salt("test-chapter")
                    .selected_text(&self.tests_story)
                    .show_ui(ui, |ui| {
                        for sid in project.stories.keys() {
                            ui.selectable_value(&mut selected, sid.clone(), sid);
                        }
                    });
                if selected != self.tests_story {
                    self.select_tests(project, &selected);
                }
                ui.label(tr(
                    "初始变量、选项动作和断言（JSON）；结果包含完整访问路径。",
                ));
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut self.tests)
                            .code_editor()
                            .desired_rows(12)
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    self.results = Value::Null;
                    if let Err(error) = self.apply_pending(project) {
                        self.error = format!("测试输入尚未完成：{error}");
                    }
                }
                let mut run = false;
                ui.horizontal(|ui| {
                    run = ui.button(tr("运行测试")).clicked();
                    if self.has_pending() && ui.button(tr("撤销未完成的输入")).clicked() {
                        let sid = if project.stories.contains_key(&self.tests_story) {
                            self.tests_story.clone()
                        } else {
                            current.into()
                        };
                        self.load_tests(project, &sid);
                    }
                });
                if run {
                    match self
                        .apply_pending(project)
                        .and_then(|_| {
                            serde_json::from_str::<Value>(&self.tests).map_err(Into::into)
                        })
                        .and_then(|cases| analysis::run_story_tests(&project.stories, &cases))
                    {
                        Ok(result) => {
                            self.error.clear();
                            self.results = result;
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
                for result in self.results.as_array().into_iter().flatten() {
                    ui.strong(format!(
                        "{} · {}",
                        result["name"].as_str().unwrap_or(""),
                        display_label(result["status"].as_str().unwrap_or(""))
                    ));
                    ui.label(result["message"].as_str().unwrap_or(""));
                    ui.collapsing(tr("访问路径与最终状态"), |ui| {
                        for v in result["visited"].as_array().into_iter().flatten() {
                            if ui
                                .link(format!(
                                    "{}/{}",
                                    v[0].as_str().unwrap_or(""),
                                    v[1].as_str().unwrap_or("")
                                ))
                                .clicked()
                            {
                                nav = Some((
                                    v[0].as_str().unwrap_or("").into(),
                                    v[1].as_str().map(str::to_owned),
                                ));
                            }
                        }
                        ui.monospace(format!(
                            "{}: {}\n{}: {}",
                            tr("变量"),
                            result["variables"],
                            tr("剧情标记"),
                            result["flags"]
                        ));
                    });
                }
            }
            _ => (),
        }
        if let Some(hits) = self.references.clone() {
            ui.separator();
            ui.strong(tr("引用结果"));
            if ui.small_button(tr("关闭引用结果")).clicked() {
                self.references = None;
            }
            let (location, _) = ui.push_id("reference-results", |ui| rows(ui, &hits)).inner;
            if location.is_some() {
                nav = location;
            }
        }
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(179, 55, 49), &self.error);
        }
        nav
    }
    fn copy_selection(&self) -> CopySelection {
        CopySelection {
            source: self.source.clone(),
            target: self.target.clone(),
            first: self.first,
            last: self.last,
            insertion: self.insertion,
        }
    }
    fn load_tests(&mut self, project: &Project, sid: &str) {
        self.tests_story = sid.into();
        self.results = Value::Null;
        self.tests_source = project
            .stories
            .get(sid)
            .and_then(|s| s["_editor"].get("tests"))
            .cloned();
        let tests = self.tests_source.clone().unwrap_or(json!([{"name":"example","story":sid,"initial":{"variables":{},"flags":{}},"actions":{"choices":[]},"assert":{"reaches_ending":true}}]));
        self.tests = serde_json::to_string_pretty(&tests).unwrap();
        self.tests_baseline = self.tests.clone();
        self.error.clear();
    }
}
fn display_label(value: &str) -> String {
    tr(match value {
        "story" => "章节",
        "node" => "步骤",
        "text" => "文本",
        "character" => "人物",
        "portrait" => "表情",
        "voice" => "音频",
        "image" => "图片",
        "variable" => "变量",
        "flag" => "剧情标记",
        "goto" => "跳转",
        "content_ref" => "素材引用",
        "mod_flag" | "mod" => "作品标记",
        "game_flag" | "game" => "游戏标记",
        "checkpoint" => "检查点",
        "condition" => "条件",
        "flow_variable" => "流程变量",
        "stat" => "人物属性",
        "read" => "读取",
        "write" => "写入",
        "warning" => "警告",
        "error" => "错误",
        "pass" => "通过",
        "fail" => "未通过",
        "unsupported" => "需要游戏内验证",
        other => other,
    })
}
fn tri(v: &Value) -> &str {
    match v.as_bool() {
        Some(true) => "是",
        Some(false) => "否",
        None => "未知",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_test_edits_survive_chapter_switch_and_refresh_after_undo() {
        let mut project = Project::new();
        let mut second = project.stories["main"].clone();
        second["id"] = json!("second");
        project.stories.insert("second".into(), second);
        let mut panel = Advanced::default();
        assert!(panel.select_tests(&mut project, "main"));
        let cases = json!([{"name":"edited test","story":"main","assert":{"reaches_ending":true}}]);
        panel.tests = serde_json::to_string(&cases).unwrap();
        assert!(panel.has_pending());
        assert!(panel.select_tests(&mut project, "second"));
        assert_eq!(project.stories["main"]["_editor"]["tests"], cases);
        assert!(!panel.has_pending());
        assert!(panel.select_tests(&mut project, "main"));
        assert_eq!(serde_json::from_str::<Value>(&panel.tests).unwrap(), cases);

        // Undo or another editing surface may replace the stored test definition.
        project.stories.get_mut("main").unwrap()["_editor"]["tests"] = json!([]);
        assert!(panel.select_tests(&mut project, "main"));
        assert_eq!(panel.tests, "[]");
        assert!(!panel.has_pending());
    }

    #[test]
    fn unfinished_test_input_blocks_switch_and_save_without_losing_text() {
        let mut project = Project::new();
        let mut second = project.stories["main"].clone();
        second["id"] = json!("second");
        project.stories.insert("second".into(), second);
        let before = project.clone();
        let mut panel = Advanced::default();
        panel.select_tests(&mut project, "main");
        panel.tests = "[{\"name\": \"still typing\"".into();
        let unfinished = panel.tests.clone();
        assert!(!panel.select_tests(&mut project, "second"));
        assert_eq!(panel.tests_story, "main");
        assert_eq!(panel.tests, unfinished);
        assert!(panel.apply_pending(&mut project).is_err());
        assert_eq!(project, before);
        assert!(panel.has_pending());

        panel.tests = "[]".into();
        assert!(panel.apply_pending(&mut project).unwrap());
        assert_eq!(project.stories["main"]["_editor"]["tests"], json!([]));
        assert!(!panel.has_pending());
        assert!(panel.select_tests(&mut project, "second"));
    }

    #[test]
    fn test_input_cannot_overwrite_external_changes_and_reset_discards_only_buffer() {
        let mut project = Project::new();
        let mut panel = Advanced::default();
        panel.select_tests(&mut project, "main");
        panel.tests = "[]".into();
        let external = json!([{"name":"external","story":"main"}]);
        project.stories.get_mut("main").unwrap()["_editor"] = json!({"tests":external});
        assert!(panel.apply_pending(&mut project).is_err());
        assert_eq!(panel.tests, "[]");
        assert_eq!(project.stories["main"]["_editor"]["tests"], external);
        panel.reset();
        assert!(!panel.has_pending());
        assert!(panel.tests_story.is_empty());
        assert_eq!(project.stories["main"]["_editor"]["tests"], external);
    }

    #[test]
    fn changing_any_copy_option_discards_preview_before_confirmation() {
        let mut project = Project::new();
        for id in ["second", "third"] {
            let mut story = project.stories["main"].clone();
            story["id"] = json!(id);
            project.stories.insert(id.into(), story);
        }
        let before = project.clone();
        let ctx = egui::Context::default();
        for changed_option in ["source", "target", "first", "last", "insertion"] {
            let mut panel = Advanced {
                page: 4,
                source: "main".into(),
                target: "second".into(),
                first: 1,
                last: 2,
                insertion: 0,
                ..Default::default()
            };
            let plan = editing::transfer(&project.stories, "main", 0, 1, "second", 0)
                .expect("valid preview");
            panel.proposal = Some((panel.copy_selection(), project.stories.clone(), plan));
            // An unchanged frame preserves the preview; editing any input invalidates it.
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    panel.show(ui, &mut project, "main");
                });
            });
            assert!(panel.proposal.is_some());
            match changed_option {
                "source" => panel.source = "third".into(),
                "target" => panel.target = "third".into(),
                "first" => panel.first = 2,
                "last" => panel.last = 1,
                "insertion" => panel.insertion = 1,
                _ => unreachable!(),
            }
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    panel.show(ui, &mut project, "main");
                });
            });
            assert!(panel.proposal.is_none(), "changed {changed_option}");
            assert_eq!(project, before, "changed {changed_option}");
        }
    }

    #[test]
    fn viewing_all_analysis_pages_does_not_modify_project() {
        let mut project = Project::new();
        let before = project.clone();
        let mut panel = Advanced::default();
        let ctx = egui::Context::default();
        for page in 0..6 {
            panel.page = page;
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    panel.show(ui, &mut project, "main");
                });
            });
            assert_eq!(project, before, "page {page}");
        }
    }
}
