//! Character recognition and per-page biography authoring.
use crate::{forms::Catalog, i18n};
use eframe::egui;
use serde_json::{json, Value};

fn editable_pages(metadata: &Value) -> Vec<Value> {
    metadata["biography_pages"]
        .as_array()
        .filter(|pages| !pages.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            metadata["biography"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| vec![json!({"text":text,"unlock_level":0})])
                .unwrap_or_default()
        })
}

fn store_pages(metadata: &mut Value, pages: Vec<Value>, had_pages: bool) {
    if !had_pages {
        metadata.as_object_mut().unwrap().remove("biography");
    }
    if pages.is_empty() {
        metadata.as_object_mut().unwrap().remove("biography_pages");
    } else {
        metadata["biography_pages"] = json!(pages);
    }
}

pub fn content(ui: &mut egui::Ui, metadata: &mut Value, catalog: &Catalog) {
    catalog.field(
        ui,
        metadata,
        "initially_known",
        &i18n::key("editor.profile.initial_known"),
        "bool",
        false,
        &[],
        &[],
    );
    ui.small(i18n::key("editor.profile.known_hint"));
    if !metadata["biography_pages"].is_null() && !metadata["biography_pages"].is_array() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            i18n::key("editor.profile.invalid_pages"),
        );
        return;
    }
    let had_pages = metadata["biography_pages"]
        .as_array()
        .is_some_and(|pages| !pages.is_empty());
    if had_pages
        && metadata["biography"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
    {
        ui.small(i18n::key("editor.profile.legacy_conflict"));
        catalog.field(
            ui,
            metadata,
            "biography",
            &i18n::key("editor.profile.legacy_text"),
            "multiline",
            true,
            &[],
            &[],
        );
    }
    let mut pages = editable_pages(metadata);
    let original = pages.clone();
    ui.label(i18n::key("editor.content.biography"));
    ui.small(i18n::key("editor.profile.pages_hint"));
    let mut remove = None;
    for (index, page) in pages.iter_mut().enumerate() {
        ui.push_id(("biography-page", index), |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(i18n::message(
                    "editor.profile.page_title",
                    &[("page", (index + 1).to_string())],
                ));
                if !page.is_object() {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        i18n::key("editor.profile.invalid_pages"),
                    );
                    return;
                }
                ui.label(i18n::key("editor.profile.unlock_level"));
                let current = page["unlock_level"].as_i64();
                egui::ComboBox::from_id_salt("level")
                    .selected_text(current.map_or_else(
                        || i18n::key("editor.profile.choose_level"),
                        |level| level.to_string(),
                    ))
                    .show_ui(ui, |ui| {
                        for level in 0..=10 {
                            if ui
                                .selectable_label(current == Some(level), level.to_string())
                                .clicked()
                            {
                                page["unlock_level"] = json!(level);
                            }
                        }
                    });
                catalog.field(
                    ui,
                    page,
                    "text",
                    &i18n::key("editor.profile.page_text"),
                    "multiline",
                    false,
                    &[],
                    &[],
                );
                if ui
                    .small_button(i18n::key("editor.profile.remove_page"))
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        pages.remove(index);
    }
    if ui
        .add_enabled(
            pages.len() < 4,
            egui::Button::new(i18n::key("editor.profile.add_page")),
        )
        .clicked()
    {
        let level = [0, 4, 7, 10][pages.len()];
        pages.push(json!({"text":"","unlock_level":level}));
    }
    if pages != original {
        store_pages(metadata, pages, had_pages);
    }
}

pub fn operation_field(ui: &mut egui::Ui, node: &mut Value, options: &[(String, String)]) -> bool {
    ui.label(i18n::key("editor.profile.operation"));
    let before = node["op"].as_str().unwrap_or("").to_owned();
    let mut selected = before.clone();
    let caption = options
        .iter()
        .find(|(id, _)| id == &selected)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| selected.clone());
    egui::ComboBox::from_id_salt("profile-operation")
        .selected_text(caption)
        .show_ui(ui, |ui| {
            for (id, label) in options {
                ui.selectable_value(&mut selected, id.clone(), label);
            }
        });
    if selected == before {
        return false;
    }
    node["op"] = json!(selected);
    if matches!(selected.as_str(), "meet" | "forget") {
        node.as_object_mut().unwrap().remove("page");
    }
    true
}

pub fn page_field(ui: &mut egui::Ui, node: &mut Value) -> bool {
    ui.label(i18n::key("editor.profile.page_number"));
    let before = node["page"].clone();
    let current = before.as_i64().filter(|page| (1..=4).contains(page));
    egui::ComboBox::from_id_salt("profile-page")
        .selected_text(current.map_or_else(
            || i18n::key("editor.profile.choose_page"),
            |page| i18n::message("editor.profile.page_title", &[("page", page.to_string())]),
        ))
        .show_ui(ui, |ui| {
            for page in 1..=4 {
                if ui
                    .selectable_label(
                        current == Some(page),
                        i18n::message("editor.profile.page_title", &[("page", page.to_string())]),
                    )
                    .clicked()
                {
                    node["page"] = json!(page);
                }
            }
        });
    before != node["page"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_a_legacy_page_keeps_its_text_and_viewing_does_not_migrate_it() {
        let mut metadata = json!({"initial_affinity":25,"biography":"旧传记。\n第二段。"});
        let original = metadata.clone();
        let mut pages = editable_pages(&metadata);
        assert_eq!(
            pages,
            json!([{"text":"旧传记。\n第二段。","unlock_level":0}])
                .as_array()
                .unwrap()
                .clone()
        );
        assert_eq!(metadata, original);
        pages[0]["unlock_level"] = json!(3);
        store_pages(&mut metadata, pages, false);
        assert!(metadata.get("biography").is_none());
        assert_eq!(
            metadata["biography_pages"][0]["text"],
            original["biography"]
        );
        assert_eq!(metadata["initial_affinity"], 25);
    }

    #[test]
    fn profile_controls_preserve_existing_documents_until_the_author_edits() {
        let context = egui::Context::default();
        let catalog = Catalog::new();
        for mut metadata in [
            json!({"biography":"旧传记"}),
            json!({"biography":"旧传记","biography_pages":[]}),
            json!({"initially_known":false,"biography_pages":[{"text":"第一篇","unlock_level":0},{"text":"第二篇","unlock_level":6}]}),
        ] {
            let original = metadata.clone();
            let _ = context.run(egui::RawInput::default(), |context| {
                egui::CentralPanel::default()
                    .show(context, |ui| content(ui, &mut metadata, &catalog));
            });
            assert_eq!(metadata, original);
        }
    }
}
