//! Reuse the native importer list; keep original chapter IDs distinct from map positions.
use super::Names;
use crate::{forms, i18n};
use eframe::egui;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Directory {
    game: PathBuf,
    chapters: Vec<String>,
    attempted: bool,
    loading: bool,
    error: Option<String>,
}
type SharedDirectory = Arc<Mutex<Directory>>;
fn directory(ctx: &egui::Context) -> SharedDirectory {
    let id = egui::Id::new("scenario_original_chapters");
    ctx.data_mut(|data| {
        if let Some(value) = data.get_temp::<SharedDirectory>(id) {
            return value;
        }
        let value = SharedDirectory::default();
        data.insert_temp(id, value.clone());
        value
    })
}
pub(super) fn set_game_directory(ctx: &egui::Context, game: &Path) {
    let shared = directory(ctx);
    let mut data = shared.lock().unwrap();
    if data.game != game {
        *data = Directory {
            game: game.to_owned(),
            ..Default::default()
        };
    }
}
fn read_directory(ctx: &egui::Context, shared: SharedDirectory) {
    let game = {
        let mut data = shared.lock().unwrap();
        if data.loading || data.game.as_os_str().is_empty() {
            return;
        }
        data.loading = true;
        data.attempted = true;
        data.error = None;
        data.game.clone()
    };
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<Vec<String>> {
            let helper = crate::asset_extraction::helper_path()?;
            let mut command = Command::new(helper);
            command.arg("--game-dir").arg(&game).arg("--story-list");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let output = command.output()?;
            let event = String::from_utf8(output.stdout)?
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find(|event| matches!(event["event"].as_str(), Some("result" | "error")))
                .ok_or_else(|| anyhow::anyhow!("{}", String::from_utf8_lossy(&output.stderr)))?;
            anyhow::ensure!(
                output.status.success() && event["event"] == "result",
                "{}",
                event["message"]
                    .as_str()
                    .unwrap_or("Cannot read original chapter directory")
            );
            let chapters = event["chapters"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid original chapter directory"))?;
            chapters
                .iter()
                .map(|value| {
                    let target = value
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Invalid original chapter ID"))?;
                    anyhow::ensure!(
                        lom_core::vanilla_patch::valid_target(target),
                        "Invalid original chapter ID: {target}"
                    );
                    Ok(target.to_owned())
                })
                .collect()
        })();
        let mut data = shared.lock().unwrap();
        if data.game == game {
            data.loading = false;
            match result {
                Ok(chapters) => data.chapters = chapters,
                Err(error) => data.error = Some(format!("{error:#}")),
            }
        }
        ctx.request_repaint();
    });
}
fn label(names: Option<&Names>, id: &str) -> String {
    names
        .and_then(|names| names.originals.get(id))
        .filter(|title| !title.is_empty() && title.as_str() != id)
        .map_or_else(|| id.to_owned(), |title| format!("{title}（{id}）"))
}
fn chapter(
    ui: &mut egui::Ui,
    event: &mut Value,
    key: &str,
    title: &str,
    names: Option<&Names>,
    chapters: &[String],
) {
    ui.push_id(key, |ui| {
        ui.label(i18n::key(title));
        let mut value = event[key].as_str().unwrap_or("").to_owned();
        let before = value.clone();
        ui.add(
            egui::TextEdit::singleline(&mut value)
                .desired_width(ui.available_width().min(340.0))
                .hint_text(i18n::key("common.quick.original_id_hint")),
        );
        if !chapters.is_empty() {
            let search_id = ui.id().with("search");
            let mut search = ui
                .ctx()
                .data(|data| data.get_temp::<String>(search_id))
                .unwrap_or_default();
            let caption = if value.is_empty() {
                i18n::key("common.quick.original_choose")
            } else {
                label(names, &value)
            };
            forms::resource_combo(ui, "directory", &caption, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut search)
                        .hint_text(i18n::key("common.quick.original_search"))
                        .desired_width(ui.available_width()),
                );
                let query = search.to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for id in chapters {
                            let name = label(names, id);
                            if !format!("{id} {name}").to_lowercase().contains(&query) {
                                continue;
                            }
                            if ui
                                .selectable_label(value == *id, name)
                                .on_hover_text(id)
                                .clicked()
                            {
                                value = id.clone();
                            }
                        }
                    });
            });
            ui.ctx()
                .data_mut(|data| data.insert_temp(search_id, search));
        }
        if value != before {
            event[key] = json!(value);
        }
        if !value.is_empty() && !lom_core::vanilla_patch::valid_target(&value) {
            ui.colored_label(
                ui.visuals().error_fg_color,
                i18n::key("common.quick.original_invalid"),
            );
        } else if !chapters.is_empty()
            && !value.is_empty()
            && !chapters.iter().any(|id| id.eq_ignore_ascii_case(&value))
        {
            ui.colored_label(
                ui.visuals().error_fg_color,
                i18n::key("common.quick.original_not_found"),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_picker_labels_reuse_imported_titles_without_rewriting_ids() {
        let stories = std::collections::BTreeMap::from([(
            "imported".into(),
            json!({
                "id":"imported","title":"已导入的原版剧情","vanilla_patch":{"target":"chapter_target"},"nodes":[]
            }),
        )]);
        let names = Names::new(&stories, None);
        assert_eq!(
            label(Some(&names), "chapter_target"),
            "已导入的原版剧情（chapter_target）"
        );
        assert_eq!(label(Some(&names), "another_chapter"), "another_chapter");
    }
    #[test]
    fn original_entry_controls_render_without_mutating_saved_ids_or_starting_game_reads() {
        for width in [260.0, 680.0] {
            let mut event =
                json!({"trigger":"story_entry","target":"chapter_target","from":"chapter_source"});
            let saved = event.clone();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            set_game_directory(&ctx, Path::new(""));
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let limit = ui.max_rect().right();
                        fields(ui, &mut event, None);
                        assert!(
                            ui.min_rect().right() <= limit + 1.0,
                            "original chapter fields overflow at {width}"
                        );
                    });
                },
            );
            assert_eq!(event, saved);
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            for title in [
                "common.quick.original_target",
                "common.quick.original_source",
            ] {
                assert!(
                    nodes
                        .iter()
                        .any(|(_, node)| node.role() == egui::accesskit::Role::Label
                            && node.value() == Some(i18n::key(title).as_str())),
                    "missing {title}"
                );
            }
            assert!(!directory(&ctx).lock().unwrap().attempted);
        }
    }
}
pub(super) fn fields(ui: &mut egui::Ui, event: &mut Value, names: Option<&Names>) {
    let shared = directory(ui.ctx());
    let (chapters, attempted, loading, error, game_empty) = {
        let data = shared.lock().unwrap();
        (
            data.chapters.clone(),
            data.attempted,
            data.loading,
            data.error.clone(),
            data.game.as_os_str().is_empty(),
        )
    };
    if !attempted && !game_empty {
        read_directory(ui.ctx(), shared.clone());
    }
    chapter(
        ui,
        event,
        "target",
        "common.quick.original_target",
        names,
        &chapters,
    );
    let mut from = event.get("from").is_some();
    if ui
        .checkbox(&mut from, i18n::key("common.quick.original_limit_source"))
        .changed()
    {
        if from {
            event["from"] = json!("");
        } else {
            event.as_object_mut().unwrap().remove("from");
        }
    }
    if from {
        chapter(
            ui,
            event,
            "from",
            "common.quick.original_source",
            names,
            &chapters,
        );
    }
    if loading {
        ui.label(i18n::key("common.quick.original_loading"));
    } else if !game_empty
        && ui
            .small_button(i18n::key("common.quick.original_refresh"))
            .clicked()
    {
        read_directory(ui.ctx(), shared);
    }
    if game_empty {
        ui.small(i18n::key("common.quick.original_game_hint"));
    }
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    ui.small(i18n::key("common.quick.original_resume_hint"));
}
