//! Advanced authoring workflows with explicit navigation and transactional edits.
use crate::i18n::tr;
use eframe::egui;
use lom_core::{analysis, editing, project::Project};
use serde_json::{json, Value};
use std::collections::BTreeMap;
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
    proposal: Option<(BTreeMap<String, Value>, editing::Transfer)>,
    tests_story: String,
    tests: String,
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
                ui.label(egui::RichText::new(&h.field).small());
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
                        "全部类型"
                    } else {
                        &self.category
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.category, String::new(), tr("全部类型"));
                        for category in editing::CATEGORIES {
                            ui.selectable_value(
                                &mut self.category,
                                category.to_string(),
                                *category,
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
                        "全部变量"
                    } else {
                        &self.symbol_kind
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
                            ui.selectable_value(&mut self.symbol_kind, k.into(), k);
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
                            s["kind"].as_str().unwrap_or(""),
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
                            let field = u["field"].as_str().unwrap_or("flag");
                            if ui
                                .link(format!(
                                    "{sid}/{} · {} · {field}",
                                    nid.unwrap_or("作品设置"),
                                    u["access"].as_str().unwrap_or("")
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
                                row["source"].as_str().unwrap_or(""),
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
                                row["severity"].as_str().unwrap_or(""),
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
                            self.proposal = Some((project.stories.clone(), t));
                            self.error.clear();
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
                if let Some((before, plan)) = &self.proposal {
                    ui.label(format!(
                        "{} → {} · {} 个节点",
                        plan.source_story, plan.target_story, plan.count
                    ));
                    for w in &plan.warnings {
                        ui.colored_label(egui::Color32::YELLOW, w);
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
                if self.tests_story.is_empty() {
                    self.load_tests(project, current);
                }
                let old = self.tests_story.clone();
                egui::ComboBox::from_id_salt("test-chapter")
                    .selected_text(&self.tests_story)
                    .show_ui(ui, |ui| {
                        for sid in project.stories.keys() {
                            ui.selectable_value(&mut self.tests_story, sid.clone(), sid);
                        }
                    });
                if old != self.tests_story {
                    let sid = self.tests_story.clone();
                    self.load_tests(project, &sid);
                }
                ui.label(tr(
                    "初始变量、选项动作和断言（JSON）；结果包含完整访问路径。",
                ));
                ui.add(
                    egui::TextEdit::multiline(&mut self.tests)
                        .code_editor()
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                );
                let mut run = false;
                let mut save = false;
                ui.horizontal(|ui| {
                    run = ui.button(tr("运行草稿测试")).clicked();
                    save = ui.button(tr("保存测试到章节（可撤销）")).clicked();
                });
                if run || save {
                    match serde_json::from_str::<Value>(&self.tests)
                        .map_err(anyhow::Error::from)
                        .and_then(|cases| {
                            let result = analysis::run_story_tests(&project.stories, &cases)?;
                            Ok((cases, result))
                        }) {
                        Ok((cases, result)) => {
                            self.error.clear();
                            self.results = result;
                            if save {
                                if let Some(s) = project.stories.get_mut(&self.tests_story) {
                                    if !s["_editor"].is_object() {
                                        s["_editor"] = json!({});
                                    }
                                    s["_editor"]["tests"] = cases;
                                }
                            }
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
                for result in self.results.as_array().into_iter().flatten() {
                    ui.strong(format!(
                        "{} · {}",
                        result["name"].as_str().unwrap_or(""),
                        result["status"].as_str().unwrap_or("")
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
                            "variables: {}\nflags: {}",
                            result["variables"], result["flags"]
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
    fn load_tests(&mut self, project: &Project, sid: &str) {
        self.tests_story = sid.into();
        self.results = Value::Null;
        let tests=project.stories.get(sid).and_then(|s|s["_editor"].get("tests")).cloned().unwrap_or(json!([{"name":"example","story":sid,"initial":{"variables":{},"flags":{}},"actions":{"choices":[]},"assert":{"reaches_ending":true}}]));
        self.tests = serde_json::to_string_pretty(&tests).unwrap();
    }
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
