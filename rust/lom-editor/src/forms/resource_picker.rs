use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Default, PartialEq, Eq)]
struct PreviewScope {
    story: String,
    node: String,
    action: Option<usize>,
}

#[derive(Clone)]
struct PreviewDraft {
    scope: PreviewScope,
    owner: egui::Id,
    field: String,
    value: Option<String>,
    source: Value,
    preview: Value,
}

fn scope_id() -> egui::Id {
    egui::Id::new("resource-preview-scope")
}

fn draft_id() -> egui::Id {
    egui::Id::new("resource-preview-draft")
}

/// The scope belongs to the selected authoring step, never to the project data.
pub(crate) fn set_preview_scope(ctx: &egui::Context, story_id: &str, node_id: &str) {
    ctx.data_mut(|data| {
        let next = PreviewScope {
            story: story_id.to_owned(),
            node: node_id.to_owned(),
            action: None,
        };
        let old = data.get_temp::<PreviewScope>(scope_id());
        if old
            .as_ref()
            .is_none_or(|old| old.story != next.story || old.node != next.node)
        {
            data.remove::<PreviewDraft>(draft_id());
        }
        data.insert_temp(scope_id(), next);
    });
}

pub(crate) fn set_preview_action(ctx: &egui::Context, action: Option<usize>) {
    ctx.data_mut(|data| {
        if let Some(mut scope) = data.get_temp::<PreviewScope>(scope_id()) {
            scope.action = action;
            data.insert_temp(scope_id(), scope);
        }
    });
}

pub(crate) fn clear_preview_draft(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        if let Some(draft) = data.get_temp::<PreviewDraft>(draft_id()) {
            data.remove::<Cursor>(draft.owner);
        }
        data.remove::<PreviewDraft>(draft_id());
    });
}

/// Only a pending browse selection needs a copy. Idle previews borrow the source.
pub(crate) fn preview_story<'a>(
    ctx: &egui::Context,
    story: &'a Value,
) -> std::borrow::Cow<'a, Value> {
    let draft = ctx.data(|data| data.get_temp::<PreviewDraft>(draft_id()));
    let Some(draft) = draft else {
        return std::borrow::Cow::Borrowed(story);
    };
    if story["id"].as_str() != Some(draft.scope.story.as_str()) {
        return std::borrow::Cow::Borrowed(story);
    }
    let mut preview = story.clone();
    let Some(node) = preview["nodes"]
        .as_array_mut()
        .and_then(|nodes| nodes.iter_mut().find(|node| node["id"] == draft.scope.node))
    else {
        clear_preview_draft(ctx);
        return std::borrow::Cow::Borrowed(story);
    };
    let target = if let Some(index) = draft.scope.action {
        node["actions"]
            .as_array_mut()
            .and_then(|actions| actions.get_mut(index))
    } else {
        Some(node)
    };
    let Some(target) = target else {
        clear_preview_draft(ctx);
        return std::borrow::Cow::Borrowed(story);
    };
    if *target != draft.source {
        // Any real edit invalidates a pending browse selection.
        clear_preview_draft(ctx);
        return std::borrow::Cow::Borrowed(story);
    }
    *target = draft.preview;
    std::borrow::Cow::Owned(preview)
}

fn set_draft(ctx: &egui::Context, owner: egui::Id, key: &str, source: &Value, value: Option<&str>) {
    let Some(scope) = ctx.data(|data| data.get_temp::<PreviewScope>(scope_id())) else {
        return;
    };
    let mut preview = source.clone();
    if let Some(value) = value {
        preview[key] = json!(value);
    } else if let Some(object) = preview.as_object_mut() {
        object.remove(key);
    }
    ctx.data_mut(|data| {
        data.insert_temp(
            draft_id(),
            PreviewDraft {
                scope,
                owner,
                field: key.to_owned(),
                value: value.map(str::to_owned),
                source: source.clone(),
                preview,
            },
        )
    });
    ctx.request_repaint();
}

pub(super) fn normalize_character_draft(catalog: &Catalog, ctx: &egui::Context, source: &Value) {
    let Some(mut draft) = ctx.data(|data| data.get_temp::<PreviewDraft>(draft_id())) else {
        return;
    };
    let scope = ctx.data(|data| data.get_temp::<PreviewScope>(scope_id()));
    if scope.as_ref() != Some(&draft.scope)
        || !matches!(draft.field.as_str(), "character" | "selected_appearance")
    {
        return;
    }
    let selection = draft.value.clone();
    let mut preview = source.clone();
    if let Some(selection) = selection {
        catalog.select_appearance(&mut preview, &selection);
    } else if let Some(object) = preview.as_object_mut() {
        object.remove("character");
    }
    draft.source = source.clone();
    draft.preview = preview;
    ctx.data_mut(|data| data.insert_temp(draft_id(), draft));
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    browse_preview: bool,
    favorites: BTreeMap<String, BTreeSet<String>>,
    recent: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Default)]
struct PreferenceStore {
    preferences: Preferences,
    path: Option<PathBuf>,
    error: Option<String>,
}

fn preferences_id() -> egui::Id {
    egui::Id::new("resource-picker-preferences")
}

pub(crate) fn configure_resource_preferences(ctx: &egui::Context, path: PathBuf) {
    if ctx
        .data(|data| data.get_temp::<PreferenceStore>(preferences_id()))
        .is_some()
    {
        return;
    }
    let mut store = PreferenceStore {
        path: Some(path.clone()),
        ..Default::default()
    };
    if path.is_file() {
        match std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<Preferences>(&bytes).map_err(|error| error.to_string())
            }) {
            Ok(preferences) => store.preferences = preferences,
            Err(error) => store.error = Some(error),
        }
    }
    ctx.data_mut(|data| data.insert_temp(preferences_id(), store));
}

fn preferences(ctx: &egui::Context) -> PreferenceStore {
    ctx.data(|data| data.get_temp::<PreferenceStore>(preferences_id()))
        .unwrap_or_default()
}

fn update_preferences(ctx: &egui::Context, update: impl FnOnce(&mut Preferences)) {
    let mut store = preferences(ctx);
    update(&mut store.preferences);
    if let Some(path) = &store.path {
        let result = serde_json::to_vec_pretty(&store.preferences)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                lom_core::project::atomic_write(path, &bytes).map_err(|error| error.to_string())
            });
        store.error = result.err();
    }
    ctx.data_mut(|data| data.insert_temp(preferences_id(), store));
}

pub(super) fn resource_category(kind: &str, node: &Value) -> Option<String> {
    let category = match kind {
        kind if is_character_selector(kind) || kind == "character_appearance" => "character",
        "portrait" => {
            return Some(format!(
                "portrait:{}",
                node["character"].as_str().unwrap_or("")
            ))
        }
        "official_picture" | "story_picture" | "event_picture" | "big_picture" => "picture",
        "user_image" | "ending_image" | "intro_image" => "user_image",
        "view" | "music" | "sound_name" | "voice" | "effect" | "camera" | "item"
        | "combat_skill" | "battle_skill" | "talent" => kind,
        _ => return None,
    };
    Some(category.to_owned())
}

#[derive(Clone, Default)]
struct Cursor {
    source: Option<String>,
    node: Value,
    scope: Option<PreviewScope>,
    candidate: Option<Option<String>>,
    tab: u8,
    order: Vec<String>,
    order_tab: Option<u8>,
    manual_id: Option<egui::Id>,
}

pub(super) struct Picker<'a> {
    id: egui::Id,
    key: &'a str,
    category: Option<String>,
    source: &'a Value,
    cursor: Cursor,
    browse: bool,
}

impl<'a> Picker<'a> {
    pub(super) fn new(
        ui: &Ui,
        node: &'a Value,
        key: &'a str,
        kind: &str,
        selected: Option<&str>,
    ) -> Self {
        let id = ui.id().with("resource-browser");
        Self::with_id(ui, node, key, kind, selected, id)
    }

    fn with_id(
        ui: &Ui,
        node: &'a Value,
        key: &'a str,
        kind: &str,
        selected: Option<&str>,
        id: egui::Id,
    ) -> Self {
        let category = resource_category(kind, node);
        let mut cursor = ui
            .data_mut(|data| data.get_temp::<Cursor>(id))
            .unwrap_or_default();
        let scope = ui
            .ctx()
            .data(|data| data.get_temp::<PreviewScope>(scope_id()));
        if cursor.scope != scope {
            cursor.order.clear();
            cursor.order_tab = None;
        }
        if cursor.source.as_deref() != selected || cursor.scope != scope || cursor.node != *node {
            let owns_draft = ui
                .ctx()
                .data(|data| data.get_temp::<PreviewDraft>(draft_id()))
                .is_some_and(|draft| draft.owner == id);
            if owns_draft {
                clear_preview_draft(ui.ctx());
            }
            cursor.source = selected.map(str::to_owned);
            cursor.node = node.clone();
            cursor.scope = scope;
            cursor.candidate = None;
        }
        let browse = category.is_some() && preferences(ui.ctx()).preferences.browse_preview;
        Self {
            id,
            key,
            category,
            source: node,
            cursor,
            browse,
        }
    }

    pub(super) fn current<'b>(&'b self, selected: Option<&'b str>) -> Option<&'b str> {
        if self.browse {
            if let Some(candidate) = &self.cursor.candidate {
                return candidate.as_deref();
            }
        }
        selected
    }

    pub(super) fn browse(&self) -> bool {
        self.browse
    }

    pub(super) fn controls(&mut self, ui: &mut Ui) {
        self.control_ui(ui, true);
    }

    fn control_ui(&mut self, ui: &mut Ui, tabs: bool) {
        if self.category.is_none() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let mut browse = self.browse;
            if ui
                .checkbox(&mut browse, crate::i18n::key("editor.picker.browse"))
                .on_hover_text(crate::i18n::key("editor.picker.browse_hint"))
                .changed()
            {
                self.browse = browse;
                self.cancel(ui.ctx());
                update_preferences(ui.ctx(), |preferences| preferences.browse_preview = browse);
            }
            if tabs {
                for (tab, label) in [
                    (0, "editor.picker.all"),
                    (1, "editor.picker.recent"),
                    (2, "editor.picker.favorites"),
                ] {
                    ui.selectable_value(&mut self.cursor.tab, tab, crate::i18n::key(label));
                }
            }
        });
        if let Some(error) = preferences(ui.ctx()).error {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "{}: {error}",
                    crate::i18n::key("editor.picker.preference_error")
                ),
            );
        }
    }

    pub(super) fn options(
        &mut self,
        ctx: &egui::Context,
        options: &[(String, String)],
    ) -> Vec<(String, String)> {
        let Some(category) = &self.category else {
            return options.to_vec();
        };
        let preferences = preferences(ctx).preferences;
        if self.cursor.order_tab != Some(self.cursor.tab) {
            self.cursor.order = match self.cursor.tab {
                1 => preferences
                    .recent
                    .get(category)
                    .cloned()
                    .unwrap_or_default(),
                2 => options
                    .iter()
                    .filter(|(id, _)| {
                        preferences
                            .favorites
                            .get(category)
                            .is_some_and(|ids| ids.contains(id))
                    })
                    .map(|(id, _)| id.clone())
                    .collect(),
                _ => Vec::new(),
            };
            self.cursor.order_tab = Some(self.cursor.tab);
        }
        if self.cursor.tab == 0 {
            options.to_vec()
        } else {
            self.cursor
                .order
                .iter()
                .filter_map(|id| options.iter().find(|(key, _)| key == id).cloned())
                .collect()
        }
    }

    /// Returns true only for a direct, committed selection.
    pub(super) fn choose(&mut self, ui: &Ui, candidate: Option<String>) -> bool {
        if self.browse {
            set_draft(
                ui.ctx(),
                self.id,
                self.key,
                self.source,
                candidate.as_deref(),
            );
            self.cursor.candidate = Some(candidate);
            false
        } else {
            self.record(ui.ctx(), candidate.as_deref());
            self.cancel(ui.ctx());
            true
        }
    }

    fn record(&self, ctx: &egui::Context, id: Option<&str>) {
        let (Some(category), Some(id)) = (&self.category, id) else {
            return;
        };
        if id.is_empty() {
            return;
        }
        update_preferences(ctx, |preferences| {
            let recent = preferences.recent.entry(category.clone()).or_default();
            recent.retain(|key| key != id);
            recent.insert(0, id.to_owned());
            recent.truncate(16);
        });
    }

    pub(super) fn row(
        &mut self,
        ui: &mut Ui,
        id: &str,
        name: &str,
        selected: bool,
    ) -> egui::Response {
        if let Some(category) = &self.category {
            ui.horizontal(|ui| {
                let favorite = preferences(ui.ctx())
                    .preferences
                    .favorites
                    .get(category)
                    .is_some_and(|ids| ids.contains(id));
                let star = ui
                    .small_button(if favorite { "★" } else { "☆" })
                    .on_hover_text(crate::i18n::key(if favorite {
                        "editor.picker.unfavorite"
                    } else {
                        "editor.picker.favorite"
                    }));
                if star.clicked() {
                    update_preferences(ui.ctx(), |preferences| {
                        let ids = preferences.favorites.entry(category.clone()).or_default();
                        if !ids.remove(id) {
                            ids.insert(id.to_owned());
                        }
                    });
                }
                ui.add_sized(
                    [ui.available_width(), ui.spacing().interact_size.y],
                    egui::Button::selectable(selected, name).truncate(),
                )
                .on_hover_text(format!("{name}\n{id}"))
            })
            .inner
        } else {
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                egui::Button::selectable(selected, name).truncate(),
            )
            .on_hover_text(format!("{name}\n{id}"))
        }
    }

    pub(super) fn actions(&mut self, ui: &mut Ui, search_id: egui::Id) -> Option<Option<String>> {
        if !self.browse {
            return None;
        }
        let mut apply = key_for_picker(ui, search_id, self.cursor.manual_id, egui::Key::Enter);
        let mut cancel = key_for_picker(ui, search_id, self.cursor.manual_id, egui::Key::Escape);
        ui.horizontal_wrapped(|ui| {
            apply |= ui
                .add_enabled(
                    self.cursor.candidate.is_some(),
                    egui::Button::new(crate::i18n::key("editor.picker.apply")),
                )
                .on_hover_text(crate::i18n::key("editor.picker.preview_keys"))
                .clicked();
            cancel |= ui
                .button(crate::i18n::key("editor.picker.cancel"))
                .clicked();
            ui.small(crate::i18n::key("editor.picker.preview_only"))
                .on_hover_text(crate::i18n::key("editor.picker.preview_keys"));
        });
        if cancel {
            self.cancel(ui.ctx());
        } else if apply {
            if let Some(choice) = self.cursor.candidate.take() {
                self.record(ui.ctx(), choice.as_deref());
                self.cancel(ui.ctx());
                return Some(choice);
            }
        }
        None
    }

    pub(super) fn cancel(&mut self, ctx: &egui::Context) {
        self.cursor.candidate = None;
        let is_owner = ctx
            .data(|data| data.get_temp::<PreviewDraft>(draft_id()))
            .is_some_and(|draft| draft.owner == self.id);
        if is_owner {
            clear_preview_draft(ctx);
        }
    }

    pub(super) fn save(&self, ui: &Ui) {
        ui.data_mut(|data| data.insert_temp(self.id, self.cursor.clone()));
    }

    pub(super) fn close(&mut self, ctx: &egui::Context) {
        self.cancel(ctx);
        self.cursor.order_tab = None;
        self.cursor.order.clear();
    }

    pub(super) fn manual_input(&mut self, id: egui::Id) {
        self.cursor.manual_id = Some(id);
    }
}

fn key_for_picker(
    ui: &mut Ui,
    search_id: egui::Id,
    manual_id: Option<egui::Id>,
    key: egui::Key,
) -> bool {
    let editing_other_text = ui.memory(|memory| memory.focused()).is_some_and(|id| {
        id != search_id
            && Some(id) != manual_id
            && egui::TextEdit::load_state(ui.ctx(), id).is_some()
    });
    !editing_other_text && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key))
}

/// Reuse browse/apply behavior in thumbnail browsers. Only the returned choice
/// should be written to the project; the clicked candidate can be preview-only.
pub(crate) fn resource_choice(
    ui: &mut Ui,
    node: &Value,
    key: &str,
    kind: &str,
    choice: Option<String>,
) -> Option<Option<String>> {
    let mut picker = Picker::new(ui, node, key, kind, node[key].as_str());
    picker.control_ui(ui, false);
    let mut selection = None;
    if let Some(choice) = choice {
        if picker.choose(ui, Some(choice.clone())) {
            selection = Some(Some(choice));
        }
    }
    if let Some(choice) = picker.actions(ui, ui.id().with("search")) {
        selection = Some(choice);
    }
    picker.save(ui);
    selection
}

#[cfg(test)]
pub(super) fn browse_for_test(ctx: &egui::Context) {
    update_preferences(ctx, |preferences| preferences.browse_preview = true);
}

pub(super) fn generic_selection(
    ui: &mut Ui,
    node: &Value,
    key: &str,
    kind: &str,
    selected: Option<&str>,
    options: &[(String, String)],
    absent: Option<&str>,
    owner: egui::Id,
) -> Option<Option<String>> {
    let mut picker = Picker::with_id(ui, node, key, kind, selected, owner);
    picker.controls(ui);
    let options = picker.options(ui.ctx(), options);
    let search_id = ui.id().with("search");
    let direction = resource_arrow_direction(ui, search_id);
    let mut search = ui
        .data_mut(|data| data.get_temp::<String>(search_id))
        .unwrap_or_default();
    let search_changed = !options.is_empty()
        && ui
            .add(
                egui::TextEdit::singleline(&mut search)
                    .id(search_id)
                    .desired_width(f32::INFINITY)
                    .hint_text(crate::i18n::key("editor.form.search_choices")),
            )
            .changed();
    let mut selection = None;
    let current = picker.current(selected).unwrap_or("").to_owned();
    let keyboard_choice = adjacent_resource(&options, &current, &search.to_lowercase(), direction);
    if let Some(choice) = &keyboard_choice {
        if picker.choose(ui, Some(choice.clone())) && selected != Some(choice.as_str()) {
            selection = Some(Some(choice.clone()));
        }
    }
    // Applying a preview must stay reachable while the candidate list scrolls.
    if let Some(applied) = picker.actions(ui, search_id) {
        selection = Some(applied);
        ui.close();
    }
    if !picker.browse() {
        ui.small(crate::i18n::key("editor.form.resource_keys"));
    }
    let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
    if search_changed {
        scroll = scroll.vertical_scroll_offset(0.0);
    }
    scroll.show(ui, |ui| {
        if let Some(label) = absent {
            if ui
                .selectable_label(picker.current(selected).is_none(), label)
                .clicked()
                && picker.choose(ui, None)
            {
                selection = Some(None);
                ui.close();
            }
            ui.separator();
        }
        for (id, name) in &options {
            if resource_matches(id, name, &search.to_lowercase()) {
                let active = picker.current(selected) == Some(id.as_str());
                let response = picker.row(ui, id, name, active);
                if keyboard_choice.as_ref() == Some(id) {
                    response.request_focus();
                    response.scroll_to_me(Some(egui::Align::Center));
                }
                if response.clicked() && picker.choose(ui, Some(id.clone())) {
                    selection = Some(Some(id.clone()));
                    ui.close();
                }
            }
        }
        ui.collapsing(tr("手动输入"), |ui| {
            let manual_id = ui.id().with("resource-manual");
            let mut manual = if picker.browse() {
                ui.data_mut(|data| data.get_temp::<String>(manual_id))
                    .unwrap_or_else(|| current.clone())
            } else {
                selected.unwrap_or("").to_owned()
            };
            let response = ui
                .add(egui::TextEdit::singleline(&mut manual).hint_text(tr("内部 ID / 自定义引用")));
            picker.manual_input(response.id);
            if response.changed() {
                let choice = if manual.is_empty() && absent.is_some() {
                    None
                } else {
                    Some(manual.clone())
                };
                if picker.choose(ui, choice.clone()) {
                    selection = Some(choice);
                }
            }
            ui.data_mut(|data| data.insert_temp(manual_id, manual));
        });
    });
    picker.save(ui);
    ui.data_mut(|data| data.insert_temp(search_id, search));
    selection
}

pub(super) fn cancel_generic(ui: &Ui, owner: egui::Id) {
    let field = ui
        .ctx()
        .data(|data| data.get_temp::<PreviewDraft>(draft_id()));
    ui.ctx().data_mut(|data| data.remove::<Cursor>(owner));
    if field.is_some_and(|draft| draft.owner == owner) {
        clear_preview_draft(ui.ctx());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_preferences_reload_and_hide_unavailable_project_resources() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resource-pickers.json");
        let context = egui::Context::default();
        configure_resource_preferences(&context, path.clone());
        update_preferences(&context, |preferences| {
            preferences.favorites.insert(
                "picture".into(),
                ["user:current".into(), "user:other-project".into()].into(),
            );
            preferences.recent.insert(
                "picture".into(),
                vec![
                    "user:other-project".into(),
                    "user:current".into(),
                    "official".into(),
                ],
            );
        });
        let reopened = egui::Context::default();
        configure_resource_preferences(&reopened, path);
        let options = vec![
            ("official".to_owned(), "原版".to_owned()),
            ("user:current".to_owned(), "本作品".to_owned()),
        ];
        let source = json!({"type":"cg","kind":"item","key":"official"});
        let _ = reopened.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut picker = Picker::new(ui, &source, "key", "story_picture", Some("official"));
                picker.cursor.tab = 1;
                let recent = picker.options(ctx, &options);
                assert_eq!(
                    recent.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
                    ["user:current", "official"]
                );
                assert!(picker.choose(ui, Some("official".into())));
                assert_eq!(
                    picker.options(ctx, &options),
                    recent,
                    "Selecting must not reorder arrow-key navigation while open"
                );
                picker.cursor.tab = 2;
                assert_eq!(picker.options(ctx, &options), vec![options[1].clone()]);
            });
        });
        assert_eq!(
            preferences(&reopened).preferences.recent["picture"][0],
            "official"
        );
    }

    #[test]
    fn resource_preview_draft_expires_after_source_edit_or_step_change() {
        let context = egui::Context::default();
        set_preview_scope(&context, "main", "one");
        let source = json!({"id":"one","type":"show","character":"player","portrait":"normal"});
        let story = json!({"id":"main","start":"one","nodes":[source.clone()]});
        set_draft(
            &context,
            egui::Id::new("test"),
            "portrait",
            &source,
            Some("angry1"),
        );
        assert_eq!(
            preview_story(&context, &story)["nodes"][0]["portrait"],
            "angry1"
        );
        let mut edited = story.clone();
        edited["nodes"][0]["portrait"] = json!("laugh1");
        assert_eq!(preview_story(&context, &edited).as_ref(), &edited);
        assert_eq!(
            preview_story(&context, &story).as_ref(),
            &story,
            "An expired selection must not return later"
        );
        set_draft(
            &context,
            egui::Id::new("test"),
            "portrait",
            &source,
            Some("angry1"),
        );
        set_preview_scope(&context, "main", "two");
        assert_eq!(preview_story(&context, &story).as_ref(), &story);
    }
}
