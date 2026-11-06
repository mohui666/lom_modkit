use crate::forms::{self, Catalog};
use crate::i18n::{key, message};
use eframe::egui;
use lom_core::story_bookmarks::{self, Bookmark, Timeline};
use lom_core::story_routes::{RouteGraph, ScriptRoutes};
use lom_core::vanilla_patch::{self, Condition, Edit, Patch, TextSite};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
    sync::mpsc,
};

struct Source {
    text: String,
    sites: Vec<TextSite>,
    translations: BTreeMap<String, String>,
}
impl Source {
    fn display(&self, site: &TextSite) -> String {
        site.key
            .as_ref()
            .and_then(|key| self.translations.get(key))
            .cloned()
            .unwrap_or_else(|| site.original.clone())
    }
}

fn bookmark_caption(mark: &Bookmark) -> String {
    if mark.year <= 0 {
        return mark.script.clone();
    }
    let stage = match mark.stage {
        1 => key("vanilla.stage_1"),
        2 => key("vanilla.stage_2"),
        3 => key("vanilla.stage_3"),
        _ => String::new(),
    };
    message(
        "vanilla.bookmark_caption",
        &[
            ("year", mark.year.to_string()),
            ("month", mark.month.to_string()),
            ("stage", stage),
            ("script", mark.script.clone()),
        ],
    )
}

#[derive(Default)]
pub struct Panel {
    game: PathBuf,
    chapters: Vec<String>,
    sources: BTreeMap<String, Source>,
    attempted: BTreeSet<String>,
    catalog_attempted: bool,
    job: Option<mpsc::Receiver<Result<Value, String>>>,
    job_target: Option<String>,
    error: Option<String>,
    active_target: String,
    chapter_filter: String,
    filter: String,
    selected: usize,
    navigation_loaded: bool,
    bookmarks: Vec<Bookmark>,
    timeline: Option<Timeline>,
    navigation_error: Option<String>,
    navigation_status: Option<String>,
    navigation_filter: String,
    timeline_tab: bool,
    routes: Option<ScriptRoutes>,
    graph: Option<RouteGraph>,
    route_view: crate::route_map::View,
    routes_attempted: bool,
    requested_bookmark: Option<Bookmark>,
    focus: Option<Bookmark>,
    imported: bool,
}

impl Panel {
    pub fn take_imported(&mut self) -> bool {
        std::mem::take(&mut self.imported)
    }
    pub fn open_routes(&mut self) {
        self.navigation_loaded = false;
        self.timeline_tab = true;
    }
    pub fn route_open(&self) -> bool {
        self.timeline_tab
    }
    pub fn focus_bookmark(&mut self, bookmark: Bookmark) {
        self.navigation_loaded = false;
        self.timeline_tab = false;
        if !bookmark.game_dir.as_os_str().is_empty() && self.game != bookmark.game_dir {
            self.game = bookmark.game_dir.clone();
            self.sources.clear();
            self.chapters.clear();
            self.attempted.clear();
            self.catalog_attempted = false;
            self.routes = None;
            self.graph = None;
            self.routes_attempted = false;
            self.job = None;
            self.job_target = None;
            self.active_target.clear();
        }
        self.focus = Some(bookmark);
        self.filter.clear();
    }
    pub fn take_bookmark(&mut self) -> Option<Bookmark> {
        self.requested_bookmark.take()
    }
    fn activate_target(&mut self, target: &str) {
        let target = target.to_ascii_lowercase();
        if self.active_target == target {
            return;
        }
        self.active_target = target.clone();
        self.selected = 0;
        self.filter.clear();
        self.navigation_status = None;
        self.error = None;
        if !self.sources.contains_key(&target) {
            self.attempted.remove(&target);
        }
        if self
            .focus
            .as_ref()
            .is_some_and(|mark| !mark.script.eq_ignore_ascii_case(&target))
        {
            self.focus = None;
        }
    }
    fn load_navigation(&mut self) {
        self.navigation_loaded = true;
        self.navigation_error = None;
        let root = story_bookmarks::default_root();
        match story_bookmarks::read(&root) {
            Ok(marks) => self.bookmarks = marks,
            Err(error) => self.navigation_error = Some(error.to_string()),
        }
        match story_bookmarks::read_timeline(&root) {
            Ok(timeline) => self.update_timeline(timeline),
            Err(error) => self.navigation_error = Some(error.to_string()),
        }
    }
    fn update_timeline(&mut self, timeline: Option<Timeline>) {
        let same_game = self
            .timeline
            .as_ref()
            .zip(timeline.as_ref())
            .is_some_and(|(old, new)| {
                old.game_dir == new.game_dir && old.game_version == new.game_version
            });
        self.timeline = timeline;
        if !same_game {
            self.routes = None;
            self.routes_attempted = false;
        }
        self.graph = self
            .timeline
            .as_ref()
            .zip(self.routes.as_ref())
            .map(|(timeline, routes)| {
                RouteGraph::with_bookmarks(timeline, routes, &self.bookmarks)
            });
    }
    fn navigation(&mut self, ui: &mut egui::Ui) {
        if !self.navigation_loaded {
            self.load_navigation();
        }
        egui::CollapsingHeader::new(key("vanilla.navigation"))
            .id_salt("vanilla-navigation")
            .default_open(false)
            .open(self.timeline_tab.then_some(true))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.timeline_tab, false, key("vanilla.marks"));
                    ui.selectable_value(&mut self.timeline_tab, true, key("vanilla.routes"));
                    if ui
                        .add_enabled(
                            self.job.is_none(),
                            egui::Button::new(key("vanilla.refresh_navigation")),
                        )
                        .clicked()
                    {
                        self.load_navigation();
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.navigation_filter)
                        .desired_width(f32::INFINITY)
                        .hint_text(key("vanilla.navigation_search")),
                );
                let query = self.navigation_filter.to_lowercase();
                if let Some(error) = &self.navigation_error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                if self.timeline_tab {
                    if let (Some(timeline), Some(graph)) = (&self.timeline, &self.graph) {
                        let marks: Vec<_> = self
                            .bookmarks
                            .iter()
                            .filter(|mark| mark.game_dir == timeline.game_dir)
                            .cloned()
                            .collect();
                        if let Some(action) = self.route_view.show(ui, graph, &marks, &query) {
                            self.requested_bookmark = match action {
                                crate::route_map::Action::OpenMark => marks.first().cloned(),
                                crate::route_map::Action::Open(script) => Some(Bookmark {
                                    schema: 1,
                                    script,
                                    text_key: String::new(),
                                    text: String::new(),
                                    game_dir: timeline.game_dir.clone(),
                                    year: 0,
                                    month: 0,
                                    stage: 0,
                                    captured_utc: String::new(),
                                }),
                            };
                        }
                    } else if self.timeline.is_some() {
                        if self.job.is_some() {
                            ui.spinner();
                            ui.label(key("vanilla.reading_routes"));
                        }
                        if let Some(error) = &self.error {
                            ui.colored_label(egui::Color32::RED, error);
                        }
                        if self.job.is_none() && ui.button(key("vanilla.read_routes")).clicked() {
                            self.routes_attempted = false;
                        }
                    } else {
                        ui.label(key("vanilla.routes_need_mark"));
                    }
                } else {
                    ui.small(key("vanilla.marks_hint"));
                    egui::ScrollArea::vertical()
                        .id_salt("original-bookmarks")
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for mark in &self.bookmarks {
                                if !format!("{} {} {}", mark.script, mark.text_key, mark.text)
                                    .to_lowercase()
                                    .contains(&query)
                                {
                                    continue;
                                }
                                let excerpt: String = mark.text.chars().take(56).collect();
                                if ui
                                    .button(format!("{}\n{excerpt}", bookmark_caption(mark)))
                                    .clicked()
                                {
                                    self.requested_bookmark = Some(mark.clone());
                                }
                            }
                            if self.bookmarks.is_empty() {
                                ui.label(key("vanilla.no_marks"));
                            }
                        });
                }
            });
    }
    #[cfg(test)]
    pub fn with_source(target: &str, source: &str) -> Self {
        let mut panel = Self {
            catalog_attempted: true,
            ..Default::default()
        };
        panel.active_target = target.to_ascii_lowercase();
        panel.attempted.insert(target.to_ascii_lowercase());
        panel.sources.insert(
            target.to_ascii_lowercase(),
            Source {
                text: source.into(),
                sites: vanilla_patch::text_sites(source),
                translations: BTreeMap::new(),
            },
        );
        panel
    }
    #[cfg(test)]
    pub fn selected_key(&self, target: &str) -> Option<&str> {
        self.sources
            .get(&target.to_ascii_lowercase())?
            .sites
            .get(self.selected)?
            .key
            .as_deref()
    }
    pub fn next_target(&self, used: &BTreeSet<String>) -> Option<String> {
        self.chapters
            .iter()
            .find(|target| !used.iter().any(|id| id.eq_ignore_ascii_case(target)))
            .cloned()
    }
    fn read(&mut self, ctx: &egui::Context, target: Option<&str>) {
        if self.job.is_some() {
            return;
        }
        if let Some(target) = target {
            self.attempted.insert(target.to_ascii_lowercase());
        } else {
            self.catalog_attempted = true;
        }
        let args = target
            .map(|target| vec!["--story-script".into(), target.into()])
            .unwrap_or_else(|| vec!["--story-list".into()]);
        self.read_request(ctx, self.game.clone(), args, target.map(str::to_owned));
    }
    fn read_routes(&mut self, ctx: &egui::Context) {
        if self.job.is_some() {
            return;
        }
        let Some(timeline) = &self.timeline else {
            return;
        };
        self.routes_attempted = true;
        self.read_request(
            ctx,
            timeline.game_dir.clone(),
            vec!["--story-routes".into()],
            None,
        );
    }
    fn read_request(
        &mut self,
        ctx: &egui::Context,
        game: PathBuf,
        args: Vec<String>,
        target: Option<String>,
    ) {
        self.error = None;
        let helper = match crate::asset_extraction::helper_path() {
            Ok(helper) => helper,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.job_target = target;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Value> {
                let mut command = Command::new(helper);
                command.arg("--game-dir").arg(game).args(args);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let output = command.output()?;
                let event = String::from_utf8(output.stdout)?
                    .lines()
                    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                    .find(|value| matches!(value["event"].as_str(), Some("result" | "error")))
                    .ok_or_else(|| {
                        anyhow::anyhow!(message(
                            "vanilla.error_no_result",
                            &[(
                                "error",
                                String::from_utf8_lossy(&output.stderr).into_owned()
                            )]
                        ))
                    })?;
                anyhow::ensure!(
                    output.status.success() && event["event"] == "result",
                    "{}",
                    event["message"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| key("vanilla.error_read_failed"))
                );
                Ok(event)
            })()
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    fn poll(&mut self) {
        let Some(receiver) = &self.job else { return };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(key("vanilla.error_interrupted").into()),
        };
        self.job = None;
        let requested_target = self.job_target.take();
        match result {
            Err(error) => {
                if requested_target
                    .as_ref()
                    .is_none_or(|target| target.eq_ignore_ascii_case(&self.active_target))
                {
                    self.error = Some(error);
                }
            }
            Ok(value) => {
                if let Some(routes) = value.get("routes") {
                    match serde_json::from_value::<ScriptRoutes>(routes.clone()) {
                        Ok(routes) => {
                            if let Some(timeline) = &self.timeline {
                                self.graph = Some(RouteGraph::with_bookmarks(
                                    timeline,
                                    &routes,
                                    &self.bookmarks,
                                ));
                            }
                            self.routes = Some(routes);
                        }
                        Err(error) => {
                            self.error = Some(message(
                                "vanilla.error_invalid_routes",
                                &[("error", error.to_string())],
                            ))
                        }
                    }
                } else if let Some(chapters) = value["chapters"].as_array() {
                    self.chapters = chapters
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                } else if let (Some(target), Some(source)) =
                    (value["target"].as_str(), value["source"].as_str())
                {
                    if requested_target
                        .as_ref()
                        .is_some_and(|requested| !requested.eq_ignore_ascii_case(target))
                    {
                        self.error = Some(key("vanilla.error_wrong_chapter").into());
                        return;
                    }
                    let translations = value["translations"]
                        .as_object()
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|(key, value)| {
                                    Some((key.clone(), value.as_str()?.to_owned()))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    self.sources.insert(
                        target.to_ascii_lowercase(),
                        Source {
                            text: source.into(),
                            sites: vanilla_patch::text_sites(source),
                            translations,
                        },
                    );
                } else {
                    self.error = Some(key("vanilla.error_incomplete_result").into());
                }
            }
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        story: &mut Value,
        catalog: &Catalog,
        configured_game: PathBuf,
        used: &BTreeSet<String>,
    ) {
        if self.game.as_os_str().is_empty() {
            self.game = configured_game;
        }
        let mut patch: Patch = match serde_json::from_value(story["vanilla_patch"].clone()) {
            Ok(patch) => patch,
            Err(error) => {
                ui.colored_label(egui::Color32::RED, error.to_string());
                return;
            }
        };
        self.activate_target(&patch.target);
        self.poll();
        ui.heading(key("vanilla.title"));
        ui.label(key("vanilla.intro"));
        if let Some(status) = &self.navigation_status {
            ui.label(key(status));
        }
        egui::CollapsingHeader::new(key("vanilla.game_directory"))
            .id_salt("vanilla-game-directory")
            .default_open(self.game.as_os_str().is_empty())
            .show(ui, |ui| {
                if ui
                    .add_enabled(
                        self.job.is_none(),
                        egui::Button::new(key("vanilla.choose_directory")),
                    )
                    .clicked()
                {
                    if let Some(game) = rfd::FileDialog::new().pick_folder() {
                        self.game = game;
                        self.chapters.clear();
                        self.sources.clear();
                        self.attempted.clear();
                        self.active_target.clear();
                        self.catalog_attempted = false;
                        self.routes = None;
                        self.graph = None;
                        self.routes_attempted = false;
                        self.error = None;
                    }
                }
                ui.label(self.game.display().to_string());
            });
        self.navigation(ui);
        if self.timeline_tab {
            if !self.routes_attempted && self.timeline.is_some() {
                self.read_routes(ui.ctx());
            }
            return;
        }
        if !self.game.as_os_str().is_empty() && !self.catalog_attempted {
            self.read(ui.ctx(), None);
        }
        ui.separator();
        ui.strong(key("vanilla.chapter"));
        let display = patch.target.clone();
        ui.add_enabled_ui(patch.edits.is_empty() && self.job.is_none(), |ui| {
            forms::resource_combo(ui, "vanilla-chapters", &display, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.chapter_filter)
                        .desired_width(f32::INFINITY)
                        .hint_text(key("vanilla.chapter_search")),
                );
                for target in &self.chapters {
                    if !target
                        .to_lowercase()
                        .contains(&self.chapter_filter.to_lowercase())
                    {
                        continue;
                    }
                    if ui
                        .add_enabled(
                            patch.target.eq_ignore_ascii_case(target)
                                || !used.iter().any(|id| id.eq_ignore_ascii_case(target)),
                            egui::Button::selectable(
                                patch.target.eq_ignore_ascii_case(target),
                                target,
                            ),
                        )
                        .clicked()
                    {
                        patch.target = target.clone();
                        ui.close();
                    }
                }
            });
        });
        self.activate_target(&patch.target);
        if patch.target != display && story["title"] == format!("原版章节 · {display}") {
            story["title"] = json!(format!("原版章节 · {}", patch.target));
        }
        if !patch.edits.is_empty() {
            ui.small(key("vanilla.chapter_has_edits"));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.job.is_none() && !self.game.as_os_str().is_empty(),
                    egui::Button::new(key("vanilla.reread_chapter")),
                )
                .clicked()
            {
                self.read(ui.ctx(), Some(&patch.target));
            }
            if self.job.is_some() {
                ui.spinner();
                ui.label(key("vanilla.reading_game"));
            }
        });
        if self.job.is_none()
            && !self.game.as_os_str().is_empty()
            && !self.attempted.contains(&patch.target.to_ascii_lowercase())
        {
            self.read(ui.ctx(), Some(&patch.target));
        }
        story["vanilla_patch"] = json!(patch);
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(180, 50, 40), error);
        }
        if self
            .job_target
            .as_ref()
            .is_some_and(|target| target.eq_ignore_ascii_case(&patch.target))
        {
            ui.label(key("vanilla.reading_dialogue"));
            return;
        }
        let Some(source) = self.sources.get(&patch.target.to_ascii_lowercase()) else {
            if self.error.is_none() && self.job.is_none() {
                ui.label(key("vanilla.not_read"));
            }
            return;
        };
        if ui.button(key("vanilla.import_chapter")).clicked() {
            match lom_core::vanilla_import::import_patch(story, &source.text, &source.translations)
            {
                Ok(imported) => {
                    *story = imported;
                    self.imported = true;
                }
                Err(error) => self.error = Some(format!("{error:#}")),
            }
            return;
        }
        ui.small(key("vanilla.import_hint"));
        if let Some(mark) = self
            .focus
            .take()
            .filter(|mark| mark.script.eq_ignore_ascii_case(&patch.target))
        {
            let matches: Vec<_> = source
                .sites
                .iter()
                .enumerate()
                .filter(|(_, site)| {
                    (!mark.text_key.is_empty()
                        && site.key.as_deref() == Some(mark.text_key.as_str()))
                        || (!mark.text.is_empty() && source.display(site) == mark.text)
                })
                .map(|(i, _)| i)
                .collect();
            if let Some(&index) = matches.first() {
                self.selected = index;
                if matches.len() > 1 {
                    self.filter = mark.text_key.clone();
                }
                self.navigation_status = Some(if matches.len() == 1 {
                    "vanilla.located_line".into()
                } else {
                    "vanilla.multiple_matches".into()
                });
            } else {
                self.selected = 0;
                self.navigation_status =
                    Some(if mark.text_key.is_empty() && mark.text.is_empty() {
                        "vanilla.located_chapter".into()
                    } else {
                        "vanilla.text_not_matched".into()
                    });
            }
        }
        self.selected = self.selected.min(source.sites.len().saturating_sub(1));
        ui.separator();
        ui.strong(key("vanilla.find_original"));
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .desired_width(f32::INFINITY)
                .hint_text(key("vanilla.text_search")),
        );
        ui.small(message(
            "vanilla.text_stats",
            &[
                ("count", source.sites.len().to_string()),
                ("edited", patch.edits.len().to_string()),
            ],
        ));
        egui::ScrollArea::vertical()
            .id_salt("vanilla-dialogue-list")
            .max_height((ui.ctx().content_rect().height() * 0.28).clamp(180.0, 320.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut visible = 0;
                for (index, site) in source.sites.iter().enumerate() {
                    let text = source.display(site);
                    if !format!("{text} {}", site.original)
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
                    {
                        continue;
                    }
                    visible += 1;
                    let edited = patch
                        .edits
                        .iter()
                        .any(|edit| edit.find == site.find && edit.occurrence == site.occurrence);
                    let excerpt: String = text.chars().take(54).collect();
                    if ui
                        .add_sized(
                            [ui.available_width(), ui.spacing().interact_size.y],
                            egui::Button::selectable(
                                self.selected == index,
                                (
                                    format!(
                                        "{} {}{}",
                                        if site.kind == "say" {
                                            key("vanilla.dialogue")
                                        } else {
                                            key("vanilla.option")
                                        },
                                        if edited { "● " } else { "" },
                                        excerpt
                                    ),
                                    egui::Atom::grow(),
                                ),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&text)
                        .clicked()
                    {
                        self.selected = index;
                    }
                }
                if visible == 0 && !source.sites.is_empty() {
                    ui.label(key("vanilla.no_matches"));
                    if ui.button(key("vanilla.clear_search")).clicked() {
                        self.filter.clear();
                    }
                }
            });
        let Some(site) = source.sites.get(self.selected) else {
            ui.label(key("vanilla.no_text_sites"));
            return;
        };
        let existing = patch
            .edits
            .iter()
            .find(|edit| edit.find == site.find && edit.occurrence == site.occurrence)
            .cloned();
        let mut edit = existing.clone().unwrap_or_else(|| Edit {
            find: site.find.clone(),
            occurrence: site.occurrence,
            kind: site.kind.into(),
            text: None,
            before: String::new(),
            after: String::new(),
            before_condition: None,
            after_condition: None,
        });
        let original = source.display(site);
        ui.separator();
        ui.strong(key("vanilla.edit_original"));
        ui.label(&original);
        let mut replace = edit.text.is_some();
        if ui
            .checkbox(&mut replace, key("vanilla.replace_original"))
            .changed()
        {
            edit.text = replace.then(|| original.clone());
        }
        if let Some(text) = &mut edit.text {
            ui.add(
                egui::TextEdit::multiline(text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        }
        if site.kind == "say" {
            insertion(
                ui,
                "原句前新增对白",
                "vanilla.insert_before",
                &mut edit.before,
                &mut edit.before_condition,
                catalog,
            );
            insertion(
                ui,
                "原句后新增对白",
                "vanilla.insert_after",
                &mut edit.after,
                &mut edit.after_condition,
                catalog,
            );
            ui.small(key("vanilla.insertion_hint"));
        }
        let restore = ui.button(key("vanilla.restore_edit")).clicked();
        if restore || existing.as_ref() != Some(&edit) {
            patch.edits.retain(|previous| {
                previous.find != site.find || previous.occurrence != site.occurrence
            });
            if !restore
                && (edit.text.is_some()
                    || !edit.before.is_empty()
                    || !edit.after.is_empty()
                    || edit.before_condition.is_some()
                    || edit.after_condition.is_some())
            {
                patch.edits.push(edit);
            }
        }
        let orphaned: Vec<_> = patch
            .edits
            .iter()
            .filter(|edit| {
                !source
                    .sites
                    .iter()
                    .any(|site| site.find == edit.find && site.occurrence == edit.occurrence)
            })
            .cloned()
            .collect();
        for orphan in orphaned {
            ui.colored_label(egui::Color32::RED, key("vanilla.orphan_edit"));
            ui.label(format!(
                "{}\n{}\n{}",
                orphan.before,
                orphan.text.as_deref().unwrap_or(""),
                orphan.after
            ));
            let remove = ui
                .push_id((&orphan.find, orphan.occurrence), |ui| {
                    ui.button(key("vanilla.remove_orphan")).clicked()
                })
                .inner;
            if remove {
                patch.edits.retain(|edit| edit != &orphan);
            }
        }
        story["vanilla_patch"] = json!(patch);
        if let Err(error) = vanilla_patch::parse(story) {
            ui.colored_label(egui::Color32::RED, error.to_string());
        }
    }

    pub fn preview(&self, ui: &mut egui::Ui, story: &Value) {
        ui.heading(key("vanilla.preview_title"));
        ui.label(key("vanilla.preview_hint"));
        let Ok(Some(patch)) = vanilla_patch::parse(story) else {
            return;
        };
        ui.label(message(
            "vanilla.preview_stats",
            &[
                ("target", patch.target.clone()),
                ("count", patch.edits.len().to_string()),
            ],
        ));
        if self
            .job_target
            .as_ref()
            .is_some_and(|target| target.eq_ignore_ascii_case(&patch.target))
        {
            ui.label(key("vanilla.reading_selected"));
            return;
        }
        let Some(source) = self.sources.get(&patch.target.to_ascii_lowercase()) else {
            return;
        };
        if let Some(site) = source.sites.get(self.selected) {
            let edit = patch
                .edits
                .iter()
                .find(|edit| edit.find == site.find && edit.occurrence == site.occurrence);
            if let Some(edit) = edit.filter(|edit| !edit.before.is_empty()) {
                preview_insertion(ui, "vanilla.before", &edit.before, &edit.before_condition);
            }
            ui.separator();
            ui.strong(key("vanilla.original_or_option"));
            ui.label(
                edit.and_then(|edit| edit.text.clone())
                    .unwrap_or_else(|| source.display(site)),
            );
            if let Some(edit) = edit.filter(|edit| !edit.after.is_empty()) {
                preview_insertion(ui, "vanilla.after", &edit.after, &edit.after_condition);
            }
        }
        ui.separator();
        egui::CollapsingHeader::new(key("vanilla.view_lua"))
            .id_salt("vanilla-modified-lua")
            .show(ui, |ui| {
                match vanilla_patch::apply(&source.text, &patch.edits) {
                    Ok(mut text) => {
                        egui::ScrollArea::both().max_height(560.0).show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut text)
                                    .code_editor()
                                    .interactive(false)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::RED, error.to_string());
                    }
                }
            });
    }
}

fn insertion(
    ui: &mut egui::Ui,
    id: &str,
    label_key: &str,
    text: &mut String,
    condition: &mut Option<Condition>,
    catalog: &Catalog,
) {
    ui.push_id(id, |ui| {
        ui.strong(key(label_key));
        ui.add(
            egui::TextEdit::multiline(text)
                .desired_rows(3)
                .hint_text(key("vanilla.empty_insertion"))
                .desired_width(f32::INFINITY),
        );
        let mut enabled = condition.is_some();
        if ui
            .checkbox(&mut enabled, key("vanilla.condition_enabled"))
            .changed()
        {
            *condition = enabled.then(Condition::default);
        }
        if let Some(condition) = condition {
            let mut node = serde_json::to_value(&*condition).unwrap();
            node["type"] = json!("flag_check");
            for (field_key, label_key, kind) in [
                ("source", "vanilla.field_source", "enum:flag_check_source"),
                ("flag", "vanilla.field_flag", "line"),
            ] {
                catalog.field(
                    ui,
                    &mut node,
                    field_key,
                    &key(label_key),
                    kind,
                    false,
                    &[],
                    &[],
                );
            }
            if node["source"] == "flag_value" {
                catalog.field(
                    ui,
                    &mut node,
                    "op",
                    &key("vanilla.field_comparison"),
                    "enum:check_op",
                    false,
                    &[],
                    &[],
                );
                catalog.field(
                    ui,
                    &mut node,
                    "value",
                    &key("vanilla.field_value"),
                    "int",
                    false,
                    &[],
                    &[],
                );
            } else {
                catalog.field(
                    ui,
                    &mut node,
                    "invert",
                    &key("vanilla.field_invert"),
                    "bool",
                    false,
                    &[],
                    &[],
                );
            }
            node.as_object_mut().unwrap().remove("type");
            *condition = serde_json::from_value(node).unwrap();
        }
    });
}
fn preview_insertion(
    ui: &mut egui::Ui,
    label_key: &str,
    text: &str,
    condition: &Option<Condition>,
) {
    ui.separator();
    ui.strong(key(label_key));
    if let Some(condition) = condition {
        ui.small(if condition.source == "flag_value" {
            message(
                "vanilla.when_value",
                &[
                    ("flag", condition.flag.clone()),
                    ("op", condition.op.clone()),
                    ("value", condition.value.to_string()),
                ],
            )
        } else {
            message(
                if condition.invert {
                    "vanilla.when_false"
                } else {
                    "vanilla.when_true"
                },
                &[("flag", condition.flag.clone())],
            )
        });
    } else {
        ui.small(key("vanilla.unconditional"));
    }
    ui.label(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_status_templates_preserve_user_identifiers_and_substitute_once() {
        for locale in crate::i18n::LOCALES {
            let stats = crate::i18n::message_for_locale(
                "vanilla.text_stats",
                &[("count", "21".into()), ("edited", "3".into())],
                locale,
            );
            assert!(stats.contains("21") && stats.contains('3'));
            assert!(!stats.contains("{count}") && !stats.contains("{edited}"));
            let condition = crate::i18n::message_for_locale(
                "vanilla.when_value",
                &[
                    ("flag", "USER_{value}".into()),
                    ("op", ">=".into()),
                    ("value", "7".into()),
                ],
                locale,
            );
            assert!(
                condition.contains("USER_{value}")
                    && condition.contains(">=")
                    && condition.contains('7')
            );
            let located = crate::i18n::message_for_locale("vanilla.located_line", &[], locale);
            assert!(!located.is_empty() && located != "vanilla.located_line");
        }
    }

    #[test]
    fn refreshing_navigation_reuses_routes_for_the_same_game_version() {
        let timeline = Timeline {
            schema: 2,
            game_dir: PathBuf::from("game-a"),
            game_version: "1".into(),
            missions: vec![],
        };
        let mut panel = Panel {
            timeline: Some(timeline.clone()),
            routes: Some(ScriptRoutes { scripts: vec![] }),
            routes_attempted: true,
            ..Default::default()
        };
        panel.update_timeline(Some(timeline.clone()));
        assert!(panel.routes.is_some());
        assert!(panel.graph.is_some());
        assert!(panel.routes_attempted);

        let mut updated = timeline;
        updated.game_version = "2".into();
        panel.update_timeline(Some(updated));
        assert!(panel.routes.is_none());
        assert!(panel.graph.is_none());
        assert!(!panel.routes_attempted);
    }

    #[test]
    fn selecting_another_chapter_clears_previous_search_and_read_error() {
        let mut panel = Panel::with_source("OLD", "say(\"旧章\")");
        panel.filter = "旧章的筛选".into();
        panel.error = Some("旧章读取失败".into());
        panel.selected = 17;
        panel.navigation_status = Some("已定位旧章".into());
        panel.attempted.insert("new".into());

        panel.activate_target("NEW");

        assert_eq!(panel.active_target, "new");
        assert!(panel.filter.is_empty());
        assert!(panel.error.is_none());
        assert!(panel.navigation_status.is_none());
        assert_eq!(panel.selected, 0);
        assert!(!panel.attempted.contains("new"));
        assert!(panel.sources.contains_key("old"));
    }

    #[test]
    fn a_previous_chapter_read_error_does_not_block_the_new_chapter() {
        let mut panel = Panel::default();
        panel.activate_target("new");
        let (sender, receiver) = mpsc::channel();
        panel.job = Some(receiver);
        panel.job_target = Some("old".into());
        sender.send(Err("旧章读取失败".into())).unwrap();

        panel.poll();

        assert!(panel.job.is_none());
        assert!(panel.job_target.is_none());
        assert!(panel.error.is_none());
    }

    #[test]
    fn chapter_search_uses_the_panel_width_and_case_insensitive_source() {
        for width in [400.0, 760.0] {
            let mut panel = Panel::with_source("DEMO", "say(\"第一句\")\nsay(\"第二句\")");
            panel.navigation_loaded = true;
            let mut story = vanilla_patch::new_story("main", "demo").unwrap();
            let catalog = Catalog::new();
            let context = egui::Context::default();
            context.enable_accesskit();
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1200.0),
                    )),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        panel.show(ui, &mut story, &catalog, PathBuf::new(), &BTreeSet::new());
                    });
                },
            );
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            assert!(nodes
                .iter()
                .any(|(_, node)| node.label() == Some("修改这处原文")));
            let search = nodes
                .iter()
                .find(|(_, node)| node.role() == egui::accesskit::Role::TextInput)
                .unwrap();
            let bounds = search.1.bounds().unwrap();
            assert!(bounds.x1 - bounds.x0 > f64::from(width - 40.0));

            panel.timeline_tab = true;
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1200.0),
                    )),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| panel.navigation(ui));
                },
            );
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            let search = nodes
                .iter()
                .find(|(_, node)| node.role() == egui::accesskit::Role::TextInput)
                .unwrap();
            let bounds = search.1.bounds().unwrap();
            assert!(bounds.x1 - bounds.x0 > f64::from(width - 70.0));
        }
    }

    #[test]
    fn refreshing_a_chapter_hides_cached_source_editing_until_the_result_arrives() {
        let mut panel = Panel::with_source("DEMO", "say(\"旧读取结果\")");
        panel.navigation_loaded = true;
        let (_sender, receiver) = mpsc::channel();
        panel.job = Some(receiver);
        panel.job_target = Some("DEMO".into());
        let mut story = vanilla_patch::new_story("main", "DEMO").unwrap();
        let catalog = Catalog::new();
        let context = egui::Context::default();
        context.enable_accesskit();
        let output = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                panel.show(ui, &mut story, &catalog, PathBuf::new(), &BTreeSet::new());
            });
        });
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        assert!(!nodes
            .iter()
            .any(|(_, node)| node.label() == Some("导入整章到步骤编辑器")));
        assert!(!nodes
            .iter()
            .any(|(_, node)| node.label() == Some("修改这处原文")));
    }

    #[test]
    fn selecting_a_chapter_in_the_dropdown_shows_its_dialogue() {
        let mut panel = Panel::with_source("OLD", "say(\"旧章\")");
        panel.sources.insert(
            "new".into(),
            Source {
                text: "say(\"新章\")".into(),
                sites: vanilla_patch::text_sites("say(\"新章\")"),
                translations: BTreeMap::new(),
            },
        );
        panel.chapters = vec!["OLD".into(), "NEW".into()];
        panel.filter = "旧章".into();
        panel.navigation_loaded = true;
        let mut story = vanilla_patch::new_story("main", "OLD").unwrap();
        let catalog = Catalog::new();
        let context = egui::Context::default();
        context.enable_accesskit();
        let draw = |panel: &mut Panel, story: &mut Value, events| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(760.0, 1200.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        panel.show(ui, story, &catalog, PathBuf::new(), &BTreeSet::new());
                    });
                },
            )
        };
        let click = |output: &egui::FullOutput, role, text: &str| {
            let target = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == role
                        && (node.label() == Some(text) || node.value() == Some(text))
                })
                .unwrap()
                .0;
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )]
        };
        let output = draw(&mut panel, &mut story, vec![]);
        let output = draw(
            &mut panel,
            &mut story,
            click(&output, egui::accesskit::Role::ComboBox, "OLD"),
        );
        let output = draw(
            &mut panel,
            &mut story,
            click(&output, egui::accesskit::Role::Button, "NEW"),
        );
        assert_eq!(story["vanilla_patch"]["target"], "NEW");
        assert!(panel.filter.is_empty());
        assert!(output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("对白 新章")));
    }
}
