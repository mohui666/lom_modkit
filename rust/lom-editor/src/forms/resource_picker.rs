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

const DEFAULT_RECENT_LIMIT: usize = 16;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    browse_preview: bool,
    favorites: BTreeMap<String, BTreeSet<String>>,
    recent: BTreeMap<String, Vec<String>>,
    character_recent_limit: usize,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            browse_preview: false,
            favorites: BTreeMap::new(),
            recent: BTreeMap::new(),
            character_recent_limit: DEFAULT_RECENT_LIMIT,
        }
    }
}

#[derive(Clone, Default)]
struct PreferenceStore {
    preferences: Preferences,
    path: Option<PathBuf>,
    error: Option<String>,
    character_recent_revision: u64,
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

fn update_character_history(ctx: &egui::Context, update: impl FnOnce(&mut Preferences)) {
    update_preferences(ctx, update);
    ctx.data_mut(|data| {
        let store = data.get_temp_mut_or_default::<PreferenceStore>(preferences_id());
        store.character_recent_revision = store.character_recent_revision.wrapping_add(1);
    });
    ctx.request_repaint();
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
        "combat_background" => "view",
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
    character_recent_revision: u64,
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
        if tabs && self.category.as_deref() == Some("character") && self.cursor.tab == 1 {
            let saved = preferences(ui.ctx()).preferences;
            let mut limit = saved.character_recent_limit.clamp(1, 200);
            let previous_limit = limit;
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(crate::i18n::key("editor.picker.character_recent_limit"));
                if ui
                    .add(
                        egui::DragValue::new(&mut limit)
                            .range(1..=200)
                            .speed(1.0)
                            .update_while_editing(false),
                    )
                    .labelled_by(label.id)
                    .changed()
                    && limit != previous_limit
                {
                    update_character_history(ui.ctx(), |preferences| {
                        preferences.character_recent_limit = limit;
                        if let Some(recent) = preferences.recent.get_mut("character") {
                            recent.truncate(limit);
                        }
                    });
                }
                if ui
                    .add_enabled(
                        saved
                            .recent
                            .get("character")
                            .is_some_and(|recent| !recent.is_empty()),
                        egui::Button::new(crate::i18n::key(
                            "editor.picker.clear_recent_characters",
                        )),
                    )
                    .clicked()
                {
                    update_character_history(ui.ctx(), |preferences| {
                        preferences.recent.remove("character");
                    });
                }
            });
        }
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
        let store = preferences(ctx);
        let preferences = store.preferences;
        let history_changed = category == "character"
            && self.cursor.tab == 1
            && self.cursor.character_recent_revision != store.character_recent_revision;
        if self.cursor.order_tab != Some(self.cursor.tab) || history_changed {
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
            self.cursor.character_recent_revision = store.character_recent_revision;
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
            let limit = if category == "character" {
                preferences.character_recent_limit.clamp(1, 200)
            } else {
                DEFAULT_RECENT_LIMIT
            };
            let recent = preferences.recent.entry(category.clone()).or_default();
            recent.retain(|key| key != id);
            recent.insert(0, id.to_owned());
            recent.truncate(limit);
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

    fn history_frame(
        ctx: &egui::Context,
        source: &Value,
        kind: &str,
        tab: u8,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<String>) {
        ctx.enable_accesskit();
        let options = ["one", "two", "three"].map(|id| (id.to_owned(), id.to_owned()));
        let mut visible = Vec::new();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 400.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut picker =
                        Picker::new(ui, source, "character", kind, source["character"].as_str());
                    picker.cursor.tab = tab;
                    picker.controls(ui);
                    visible = picker
                        .options(ctx, &options)
                        .into_iter()
                        .map(|(id, _)| id)
                        .collect();
                    picker.save(ui);
                });
            },
        );
        (output, visible)
    }

    fn limit_target(output: &egui::FullOutput) -> egui::accesskit::NodeId {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(id, node)| node.numeric_value().is_some().then_some(*id))
            .unwrap()
    }

    #[test]
    fn old_character_preferences_keep_sixteen_and_record_without_duplicate_or_arrow_reordering() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resource-pickers.json");
        let legacy = br#"{"browse_preview":false,"favorites":{"character":["favorite"]},"recent":{"character":["one","two"],"music":["original-music"]}}"#;
        std::fs::write(&path, legacy).unwrap();
        let ctx = egui::Context::default();
        configure_resource_preferences(&ctx, path.clone());
        assert_eq!(preferences(&ctx).preferences.character_recent_limit, 16);
        assert_eq!(std::fs::read(&path).unwrap(), legacy);
        let source = json!({"id":"show","type":"show","character":"one"});
        let original = source.clone();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut picker = Picker::new(
                    ui,
                    &source,
                    "character",
                    "character_appearance",
                    Some("one"),
                );
                for index in 0..20 {
                    picker.record(ctx, Some(&format!("actor{index}")));
                }
                picker.record(ctx, Some("actor10"));
                let recent = preferences(ctx).preferences.recent["character"].clone();
                assert_eq!(recent.len(), 16);
                assert_eq!(recent[0], "actor10");
                assert_eq!(recent.iter().filter(|id| *id == "actor10").count(), 1);
                assert_eq!(recent.last().unwrap(), "actor4");
                picker.cursor.tab = 1;
                let options: Vec<_> = (0..20)
                    .map(|index| (format!("actor{index}"), format!("actor{index}")))
                    .collect();
                let before = picker.options(ctx, &options);
                picker.record(ctx, Some("actor4"));
                assert_eq!(
                    picker.options(ctx, &options),
                    before,
                    "Recording a selection must keep open arrow navigation stable"
                );
                assert_eq!(
                    preferences(ctx).preferences.recent["character"][0],
                    "actor4"
                );
                update_character_history(ctx, |preferences| preferences.character_recent_limit = 2);
                let music = Picker::new(ui, &source, "name", "music", None);
                for index in 0..20 {
                    music.record(ctx, Some(&format!("track{index}")));
                }
                assert_eq!(preferences(ctx).preferences.recent["music"].len(), 16);
                picker.record(ctx, Some("actor18"));
                assert_eq!(
                    preferences(ctx).preferences.recent["character"],
                    ["actor18", "actor4"]
                );
            });
        });
        assert_eq!(source, original);
        assert!(preferences(&ctx).preferences.favorites["character"].contains("favorite"));
    }

    #[test]
    fn character_recent_limit_and_clear_buttons_refresh_cached_rows_and_survive_reopening() {
        use egui::accesskit::{Action, ActionData, ActionRequest, Role};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resource-pickers.json");
        let ctx = egui::Context::default();
        configure_resource_preferences(&ctx, path.clone());
        update_preferences(&ctx, |preferences| {
            preferences.browse_preview = true;
            preferences.recent.insert(
                "character".into(),
                vec!["three".into(), "two".into(), "one".into()],
            );
            preferences
                .recent
                .insert("music".into(), vec!["track".into()]);
            preferences
                .favorites
                .insert("character".into(), ["favorite".into()].into());
        });
        let source = json!({"id":"say","type":"say","character":"one","text":"unchanged"});
        let original = source.clone();
        let (output, rows) = history_frame(&ctx, &source, "character", 1, vec![]);
        assert_eq!(rows, ["three", "two", "one"]);
        let event = egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target: limit_target(&output),
            data: Some(ActionData::NumericValue(2.0)),
        });
        let (_, rows) = history_frame(&ctx, &source, "character", 1, vec![event]);
        assert_eq!(
            rows,
            ["three", "two"],
            "The cached recent order must refresh in the same frame"
        );
        let reopened = egui::Context::default();
        configure_resource_preferences(&reopened, path.clone());
        assert_eq!(preferences(&reopened).preferences.character_recent_limit, 2);
        let (output, rows) = history_frame(&reopened, &source, "character", 1, vec![]);
        assert_eq!(rows, ["three", "two"]);
        let label = crate::i18n::key("editor.picker.clear_recent_characters");
        let target = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(id, node)| {
                (node.role() == Role::Button && node.label() == Some(label.as_str())).then_some(*id)
            })
            .unwrap();
        let event = egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Click,
            target,
            data: None,
        });
        assert!(
            history_frame(&reopened, &source, "character", 1, vec![event])
                .1
                .is_empty()
        );
        let again = egui::Context::default();
        configure_resource_preferences(&again, path);
        let saved = preferences(&again).preferences;
        assert!(!saved.recent.contains_key("character"));
        assert_eq!(saved.character_recent_limit, 2);
        assert_eq!(saved.recent["music"], ["track"]);
        assert!(saved.favorites["character"].contains("favorite"));
        assert!(saved.browse_preview);
        assert_eq!(source, original);
        for (kind, tab) in [("character", 0), ("character", 2), ("music", 1)] {
            let (output, _) = history_frame(&again, &source, kind, tab, vec![]);
            assert!(!output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| node.numeric_value().is_some()
                    || node.label() == Some(label.as_str())));
        }
    }

    #[test]
    fn typing_a_larger_character_limit_does_not_truncate_on_intermediate_digits() {
        use egui::accesskit::{Action, ActionRequest};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resource-pickers.json");
        let ctx = egui::Context::default();
        configure_resource_preferences(&ctx, path.clone());
        let recent: Vec<_> = (0..16).map(|index| format!("actor{index}")).collect();
        update_preferences(&ctx, |preferences| {
            preferences
                .recent
                .insert("character".into(), recent.clone());
        });
        let before = std::fs::read(&path).unwrap();
        let source = json!({"id":"show","type":"show","character":"one"});
        let (output, _) = history_frame(&ctx, &source, "character", 1, vec![]);
        let focus = egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target: limit_target(&output),
            data: None,
        });
        history_frame(&ctx, &source, "character", 1, vec![focus]);
        history_frame(
            &ctx,
            &source,
            "character",
            1,
            vec![egui::Event::Text("2".into())],
        );
        assert_eq!(preferences(&ctx).preferences.character_recent_limit, 16);
        assert_eq!(preferences(&ctx).preferences.recent["character"], recent);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        history_frame(
            &ctx,
            &source,
            "character",
            1,
            vec![egui::Event::Text("00".into())],
        );
        assert_eq!(preferences(&ctx).preferences.character_recent_limit, 16);
        history_frame(
            &ctx,
            &source,
            "character",
            1,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(preferences(&ctx).preferences.character_recent_limit, 200);
        assert_eq!(preferences(&ctx).preferences.recent["character"], recent);
        let reopened = egui::Context::default();
        configure_resource_preferences(&reopened, path);
        assert_eq!(
            preferences(&reopened).preferences.character_recent_limit,
            200
        );
        assert_eq!(
            preferences(&reopened).preferences.recent["character"],
            recent
        );
    }

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
