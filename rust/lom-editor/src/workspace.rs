//! Editor state, initialization and frame composition. See workspace/ for each workflow.
mod assets;
mod chapter_navigation;
mod dialogs;
mod history;
mod navigation;
mod preview_panel;
mod project_actions;
mod project_io;
mod properties;
mod recovery;
mod settings;
mod steps;
mod text_edit;
mod tools;

use crate::i18n::tr;
use crate::tools_panel::ToolPage;
use crate::{
    authoring,
    forms::{value_editor, Catalog},
    preview::{self, Preview},
};
use anyhow::Context;
use eframe::egui::{self, Color32, RichText, Vec2};
use lom_core::project::Project;
use project_io::{compile_with_assets, open_project, template_project};
use recovery::recovery_owner_active;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub(crate) use text_edit::retarget;
use text_edit::{
    chapter_group, rename_story_refs, rename_translations, replace_prose, set_chapter_group, short,
    story_reference_count, valid_id,
};

use crate::shell::{self, style, surface, ACCENT};
#[derive(Clone, PartialEq)]
struct Snapshot {
    manifest: Value,
    scenario: Option<Value>,
    stories: BTreeMap<String, Value>,
    paths: BTreeMap<String, PathBuf>,
    assets: std::sync::Arc<BTreeMap<String, Vec<u8>>>,
}
#[derive(Clone)]
enum Pending {
    New,
    NewAddon,
    NewVanillaPatch,
    OpenVanillaRoutes,
    OpenVanillaBookmark(lom_core::story_bookmarks::Bookmark),
    Template(String),
    Recover(PathBuf),
    Open(PathBuf),
    EditorUpdate,
    Quit,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Center {
    Node,
    Chapter,
    Initial,
    Manifest,
    Events,
    Tasks,
    Localization,
    Assets,
    Tools,
    Advanced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Story,
    Materials,
    Game,
    Release,
}
#[derive(Clone, Copy, PartialEq)]
enum View {
    Stage,
    Portraits,
    Graph,
    Lua,
    Checks,
    Statistics,
}
#[derive(Clone)]
enum ChapterAction {
    New,
    Duplicate(Value),
    NewGroup,
    RenameGroup(String),
}
struct ChapterDialog {
    action: ChapterAction,
    name: String,
    error: Option<String>,
    focus_name: bool,
}
struct DeleteChapterDialog {
    chapter: String,
    replacement: String,
    error: Option<String>,
}
#[derive(Clone)]
enum RenameTarget {
    Node { story: String, node: String },
    Chapter(String),
}
struct RenameDialog {
    target: RenameTarget,
    value: String,
    error: Option<String>,
    focus_name: bool,
}
struct AssetImportDialog {
    extends: String,
    kind: usize,
    name: String,
    paths: Vec<PathBuf>,
    error: Option<String>,
}
impl AssetImportDialog {
    fn select_files(&mut self, paths: Vec<PathBuf>) {
        if self.paths.is_empty() || ![1, 8].contains(&self.kind) {
            self.name = paths
                .first()
                .and_then(|path| path.file_stem())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        if [1, 8].contains(&self.kind) {
            for path in paths {
                if !self.paths.contains(&path) {
                    self.paths.push(path);
                }
            }
        } else {
            self.paths = paths;
        }
        self.error = None;
    }
}
struct StepDrag {
    chapter: String,
    indices: Vec<usize>,
}

fn step_edge_scroll(rect: egui::Rect, pointer: egui::Pos2) -> f32 {
    if !rect.contains(pointer) {
        return 0.0;
    }
    let edge = 48.0_f32.min(rect.height() / 3.0);
    if edge <= 0.0 {
        return 0.0;
    }
    if pointer.y < rect.top() + edge {
        -600.0 * (1.0 - (pointer.y - rect.top()) / edge)
    } else if pointer.y > rect.bottom() - edge {
        600.0 * (1.0 - (rect.bottom() - pointer.y) / edge)
    } else {
        0.0
    }
}

struct App {
    project: Project,
    catalog: Catalog,
    preview: Preview,
    current: String,
    selected: usize,
    multiselect: BTreeSet<usize>,
    selection_anchor: Option<(String, usize)>,
    center: Center,
    scenario_selection: crate::scenario_panel::Selection,
    view: View,
    search: String,
    replace: String,
    status: String,
    error: Option<String>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Snapshot,
    saved: Snapshot,
    document_dirty: bool,
    pending_history: Option<Snapshot>,
    last_edit: Instant,
    clipboard: Vec<Value>,
    pending: Option<Pending>,
    allow_close: bool,
    asset_dirty: bool,
    lua: String,
    vanilla: crate::vanilla_panel::Panel,
    flag_reference: crate::vanilla_panel::FlagReference,
    diagnostics: Vec<lom_core::release::PreflightIssue>,
    error_issues: Vec<lom_core::release::PreflightIssue>,
    needs_compile: bool,
    chapter_dialog: Option<ChapterDialog>,
    delete_chapter_dialog: Option<DeleteChapterDialog>,
    rename_dialog: Option<RenameDialog>,
    settings_open: bool,
    settings_page: usize,
    check_updates_on_startup: bool,
    auto_save_project: bool,
    auto_save_seconds: u64,
    last_auto_save: Instant,
    settings_error: Option<String>,
    settings_comma_down: bool,
    dialog_ime: bool,
    recovery_dir: PathBuf,
    recovery_paths: BTreeMap<String, PathBuf>,
    recovery_assets: BTreeSet<String>,
    recovery_candidates: Vec<PathBuf>,
    last_recovery: Instant,
    templates: Value,
    locale: String,
    auto: bool,
    preview_waiting_branch: bool,
    last_step: Instant,
    preview_elapsed: Option<f64>,
    preview_location: (String, usize),
    tools_report: Value,
    screenshot: Option<PathBuf>,
    frame: usize,
    asset_selection: String,
    asset_import_dialog: Option<AssetImportDialog>,
    image_import_target: Option<(String, String)>,
    shell: shell::Shell,
    scroll_selection: (String, usize),
    bulk_key: String,
    bulk_draft: Value,
    template_name: String,
    pending_template: Option<(Vec<Value>, Vec<String>)>,
    section_parent: usize,
    recent: Vec<PathBuf>,
    node_search: String,
    steps_panel_open: bool,
    tools_panel: crate::tools_panel::ToolsPanel,
    advanced: crate::advanced::Advanced,
    content_panel: crate::content_panel::ContentPanel,
    audio: crate::audio::Player,
    updater: crate::editor_update::Updater,
}
fn user_root() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("lom_modkit");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/lom_modkit")
    } else {
        home.join(".local/share/lom_modkit")
    }
}
fn dialog_actions(ui: &mut egui::Ui, primary: &str, enabled: bool) -> (bool, bool) {
    ui.add_space(16.0);
    shell::dialog_buttons(ui, |ui| {
        let submit = ui
            .add_enabled(
                enabled,
                egui::Button::new(RichText::new(tr(primary)).color(Color32::WHITE))
                    .fill(ACCENT)
                    .min_size(Vec2::new(80.0, 30.0)),
            )
            .clicked();
        let cancel = ui
            .add_sized([80.0, 30.0], egui::Button::new(tr("取消")))
            .clicked();
        (submit, cancel)
    })
}
fn snapshot(p: &Project) -> Snapshot {
    Snapshot {
        manifest: p.manifest.clone(),
        scenario: p.scenario.clone(),
        stories: p.stories.clone(),
        paths: p.paths.clone(),
        assets: std::sync::Arc::new(p.assets.clone()),
    }
}
fn repository() -> PathBuf {
    std::env::var_os("LOM_MODKIT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_root().join("rust"))
}

pub fn run() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let update_job = std::env::var_os("LOM_EDITOR_UPDATE_JOB").map(PathBuf::from);
    let updated = update_job.is_some();
    // The installer passes this only to its restarted editor, including older installers.
    std::env::remove_var("LOM_EDITOR_UPDATE_JOB");
    #[cfg(windows)]
    if let Some(job) = update_job {
        if let Err(error) = lom_core::editor_update::finish_job(&job) {
            eprintln!("更新已完成，临时安装文件清理失败：{error:#}");
        }
    }
    if args.first().map(String::as_str) == Some("--smoke-preview") {
        let path = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
            eprintln!("用法：lom-editor --smoke-preview <项目目录/JSON/lommod>");
            std::process::exit(2);
        });
        match smoke(&path) {
            Ok(v) => {
                println!("{}", v);
                return Ok(());
            }
            Err(e) => {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
        }
    }
    let mut open = None;
    let mut screenshot = None;
    let mut initial_chapter = None;
    let mut initial_node = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" => {
                i += 1;
                screenshot = args.get(i).map(PathBuf::from);
            }
            "--chapter" => {
                i += 1;
                initial_chapter = args.get(i).cloned();
            }
            "--node" => {
                i += 1;
                initial_node = args.get(i).cloned();
            }
            s if !s.starts_with('-') => open = Some(PathBuf::from(s)),
            _ => {}
        }
        i += 1;
    }
    let icon =
        image::load_from_memory(include_bytes!("../../../editor/assets/lom_editor_icon.png"))
            .ok()
            .map(|i| {
                let i = i.into_rgba8();
                egui::IconData {
                    width: i.width(),
                    height: i.height(),
                    rgba: i.into_raw(),
                }
            });
    let mut viewport = shell::viewport();
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "活侠传剧情编辑器 · Rust",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = App::new(cc, open, screenshot);
            if let Some(chapter) = initial_chapter.filter(|id| app.project.stories.contains_key(id))
            {
                app.current = chapter;
                app.selected = 0;
            }
            if let Some(node) = initial_node {
                if let Some(index) = app.story()["nodes"]
                    .as_array()
                    .and_then(|nodes| nodes.iter().position(|n| n["id"] == node))
                {
                    app.selected = index;
                }
            }
            if app.screenshot.is_none() {
                app.tools_panel.sync_game_host();
            }
            if updated {
                app.updater.show_completed();
            } else if app.check_updates_on_startup && app.screenshot.is_none() {
                app.updater.check_on_startup(&cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
}
fn smoke(path: &Path) -> anyhow::Result<Value> {
    let p = Project::open(path)?;
    let catalog = Catalog::new();
    let mut nodes = 0;
    let mut previews = 0;
    for (id, story) in &p.stories {
        lom_core::validate::validate_story_with_catalog(
            story,
            &lom_core::validate::catalog_with_assets(&p.assets)?,
        )?;
        let lua = compile_with_assets(&p, story)?;
        anyhow::ensure!(!lua.is_empty(), "空 Lua: {id}");
        for node in story["nodes"].as_array().into_iter().flatten() {
            nodes += 1;
            anyhow::ensure!(
                catalog.schema["NODE_SCHEMAS"][node["type"].as_str().unwrap_or("")].is_object(),
                "节点表单缺失"
            );
            let s = preview::simulate(story, node["id"].as_str().unwrap_or(""));
            anyhow::ensure!(s["actors"].is_object(), "预览状态失败");
            previews += 1;
        }
    }
    Ok(
        json!({"ok":true,"runtime":"native-rust","chapters":p.stories.len(),"nodes":nodes,"preview_states":previews,"schema_node_types":catalog.schema["NODE_SCHEMAS"].as_object().unwrap().len(),"game_tested":false}),
    )
}
impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        open: Option<PathBuf>,
        screenshot: Option<PathBuf>,
    ) -> Self {
        style(&cc.egui_ctx);
        let mut p = Project::new_scenario("sandbox");
        let mut error = None;
        if let Some(path) = open {
            match open_project(&path) {
                Ok(next) => p = next,
                Err(e) => error = Some(format!("无法打开 {}\n{e:#}", path.display())),
            }
        }
        let current = p.manifest["entry"]
            .as_str()
            .filter(|id| p.stories.contains_key(*id))
            .map(str::to_owned)
            .unwrap_or_else(|| p.stories.keys().next().cloned().unwrap_or_default());
        let selected = 0;
        let mut catalog = Catalog::new();
        catalog.sync_assets(&p.assets);
        catalog.sync_story_titles(&p.stories);
        let snap = snapshot(&p);
        let base = user_root().join("rust/recovery");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let recovery_dir = base.join(format!("{}-{stamp}", std::process::id()));
        let recovery_candidates = std::fs::read_dir(&base)
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.join("session.json").is_file() && !recovery_owner_active(p))
                    .collect()
            })
            .unwrap_or_default();
        let mut preview = Preview::new(&repository());
        let prefs = lom_core::load_json(user_root().join("rust/preferences.json"))
            .or_else(|_| lom_core::load_json(user_root().join("settings.json")))
            .unwrap_or(json!({}));
        preview.legacy_character_rotation = prefs["legacy_character_rotation"]
            .as_bool()
            .unwrap_or(false);
        if let Some(locale) = prefs["ui_locale"]
            .as_str()
            .or_else(|| prefs["language"].as_str())
        {
            crate::i18n::set_locale(locale);
        }
        if let Some(path) = prefs["preview_library_dir"].as_str() {
            let _ = preview.set_library(Path::new(path));
        }
        let templates =
            lom_core::load_json(user_root().join("rust/node-templates.json")).unwrap_or(json!({}));
        Self {
            project: p,
            catalog,
            preview,
            current,
            selected,
            multiselect: BTreeSet::new(),
            selection_anchor: None,
            center: Center::Node,
            scenario_selection: Default::default(),
            view: View::Stage,
            search: String::new(),
            replace: String::new(),
            status: String::new(),
            error,
            undo: vec![],
            redo: vec![],
            last: snap.clone(),
            saved: snap,
            document_dirty: false,
            pending_history: None,
            last_edit: Instant::now(),
            clipboard: vec![],
            pending: None,
            allow_close: false,
            asset_dirty: false,
            lua: String::new(),
            vanilla: Default::default(),
            flag_reference: Default::default(),
            diagnostics: vec![],
            error_issues: vec![],
            needs_compile: true,
            chapter_dialog: None,
            delete_chapter_dialog: None,
            rename_dialog: None,
            settings_open: false,
            settings_page: 0,
            check_updates_on_startup: prefs["check_updates_on_startup"].as_bool().unwrap_or(true),
            auto_save_project: prefs["auto_save_project"].as_bool().unwrap_or(true),
            auto_save_seconds: prefs["auto_save_seconds"]
                .as_u64()
                .filter(|seconds| [30, 60, 300, 600].contains(seconds))
                .unwrap_or(300),
            last_auto_save: Instant::now(),
            settings_error: None,
            settings_comma_down: false,
            dialog_ime: false,
            recovery_dir,
            recovery_paths: BTreeMap::new(),
            recovery_assets: BTreeSet::new(),
            recovery_candidates,
            last_recovery: Instant::now(),
            templates,
            locale: "cht".into(),
            auto: false,
            preview_waiting_branch: false,
            last_step: Instant::now(),
            preview_elapsed: None,
            preview_location: (String::new(), usize::MAX),
            tools_report: json!({}),
            screenshot,
            frame: 0,
            asset_selection: String::new(),
            asset_import_dialog: None,
            image_import_target: None,
            shell: shell::Shell::from_preferences(&prefs),
            scroll_selection: (String::new(), usize::MAX),
            bulk_key: String::new(),
            bulk_draft: json!({}),
            template_name: String::new(),
            pending_template: None,
            section_parent: 0,
            recent: lom_core::load_json(user_root().join("rust/recent.json"))
                .ok()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            node_search: String::new(),
            steps_panel_open: true,
            tools_panel: crate::tools_panel::ToolsPanel::default(),
            advanced: crate::advanced::Advanced::default(),
            content_panel: Default::default(),
            audio: Default::default(),
            updater: Default::default(),
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.updater.shutdown();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.shell.clear_color()
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.shell.apply_layout(ctx);
        if self.updater.poll() {
            match self.updater.install(self.project.source.as_deref()) {
                Ok(()) => {
                    self.allow_close = true;
                    self.clear_recovery();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                Err(error) => self.error = Some(format!("更新未完成，编辑器尚未退出。\n{error:#}")),
            }
        }
        self.tools_panel.poll(ctx);
        if self.tools_panel.take_update_check() {
            self.updater.check(ctx);
        }
        if std::mem::take(&mut self.preview.extract_requested) {
            self.tools_panel.start_extraction();
        }
        if let Some(names) = self.preview.effect_extract_requested.take() {
            self.tools_panel.start_effect_capture(Some(names));
        }
        while let Some((path, partial)) = self.tools_panel.take_effect_library() {
            match self
                .preview
                .set_effect_library(&path)
                .and_then(|_| crate::effect_capture::remember(&path))
            {
                Ok(()) => {
                    self.status = tr(if partial {
                        "已载入已完成的特效缓存；本次提取未全部完成。"
                    } else {
                        "原版特效预览已载入"
                    })
                }
                Err(error) => self.tools_panel.present(
                    "载入原版特效预览",
                    json!({"error":crate::error_ui::capture(error)}),
                ),
            }
        }
        if let Some(path) = self.tools_panel.take_preview_library() {
            match self.preview.set_library(&path) {
                Ok(()) => {
                    let preference_path = user_root().join("rust/preferences.json");
                    let result = (|| -> anyhow::Result<()> {
                        let mut prefs = lom_core::load_json(&preference_path).unwrap_or(json!({}));
                        anyhow::ensure!(prefs.is_object(), "偏好设置格式不正确");
                        prefs["preview_library_dir"] = json!(path);
                        lom_core::project::atomic_write(
                            &preference_path,
                            &lom_core::stable_json(&prefs)?,
                        )
                    })();
                    if let Err(error) = result {
                        self.error = Some(format!("素材已载入，但无法记住素材位置：{error:#}"));
                    }
                    self.status = tr("提取完成，预览素材已自动载入");
                }
                Err(error) => self.tools_panel.present(
                    "载入预览素材",
                    json!({"error":crate::error_ui::capture(error)}),
                ),
            }
        }
        self.frame += 1;
        self.shell.install(frame);
        if let Err(error) = self.audio.update() {
            self.error = Some(crate::error_ui::capture(error));
        }
        #[cfg(target_os = "macos")]
        if self.frame == 1 {
            if let Err(error) = crate::macos_window::install_settings_menu(ctx) {
                self.error = Some(error);
            }
        }
        if self.chapter_dialog.is_none()
            && self.delete_chapter_dialog.is_none()
            && self.rename_dialog.is_none()
            && self.asset_import_dialog.is_none()
            && crate::macos_window::take_settings_request()
        {
            self.settings_open = true;
            self.settings_error = None;
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending = Some(Pending::Quit);
            } else {
                self.clear_recovery();
            }
        }
        self.shortcuts(ctx);
        if self.needs_compile
            && (self.frame == 1 || self.last_edit.elapsed() > Duration::from_millis(400))
        {
            self.compile();
        }
        if self.frame == 1 && self.screenshot.is_none() {
            self.remember_source();
        }
        self.sync_preview_selection();
        self.advance_preview();
        self.toolbar(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(surface(false))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr(if self.dirty() {
                            "未保存"
                        } else {
                            "已保存"
                        }))
                        .color(if self.dirty() {
                            ACCENT
                        } else {
                            Color32::from_rgb(104, 114, 125)
                        })
                        .small(),
                    );
                    if !self.status.is_empty() {
                        ui.separator();
                        ui.add(egui::Label::new(RichText::new(&self.status).small()).truncate())
                            .on_hover_text(&self.status);
                    }
                    self.tools_panel.status_ui(ui);
                });
            });
        if self.story_panels_visible() {
            self.sidebar(ctx);
            self.step_palette(ctx);
            self.right(ctx);
        }
        self.center(ctx);
        self.flag_reference
            .show(ctx, self.tools_panel.game_directory());
        self.track();
        self.dialogs(ctx);
        self.tools_panel.show_dialog(ctx, &self.project);
        if let Some(destination) = self.tools_panel.take_location() {
            self.go_to_problem(ctx, destination);
        }
        if let Some(message) =
            ctx.data_mut(|data| data.remove_temp::<String>(egui::Id::new("locate-error-message")))
        {
            crate::error_location::focus_field(ctx, &message);
            self.go_to_problem(ctx, crate::error_location::resolve(&message, &self.project));
        }
        self.autosave();
        self.shell.set_title(
            ctx,
            format!(
                "{}{} · 活侠传剧情编辑器",
                if self.dirty() { "* " } else { "" },
                self.project.manifest["name"].as_str().unwrap_or("新作品")
            ),
        );
        if self.frame == 12 && self.screenshot.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let screenshots = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        if let (Some(path), Some(image)) = (self.screenshot.clone(), screenshots.first()) {
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            match image::save_buffer(
                &path,
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            ) {
                Ok(()) => {
                    println!("GUI screenshot: {}", path.display());
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => {
                    eprintln!("Screenshot failed: {e:#}");
                    self.error = Some(crate::error_ui::capture(e));
                }
            }
            self.screenshot = None;
        }
        if self.auto && !self.preview_waiting_branch {
            // Vsync already paces playback. An extra 16 ms delay can miss the
            // next refresh and halve the frame rate on a 60 Hz display.
            ctx.request_repaint();
        } else {
            let delay = self.idle_repaint_delay();
            if delay != Duration::MAX {
                ctx.request_repaint_after(delay);
            }
        }
    }
}
fn report(ui: &mut egui::Ui, value: &Value, depth: usize) {
    crate::tool_report::show(ui, value, depth);
}
#[cfg(test)]
#[path = "workspace/application_tests.rs"]
mod application_regression_tests;
#[cfg(test)]
#[path = "workspace/unit_tests.rs"]
mod tests;
