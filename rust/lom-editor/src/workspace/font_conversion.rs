//! Explicit, previewed conversion of legacy font sizes; runtime sizing is separate.
use super::*;

const RECORD: &str = "legacy_font_size_conversion";
const RULE: &str = "scale_70_percent_v1";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Chapter,
    Project,
}

pub(super) struct Dialog {
    chapter: String,
    scope: Scope,
    error: Option<String>,
    target_size: Option<i64>,
}

struct Row {
    old: String,
    new: String,
    count: usize,
    status: &'static str,
}

#[derive(Default)]
struct Plan {
    rows: Vec<Row>,
    changes: BTreeMap<String, Vec<(usize, i64)>>,
    issues: Vec<String>,
    processed_chapters: usize,
}

impl Plan {
    fn count(&self) -> usize {
        self.changes.values().map(Vec::len).sum()
    }
}

fn manual_size(value: Option<&Value>) -> Option<i64> {
    match value.and_then(Value::as_str) {
        Some("small") => Some(24),
        Some("large") => Some(60),
        _ => value
            .and_then(Value::as_i64)
            .filter(|size| (1..=200).contains(size)),
    }
}

fn size_label(value: Option<&Value>) -> String {
    match value {
        None => crate::i18n::key("font_conversion.unset"),
        Some(value) if value == "small" => crate::i18n::key("font_conversion.small"),
        Some(value) if value == "large" => crate::i18n::key("font_conversion.large"),
        Some(value) if value == "normal" => crate::i18n::key("font_conversion.normal"),
        Some(value) => value.to_string(),
    }
}

fn plan(project: &Project, chapter: &str, scope: Scope) -> Plan {
    let mut plan = Plan::default();
    let mut rows = BTreeMap::new();
    for (id, story) in &project.stories {
        if scope == Scope::Chapter && id != chapter {
            continue;
        }
        let processed = story["_editor"][RECORD]["rule"] == RULE;
        if processed {
            plan.processed_chapters += 1;
        }
        let blocked = if !story["_editor"].is_null() && !story["_editor"].is_object() {
            Some("font_conversion.issue.editor")
        } else if !processed && story["_editor"].get(RECORD).is_some() {
            Some("font_conversion.issue.record")
        } else {
            None
        };
        for (index, node) in story["nodes"].as_array().into_iter().flatten().enumerate() {
            if node["type"] != "say" {
                continue;
            }
            let value = node.get("font_size");
            let size = manual_size(value);
            let default =
                value.is_none() || value == Some(&json!(0)) || value == Some(&json!("normal"));
            let old = size_label(value);
            let (new, status, issue) = if default {
                (
                    crate::i18n::key("font_conversion.default"),
                    "font_conversion.status.default",
                    None,
                )
            } else if size.is_none() {
                (
                    old.clone(),
                    "font_conversion.status.invalid",
                    Some("font_conversion.issue.invalid"),
                )
            } else if processed {
                (old.clone(), "font_conversion.status.processed", None)
            } else if let Some(issue) = blocked {
                (old.clone(), "font_conversion.status.blocked", Some(issue))
            } else {
                // Integer arithmetic gives exact positive half-up rounding,
                // including values such as 45 * 70% = 31.5 -> 32.
                let converted = ((size.unwrap() * 7 + 5) / 10).clamp(1, 200);
                if value.and_then(Value::as_i64) == Some(converted) {
                    (
                        converted.to_string(),
                        "font_conversion.status.minimum",
                        None,
                    )
                } else {
                    plan.changes
                        .entry(id.clone())
                        .or_default()
                        .push((index, converted));
                    (
                        converted.to_string(),
                        "font_conversion.status.convert",
                        None,
                    )
                }
            };
            *rows
                .entry((
                    size.unwrap_or(if default { 0 } else { 201 }),
                    old,
                    new,
                    status,
                ))
                .or_insert(0) += 1;
            if let Some(issue) = issue {
                plan.issues.push(format!(
                    "{} / {}：{}",
                    id,
                    node["id"].as_str().unwrap_or("?"),
                    crate::i18n::key(issue)
                ));
            }
        }
    }
    plan.rows = rows
        .into_iter()
        .map(|((_, old, new, status), count)| Row {
            old,
            new,
            count,
            status,
        })
        .collect();
    plan
}

fn uniform_plan(project: &Project, chapter: &str, scope: Scope, size: i64) -> Plan {
    let mut plan = Plan::default();
    let mut rows = BTreeMap::<String, usize>::new();
    for (id, story) in &project.stories {
        if scope == Scope::Chapter && id != chapter {
            continue;
        }
        for (index, node) in story["nodes"].as_array().into_iter().flatten().enumerate() {
            if node["type"] != "say" || node.get("font_size").and_then(Value::as_i64) == Some(size)
            {
                continue;
            }
            plan.changes
                .entry(id.clone())
                .or_default()
                .push((index, size));
            *rows.entry(size_label(node.get("font_size"))).or_default() += 1;
        }
    }
    plan.rows = rows
        .into_iter()
        .map(|(old, count)| Row {
            old,
            new: size.to_string(),
            count,
            status: "font_uniform.status",
        })
        .collect();
    plan
}

impl App {
    pub(super) fn begin_font_conversion(&mut self) {
        if self.apply_pending_drafts() {
            self.font_conversion_dialog = Some(Dialog {
                chapter: self.current.clone(),
                scope: Scope::Project,
                error: None,
                target_size: None,
            });
        }
    }

    pub(super) fn begin_font_uniform(&mut self) {
        if self.apply_pending_drafts() {
            self.font_conversion_dialog = Some(Dialog {
                chapter: self.current.clone(),
                scope: Scope::Project,
                error: None,
                target_size: Some(28),
            });
        }
    }

    fn set_uniform_fonts(
        &mut self,
        chapter: &str,
        scope: Scope,
        size: i64,
    ) -> anyhow::Result<usize> {
        anyhow::ensure!((1..=200).contains(&size), "统一字号必须为 1～200 的整数");
        anyhow::ensure!(
            scope == Scope::Project || self.project.stories.contains_key(chapter),
            "所选章节已不存在"
        );
        let plan = uniform_plan(&self.project, chapter, scope, size);
        let count = plan.count();
        if count == 0 {
            return Ok(0);
        }
        self.track();
        self.flush();
        for (id, changes) in plan.changes {
            let story = self.project.stories.get_mut(&id).unwrap();
            for (index, target) in changes {
                story["nodes"][index]["font_size"] = json!(target);
            }
        }
        self.track();
        self.flush();
        self.status = crate::i18n::message(
            "font_uniform.done",
            &[("count", count.to_string()), ("size", size.to_string())],
        );
        Ok(count)
    }

    fn convert_legacy_fonts(&mut self, chapter: &str, scope: Scope) -> anyhow::Result<usize> {
        anyhow::ensure!(
            scope == Scope::Project || self.project.stories.contains_key(chapter),
            "所选章节已不存在"
        );
        let plan = plan(&self.project, chapter, scope);
        let count = plan.count();
        if count == 0 {
            return Ok(0);
        }
        self.track();
        self.flush();
        for (id, changes) in plan.changes {
            let story = self.project.stories.get_mut(&id).unwrap();
            for (index, size) in changes {
                story["nodes"][index]["font_size"] = json!(size);
            }
            let editor = story
                .as_object_mut()
                .unwrap()
                .entry("_editor")
                .or_insert_with(|| json!({}));
            if editor.is_null() {
                *editor = json!({});
            }
            editor[RECORD] = json!({"rule":RULE});
        }
        self.track();
        self.flush();
        self.status = crate::i18n::message("font_conversion.done", &[("count", count.to_string())]);
        Ok(count)
    }

    pub(super) fn font_conversion(&mut self, ctx: &egui::Context) {
        if self.font_conversion_dialog.is_none()
            || self.error.is_some()
            || self.tools_panel.dialog_open()
        {
            return;
        }
        let (_, escape) = self.dialog_keys(ctx);
        let mut dialog = self.font_conversion_dialog.take().unwrap();
        let uniform = dialog.target_size.is_some();
        let mut close = escape;
        let mut submit = false;
        egui::Modal::new(egui::Id::new(("font-batch", uniform)))
            .frame(shell::dialog_frame_for(ctx))
            .show(ctx, |ui| {
                ui.set_width(650.0_f32.min((ctx.content_rect().width() - 60.0).max(260.0)));
                close |= shell::dialog_header(
                    ui,
                    &crate::i18n::key(if uniform {
                        "font_uniform.title"
                    } else {
                        "font_conversion.title"
                    }),
                );
                if let Some(size) = &mut dialog.target_size {
                    ui.label(crate::i18n::key("font_uniform.hint"));
                    ui.horizontal(|ui| {
                        let label = ui.label(crate::i18n::key("font_uniform.size"));
                        ui.add(egui::DragValue::new(size).range(1..=200).speed(1))
                            .labelled_by(label.id);
                    });
                } else {
                    ui.label(crate::i18n::key("font_conversion.rule"));
                    ui.small(crate::i18n::key("font_conversion.defaults"));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::key("video.scope"));
                    ui.selectable_value(
                        &mut dialog.scope,
                        Scope::Chapter,
                        crate::i18n::key("video.scope.chapter"),
                    );
                    ui.selectable_value(
                        &mut dialog.scope,
                        Scope::Project,
                        crate::i18n::key("video.scope.project"),
                    );
                });
                let plan = if let Some(size) = dialog.target_size {
                    uniform_plan(&self.project, &dialog.chapter, dialog.scope, size)
                } else {
                    plan(&self.project, &dialog.chapter, dialog.scope)
                };
                ui.strong(crate::i18n::message(
                    if uniform {
                        "font_uniform.count"
                    } else {
                        "font_conversion.count"
                    },
                    &[
                        ("count", plan.count().to_string()),
                        ("chapters", plan.changes.len().to_string()),
                    ],
                ));
                if plan.processed_chapters > 0 {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        crate::i18n::message(
                            "font_conversion.processed",
                            &[("count", plan.processed_chapters.to_string())],
                        ),
                    );
                }
                egui::ScrollArea::vertical()
                    .id_salt("font-conversion-preview")
                    .max_height((ctx.content_rect().height() * 0.45).max(100.0))
                    .show(ui, |ui| {
                        egui::Grid::new("font-conversion-mappings")
                            .striped(true)
                            .num_columns(4)
                            .show(ui, |ui| {
                                for key in [
                                    "font_conversion.old",
                                    "font_conversion.new",
                                    "font_conversion.quantity",
                                    "font_conversion.action",
                                ] {
                                    ui.strong(crate::i18n::key(key));
                                }
                                ui.end_row();
                                for row in &plan.rows {
                                    ui.add(egui::Label::new(&row.old).truncate())
                                        .on_hover_text(&row.old);
                                    ui.label(&row.new);
                                    ui.label(row.count.to_string());
                                    ui.label(crate::i18n::key(row.status));
                                    ui.end_row();
                                }
                            });
                        if !plan.issues.is_empty() {
                            ui.collapsing(
                                crate::i18n::message(
                                    "font_conversion.issues",
                                    &[("count", plan.issues.len().to_string())],
                                ),
                                |ui| {
                                    for issue in &plan.issues {
                                        ui.label(issue);
                                    }
                                },
                            );
                        }
                    });
                ui.small(crate::i18n::key(if uniform {
                    "font_uniform.undo_hint"
                } else {
                    "font_conversion.record_hint"
                }));
                if let Some(error) = &dialog.error {
                    crate::error_ui::show(ui, error);
                }
                ui.add_space(10.0);
                shell::dialog_buttons(ui, |ui| {
                    close |= ui.button(tr("取消")).clicked();
                    submit = ui
                        .add_enabled(
                            plan.count() > 0,
                            egui::Button::new(crate::i18n::key(if uniform {
                                "font_uniform.apply"
                            } else {
                                "font_conversion.apply"
                            })),
                        )
                        .clicked();
                });
            });
        if close {
            return;
        }
        if submit && self.apply_pending_drafts() {
            let result = if let Some(size) = dialog.target_size {
                self.set_uniform_fonts(&dialog.chapter, dialog.scope, size)
            } else {
                self.convert_legacy_fonts(&dialog.chapter, dialog.scope)
            };
            match result {
                Ok(_) => return,
                Err(error) => dialog.error = Some(crate::error_ui::capture(error)),
            }
        }
        self.font_conversion_dialog = Some(dialog);
    }
}

#[cfg(test)]
#[path = "font_conversion_tests.rs"]
mod tests;
