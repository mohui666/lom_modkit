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
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
struct App {
    project: Project,
    catalog: Catalog,
    preview: Preview,
    current: String,
    selected: usize,
    multiselect: BTreeSet<usize>,
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
            match Project::open(&path) {
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
    fn remember_source(&mut self) {
        if let Some(path) = self.project.source.clone() {
            if path.starts_with(user_root().join("rust/recovery")) {
                return;
            }
            self.recent.retain(|p| p != &path);
            self.recent.insert(0, path);
            self.recent.truncate(12);
            if let Ok(bytes) = lom_core::stable_json(&json!(self.recent)) {
                if let Err(e) =
                    lom_core::project::atomic_write(&user_root().join("rust/recent.json"), &bytes)
                {
                    self.status = format!(
                        "最近列表保存失败。{}",
                        crate::error_ui::text(&crate::error_ui::capture(e))
                    );
                }
            }
        }
    }
    fn persist_templates(&mut self) {
        match lom_core::stable_json(&self.templates).and_then(|b| {
            lom_core::project::atomic_write(&user_root().join("rust/node-templates.json"), &b)
        }) {
            Ok(()) => self.status = "节点模板已保存到本机创作数据目录".into(),
            Err(e) => self.error = Some(crate::error_ui::capture(e)),
        }
    }
    fn save_selected_template(&mut self) {
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
    fn dirty(&self) -> bool {
        self.document_dirty
            || self.asset_dirty
            || self.content_panel.has_pending()
            || self.advanced.has_pending()
    }
    fn mark_saved(&mut self) {
        self.saved = self.last.clone();
        self.document_dirty = false;
    }
    fn is_vanilla_patch(&self) -> bool {
        self.project
            .stories
            .get(&self.current)
            .is_some_and(|story| story.get("vanilla_patch").is_some())
    }
    fn story(&self) -> Value {
        self.project
            .stories
            .get(&self.current)
            .cloned()
            .unwrap_or(json!({"nodes":[]}))
    }
    fn begin_chapter(&mut self, duplicate: bool) {
        if !self.apply_pending_drafts() {
            return;
        }
        let (action, name) = if duplicate {
            let story = self.story();
            let name = format!(
                "{}{}",
                story["title"].as_str().unwrap_or(&self.current),
                tr("的副本")
            );
            (ChapterAction::Duplicate(story), name)
        } else {
            (ChapterAction::New, tr("新章节"))
        };
        self.chapter_dialog = Some(ChapterDialog {
            action,
            name,
            error: None,
            focus_name: true,
        });
    }
    fn commit_chapter(&mut self) -> anyhow::Result<()> {
        let draft = self
            .chapter_dialog
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("没有待创建的章节"))?;
        let name = draft.name.trim().to_owned();
        if matches!(
            draft.action,
            ChapterAction::NewGroup | ChapterAction::RenameGroup(_)
        ) {
            anyhow::ensure!(
                !name.is_empty()
                    && name.chars().count() <= 80
                    && !name.chars().any(char::is_control),
                "分组名称需要 1～80 个字，不能包含换行或控制字符。"
            );
            let previous = match &draft.action {
                ChapterAction::RenameGroup(group) => Some(group.clone()),
                _ => None,
            };
            anyhow::ensure!(
                previous.as_deref() == Some(&name)
                    || !self
                        .project
                        .stories
                        .values()
                        .any(|story| chapter_group(story) == name),
                "已有同名分组，请使用“移入分组”。"
            );
            if let Some(previous) = previous {
                self.flush();
                for story in self
                    .project
                    .stories
                    .values_mut()
                    .filter(|story| chapter_group(story) == previous)
                {
                    set_chapter_group(story, &name)?;
                }
                self.track();
                self.flush();
            } else {
                self.move_chapter_to_group(&name)?;
            }
            self.chapter_dialog = None;
            self.status = format!("分组已更新 · {name}");
            return Ok(());
        }
        anyhow::ensure!(!name.is_empty(), "请填写章节名称。");
        anyhow::ensure!(
            name.chars().count() <= 80 && !name.chars().any(char::is_control),
            "章节名称最多 80 个字，不能包含换行或控制字符。"
        );
        let mut story = match &draft.action {
            ChapterAction::New => Project::new().stories["main"].clone(),
            // A chapter copy retains explicit destinations and test semantics. Only
            // its own identity/title changes; it must not silently retarget the story.
            ChapterAction::Duplicate(story) => story.clone(),
            ChapterAction::NewGroup | ChapterAction::RenameGroup(_) => unreachable!(),
        };
        let id = (1..)
            .map(|n| format!("chapter{n}"))
            .find(|id| {
                !self
                    .project
                    .stories
                    .keys()
                    .any(|key| key.eq_ignore_ascii_case(id))
                    && !self.project.paths.values().any(|path| {
                        path.to_string_lossy()
                            .eq_ignore_ascii_case(&format!("{id}.json"))
                    })
            })
            .unwrap();
        story["id"] = json!(id);
        story["title"] = json!(name);
        self.flush();
        self.project.stories.insert(id.clone(), story);
        self.current = id;
        self.selected = 0;
        self.multiselect.clear();
        self.search.clear();
        self.center = Center::Node;
        self.auto = false;
        self.track();
        self.flush();
        self.chapter_dialog = None;
        self.status = format!("{}：{name}", tr("已创建章节"));
        Ok(())
    }
    fn begin_chapter_group(&mut self, rename: bool) {
        if !self.apply_pending_drafts() {
            return;
        }
        let group = chapter_group(&self.story()).to_owned();
        self.chapter_dialog = Some(ChapterDialog {
            action: if rename {
                ChapterAction::RenameGroup(group.clone())
            } else {
                ChapterAction::NewGroup
            },
            name: if rename { group } else { String::new() },
            error: None,
            focus_name: true,
        });
    }
    fn move_chapter_to_group(&mut self, group: &str) -> anyhow::Result<()> {
        self.flush();
        let story = self
            .project
            .stories
            .get_mut(&self.current)
            .ok_or_else(|| anyhow::anyhow!("当前章节不存在"))?;
        set_chapter_group(story, group)?;
        self.track();
        self.flush();
        Ok(())
    }
    fn chapter_reference_count(&self, chapter: &str) -> usize {
        usize::from(self.project.manifest["entry"] == chapter)
            + story_reference_count(&self.project.manifest, chapter)
            + self
                .project
                .scenario
                .as_ref()
                .map(|config| story_reference_count(config, chapter))
                .unwrap_or(0)
            + self
                .project
                .stories
                .iter()
                .filter(|(id, _)| id.as_str() != chapter)
                .map(|(_, story)| story_reference_count(story, chapter))
                .sum::<usize>()
    }
    fn begin_delete_chapter(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        if self.project.stories.len() <= 1 {
            self.error = Some(tr("作品至少需要保留一个章节。"));
            return;
        }
        let replacement = self
            .project
            .stories
            .keys()
            .find(|id| *id != &self.current)
            .cloned()
            .unwrap();
        self.preview.cancel_layout_drag();
        self.auto = false;
        self.delete_chapter_dialog = Some(DeleteChapterDialog {
            chapter: self.current.clone(),
            replacement,
            error: None,
        });
    }
    fn commit_delete_chapter(&mut self) -> anyhow::Result<()> {
        let draft = self
            .delete_chapter_dialog
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("没有待删除的章节"))?;
        let chapter = draft.chapter.clone();
        let replacement = draft.replacement.clone();
        anyhow::ensure!(
            self.project.stories.contains_key(&chapter),
            "原章节已不存在。"
        );
        anyhow::ensure!(self.project.stories.len() > 1, "作品至少需要保留一个章节。");
        anyhow::ensure!(
            replacement != chapter && self.project.stories.contains_key(&replacement),
            "请选择一个仍存在的替代章节。"
        );
        let name = self.project.stories[&chapter]["title"]
            .as_str()
            .unwrap_or(&chapter)
            .to_owned();
        self.flush();
        self.project.stories.remove(&chapter);
        self.project.paths.remove(&chapter);
        for story in self.project.stories.values_mut() {
            rename_story_refs(story, &chapter, &replacement);
        }
        rename_story_refs(&mut self.project.manifest, &chapter, &replacement);
        if self.project.manifest["entry"] == chapter {
            self.project.manifest["entry"] = json!(replacement);
        }
        if let Some(config) = &mut self.project.scenario {
            rename_story_refs(config, &chapter, &replacement);
        }
        if self.current == chapter {
            self.current = replacement;
        }
        self.selected = 0;
        self.multiselect.clear();
        self.search.clear();
        self.section_parent = 0;
        self.center = Center::Chapter;
        self.preview.cancel_layout_drag();
        self.auto = false;
        self.track();
        self.flush();
        self.delete_chapter_dialog = None;
        self.status = format!("{}：{name} · {}", tr("已删除章节"), tr("可撤销"));
        Ok(())
    }
    fn commit_rename(&mut self) -> anyhow::Result<()> {
        let draft = self
            .rename_dialog
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("没有待修改的标识"))?;
        let new = draft.value.trim().to_owned();
        let target = draft.target.clone();
        match &target {
            RenameTarget::Node { story, node } => {
                let chapter = self
                    .project
                    .stories
                    .get(story)
                    .ok_or_else(|| anyhow::anyhow!("原章节已不存在。"))?;
                anyhow::ensure!(
                    !new.is_empty() && new.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                    "步骤标识只能包含字母、数字和下划线。"
                );
                anyhow::ensure!(
                    chapter["nodes"]
                        .as_array()
                        .is_some_and(|nodes| nodes.iter().any(|n| n["id"] == *node)),
                    "原步骤已不存在。"
                );
                anyhow::ensure!(
                    new == *node
                        || !chapter["nodes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|n| n["id"] == new),
                    "此步骤标识已被使用。"
                );
            }
            RenameTarget::Chapter(old) => {
                anyhow::ensure!(self.project.stories.contains_key(old), "原章节已不存在。");
                anyhow::ensure!(
                    valid_id(&new),
                    "章节标识应为 1–64 位字母、数字、下划线或连字符。"
                );
                anyhow::ensure!(
                    new == *old || !self.project.stories.contains_key(&new),
                    "此章节标识已被使用。"
                );
            }
        }
        self.flush();
        match target {
            RenameTarget::Node { story, node } => {
                let chapter = self.project.stories.get_mut(&story).unwrap();
                let index = chapter["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|n| n["id"] == node)
                    .unwrap();
                chapter["nodes"][index]["id"] = json!(new);
                retarget(chapter, &BTreeMap::from([(node.clone(), new.clone())]));
                rename_translations(chapter, &node, &new);
            }
            RenameTarget::Chapter(old) => {
                let mut story = self.project.stories.remove(&old).unwrap();
                story["id"] = json!(new);
                rename_story_refs(&mut story, &old, &new);
                for other in self.project.stories.values_mut() {
                    rename_story_refs(other, &old, &new);
                }
                rename_story_refs(&mut self.project.manifest, &old, &new);
                if let Some(config) = &mut self.project.scenario {
                    rename_story_refs(config, &old, &new);
                }
                if self.project.manifest["entry"] == old {
                    self.project.manifest["entry"] = json!(new);
                }
                if let Some(path) = self.project.paths.remove(&old) {
                    self.project.paths.insert(new.clone(), path);
                }
                self.project.stories.insert(new.clone(), story);
                if self.current == old {
                    self.current = new;
                }
            }
        }
        self.track();
        self.flush();
        self.rename_dialog = None;
        Ok(())
    }
    fn flush(&mut self) {
        if let Some(s) = self.pending_history.take() {
            self.undo.push(s);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
    }
    fn track(&mut self) {
        let assets_changed = *self.last.assets != self.project.assets;
        if self.last.manifest != self.project.manifest
            || self.last.scenario != self.project.scenario
            || self.last.stories != self.project.stories
            || self.last.paths != self.project.paths
            || assets_changed
        {
            let current = Snapshot {
                manifest: self.project.manifest.clone(),
                scenario: self.project.scenario.clone(),
                stories: self.project.stories.clone(),
                paths: self.project.paths.clone(),
                assets: if assets_changed {
                    std::sync::Arc::new(self.project.assets.clone())
                } else {
                    self.last.assets.clone()
                },
            };
            self.auto = false;
            self.preview_elapsed = None;
            if current.assets != self.last.assets {
                self.catalog.sync_assets(&self.project.assets);
            }
            self.catalog.sync_story_titles(&self.project.stories);
            if self.pending_history.is_none() {
                self.pending_history = Some(self.last.clone());
                self.redo.clear();
            }
            self.last = current;
            self.document_dirty = self.last != self.saved;
            self.last_edit = Instant::now();
            self.needs_compile = true;
        }
        if self.last_edit.elapsed() > Duration::from_millis(650) {
            self.flush();
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.preview.cancel_layout_drag();
        self.content_panel.reset();
        self.advanced.reset();
        self.project.stories = s.stories;
        self.project.manifest = s.manifest;
        self.project.scenario = s.scenario;
        self.project.paths = s.paths;
        self.project.assets = (*s.assets).clone();
        self.catalog.sync_assets(&self.project.assets);
        self.catalog.sync_story_titles(&self.project.stories);
        self.asset_dirty = false;
        if !self.project.stories.contains_key(&self.current) {
            self.current = self
                .project
                .stories
                .keys()
                .next()
                .cloned()
                .unwrap_or_default();
        }
        self.selected = self.selected.min(
            self.story()["nodes"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(1)
                .saturating_sub(1),
        );
        self.last = snapshot(&self.project);
        self.document_dirty = self.last != self.saved;
        self.pending_history = None;
        self.needs_compile = true;
        self.multiselect.clear();
    }
    fn undo(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        if let Some(s) = self.undo.pop() {
            self.redo.push(snapshot(&self.project));
            self.restore(s);
            self.status = "已撤销".into();
        }
    }
    fn redo(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        if let Some(s) = self.redo.pop() {
            self.undo.push(snapshot(&self.project));
            self.restore(s);
            self.status = "已重做".into();
        }
    }
    fn request(&mut self, p: Pending, ctx: &egui::Context) {
        self.flush();
        if self.dirty() {
            self.pending = Some(p);
        } else {
            self.perform(p, ctx);
        }
    }
    fn perform(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::EditorUpdate => self.updater.download(ctx),
            Pending::Quit => {
                self.allow_close = true;
                self.clear_recovery();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Pending::New => self.install(Project::new_scenario("sandbox")),
            Pending::NewAddon => self.install(Project::new_scenario("addon")),
            Pending::NewVanillaPatch => match Project::new_vanilla_patch("ch1_1") {
                Ok(project) => {
                    self.install(project);
                    self.saved.manifest = json!({});
                    self.document_dirty = true;
                }
                Err(error) => self.error = Some(crate::error_ui::capture(error)),
            },
            Pending::OpenVanillaRoutes => match Project::new_vanilla_patch("ch1_1") {
                Ok(project) => {
                    self.install(project);
                    self.vanilla.open_routes();
                }
                Err(error) => self.error = Some(crate::error_ui::capture(error)),
            },
            Pending::OpenVanillaBookmark(mark) => match Project::new_vanilla_patch(&mark.script) {
                Ok(project) => {
                    self.install(project);
                    self.saved.manifest = json!({});
                    self.document_dirty = true;
                    self.vanilla.focus_bookmark(mark);
                }
                Err(error) => self.error = Some(crate::error_ui::capture(error)),
            },
            Pending::Template(key) => match template_project(&key) {
                Ok(project) => {
                    self.install(project);
                    self.saved.manifest = json!({});
                    self.document_dirty = true;
                    self.status =
                        "已从模板创建项目；user:template.* 占位资源需要在内容库中替换".into();
                }
                Err(error) => self.error = Some(crate::error_ui::capture(error)),
            },
            Pending::Recover(path) => self.recover(&path),
            Pending::Open(path) => match Project::open(&path) {
                Ok(p) => self.install(p),
                Err(e) => self.error = Some(format!("打开失败\n{}\n{e:#}", path.display())),
            },
        }
    }
    fn open_vanilla_routes(&mut self, ctx: &egui::Context) {
        if !self.apply_pending_drafts() {
            return;
        }
        let chapter = self
            .project
            .stories
            .iter()
            .find(|(_, story)| story.get("vanilla_patch").is_some())
            .map(|(id, _)| id.clone());
        if let Some(chapter) = chapter {
            self.flush();
            self.current = chapter;
            self.selected = 0;
            self.center = Center::Node;
            self.vanilla.open_routes();
        } else {
            self.request(Pending::OpenVanillaRoutes, ctx);
        }
    }
    fn open_story_bookmark(
        &mut self,
        mark: lom_core::story_bookmarks::Bookmark,
        ctx: &egui::Context,
    ) {
        if !self.apply_pending_drafts() {
            return;
        }
        let existing = self
            .project
            .stories
            .iter()
            .find(|(_, story)| {
                lom_core::vanilla_import::target(story)
                    .is_some_and(|target| target.eq_ignore_ascii_case(&mark.script))
            })
            .map(|(id, _)| id.clone());
        if let Some(id) = existing {
            self.open_center(Center::Node);
            self.current = id;
            self.selected = 0;
            self.multiselect.clear();
            self.auto = false;
            self.audio.stop();
            self.needs_compile = true;
            if self.story().get("vanilla_import").is_some() {
                self.selected = lom_core::vanilla_import::bookmark_node(
                    &self.story(),
                    &mark.text_key,
                    &mark.text,
                    self.project.manifest["id"].as_str().unwrap_or(""),
                )
                .unwrap_or(0);
                self.view = View::Stage;
                self.search.clear();
                self.status = "已定位到整章导入的原版步骤。".into();
            } else {
                self.vanilla.focus_bookmark(mark);
            }
        } else if self
            .project
            .stories
            .values()
            .any(|story| lom_core::vanilla_import::target(story).is_some())
        {
            let id = (1..)
                .map(|i| format!("patch{i}"))
                .find(|id| !self.project.stories.contains_key(id))
                .unwrap();
            match lom_core::vanilla_patch::new_story(&id, &mark.script) {
                Ok(story) => {
                    self.flush();
                    self.project.stories.insert(id.clone(), story);
                    self.open_center(Center::Node);
                    self.current = id;
                    self.selected = 0;
                    self.multiselect.clear();
                    self.auto = false;
                    self.audio.stop();
                    self.needs_compile = true;
                    self.vanilla.focus_bookmark(mark);
                }
                Err(error) => self.error = Some(crate::error_ui::capture(error)),
            }
        } else {
            self.request(Pending::OpenVanillaBookmark(mark), ctx);
        }
    }
    fn install(&mut self, p: Project) {
        self.preview.cancel_layout_drag();
        self.clear_recovery();
        self.project = p;
        self.last_auto_save = Instant::now();
        self.center = Center::Node;
        self.scenario_selection = Default::default();
        self.advanced = Default::default();
        self.content_panel.reset();
        self.vanilla = Default::default();
        self.chapter_dialog = None;
        self.delete_chapter_dialog = None;
        self.rename_dialog = None;
        self.asset_import_dialog = None;
        self.image_import_target = None;
        self.audio.stop();
        self.auto = false;
        self.preview_elapsed = None;
        self.remember_source();
        self.catalog.sync_assets(&self.project.assets);
        self.catalog.sync_story_titles(&self.project.stories);
        self.current = self.project.manifest["entry"]
            .as_str()
            .filter(|id| self.project.stories.contains_key(*id))
            .unwrap_or_else(|| {
                self.project
                    .stories
                    .keys()
                    .next()
                    .map(String::as_str)
                    .unwrap_or("")
            })
            .into();
        self.selected = 0;
        self.multiselect.clear();
        self.undo.clear();
        self.redo.clear();
        self.pending_history = None;
        self.last = snapshot(&self.project);
        self.mark_saved();
        self.asset_dirty = false;
        self.needs_compile = true;
        self.status = format!("已载入 {} 个章节", self.project.stories.len());
        if let Some(notice) = self
            .project
            .stories
            .values()
            .find_map(|story| story["lua_import"]["notice"].as_str())
        {
            self.status = notice.to_owned();
        }
    }
    fn save(&mut self, as_new: bool) -> bool {
        if !self.apply_pending_drafts() {
            return false;
        }
        self.flush();
        let path = if as_new { None } else { self.save_target() };
        let path = path.or_else(|| {
            rfd::FileDialog::new()
                .set_title("选择项目保存目录（保存全部章节与素材）")
                .pick_folder()
        });
        let Some(path) = path else { return false };
        self.save_at(&path, false)
    }
    fn save_target(&self) -> Option<PathBuf> {
        self.project.source.clone().filter(|path| {
            path.extension().and_then(|x| x.to_str()) != Some("json")
                || (self.project.stories.len() == 1 && self.project.manifest == self.saved.manifest)
        })
    }
    fn save_at(&mut self, path: &Path, automatic: bool) -> bool {
        match crate::persistence::save_project(
            &mut self.project,
            &path,
            self.saved.assets.as_ref(),
            &self.saved.stories,
            &self.saved.paths,
        ) {
            Ok(()) => {
                self.remember_source();
                self.last = snapshot(&self.project);
                self.mark_saved();
                self.asset_dirty = false;
                self.last_auto_save = Instant::now();
                self.clear_recovery();
                self.status = if automatic {
                    crate::i18n::message(
                        "editor.autosave.saved",
                        &[
                            ("count", self.project.stories.len().to_string()),
                            ("path", path.display().to_string()),
                        ],
                    )
                } else {
                    format!(
                        "已保存全部 {} 个章节 · {}",
                        self.project.stories.len(),
                        path.display()
                    )
                };
                true
            }
            Err(e) => {
                self.error = Some(if automatic {
                    crate::i18n::message(
                        "editor.autosave.failed",
                        &[
                            ("path", path.display().to_string()),
                            ("error", crate::error_ui::capture(e)),
                        ],
                    )
                } else {
                    format!("保存失败；修改仍保留在编辑器中。\n{e:#}")
                });
                false
            }
        }
    }
    fn export(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        if !self.check_before_run_or_export() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("活侠传 Mod", &["lommod"])
            .set_file_name(format!(
                "{}.lommod",
                self.project.manifest["id"].as_str().unwrap_or("my_mod")
            ))
            .save_file()
        {
            match self.project.export(&path) {
                Ok(p) => self.status = format!("已导出 {}", p.display()),
                Err(e) => self.error = Some(format!("导出失败\n{e:#}")),
            }
        }
    }
    fn export_play_bundle(&mut self) {
        if self.apply_pending_drafts() && self.check_before_run_or_export() {
            self.flush();
            self.tools_panel.export_play_bundle(&self.project);
        }
    }
    fn compile(&mut self) {
        if let Some(story) = self.project.stories.get_mut(&self.current) {
            lom_core::native_flow::refresh_branch_labels(story);
        }
        self.catalog.data["project_flags"] = json!(self
            .project
            .stories
            .values()
            .flat_map(|s| s["nodes"].as_array().into_iter().flatten())
            .filter(|n| n["type"] == "flag")
            .filter_map(|n| n["flag"].as_str())
            .collect::<BTreeSet<_>>());
        self.diagnostics.clear();
        for (id, s) in &self.project.stories {
            self.diagnostics
                .extend(lom_core::release::compiler_issues_with_catalog(
                    id,
                    s,
                    &self.catalog.data,
                ));
        }
        match compile_with_assets(&self.project, &self.story()) {
            Ok(lua) => self.lua = lua,
            Err(e) => self.lua = crate::error_ui::text(&crate::error_ui::capture(e)),
        };
        self.needs_compile = false;
    }
    fn next_id(&self, kind: &str) -> String {
        let s = self.story();
        let ids: BTreeSet<_> = s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str())
            .collect();
        (1..)
            .map(|n| format!("{kind}{n}"))
            .find(|s| !ids.contains(s.as_str()))
            .unwrap()
    }
    fn add_node(&mut self, kind: &str) {
        if matches!(kind, "native_action" | "native_branch")
            && self.story()["vanilla_import"]["schema"] != 2
        {
            self.error = Some("原版指令与原版条件只用于结构化导入的原版章节。".into());
            return;
        }
        if self.story().get("vanilla_import").is_some() && matches!(kind, "combat" | "battle") {
            self.error = Some("新增战斗会切换场景，当前原版整章的局部状态无法跨场景续接。".into());
            return;
        }
        self.flush();
        let mut node = self.catalog.new_node(kind, self.next_id(kind));
        if self.story().get("vanilla_import").is_some() {
            if kind == "end" {
                node["resume_original"] = json!(true);
            }
            if kind == "say" {
                node["inherit_dialog"] = json!(true);
            }
        }
        let mut story = self.story();
        match insert_nodes_after(&mut story, self.selected, vec![node]) {
            Ok(at) => {
                self.project.stories.insert(self.current.clone(), story);
                self.selected = at;
                self.multiselect.clear();
                self.center = Center::Node;
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    fn selected_indices(&self) -> Vec<usize> {
        if self.multiselect.is_empty() {
            vec![self.selected]
        } else {
            self.multiselect.iter().copied().collect()
        }
    }
    fn copy(&mut self) {
        let s = self.story();
        self.clipboard = self
            .selected_indices()
            .into_iter()
            .filter_map(|i| s["nodes"].get(i).cloned())
            .collect();
        self.status = format!("已复制 {} 个节点，可切换章节后粘贴", self.clipboard.len());
    }
    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.flush();
        let mut clones = self.clipboard.clone();
        let mut story = self.story();
        let imported = story.get("vanilla_import").is_some();
        if story["vanilla_import"]["schema"] != 2
            && clones.iter().any(|node| {
                matches!(
                    node["type"].as_str(),
                    Some("native_action" | "native_branch")
                )
            })
        {
            self.error = Some("原版指令与原版条件只用于结构化导入的原版章节。".into());
            return;
        }
        if imported
            && clones
                .iter()
                .any(|node| matches!(node["type"].as_str(), Some("combat" | "battle")))
        {
            self.error = Some("新增战斗会切换场景，当前原版整章的局部状态无法跨场景续接。".into());
            return;
        }
        let mut used: BTreeSet<String> = story["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str().map(str::to_owned))
            .collect();
        let mut map = BTreeMap::new();
        for n in &mut clones {
            let old = n["id"].as_str().unwrap_or("node").to_owned();
            let id = (1..)
                .map(|i| format!("{old}_copy{i}"))
                .find(|v| !used.contains(v))
                .unwrap();
            used.insert(id.clone());
            map.insert(old, id.clone());
            n["id"] = id.into();
            if let Some(editor) = n.get_mut("_editor").and_then(Value::as_object_mut) {
                editor.remove("vanilla_origin");
            }
            if !imported {
                n.as_object_mut().unwrap().remove("inherit_dialog");
                n.as_object_mut().unwrap().remove("resume_original");
            }
        }
        for n in &mut clones {
            retarget(n, &map);
        }
        if story["vanilla_import"]["schema"] == 2 {
            lom_core::native_flow::rebind_copied_locals(&mut clones, &story);
        }
        let count = clones.len();
        match insert_nodes_after(&mut story, self.selected, clones) {
            Ok(at) => {
                self.project.stories.insert(self.current.clone(), story);
                self.selected = at;
                self.multiselect = (at..at + count).collect();
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    fn delete(&mut self) {
        let selected: BTreeSet<_> = self.selected_indices().into_iter().collect();
        let old_nodes = self.story()["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let old_ids: Vec<String> = old_nodes
            .iter()
            .filter_map(|n| n["id"].as_str().map(str::to_owned))
            .collect();
        if old_nodes.len() <= selected.len() {
            self.error = Some("每个章节至少需要保留一个节点。".into());
            return;
        }
        let removed: BTreeSet<String> = selected
            .iter()
            .filter_map(|i| old_nodes.get(*i)?.get("id")?.as_str().map(str::to_owned))
            .collect();
        let mut story = self.story();
        story["nodes"]
            .as_array_mut()
            .unwrap()
            .retain(|n| !removed.contains(n["id"].as_str().unwrap_or("")));
        let reconnect = if story["vanilla_import"]["schema"] == 2 {
            reconnect_deleted_structured(&mut story, &old_nodes, &removed)
        } else {
            reconnect_deleted_offsets(&mut story, &old_nodes, &removed)
        };
        if let Err(error) = reconnect {
            self.error = Some(crate::error_ui::capture(error));
            return;
        }
        if removed.contains(story["start"].as_str().unwrap_or("")) {
            story["start"] = story["nodes"][0]["id"].clone();
        }
        repair_deleted_metadata(&mut story, &old_ids, &removed);
        self.flush();
        self.project.stories.insert(self.current.clone(), story);
        self.selected = self.selected.min(
            self.story()["nodes"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0)
                .saturating_sub(1),
        );
        self.multiselect.clear();
    }
    fn move_node(&mut self, to: usize) {
        self.flush();
        if let Some(story) = self.project.stories.get_mut(&self.current) {
            let old_front = story["nodes"][0]["id"].clone();
            let follow = story["start"] == old_front;
            if let Some(nodes) = story["nodes"].as_array_mut() {
                if self.selected < nodes.len() && to < nodes.len() {
                    let node = nodes.remove(self.selected);
                    nodes.insert(to, node);
                    if follow {
                        story["start"] = nodes[0]["id"].clone();
                    }
                    self.selected = to;
                    self.multiselect.clear();
                }
            }
        }
    }
    fn recover(&mut self, path: &Path) {
        if recovery_owner_active(path) {
            self.error = Some("此恢复副本可能属于仍在运行的编辑器实例，暂不读取或移除。".into());
            return;
        }
        match Project::open(path) {
            Ok(mut project) => {
                let session = lom_core::load_json(path.join("session.json")).unwrap_or(json!({}));
                project.source = session["source"].as_str().map(PathBuf::from);
                if let Ok(paths) =
                    serde_json::from_value::<BTreeMap<String, PathBuf>>(session["paths"].clone())
                {
                    project.paths = paths;
                }
                if let Some(story_subdir) = session["story_subdir"].as_bool() {
                    project.story_subdir = story_subdir;
                }
                if let Some(has_manifest) = session["has_manifest"].as_bool() {
                    project.has_manifest = has_manifest;
                }
                self.install(project);
                self.saved = Snapshot {
                    manifest: json!({}),
                    scenario: None,
                    stories: BTreeMap::new(),
                    paths: BTreeMap::new(),
                    assets: std::sync::Arc::new(BTreeMap::new()),
                };
                self.document_dirty = true;
                self.current = session["current"]
                    .as_str()
                    .filter(|id| self.project.stories.contains_key(*id))
                    .unwrap_or(&self.current)
                    .to_owned();
                self.selected = (session["selected"].as_u64().unwrap_or(0) as usize).min(
                    self.story()["nodes"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or(1)
                        .saturating_sub(1),
                );
                self.recovery_candidates
                    .retain(|candidate| candidate != path);
                self.status = "已恢复创作副本，请检查并保存到原项目或新目录".into();
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    fn clear_recovery(&mut self) {
        if self.recovery_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.recovery_dir);
        }
        self.recovery_paths.clear();
        self.recovery_assets.clear();
    }
    fn idle_repaint_delay(&self) -> Duration {
        let mut delay = Duration::MAX;
        if self.needs_compile {
            delay = delay.min(Duration::from_millis(400).saturating_sub(self.last_edit.elapsed()));
        }
        if self.pending_history.is_some() {
            delay = delay.min(Duration::from_millis(650).saturating_sub(self.last_edit.elapsed()));
        }
        if self.dirty() {
            delay = delay.min(Duration::from_secs(30).saturating_sub(self.last_recovery.elapsed()));
            if self.auto_save_project
                && !self.saved.stories.is_empty()
                && self.save_target().is_some()
            {
                delay = delay.min(
                    Duration::from_secs(self.auto_save_seconds)
                        .saturating_sub(self.last_auto_save.elapsed()),
                );
            }
        }
        if self.audio.playing() {
            delay = delay.min(Duration::from_millis(200));
        }
        if self.screenshot.is_some() {
            delay = delay.min(Duration::from_millis(70));
        }
        delay
    }
    fn autosave(&mut self) {
        if self.auto_save_project
            && self.dirty()
            && !self.saved.stories.is_empty()
            && self.last_auto_save.elapsed() >= Duration::from_secs(self.auto_save_seconds)
        {
            // Do not commit half-entered drafts or resolve a pending save/switch dialog.
            self.last_auto_save = Instant::now();
            if self.pending.is_none()
                && self.error.is_none()
                && !self.content_panel.has_pending()
                && !self.advanced.has_pending()
            {
                if let Some(path) = self.save_target() {
                    self.flush();
                    if self.save_at(&path, true) {
                        return;
                    }
                }
            }
        }
        if !self.dirty() || self.last_recovery.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.last_recovery = Instant::now();
        let mut p = self.project.clone();
        let pending_warning = self.content_panel.apply_pending_to_snapshot(&mut p).err();
        p.paths = self.recovery_paths.clone();
        p.story_subdir = true;
        // Only earlier successful writes establish ownership of recovery files.
        // Never borrow the original project's filenames or trust a preexisting directory.
        p.source = if self.recovery_paths.is_empty() {
            None
        } else {
            Some(
                self.recovery_dir
                    .canonicalize()
                    .unwrap_or_else(|_| self.recovery_dir.clone()),
            )
        };
        let result = (|| -> anyhow::Result<()> {
            p.save_to(&self.recovery_dir)?;
            let previous_paths = self.recovery_paths.clone();
            let previous_assets = self.recovery_assets.clone();
            self.recovery_paths.extend(p.paths.clone());
            self.recovery_assets.extend(p.assets.keys().cloned());
            // Undoing chapter creation/renaming must not resurrect an obsolete
            // chapter when recovery reopens the directory. Only remove owned files.
            for (id, filename) in previous_paths {
                if !p.paths.values().any(|path| path == &filename) {
                    let file = self.recovery_dir.join("story").join(&filename);
                    if file.exists() {
                        std::fs::remove_file(file)?;
                    }
                    self.recovery_paths.remove(&id);
                }
            }
            for name in previous_assets {
                if !p.assets.contains_key(&name) {
                    let file = self.recovery_dir.join(&name);
                    if file.exists() {
                        let file =
                            lom_core::package::resolve_confined_file(&self.recovery_dir, &file)?;
                        anyhow::ensure!(
                            file == self.recovery_dir.canonicalize()?.join(&name),
                            "恢复素材路径已改变，未移除：{name}"
                        );
                        std::fs::remove_file(file)?;
                    }
                    self.recovery_assets.remove(&name);
                }
            }
            let session = json!({"source":self.project.source,"paths":self.project.paths,"story_subdir":self.project.story_subdir,"has_manifest":self.project.has_manifest,"current":self.current,"selected":self.selected});
            lom_core::project::atomic_write(
                &self.recovery_dir.join("session.json"),
                &lom_core::stable_json(&session)?,
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            self.status = format!(
                "自动恢复副本写入失败。{}",
                crate::error_ui::text(&crate::error_ui::capture(e))
            );
        } else if let Some(error) = pending_warning {
            self.status = format!(
                "素材输入尚未完成。{}",
                crate::error_ui::text(&crate::error_ui::capture(error))
            );
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let windows_settings_release = cfg!(windows)
            && ctx.input(|input| {
                if !input.focused {
                    self.settings_comma_down = false;
                    return false;
                }
                let mut requested = false;
                for event in &input.events {
                    if let egui::Event::Key {
                        key: egui::Key::Comma,
                        pressed,
                        modifiers,
                        ..
                    } = event
                    {
                        if *pressed {
                            self.settings_comma_down = true;
                        } else {
                            // Windows can deliver Ctrl+, only on release. Track
                            // every comma press, even in a modal, so a normal
                            // down/up pair never triggers the shortcut twice.
                            let had_press = std::mem::take(&mut self.settings_comma_down);
                            requested |= !had_press
                                && modifiers.ctrl
                                && modifiers.command
                                && !modifiers.alt
                                && !modifiers.shift
                                && !modifiers.mac_cmd;
                        }
                    }
                }
                requested
            });
        if self.chapter_dialog.is_some()
            || self.delete_chapter_dialog.is_some()
            || self.rename_dialog.is_some()
            || self.asset_import_dialog.is_some()
            || self.settings_open
            || self.updater.open
            || self.tools_panel.dialog_open()
            || self.error.is_some()
            || self.pending.is_some()
            || self.pending_template.is_some()
            || (!self.recovery_candidates.is_empty() && self.screenshot.is_none())
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma))
            || windows_settings_release
        {
            self.settings_open = true;
            self.settings_error = None;
            return;
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            )
        }) {
            self.save(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(false);
        }
        let editing_text = ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if !editing_text
            && ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                )
            })
        {
            self.redo();
        } else if !editing_text
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
        {
            self.undo();
        }
        if !editing_text && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y))
        {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.request(Pending::New, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F5)) {
            self.play_chapter_start();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.play_current();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F6)) {
            if self.open_center(Center::Node) {
                self.compile();
                self.view = View::Checks;
            }
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::F,
            )
        }) {
            if self.open_center(Center::Advanced) {
                self.advanced.page = 0;
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.tools_panel.open_help();
        }
        if !ctx.wants_keyboard_input()
            && matches!(self.center, Center::Node | Center::Chapter)
            && !self.is_vanilla_patch()
        {
            let (copy, paste) = ctx.input_mut(|input| {
                let mut copy = input.consume_key(egui::Modifiers::COMMAND, egui::Key::C);
                let mut paste = input.consume_key(egui::Modifiers::COMMAND, egui::Key::V);
                input.events.retain(|event| match event {
                    egui::Event::Copy => {
                        copy = true;
                        false
                    }
                    egui::Event::Paste(_) => {
                        paste = true;
                        false
                    }
                    _ => true,
                });
                (copy, paste)
            });
            if copy {
                self.copy();
            }
            if paste {
                self.paste();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                self.delete();
            }
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::O,
            )
        }) {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.request(Pending::Open(path), ctx);
            }
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("剧情、Lua 或 Mod 包", &["json", "lua", "lommod"])
                .pick_file()
            {
                self.request(Pending::Open(path), ctx);
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) {
            self.navigate(Section::Materials);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7)) {
            if self.open_center(Center::Node) {
                self.view = View::Graph;
            }
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        let shortcut =
            |modifiers, key| ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, key));
        egui::TopBottomPanel::top("commands")
            .frame(surface(false))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    #[cfg(not(target_os = "macos"))]
                    if ui
                        .add(egui::Button::new(tr("设置")).shortcut_text("Ctrl+,"))
                        .clicked()
                    {
                        self.settings_open = true;
                        self.settings_error = None;
                    }
                    ui.menu_button(tr("文件"), |ui| {
                        ui.menu_button(crate::i18n::key("editor.workspace.new_project"), |ui| {
                            if ui.add(egui::Button::new(crate::i18n::key("editor.workspace.new_free"))
                                .shortcut_text(shortcut(egui::Modifiers::COMMAND, egui::Key::N))).clicked() {
                                ui.close();
                                self.request(Pending::New, ctx);
                            }
                            if ui.button(crate::i18n::key("editor.workspace.new_extra")).clicked() {
                                ui.close();
                                self.request(Pending::NewAddon,ctx);
                            }
                            ui.menu_button(tr("从模板新建"), |ui| {
                                let templates: Value = serde_json::from_str(include_str!(
                                    "../data/project-templates.json"
                                ))
                                .expect("built-in templates");
                                for (key, item) in templates.as_object().unwrap() {
                                    if ui
                                        .button(item["name"].as_str().unwrap_or(key))
                                        .on_hover_text(item["description"].as_str().unwrap_or(""))
                                        .clicked()
                                    {
                                        ui.close();
                                        self.request(Pending::Template(key.clone()), ctx);
                                    }
                                }
                            });
                        });
                        ui.separator();
                        if ui.add(egui::Button::new(tr("打开目录…"))
                            .shortcut_text(shortcut(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::O))).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.request(Pending::Open(path), ctx);
                            }
                        }
                        if ui.add(egui::Button::new(tr("打开 JSON / Mod 包…"))
                            .shortcut_text(shortcut(egui::Modifiers::COMMAND, egui::Key::O))).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("剧情、Lua 或 Mod 包", &["json", "lua", "lommod"])
                                .pick_file()
                            {
                                self.request(Pending::Open(path), ctx);
                            }
                        }
                        ui.menu_button(tr("最近打开"), |ui| {
                            if self.recent.is_empty() {
                                ui.label(tr("还没有最近项目"));
                            }
                            for path in self.recent.clone() {
                                if ui.button(path.display().to_string()).clicked() {
                                    ui.close();
                                    self.request(Pending::Open(path), ctx);
                                }
                            }
                        });
                        ui.separator();
                        let save_label = tr("保存全部章节  ⌘S");
                        let save_label = if cfg!(target_os = "macos") {
                            save_label
                        } else {
                            save_label.replace("⌘S", "Ctrl+S")
                        };
                        if ui.button(save_label).clicked() {
                            ui.close();
                            self.save(false);
                        }
                        if ui.add(egui::Button::new(tr("另存项目目录…"))
                            .shortcut_text(shortcut(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S))).clicked() {
                            ui.close();
                            self.save(true);
                        }
                        ui.separator();
                        ui.menu_button(crate::i18n::key("editor.workspace.original_tools"), |ui| {
                            if ui.button(crate::i18n::key("editor.workspace.import_original")).clicked() {
                                ui.close();
                                self.request(Pending::NewVanillaPatch, ctx);
                            }
                            if ui.button(crate::i18n::key("editor.workspace.routes")).clicked() {
                                ui.close();self.open_vanilla_routes(ctx);
                            }
                            if ui.button(crate::i18n::key("editor.workspace.bookmark")).clicked() {
                                ui.close();
                                match lom_core::story_bookmarks::read(&lom_core::story_bookmarks::default_root()) {
                                    Ok(marks) => if let Some(mark)=marks.into_iter().next() {
                                        self.open_story_bookmark(mark,ctx);
                                    } else {self.error=Some("还没有游戏标记。请在原版剧情中按 Ctrl+F6 后再打开。".into());},
                                    Err(error)=>self.error=Some(crate::error_ui::capture(error)),
                                }
                            }
                        });
                        ui.menu_button(crate::i18n::key("editor.workspace.export_menu"), |ui| {
                            if ui.button(crate::i18n::key("editor.workspace.release_settings")).clicked() {
                                self.open_center(Center::Manifest);
                                ui.close();
                            }
                            if ui.button(tr("导出 .lommod…")).clicked() {
                                ui.close();
                                self.export();
                            }
                            if ui.button(crate::i18n::key("editor.workspace.export_player")).clicked() {
                                ui.close();
                                self.export_play_bundle();
                            }
                        });
                        ui.menu_button(crate::i18n::key("editor.workspace.preview_assets"), |ui| {
                            if ui.button(tr("从游戏提取预览素材…")).clicked() {
                                ui.close();
                                self.tools_panel.start_extraction();
                            }
                            if ui.button(tr("选择预览素材库…")).on_hover_text(tr("选择包含 preview_map.json 的已提取素材库；选择游戏目录则自动开始提取。")).clicked() {
                                ui.close();
                                self.choose_preview_library();
                            }
                        });
                    });

                    ui.menu_button(crate::i18n::key("editor.workspace.edit"), |ui| {
                        if ui
                            .add_enabled(
                                !self.undo.is_empty() || self.pending_history.is_some(),
                                egui::Button::new(tr("撤销"))
                                    .shortcut_text(shortcut(egui::Modifiers::COMMAND, egui::Key::Z)),
                            )
                            .clicked()
                        {
                            self.undo();
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.redo.is_empty(), egui::Button::new(tr("重做"))
                                .shortcut_text(shortcut(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z)))
                            .clicked()
                        {
                            self.redo();
                            ui.close();
                        }
                        if ui.add_enabled(matches!(self.center, Center::Node | Center::Chapter) && !self.is_vanilla_patch(), egui::Button::new(tr("复制"))).clicked() {
                            self.copy();
                            ui.close();
                        }
                        if ui.add_enabled(matches!(self.center, Center::Node | Center::Chapter) && !self.is_vanilla_patch(), egui::Button::new(tr("粘贴"))).clicked() {
                            self.paste();
                            ui.close();
                        }
                        if ui.button(tr("多语言")).clicked() {
                            self.open_center(Center::Localization);
                            ui.close();
                        }
                        ui.separator();
                        ui.menu_button(tr("高级"), |ui| {
                            if ui.button(tr("修改步骤标识…")).clicked()
                                && self.apply_pending_drafts()
                            {
                                if let Some(id) =
                                    self.story()["nodes"][self.selected]["id"].as_str()
                                {
                                    self.rename_dialog = Some(RenameDialog {
                                        target: RenameTarget::Node {
                                            story: self.current.clone(),
                                            node: id.to_owned(),
                                        },
                                        value: id.to_owned(),
                                        error: None,
                                        focus_name: true,
                                    });
                                }
                                ui.close();
                            }
                            if ui.button(tr("修改章节标识…")).clicked()
                                && self.apply_pending_drafts()
                            {
                                self.rename_dialog = Some(RenameDialog {
                                    target: RenameTarget::Chapter(self.current.clone()),
                                    value: self.current.clone(),
                                    error: None,
                                    focus_name: true,
                                });
                                ui.close();
                            }
                        });
                    });
                    ui.menu_button(tr("剧情工具"), |ui| {
                        if ui.add_enabled(cfg!(windows), egui::Button::new(crate::i18n::key("editor.play.current"))).clicked() {
                            self.play_current();
                            ui.close();
                        }
                        if ui.add_enabled(cfg!(windows), egui::Button::new(crate::i18n::key("editor.play.chapter_start"))).clicked() {
                            self.play_chapter_start();
                            ui.close();
                        }
                        ui.separator();
                        for (page, label) in [
                            "全局查找",
                            "变量管理",
                            "条件检查",
                            "路径模拟",
                            "跨章节复制",
                            "离线测试",
                        ]
                        .iter()
                        .enumerate()
                        {
                            if ui.button(tr(label)).clicked() {
                                if self.open_center(Center::Advanced) { self.advanced.page = page; }
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(tr("批量修改与模板")).clicked() {
                            self.open_tool_page(ToolPage::Editing);
                            ui.close();
                        }
                        if ui.button(tr("迁移与诊断")).clicked() {
                            self.open_tool_page(ToolPage::Maintenance);
                            ui.close();
                        }
                        if ui.button(tr("导出诊断报告…")).clicked() {
                            ui.close();
                            self.diagnostic();
                        }
                        if ui.button(tr("检查项目")).clicked() {
                            self.navigate(Section::Story);
                            self.compile();
                            self.view = View::Checks;
                            ui.close();
                        }
                        if ui.button(tr("统计")).clicked() {
                            self.navigate(Section::Story);
                            self.view = View::Statistics;
                            ui.close();
                        }
                    });
                    if ui
                        .button(crate::i18n::key("menu.help_item"))
                        .on_hover_text("F1")
                        .clicked()
                    {
                        self.tools_panel.open_help();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                cfg!(windows),
                                egui::Button::new(crate::i18n::key("toolbar.play")),
                            )
                            .on_hover_text(crate::i18n::key("editor.play.hint"))
                            .clicked()
                        {
                            self.play_current();
                        }
                        if ui
                            .add(
                                egui::Button::new(crate::i18n::key("toolbar.export"))
                                    .fill(Color32::from_rgb(213, 231, 247)),
                            )
                            .clicked()
                        {
                            self.export();
                        }
                        if ui.button(crate::i18n::key("editor.workspace.export_short")).clicked() {
                            self.export_play_bundle();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for (section, label) in [
                        (Section::Story, "剧情编辑"),
                        (Section::Materials, "素材管理"),
                        (Section::Game, "游戏与 MOD"),
                        (Section::Release, "发布"),
                    ] {
                        if ui.add_sized([112.0, 30.0], egui::Button::selectable(self.section() == section, tr(label))).clicked() {
                            self.navigate(section);
                        }
                    }
                });
            });
    }
    fn section(&self) -> Section {
        match self.center {
            Center::Assets => Section::Materials,
            Center::Tools => match self.tools_panel.page {
                ToolPage::Previews | ToolPage::Library => Section::Materials,
                ToolPage::Game => Section::Game,
                ToolPage::Release => Section::Release,
                _ => Section::Story,
            },
            _ => Section::Story,
        }
    }
    fn go_to_problem(
        &mut self,
        ctx: &egui::Context,
        destination: crate::error_location::Destination,
    ) {
        use crate::error_location::Destination;
        if destination == Destination::Current {
            ctx.data_mut(|data| data.remove::<String>(egui::Id::new("problem-field")));
            return;
        }
        if matches!(&destination, Destination::Asset(path) if self.center == Center::Assets && self.asset_selection == *path)
        {
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        self.auto = false;
        self.audio.stop();
        match destination {
            Destination::Story(id, node) => {
                if let Some(story) = self.project.stories.get_mut(&id) {
                    if let Some(index) = node.and_then(|node| {
                        story["nodes"]
                            .as_array()?
                            .iter()
                            .position(|n| n["id"] == node)
                    }) {
                        self.selected = index;
                        self.center = Center::Node;
                        self.view = View::Stage;
                        authoring::expand_for_node(story, index);
                    } else {
                        self.center = Center::Chapter;
                    }
                    self.current = id;
                    self.multiselect.clear();
                    self.search.clear();
                    self.needs_compile = true;
                }
            }
            Destination::Manifest => {
                self.center = Center::Manifest;
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::new("reveal-manifest-fields"), true)
                });
            }
            Destination::Asset(path) => {
                self.center = Center::Assets;
                self.asset_selection = path;
            }
            Destination::Assets => self.center = Center::Assets,
            Destination::Events => self.center = Center::Events,
            Destination::Game => {
                self.center = Center::Tools;
                self.tools_panel.page = ToolPage::Game;
            }
            Destination::PreviewLibrary => {
                self.center = Center::Tools;
                self.tools_panel.page = ToolPage::Previews;
            }
            Destination::Settings => self.settings_open = true,
            Destination::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("作品、剧情或脚本", &["json", "lua", "lommod"])
                    .pick_file()
                {
                    self.request(Pending::Open(path), ctx);
                }
            }
            Destination::Current => {}
        }
    }
    fn open_tool_page(&mut self, page: ToolPage) {
        if self.open_center(Center::Tools) {
            self.tools_panel.page = page;
        }
    }
    fn open_center(&mut self, center: Center) -> bool {
        if !self.apply_pending_drafts() {
            return false;
        }
        self.flush();
        self.center = center;
        if matches!(
            center,
            Center::Events | Center::Tasks | Center::Manifest | Center::Initial
        ) {
            self.auto = false;
            self.audio.stop();
        }
        true
    }
    fn navigate(&mut self, section: Section) {
        if self.section() == section && section != Section::Story {
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        self.preview.cancel_layout_drag();
        match section {
            Section::Story => self.center = Center::Node,
            Section::Materials => self.center = Center::Assets,
            Section::Game => self.open_tool_page(ToolPage::Game),
            Section::Release => self.open_tool_page(ToolPage::Release),
        }
    }
    fn story_panels_visible(&self) -> bool {
        self.section() == Section::Story
            && !(self.center == Center::Tools && self.tools_panel.page == ToolPage::Maintenance)
    }
    fn material_navigation(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("素材管理"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.center == Center::Assets, tr("作品素材"))
                .clicked()
                && self.apply_pending_drafts()
            {
                self.center = Center::Assets;
            }
            for (page, label) in [
                (ToolPage::Previews, "游戏预览素材"),
                (ToolPage::Library, "共享内容库"),
            ] {
                if ui
                    .selectable_label(
                        self.center == Center::Tools && self.tools_panel.page == page,
                        tr(label),
                    )
                    .clicked()
                {
                    self.open_tool_page(page);
                }
            }
        });
        ui.separator();
    }
    fn play_current(&mut self) {
        self.play(false);
    }
    fn play_chapter_start(&mut self) {
        self.play(true);
    }
    fn play(&mut self, from_start: bool) {
        if !self.apply_pending_drafts() {
            return;
        }
        if !self.check_before_run_or_export() {
            return;
        }
        if cfg!(windows) {
            self.tools_panel.refresh_game();
            if !self.tools_panel.ready_to_play() {
                self.open_tool_page(ToolPage::Game);
                self.status = tr("请先完成游戏配置，再返回剧情试玩。");
                return;
            }
        }
        self.flush();
        let node = self.playtest_node(from_start);
        match self
            .tools_panel
            .play_from_current(&self.project, &self.current, &node)
        {
            Ok(status) => self.status = status,
            Err(e) => self.error = Some(crate::error_ui::capture(e)),
        }
    }
    fn playtest_node(&self, from_start: bool) -> String {
        let story = self.story();
        (if from_start {
            &story["start"]
        } else {
            &story["nodes"][self.selected]["id"]
        })
        .as_str()
        .unwrap_or("")
        .to_owned()
    }
    fn check_before_run_or_export(&mut self) -> bool {
        self.error_issues = lom_core::release::run_preflight(
            &self.project,
            lom_core::release::Profile::Editing,
            env!("CARGO_PKG_VERSION"),
        )
        .into_iter()
        .filter(|issue| issue.severity == "error")
        .collect();
        if self.error_issues.is_empty() {
            return true;
        }
        self.error = Some(crate::i18n::message(
            "editor.error.issue_count",
            &[("count", self.error_issues.len().to_string())],
        ));
        false
    }
    fn open_initial_setup(&mut self) {
        if self.is_vanilla_patch() {
            return;
        }
        if !self.open_center(Center::Initial) {
            return;
        }
        if let Some(config) = &self.project.scenario {
            if let Some(script) = config["events"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|event| event["id"] == config["entry_event"])
                .and_then(|event| event["script"].as_str())
                .filter(|id| self.project.stories.contains_key(*id))
            {
                self.current = script.to_owned();
            }
        }
        self.selected = 0;
        self.multiselect.clear();
    }
    fn step_palette(&mut self, ctx: &egui::Context) {
        let story = self.project.stories.get(&self.current);
        let imported = story.is_some_and(|story| story.get("vanilla_import").is_some());
        let structured = story.is_some_and(|story| story["vanilla_import"]["schema"] == 2);
        if story.is_some_and(|story| story.get("vanilla_patch").is_some()) {
            return;
        }
        if !self.steps_panel_open || !matches!(self.center, Center::Node | Center::Chapter) {
            return;
        }
        egui::SidePanel::left("step-palette")
            .frame(surface(true))
            .default_width(168.0)
            .min_width(140.0)
            .max_width(260.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(crate::i18n::key("editor.workspace.add_steps"));
                    if ui
                        .small_button("×")
                        .on_hover_text(crate::i18n::key("editor.workspace.collapse_steps"))
                        .clicked()
                    {
                        self.steps_panel_open = false;
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.node_search)
                        .desired_width(f32::INFINITY)
                        .hint_text(crate::i18n::key("editor.workspace.search_steps")),
                );
                ui.small(crate::i18n::key("editor.workspace.insert_hint"));
                if ui
                    .button(crate::i18n::key("editor.workspace.parallel"))
                    .on_hover_text(crate::i18n::key("editor.workspace.parallel_hint"))
                    .clicked()
                {
                    self.add_node("parallel");
                }
                ui.separator();
                let groups = self.catalog.schema["NODE_GROUPS_SRC"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let search = self.node_search.to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("step-palette-list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (group_i, group) in groups.iter().enumerate() {
                            let kinds: Vec<_> = group[1]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .filter(|kind| {
                                    structured
                                        || !matches!(*kind, "native_action" | "native_branch")
                                })
                                .filter(|kind| {
                                    self.project.scenario.is_none()
                                        || imported
                                        || lom_core::scenario::performance_node(kind)
                                })
                                .filter(|kind| {
                                    search.is_empty()
                                        || format!("{} {kind}", self.catalog.label(kind))
                                            .to_lowercase()
                                            .contains(&search)
                                })
                                .collect();
                            if kinds.is_empty() {
                                continue;
                            }
                            egui::CollapsingHeader::new(crate::i18n::key(
                                [
                                    "workspace.palette.stage",
                                    "workspace.palette.stats",
                                    "workspace.palette.gameplay",
                                    "workspace.palette.flow",
                                ][group_i.min(3)],
                            ))
                            .id_salt(("palette-group", group_i))
                            .default_open(true)
                            .show(ui, |ui| {
                                for kind in kinds {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 26.0],
                                            egui::Button::new(self.catalog.label(kind)),
                                        )
                                        .on_hover_text(kind)
                                        .clicked()
                                    {
                                        self.add_node(kind);
                                    }
                                }
                            });
                        }
                    });
            });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("chapters")
            .frame(surface(true))
            .default_width(shell::NAVIGATION_WIDTH)
            .min_width(180.0)
            .max_width((ctx.available_rect().width() - 620.0).max(180.0))
            .resizable(true)
            .show(ctx, |ui| {
                if self.screenshot.is_some() && self.frame == 12 {
                    println!("Layout navigation width: {}", ui.max_rect().width());
                }
                if self.project.scenario.is_some() {
                    ui.strong(
                        self.project.manifest["name"]
                            .as_str()
                            .unwrap_or("未命名作品"),
                    );
                    ui.horizontal(|ui| {
                        ui.small(crate::i18n::key(
                            if self.project.manifest["play_mode"] == "addon" {
                                "workspace.mode.addon"
                            } else {
                                "workspace.mode.free"
                            },
                        ));
                        if ui
                            .small_button(crate::i18n::key("editor.workspace.release_settings"))
                            .clicked()
                        {
                            self.open_center(Center::Manifest);
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .selectable_label(
                                matches!(self.center, Center::Node | Center::Chapter),
                                crate::i18n::key("editor.workspace.story_content"),
                            )
                            .clicked()
                        {
                            self.open_center(Center::Node);
                        }
                        if ui
                            .selectable_label(
                                matches!(self.center, Center::Events | Center::Tasks),
                                crate::i18n::key("editor.workspace.schedule"),
                            )
                            .clicked()
                        {
                            self.open_center(Center::Events);
                        }
                        if ui
                            .selectable_label(
                                self.center == Center::Initial,
                                crate::i18n::key("editor.workspace.initial"),
                            )
                            .clicked()
                        {
                            self.open_initial_setup();
                        }
                    });
                    ui.separator();
                    if matches!(self.center, Center::Events | Center::Tasks) {
                        let task_page = self.center == Center::Tasks;
                        egui::ScrollArea::vertical()
                            .id_salt("scenario_navigation")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if let Some(tasks) = crate::scenario_panel::navigation(
                                    ui,
                                    &mut self.project,
                                    task_page,
                                    &mut self.scenario_selection,
                                ) {
                                    self.open_center(if tasks {
                                        Center::Tasks
                                    } else {
                                        Center::Events
                                    });
                                }
                            });
                        return;
                    }
                    if self.center == Center::Initial {
                        ui.strong(crate::i18n::key("editor.workspace.opening"));
                        ui.small(crate::i18n::key("editor.workspace.opening_hint"));
                        return;
                    }
                    if self.center == Center::Manifest {
                        ui.strong(tr("作品设置"));
                        ui.small(crate::i18n::key("editor.workspace.settings_hint"));
                        return;
                    }
                    ui.small(crate::i18n::key("editor.workspace.story_hint"));
                }
                egui::TopBottomPanel::bottom("add-step-bottom")
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
                    .show_inside(ui, |ui| {
                        ui.toggle_value(&mut self.steps_panel_open, tr("＋ 添加步骤"))
                            .on_hover_text(crate::i18n::key("editor.workspace.panel_toggle"));
                        ui.small(crate::i18n::key("editor.workspace.panel_resize"));
                    });
                let old = self.current.clone();
                let mut chapter_names: Vec<_> = self
                    .project
                    .stories
                    .iter()
                    .map(|(id, story)| {
                        (id.clone(), story["title"].as_str().unwrap_or(id).to_owned())
                    })
                    .collect();
                crate::forms::number_duplicate_labels(&mut chapter_names);
                ui.label(crate::i18n::key("nav.story"));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("chapters")
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .truncate()
                        .width((ui.available_width() - 112.0).max(75.0))
                        .selected_text(
                            chapter_names
                                .iter()
                                .find(|(id, _)| id == &self.current)
                                .map(|(_, name)| name.as_str())
                                .unwrap_or(&self.current),
                        )
                        .show_ui(ui, |ui| {
                            let mut groups = BTreeMap::<&str, Vec<(&String, &String)>>::new();
                            for (id, name) in &chapter_names {
                                groups
                                    .entry(chapter_group(&self.project.stories[id]))
                                    .or_default()
                                    .push((id, name));
                            }
                            let grouped = groups.keys().any(|group| !group.is_empty());
                            for (group, chapters) in groups {
                                if grouped {
                                    egui::CollapsingHeader::new(if group.is_empty() {
                                        tr("未分组")
                                    } else {
                                        group.to_owned()
                                    })
                                    .id_salt(("chapter-group", group))
                                    .default_open(
                                        chapters.iter().any(|(id, _)| *id == &self.current),
                                    )
                                    .show(ui, |ui| {
                                        for (id, name) in chapters {
                                            if ui
                                                .selectable_value(
                                                    &mut self.current,
                                                    id.clone(),
                                                    name,
                                                )
                                                .clicked()
                                            {
                                                ui.close_kind(egui::UiKind::Menu);
                                            }
                                        }
                                    });
                                } else {
                                    for (id, name) in chapters {
                                        if ui
                                            .selectable_value(&mut self.current, id.clone(), name)
                                            .clicked()
                                        {
                                            ui.close_kind(egui::UiKind::Menu);
                                        }
                                    }
                                }
                            }
                        });
                    ui.menu_button("＋", |ui| {
                        if ui.button(tr("新建章节…")).clicked() {
                            self.begin_chapter(false);
                            ui.close();
                        }
                        if ui.button(tr("复制当前章节…")).clicked() {
                            self.begin_chapter(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(tr("新建章节分组…")).clicked() {
                            self.begin_chapter_group(false);
                            ui.close();
                        }
                        let current_group = chapter_group(&self.story()).to_owned();
                        if ui
                            .add_enabled(
                                !current_group.is_empty(),
                                egui::Button::new(tr("重命名当前分组…")),
                            )
                            .clicked()
                        {
                            self.begin_chapter_group(true);
                            ui.close();
                        }
                        ui.menu_button(tr("移入分组"), |ui| {
                            let groups: BTreeSet<_> = self
                                .project
                                .stories
                                .values()
                                .map(chapter_group)
                                .filter(|group| !group.is_empty())
                                .map(str::to_owned)
                                .collect();
                            for group in std::iter::once(String::new()).chain(groups) {
                                if ui
                                    .selectable_label(
                                        current_group == group,
                                        if group.is_empty() {
                                            tr("未分组")
                                        } else {
                                            group.clone()
                                        },
                                    )
                                    .clicked()
                                {
                                    if let Err(error) = self.move_chapter_to_group(&group) {
                                        self.error = Some(crate::error_ui::capture(error));
                                    }
                                    ui.close();
                                }
                            }
                        });
                    });

                    ui.menu_button("…", |ui| {
                        if ui.button(tr("复制")).clicked() {
                            self.copy();
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.clipboard.is_empty(), egui::Button::new(tr("粘贴")))
                            .clicked()
                        {
                            self.paste();
                            ui.close();
                        }
                        if ui.button(tr("上移")).clicked() && self.selected > 0 {
                            self.move_node(self.selected - 1);
                            ui.close();
                        }
                        if ui.button(tr("下移")).clicked() {
                            self.move_node(self.selected + 1);
                            ui.close();
                        }
                        if ui.button(tr("删除")).clicked() {
                            self.delete();
                            ui.close();
                        }
                        if ui.button(tr("存为模板")).clicked() {
                            self.open_tool_page(ToolPage::Editing);
                            ui.close();
                        }
                        ui.separator();
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text(tr("搜索所有章节 · ID / 台词 / 角色")),
                        );
                    });
                });
                if old != self.current {
                    self.flush();
                    self.selected = 0;
                    self.multiselect.clear();
                    self.needs_compile = true;
                    self.center = Center::Node;
                }
                ui.add_space(4.0);
                if self.project.scenario.is_none()
                    && ui
                        .selectable_label(
                            self.center == Center::Initial,
                            crate::i18n::key("editor.workspace.initial"),
                        )
                        .clicked()
                {
                    self.open_initial_setup();
                }
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new(tr("章节设置")).selected(self.center == Center::Chapter),
                    )
                    .on_hover_text(tr("修改章节名称、入口步骤，或删除当前章节"))
                    .clicked()
                {
                    self.open_center(Center::Chapter);
                }
                ui.separator();
                if !self.search.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: {}", tr("查找"), self.search));
                        if ui.small_button("×").clicked() {
                            self.search.clear();
                        }
                    });
                }
                let story = self.story();
                if let Some(patch) = story.get("vanilla_patch") {
                    ui.strong(crate::i18n::key("editor.patch.title"));
                    ui.label(crate::i18n::message(
                        "editor.patch.chapter",
                        &[("target", patch["target"].as_str().unwrap_or("").into())],
                    ));
                    ui.label(crate::i18n::key("editor.patch.insert_hint"));
                    let used: BTreeSet<_> = self
                        .project
                        .stories
                        .values()
                        .filter_map(|story| {
                            lom_core::vanilla_import::target(story).map(str::to_owned)
                        })
                        .collect();
                    let next = self.vanilla.next_target(&used);
                    if ui
                        .add_enabled(
                            next.is_some(),
                            egui::Button::new(crate::i18n::key("editor.patch.add_chapter")),
                        )
                        .clicked()
                    {
                        let target = next.unwrap();
                        let id = (1..)
                            .map(|i| format!("patch{i}"))
                            .find(|id| !self.project.stories.contains_key(id))
                            .unwrap();
                        match lom_core::vanilla_patch::new_story(&id, &target) {
                            Ok(story) => {
                                self.flush();
                                self.project.stories.insert(id.clone(), story);
                                self.current = id;
                                self.selected = 0;
                                self.multiselect.clear();
                            }
                            Err(error) => self.error = Some(crate::error_ui::capture(error)),
                        }
                    }
                    return;
                }
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                if !self.search.is_empty() {
                    egui::ScrollArea::vertical()
                        .id_salt("step-search")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let query = self.search.to_lowercase();
                            for (sid, s) in self.project.stories.clone() {
                                for (index, node) in
                                    s["nodes"].as_array().into_iter().flatten().enumerate()
                                {
                                    if node.to_string().to_lowercase().contains(&query)
                                        && ui
                                            .selectable_label(
                                                self.current == sid && self.selected == index,
                                                format!(
                                                    "{} · {:02} {} · {}",
                                                    s["title"].as_str().unwrap_or(&sid),
                                                    index + 1,
                                                    self.catalog
                                                        .label(node["type"].as_str().unwrap_or("")),
                                                    short(node["text"].as_str().unwrap_or(""), 22)
                                                ),
                                            )
                                            .on_hover_text(format!(
                                                "{sid} / {}",
                                                node["id"].as_str().unwrap_or("")
                                            ))
                                            .clicked()
                                    {
                                        self.current = sid.clone();
                                        self.selected = index;
                                        self.center = Center::Node;
                                        self.needs_compile = true;
                                        if let Some(s) = self.project.stories.get_mut(&sid) {
                                            authoring::expand_for_node(s, index);
                                        }
                                    }
                                }
                            }
                        });
                    return;
                }
                if let Err(error) = authoring::validate_sections(&story) {
                    crate::error_ui::show_with_actions(ui, &crate::error_ui::capture(error));
                }
                let nodes = story["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let rows = authoring::section_rows(&story);
                let row_height = 44.0;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("steps")
                    .auto_shrink([false, false]);
                if self.scroll_selection != (self.current.clone(), self.selected) {
                    if let Some(row) = rows.iter().position(|row| {
                        matches!(row, authoring::Row::Node { index, .. } if *index == self.selected)
                    }) {
                        let offset = row as f32 * (row_height + ui.spacing().item_spacing.y)
                            - (ui.available_height() - row_height) * 0.5;
                        scroll = scroll.vertical_scroll_offset(offset.max(0.0));
                        self.scroll_selection = (self.current.clone(), self.selected);
                    }
                }
                scroll.show_rows(ui, row_height, rows.len(), |ui, visible| {
                    let mut drop_to = None;
                    for row in rows.into_iter().skip(visible.start).take(visible.len()) {
                        let (index, depth) = match row {
                            authoring::Row::Header {
                                section,
                                group,
                                depth,
                                title,
                                collapsed,
                            } => {
                                let response = ui
                                    .horizontal(|ui| {
                                        ui.set_min_height(row_height);
                                        ui.add_space(depth as f32 * 10.0);
                                        ui.selectable_label(
                                            false,
                                            format!(
                                                "{} {}",
                                                if collapsed { "▶" } else { "▼" },
                                                title
                                            ),
                                        )
                                    })
                                    .inner;
                                if response.clicked() {
                                    let s = self.project.stories.get_mut(&self.current).unwrap();
                                    let item = if let Some(g) = group {
                                        &mut s["_editor"]["sections"][section]["groups"][g]
                                    } else {
                                        &mut s["_editor"]["sections"][section]
                                    };
                                    item["collapsed"] = (!collapsed).into();
                                }
                                continue;
                            }
                            authoring::Row::Node { index, depth } => (index, depth),
                        };
                        let node = &nodes[index];
                        let summary = if matches!(
                            node["type"].as_str(),
                            Some("native_action" | "native_branch")
                        ) {
                            crate::native_forms::summary(node)
                        } else if node["type"] == "parallel" {
                            format!(
                                "{} {}",
                                node["actions"].as_array().map_or(0, Vec::len),
                                tr("个同时动作")
                            )
                        } else if node["type"] == "end" {
                            if let Some(next) =
                                node["next_script"].as_str().filter(|s| !s.is_empty())
                            {
                                format!(
                                    "{} {}",
                                    tr("转到章节"),
                                    self.project
                                        .stories
                                        .get(next)
                                        .and_then(|s| s["title"].as_str())
                                        .unwrap_or(next)
                                )
                            } else {
                                tr("返回自由模式")
                            }
                        } else if let Some(text) = node["text"].as_str().filter(|v| !v.is_empty()) {
                            text.to_owned()
                        } else if let Some(character) =
                            node["character"].as_str().filter(|v| !v.is_empty())
                        {
                            let name = self.catalog.data["characters"]
                                .as_array()
                                .and_then(|a| a.iter().find(|v| v["id"] == character))
                                .and_then(|v| v["name"].as_str())
                                .unwrap_or(character);
                            let name = crate::i18n::term("characters", character, name);
                            let position = node["position"]
                                .as_str()
                                .or_else(|| node["to"].as_str())
                                .unwrap_or("");
                            if position.is_empty() {
                                name
                            } else {
                                format!("{name} · {position}")
                            }
                        } else {
                            node["title"]
                                .as_str()
                                .or_else(|| node["view"].as_str())
                                .or_else(|| node["next_script"].as_str())
                                .unwrap_or("")
                                .to_owned()
                        };
                        let response = ui
                            .horizontal(|ui| {
                                ui.add_space(depth as f32 * 9.0);
                                let selected =
                                    self.selected == index || self.multiselect.contains(&index);
                                let primary = egui::TextFormat {
                                    font_id: egui::FontId::proportional(14.0),
                                    color: ui.visuals().text_color(),
                                    ..Default::default()
                                };
                                let secondary = egui::TextFormat {
                                    font_id: egui::FontId::proportional(12.0),
                                    color: if selected {
                                        Color32::from_rgb(70, 96, 120)
                                    } else {
                                        Color32::from_rgb(103, 113, 125)
                                    },
                                    ..Default::default()
                                };
                                let mut label = egui::text::LayoutJob::default();
                                label.wrap.max_width = (ui.available_width() - 24.0).max(40.0);
                                label.wrap.max_rows = 2;
                                label.append(
                                    &format!("{:02}  ", index + 1),
                                    0.0,
                                    secondary.clone(),
                                );
                                label.append(
                                    &self.catalog.label(node["type"].as_str().unwrap_or("")),
                                    0.0,
                                    primary,
                                );
                                let remark = node["_editor"]["note"]
                                    .as_str()
                                    .filter(|note| !note.trim().is_empty())
                                    .or_else(|| node["id"].as_str())
                                    .unwrap_or("");
                                let preview = if summary.is_empty() {
                                    remark.to_owned()
                                } else {
                                    format!("{remark} · {summary}")
                                };
                                let preview =
                                    preview.split_whitespace().collect::<Vec<_>>().join(" ");
                                if !preview.is_empty() {
                                    label.append("\n", 0.0, secondary.clone());
                                    label.append(&short(&preview, 26), 23.0, secondary);
                                }
                                let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
                                ui.add_sized(
                                    [ui.available_width(), row_height],
                                    egui::Button::new(label)
                                        .right_text("")
                                        .selected(
                                            self.selected == index
                                                || self.multiselect.contains(&index),
                                        )
                                        .sense(egui::Sense::click_and_drag()),
                                )
                            })
                            .inner
                            .on_hover_text(format!(
                                "{}\n{}",
                                node["id"].as_str().unwrap_or(""),
                                summary
                            ));
                        if response.clicked() {
                            let modifiers = ui.input(|i| i.modifiers);
                            if modifiers.shift {
                                self.multiselect =
                                    (self.selected.min(index)..=self.selected.max(index)).collect();
                            } else if modifiers.command || modifiers.ctrl {
                                if self.multiselect.is_empty() {
                                    self.multiselect.insert(self.selected);
                                }
                                if !self.multiselect.insert(index) {
                                    self.multiselect.remove(&index);
                                }
                            } else {
                                self.multiselect.clear();
                            }
                            self.selected = index;
                            self.center = Center::Node;
                        }
                        response.context_menu(|ui| {
                            if ui.button(tr("复制")).clicked() {
                                self.copy();
                                ui.close();
                            }
                            if ui.button(tr("粘贴")).clicked() {
                                self.paste();
                                ui.close();
                            }
                            if ui.button(tr("删除")).clicked() {
                                self.delete();
                                ui.close();
                            }
                        });
                        if response.drag_started() {
                            response.dnd_set_drag_payload(index);
                        }
                        if let Some(from) = response.dnd_release_payload::<usize>() {
                            drop_to = Some((*from, index));
                        }
                    }
                    if let Some((from, to)) = drop_to {
                        self.selected = from;
                        self.move_node(to);
                    }
                });
            });
    }
    fn sync_preview_selection(&mut self) {
        let location = (self.current.clone(), self.selected);
        if self.preview_location != location {
            self.preview_location = location;
            self.auto = false;
            self.preview_waiting_branch = false;
            self.preview_elapsed = None;
            self.last_step = Instant::now();
        }
    }
    fn advance_preview(&mut self) {
        if self.auto
            && !self.preview_waiting_branch
            && self.last_step.elapsed().as_secs_f64()
                >= preview::playback_duration(&self.story()["nodes"][self.selected])
        {
            let story = self.story();
            let node = &story["nodes"][self.selected];
            let branching = [
                "choice",
                "branch",
                "dice",
                "combat",
                "battle",
                "battle_result",
                "stat_check",
                "affinity_check",
                "item_check",
                "talent_check",
                "flag_check",
                "activity",
                "quest_check",
                "persistent_check",
            ];
            if branching.contains(&node["type"].as_str().unwrap_or("")) {
                self.preview_waiting_branch = true;
                self.preview_elapsed = Some(preview::playback_duration(node));
                self.status = "流程在条件或选项处分叉，等待选择后继续播放".into();
            } else {
                let targets = lom_core::analysis::successors(&story, self.selected);
                if targets.len() == 1 {
                    if let Some(index) = story["nodes"]
                        .as_array()
                        .and_then(|nodes| nodes.iter().position(|n| n["id"] == targets[0]))
                    {
                        self.selected = index;
                        self.preview_location = (self.current.clone(), index);
                        self.preview_elapsed = Some(0.0);
                    } else {
                        self.auto = false;
                        self.preview_elapsed = Some(preview::playback_duration(node));
                    }
                } else {
                    self.auto = false;
                    self.preview_elapsed = Some(preview::playback_duration(node));
                }
            }
            self.last_step = Instant::now();
        }
    }
    fn follow_preview_target(&mut self, target: &str) {
        let story = self.story();
        if let Some(index) = story["nodes"].as_array().and_then(|nodes| {
            nodes
                .iter()
                .position(|node| node["id"].as_str() == Some(target))
        }) {
            // A choice is a playback transition, not manual editor navigation.
            // Preserve the user's play/pause state and start the target's clock.
            self.selected = index;
            self.preview_location = (self.current.clone(), index);
            self.preview_waiting_branch = false;
            self.preview_elapsed = self.auto.then_some(0.0);
            self.last_step = Instant::now();
            self.status.clear();
        }
    }
    fn follow_scenario_link(&mut self, link: crate::scenario_panel::Navigate) {
        use crate::scenario_panel::Navigate;
        match link {
            Navigate::Story(id) => {
                if self.open_center(Center::Node) && self.project.stories.contains_key(&id) {
                    self.current = id;
                    self.selected = 0;
                    self.multiselect.clear();
                    self.needs_compile = true;
                }
            }
            Navigate::Event(id) => {
                if self.open_center(Center::Events) {
                    self.scenario_selection.event = Some(id);
                }
            }
            Navigate::Task(id) => {
                if self.open_center(Center::Tasks) {
                    self.scenario_selection.task = Some(id);
                }
            }
        }
    }
    fn right(&mut self, ctx: &egui::Context) {
        if self.center == Center::Node && self.is_vanilla_patch() {
            if self.vanilla.route_open() {
                return;
            }
            let story = self.story();
            egui::SidePanel::right("vanilla-preview")
                .frame(surface(true))
                .default_width(340.0)
                .resizable(true)
                .show(ctx, |ui| {
                    self.vanilla.preview(ui, &story);
                });
            return;
        }
        if self.view != View::Stage {
            self.preview.cancel_layout_drag();
        }
        if self.project.scenario.is_some()
            && matches!(
                self.center,
                Center::Events | Center::Tasks | Center::Manifest
            )
        {
            self.preview.cancel_layout_drag();
            egui::SidePanel::right("scenario_context")
                .frame(surface(true))
                .default_width(300.0)
                .min_width(280.0)
                .max_width((ctx.available_rect().width() - 350.0).max(280.0))
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if self.center == Center::Manifest {
                            ui.heading(crate::i18n::key("editor.workspace.overview"));
                            ui.label(crate::i18n::key(
                                if self.project.manifest["play_mode"] == "addon" {
                                    "workspace.mode.addon_hint"
                                } else {
                                    "workspace.mode.free_hint"
                                },
                            ));
                            ui.add_space(12.0);
                            ui.label(crate::i18n::message(
                                "editor.workspace.story_count",
                                &[("count", self.project.stories.len().to_string())],
                            ));
                            if let Some(c) = &self.project.scenario {
                                ui.label(crate::i18n::message(
                                    "workspace.overview.counts",
                                    &[
                                        (
                                            "events",
                                            c["events"].as_array().map_or(0, Vec::len).to_string(),
                                        ),
                                        (
                                            "tasks",
                                            c["tasks"].as_array().map_or(0, Vec::len).to_string(),
                                        ),
                                    ],
                                ));
                            }
                        } else if let Some(link) = crate::scenario_panel::context(
                            ui,
                            &self.project,
                            self.center == Center::Tasks,
                            &self.scenario_selection,
                        ) {
                            self.follow_scenario_link(link);
                        }
                    });
                });
            return;
        }
        self.sync_preview_selection();
        egui::SidePanel::right("preview")
            .frame(surface(true))
            .default_width(shell::PREVIEW_WIDTH)
            .min_width(320.0)
            .max_width((ctx.available_rect().width() - 300.0).max(320.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (view, key) in [
                        (View::Stage, "tab.preview"),
                        (View::Portraits, "portrait.preview"),
                        (View::Graph, "tab.flow"),
                        (View::Lua, "tab.compile"),
                    ] {
                        let selected =
                            self.view == view || (view == View::Lua && self.view == View::Checks);
                        if ui
                            .add(
                                egui::Button::selectable(selected, crate::i18n::key(key))
                                    .frame(selected),
                            )
                            .clicked()
                        {
                            self.preview.cancel_layout_drag();
                            self.view = view;
                        }
                    }
                });
                ui.separator();
                let mut story = self.story();
                match self.view {
                    View::Stage => {
                        egui::TopBottomPanel::bottom("stage-transport")
                            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
                            .show_inside(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add_space(((ui.available_width() - 350.0) / 2.0).max(0.0));
                                    if ui.button(crate::i18n::key("stage.home")).clicked() {
                                        self.selected = story["nodes"]
                                            .as_array()
                                            .and_then(|a| {
                                                a.iter().position(|n| n["id"] == story["start"])
                                            })
                                            .unwrap_or(0);
                                    }
                                    if ui.button(crate::i18n::key("stage.prev")).clicked() {
                                        self.selected = self.selected.saturating_sub(1);
                                    }
                                    ui.label(format!(
                                        "{} / {}",
                                        self.selected + 1,
                                        story["nodes"].as_array().map(Vec::len).unwrap_or(0)
                                    ));
                                    if ui.button(crate::i18n::key("stage.next")).clicked() {
                                        self.selected = (self.selected + 1).min(
                                            story["nodes"]
                                                .as_array()
                                                .map(Vec::len)
                                                .unwrap_or(1)
                                                .saturating_sub(1),
                                        );
                                    }
                                    if ui
                                        .button(crate::i18n::key(if self.auto {
                                            "stage.pause"
                                        } else {
                                            "stage.play"
                                        }))
                                        .clicked()
                                    {
                                        if self.auto {
                                            self.preview_elapsed =
                                                Some(self.last_step.elapsed().as_secs_f64());
                                            self.auto = false;
                                            self.preview_waiting_branch = false;
                                        } else {
                                            self.last_step = Instant::now()
                                                - Duration::from_secs_f64(
                                                    self.preview_elapsed.unwrap_or(0.0),
                                                );
                                            self.preview_elapsed = Some(0.0);
                                            self.auto = true;
                                        }
                                    }
                                });
                                let node = &story["nodes"][self.selected];
                                if node["type"] == "transition" {
                                    ui.label(RichText::new(tr("黑幕转场预览（节奏示意）")).small());
                                } else if self
                                    .preview
                                    .state(&story, node["id"].as_str().unwrap_or(""))["blackout"]
                                    ["amount"]
                                    == 1.0
                                {
                                    ui.label(
                                        RichText::new(tr("黑幕遮挡中；播放到揭幕步骤后显示画面"))
                                            .small(),
                                    );
                                } else if node["type"] == "parallel" {
                                    ui.label(RichText::new(tr("预览人物移动与淡入淡出")).small());
                                }
                            });
                        self.sync_preview_selection();
                        if self.preview.layout_mode {
                            self.auto = false;
                            self.preview_waiting_branch = false;
                            self.preview_elapsed = None;
                        }
                        let elapsed = if self.auto && !self.preview_waiting_branch {
                            Some(self.last_step.elapsed().as_secs_f64())
                        } else {
                            self.preview_elapsed
                        };
                        if let Some(target) =
                            self.preview
                                .show(ui, &self.project, &story, self.selected, elapsed)
                        {
                            self.follow_preview_target(&target);
                        }
                        if let Some(edit) = self.preview.mood_edit.take() {
                            self.auto = false;
                            self.preview_waiting_branch = false;
                            self.preview_elapsed = None;
                            let node = &mut story["nodes"][self.selected];
                            let mood = if let Some(action) = edit.action {
                                &mut node["actions"][action]
                            } else {
                                node
                            };
                            mood["offset_x"] = json!(edit.x);
                            mood["offset_y"] = json!(edit.y);
                            self.project
                                .stories
                                .insert(self.current.clone(), story.clone());
                        }
                        if let Some(change) = self.preview.layout_edit.take() {
                            let mut next = story.clone();
                            match crate::preview_layout::apply(&mut next, self.selected, &change) {
                                Ok(selected) => {
                                    self.track();
                                    self.flush();
                                    story = next;
                                    self.project
                                        .stories
                                        .insert(self.current.clone(), story.clone());
                                    self.selected = selected;
                                    self.multiselect.clear();
                                    self.auto = false;
                                    self.preview_elapsed = None;
                                    self.track();
                                    self.flush();
                                    self.status = "已调整画面，可撤销".into();
                                }
                                Err(error) => self.error = Some(crate::error_ui::capture(error)),
                            }
                        }
                    }
                    View::Portraits => {
                        self.preview
                            .show_portrait(ui, &self.project, &story, self.selected);
                    }
                    View::Graph => {
                        if let Some(i) = preview::graph(ui, &self.project, &story, self.selected) {
                            self.selected = i;
                            self.center = Center::Node;
                        }
                    }
                    View::Lua | View::Checks => {
                        ui.horizontal(|ui| {
                            if ui.button(tr("检查项目")).clicked() {
                                self.compile();
                            }
                            if ui.button(tr("复制 Lua")).clicked() {
                                match compile_with_assets(&self.project, &story) {
                                    Ok(lua) => ctx.copy_text(lua),
                                    Err(error) => {
                                        self.error = Some(crate::error_ui::capture(error))
                                    }
                                }
                            }
                            if ui.button(tr("导出 Lua…")).clicked() {
                                match compile_with_assets(&self.project, &story) {
                                    Ok(lua) => {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .set_file_name(format!("{}.lua", self.current))
                                            .save_file()
                                        {
                                            if let Err(error) = lom_core::project::atomic_write(
                                                &path,
                                                lua.as_bytes(),
                                            ) {
                                                self.error = Some(crate::error_ui::capture(error));
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        self.error = Some(crate::error_ui::capture(error))
                                    }
                                }
                            }
                        });
                        if !self.diagnostics.is_empty() {
                            egui::ScrollArea::vertical()
                                .id_salt("compile-errors")
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    for issue in self.diagnostics.clone() {
                                        let location = if issue.node_id.is_empty() {
                                            issue.story_id.clone()
                                        } else {
                                            format!("{}/{}", issue.story_id, issue.node_id)
                                        };
                                        if ui.link(location).clicked() {
                                            if let Some(story) =
                                                self.project.stories.get_mut(&issue.story_id)
                                            {
                                                if let Some(index) =
                                                    story["nodes"].as_array().and_then(|nodes| {
                                                        nodes.iter().position(|node| {
                                                            !issue.node_id.is_empty()
                                                                && node["id"] == issue.node_id
                                                        })
                                                    })
                                                {
                                                    self.selected = index;
                                                    self.center = Center::Node;
                                                    authoring::expand_for_node(story, index);
                                                } else {
                                                    self.center = Center::Chapter;
                                                }
                                                self.current = issue.story_id;
                                                self.multiselect.clear();
                                                self.search.clear();
                                                self.needs_compile = true;
                                            }
                                        }
                                        let message = if issue.severity == "error" {
                                            format!("错误：{}", issue.message)
                                        } else {
                                            issue.message
                                        };
                                        ui.colored_label(
                                            Color32::from_rgb(179, 55, 49),
                                            crate::error_ui::text(&message),
                                        );
                                    }
                                });
                        }
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.lua)
                                    .code_editor()
                                    .interactive(false)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    }
                    View::Statistics => {
                        let analysis = lom_core::analysis::analyze_project(
                            &self.project.stories,
                            &self.project.manifest,
                        );
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            report(ui, &analysis["stats"], 0);
                        });
                    }
                }
            });
    }
    fn center(&mut self, ctx: &egui::Context) {
        if self.center == Center::Node && self.is_vanilla_patch() {
            let mut story = self.story();
            let used = self
                .project
                .stories
                .iter()
                .filter(|(id, _)| *id != &self.current)
                .filter_map(|(_, story)| lom_core::vanilla_import::target(story).map(str::to_owned))
                .collect();
            egui::CentralPanel::default()
                .frame(surface(true))
                .show(ctx, |ui| {
                    if self.vanilla.route_open() {
                        self.vanilla.show(
                            ui,
                            &mut story,
                            &self.catalog,
                            self.tools_panel.game_directory(),
                            &used,
                        );
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.vanilla.show(
                                ui,
                                &mut story,
                                &self.catalog,
                                self.tools_panel.game_directory(),
                                &used,
                            );
                        });
                    }
                });
            let imported = self.vanilla.take_imported();
            if imported {
                self.flush();
            }
            self.project.stories.insert(self.current.clone(), story);
            if imported {
                self.selected = 0;
                self.multiselect.clear();
                self.view = View::Stage;
                self.auto = false;
                self.audio.stop();
                self.needs_compile = true;
                self.track();
                self.flush();
                self.status = "已导入原版整章，可在步骤间插入内容。F5 从原版章节入口试玩。".into();
            }
            if let Some(mark) = self.vanilla.take_bookmark() {
                self.open_story_bookmark(mark, ctx);
            }
            return;
        }
        egui::CentralPanel::default()
            .frame(surface(true))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(format!(
                        "properties-{:?}-{:?}",
                        self.center, self.tools_panel.page
                    ))
                    .show(ui, |ui| match self.center {
                        Center::Node => {
                            let mut story = self.story();
                            if let Some(notice) = story["lua_import"]["notice"].as_str() {
                                ui.small(notice);
                                ui.separator();
                            }
                            if let Some(target) = story["vanilla_import"]["target"].as_str() {
                                ui.small(crate::i18n::message(
                                    "editor.original.preview",
                                    &[("target", target.into())],
                                ));
                            }
                            let ids: Vec<_> = story["nodes"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|n| n["id"].as_str().map(str::to_owned))
                                .collect();
                            let stories = self.project.stories.keys().cloned().collect::<Vec<_>>();
                            if let Some(mut node) = story["nodes"].get(self.selected).cloned() {
                                let event_picture = node["type"] == "cg" && node["kind"] == "event";
                                if event_picture {
                                    ui.small(crate::i18n::key("editor.workspace.event_hint"));
                                }
                                if matches!(
                                    node["type"].as_str(),
                                    Some("background" | "custom_cg" | "overlay")
                                ) || event_picture && node["action"] == "show"
                                {
                                    ui.small(crate::i18n::key("editor.workspace.image_hint"));
                                    if ui
                                        .button(crate::i18n::key("editor.workspace.import_image"))
                                        .clicked()
                                    {
                                        self.image_import_target = Some((
                                            self.current.clone(),
                                            node["id"].as_str().unwrap_or("").to_owned(),
                                        ));
                                        self.asset_import_dialog = Some(AssetImportDialog {
                                            extends: String::new(),
                                            kind: 0,
                                            name: String::new(),
                                            paths: vec![],
                                            error: None,
                                        });
                                    }
                                }
                                let official = node["type"] == "cg"
                                    && matches!(
                                        node["kind"].as_str(),
                                        Some("event" | "picture" | "item" | "big")
                                    );
                                let custom = matches!(
                                    node["type"].as_str(),
                                    Some("background" | "custom_cg" | "overlay")
                                );
                                let scene =
                                    matches!(node["type"].as_str(), Some("scene" | "combat"));
                                let mut image_changed = false;
                                if (official || custom || scene)
                                    && !matches!(
                                        node["action"].as_str(),
                                        Some("hide" | "clear" | "fadeout")
                                    )
                                {
                                    let key = if official {
                                        "key"
                                    } else if node["type"] == "scene" {
                                        "view"
                                    } else if scene {
                                        "background"
                                    } else {
                                        "image"
                                    };
                                    let options = self.catalog.options(
                                        if event_picture {
                                            "event_picture"
                                        } else if official {
                                            "official_picture"
                                        } else if scene {
                                            "view"
                                        } else {
                                            "user_image"
                                        },
                                        &node,
                                        &[],
                                        &[],
                                    );
                                    ui.push_id(
                                        (
                                            &self.current,
                                            node["id"].as_str().unwrap_or("").to_owned(),
                                        ),
                                        |ui| {
                                            if let Some(image) = self.preview.image_browser(
                                                ui,
                                                &self.project,
                                                &options,
                                                node[key].as_str().unwrap_or(""),
                                                official,
                                            ) {
                                                node[key] = json!(image);
                                                image_changed = true;
                                            }
                                        },
                                    );
                                }
                                if node["type"] == "rotate" {
                                    let mut legacy = self.preview.legacy_character_rotation;
                                    if ui
                                        .checkbox(
                                            &mut legacy,
                                            crate::i18n::key("editor.workspace.legacy_rotation"),
                                        )
                                        .on_hover_text(crate::i18n::key("editor.rotation.hint"))
                                        .changed()
                                    {
                                        if let Err(error) = self.set_legacy_character_rotation(
                                            legacy,
                                            &user_root().join("rust/preferences.json"),
                                        ) {
                                            self.error = Some(crate::error_ui::capture(error));
                                        }
                                    }
                                }
                                self.catalog.stage_characters = Some(
                                    self.preview
                                        .state_before(&story, node["id"].as_str().unwrap_or(""))
                                        ["actors"]
                                        .as_object()
                                        .into_iter()
                                        .flatten()
                                        .map(|(id, _)| id.clone())
                                        .collect(),
                                );
                                let response =
                                    self.catalog.node_form(ui, &mut node, &ids, &stories);
                                self.catalog.stage_characters = None;
                                if let Some(kind) = response.help {
                                    self.tools_panel.open_node_help(&kind);
                                }
                                if matches!(node["type"].as_str(), Some("music" | "sound")) {
                                    ui.horizontal(|ui| {
                                        let name = node["name"].as_str().unwrap_or("");
                                        let playable = !name.is_empty()
                                            && node["op"].as_str().unwrap_or("play") == "play";
                                        if ui
                                            .add_enabled(
                                                playable,
                                                egui::Button::new(tr("试听音频")),
                                            )
                                            .clicked()
                                        {
                                            let result = if name.starts_with("user:") {
                                                self.audio.play_content(&self.project, name)
                                            } else {
                                                let kind = if node["type"] == "music" {
                                                    "music"
                                                } else if node["kind"] == "env" {
                                                    "env"
                                                } else {
                                                    "sound"
                                                };
                                                (|| -> anyhow::Result<()> {
                                                    let helper = crate::components::extractor()?;
                                                    let decoder = crate::components::decoder()?;
                                                    self.audio.play_official(
                                                        &self.tools_panel.game_directory(),
                                                        kind,
                                                        name,
                                                        &helper,
                                                        &decoder,
                                                    )
                                                })()
                                            };
                                            if let Err(error) = result
                                                .with_context(|| format!("无法试听音频“{name}”"))
                                            {
                                                self.error = Some(crate::error_ui::capture(error));
                                            }
                                        }
                                        if ui
                                            .add_enabled(
                                                self.audio.playing(),
                                                egui::Button::new(tr("停止试听")),
                                            )
                                            .clicked()
                                        {
                                            self.audio.stop();
                                        }
                                        if self.audio.preparing() {
                                            ui.spinner();
                                            ui.label(crate::i18n::key("editor.audio.preparing"));
                                        }
                                    });
                                    ui.small(crate::i18n::key("editor.audio.preview_hint"));
                                }
                                if response.changed || image_changed {
                                    story["nodes"][self.selected] = node.clone();
                                    self.project
                                        .stories
                                        .insert(self.current.clone(), story.clone());
                                }
                            } else {
                                ui.heading(tr("还没有步骤"));
                                ui.label(crate::i18n::key("editor.workspace.empty"));
                            }
                        }
                        Center::Chapter => self.chapter(ui),
                        Center::Initial => {
                            let old = self.current.clone();
                            crate::initial_setup::show(
                                ui,
                                &mut self.project,
                                &self.catalog,
                                &mut self.current,
                            );
                            if old != self.current {
                                self.selected = 0;
                                self.multiselect.clear();
                                self.needs_compile = true;
                            }
                        }
                        Center::Manifest => {
                            crate::manifest_panel::show(
                                ui,
                                &mut self.project.manifest,
                                &self.catalog,
                                &self.project.stories.keys().cloned().collect::<Vec<_>>(),
                            );
                        }
                        Center::Events => crate::scenario_panel::show(
                            ui,
                            &mut self.project,
                            &self.catalog,
                            false,
                            &self.scenario_selection,
                        ),
                        Center::Tasks => crate::scenario_panel::show(
                            ui,
                            &mut self.project,
                            &self.catalog,
                            true,
                            &self.scenario_selection,
                        ),
                        Center::Localization => self.localization(ui),
                        Center::Assets => {
                            self.material_navigation(ui);
                            self.assets(ui);
                        }
                        Center::Tools => self.tools(ui),
                        Center::Advanced => {
                            if let Some((sid, nid)) =
                                self.advanced.show(ui, &mut self.project, &self.current)
                            {
                                self.flush();
                                if let Some(nid) = nid {
                                    if let Some(s) = self.project.stories.get(&sid) {
                                        if let Some(index) = s["nodes"]
                                            .as_array()
                                            .and_then(|ns| ns.iter().position(|n| n["id"] == nid))
                                        {
                                            self.current = sid;
                                            self.selected = index;
                                            self.multiselect.clear();
                                            self.search.clear();
                                            self.center = Center::Node;
                                            self.needs_compile = true;
                                        }
                                    }
                                } else if self.project.stories.contains_key(&sid) {
                                    self.current = sid;
                                    self.center = Center::Chapter;
                                } else {
                                    self.center = Center::Manifest;
                                }
                            }
                        }
                    });
            });
    }
    fn chapter(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(tr("章节设置"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.project.stories.len() > 1,
                        egui::Button::new(
                            RichText::new(tr("删除章节…")).color(ui.visuals().error_fg_color),
                        ),
                    )
                    .on_hover_text(tr("删除当前章节及其步骤，可撤销；作品至少保留一个章节"))
                    .clicked()
                {
                    self.begin_delete_chapter();
                }
            });
        });
        let mut story = self.story();
        let mut title = story["title"].as_str().unwrap_or_default().to_owned();
        ui.label(tr("章节名称"));
        if ui
            .add(egui::TextEdit::singleline(&mut title).desired_width(f32::INFINITY))
            .changed()
        {
            story["title"] = json!(title);
        }
        if story.get("vanilla_patch").is_some() {
            ui.label(crate::i18n::key("editor.patch.chapter_hint"));
            self.project.stories.insert(self.current.clone(), story);
            return;
        }
        if story["vanilla_import"]["schema"] == 1 {
            ui.small(crate::i18n::key("editor.upgrade.hint"));
            if ui
                .button(crate::i18n::key("editor.upgrade.button"))
                .clicked()
            {
                let selected = story["nodes"][self.selected]["id"].clone();
                match lom_core::native_upgrade::upgrade(&story) {
                    Ok(upgraded) => {
                        self.flush();
                        story = upgraded;
                        self.selected = story["nodes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .position(|node| node["id"] == selected)
                            .unwrap_or(0);
                        self.multiselect.clear();
                        self.needs_compile = true;
                        self.status = crate::i18n::key("editor.upgrade.done");
                    }
                    Err(error) => {
                        self.error = Some(crate::i18n::message(
                            "editor.upgrade.error",
                            &[("error", crate::error_ui::capture(error))],
                        ))
                    }
                }
            }
        }
        let ids = story["nodes"].as_array().cloned().unwrap_or_default();
        let mut start = story["start"].as_str().unwrap_or("").to_owned();
        ui.label(format!("{} *", tr("入口步骤")));
        if let Some(target) = story["vanilla_import"]["target"].as_str() {
            ui.label(crate::i18n::message(
                "editor.original.target",
                &[("target", target.into())],
            ));
            ui.small(if story["vanilla_import"]["schema"] == 2 {
                crate::i18n::key("editor.original.structured_hint")
            } else {
                crate::i18n::key("editor.original.legacy_hint")
            });
        }
        ui.add_enabled_ui(story.get("vanilla_import").is_none(), |ui| {
            egui::ComboBox::from_id_salt("start")
                .selected_text(
                    ids.iter()
                        .enumerate()
                        .find(|(_, node)| node["id"] == start)
                        .map(|(index, node)| {
                            format!(
                                "{:02} {}",
                                index + 1,
                                self.catalog.label(node["type"].as_str().unwrap_or(""))
                            )
                        })
                        .unwrap_or_else(|| tr("选择入口步骤")),
                )
                .show_ui(ui, |ui| {
                    for (index, n) in ids.iter().enumerate() {
                        if let Some(id) = n["id"].as_str() {
                            ui.selectable_value(
                                &mut start,
                                id.into(),
                                format!(
                                    "{:02} {}",
                                    index + 1,
                                    self.catalog.label(n["type"].as_str().unwrap_or(""))
                                ),
                            );
                        }
                    }
                });
        });
        story["start"] = start.into();
        let mut mood = story["mood"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut mood, tr("显示官方心情气泡")).changed() {
            story["mood"] = mood.into();
        }
        ui.separator();
        ui.heading(tr("分区与分组"));
        ui.label(tr(
            "分区不能重叠；分组须位于所属分区内。Shift 可选择连续步骤。",
        ));
        if ui.button(tr("将选中步骤划为新分区")).clicked() {
            match authoring::add_section(
                &story,
                &self.selected_indices().into_iter().collect(),
                None,
            ) {
                Ok(next) => story = next,
                Err(e) => self.error = Some(crate::error_ui::capture(e)),
            }
        }
        ui.horizontal(|ui| {
            let sections = story["_editor"]["sections"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut section_names: Vec<_> = sections
                .iter()
                .enumerate()
                .map(|(index, section)| {
                    (
                        section["id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| index.to_string()),
                        section["title"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| crate::i18n::key("editor.workspace.section")),
                    )
                })
                .collect();
            crate::forms::number_duplicate_labels(&mut section_names);
            egui::ComboBox::from_id_salt("parent-section")
                .selected_text(
                    section_names
                        .get(self.section_parent)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_else(|| crate::i18n::key("editor.workspace.section_owner")),
                )
                .show_ui(ui, |ui| {
                    for (i, (_, name)) in section_names.iter().enumerate() {
                        ui.selectable_value(&mut self.section_parent, i, name);
                    }
                });
            if ui
                .add_enabled(
                    !sections.is_empty(),
                    egui::Button::new(tr("将选中步骤划为分组")),
                )
                .clicked()
            {
                match authoring::add_section(
                    &story,
                    &self.selected_indices().into_iter().collect(),
                    Some(self.section_parent),
                ) {
                    Ok(next) => story = next,
                    Err(e) => self.error = Some(crate::error_ui::capture(e)),
                }
            }
        });
        if let Some(result) = authoring::section_editor(ui, &story) {
            match result {
                Ok(next) => story = next,
                Err(e) => self.error = Some(format!("分区/分组修改未应用：{e:#}")),
            }
        }
        ui.collapsing(tr("高级"), |ui| {
        if let Some(meta) = story.get_mut("_editor").and_then(Value::as_object_mut) {
            let mut other = Value::Object(
                meta.iter()
                    .filter(|(k, _)| k.as_str() != "sections")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
            if value_editor(ui, "离线测试及创作扩展数据", &mut other, 0) {
                let sections = meta.get("sections").cloned();
                *meta = other.as_object().cloned().unwrap_or_default();
                if let Some(sections) = sections {
                    meta.insert("sections".into(), sections);
                }
            }
        }
        if ui.button(tr("添加章节离线测试")).clicked() {
            if !story["_editor"].is_object() {
                story["_editor"] = json!({});
            }
            if !story["_editor"]["tests"].is_array() {
                story["_editor"]["tests"] = json!([]);
            }
            let tests = story["_editor"]["tests"].as_array_mut().unwrap();
            tests.push(json!({"name":format!("测试{}",tests.len()+1),"story":self.current,"initial":{"variables":{},"flags":{}},"actions":{"choices":[]},"assert":{"reaches_node":[]}}));
        }
        });
        self.project.stories.insert(self.current.clone(), story);
    }
    fn localization(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("多语言对白"));
        ui.label(crate::i18n::key("editor.workspace.translation_hint"));
        let mut s = self.story();
        if !s["localization"].is_object() {
            if ui.button(tr("启用本章多语言")).clicked() {
                s["localization"] =
                    json!({"default_locale":"chs","fallback_locale":"chs","translations":{}});
                self.project.stories.insert(self.current.clone(), s);
            }
            return;
        }
        if ui.button(tr("停用本章多语言（可撤销）")).clicked() {
            s.as_object_mut().unwrap().remove("localization");
            self.project.stories.insert(self.current.clone(), s);
            return;
        }
        let mut default = s["localization"]["default_locale"]
            .as_str()
            .unwrap_or("chs")
            .to_owned();
        let mut fallback = s["localization"]["fallback_locale"]
            .as_str()
            .unwrap_or("chs")
            .to_owned();
        for (label, current) in [
            ("源语言", &mut default),
            ("回退语言", &mut fallback),
            ("正在翻译", &mut self.locale),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{}{}",
                    tr(label),
                    if label == "源语言" { " *" } else { "" }
                ));
                egui::ComboBox::from_id_salt(label)
                    .selected_text(current.as_str())
                    .show_ui(ui, |ui| {
                        for (id, name) in [
                            ("chs", "简体中文"),
                            ("cht", "繁體中文"),
                            ("ja", "日本語"),
                            ("ko", "한국어"),
                        ] {
                            ui.selectable_value(current, id.into(), name);
                        }
                    });
            });
        }
        if s["localization"]["default_locale"].as_str() != Some(&default) {
            if let Some(translations) = s["localization"]["translations"].as_object_mut() {
                translations.remove(&default);
            }
        }
        s["localization"]["default_locale"] = default.clone().into();
        s["localization"]["fallback_locale"] = fallback.into();
        let sources = lom_core::localization::iter_localizable_texts(&s);
        let translated = sources
            .iter()
            .filter(|(key, _)| {
                s["localization"]["translations"][&self.locale][key]
                    .as_str()
                    .is_some_and(|v| !v.is_empty())
            })
            .count();
        ui.label(format!(
            "{}：{} / {}",
            tr("已翻译"),
            if self.locale == default {
                sources.len()
            } else {
                translated
            },
            sources.len()
        ));
        if self.locale == default {
            ui.label(tr("当前选择为源语言，请在节点属性中修改原文。"));
        } else {
            let sources = lom_core::localization::iter_localizable_texts(&s);
            for (key, source) in sources {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new(&key).small().color(ACCENT));
                    ui.label(&source);
                    let mut translated = s["localization"]["translations"][&self.locale][&key]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut translated)
                                .hint_text(tr("未翻译，使用回退语言"))
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        if translated.is_empty() {
                            if let Some(o) =
                                s["localization"]["translations"][&self.locale].as_object_mut()
                            {
                                o.remove(&key);
                            }
                        } else {
                            s["localization"]["translations"][&self.locale][&key] =
                                translated.into();
                        }
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(tr("导出本章翻译…")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(format!("{}-translations.json", self.current))
                    .save_file()
                {
                    if let Err(e) = lom_core::project::atomic_write(
                        &path,
                        &lom_core::stable_json(&s["localization"]).unwrap_or_default(),
                    ) {
                        self.error = Some(crate::error_ui::capture(e));
                    }
                }
            }
            if ui.button(tr("导入本章翻译…")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match lom_core::load_json(path) {
                        Ok(config) => {
                            let mut candidate = s.clone();
                            candidate["localization"] = config;
                            match lom_core::localization::validate_story_localization(&candidate) {
                                Ok(()) => s = candidate,
                                Err(e) => self.error = Some(crate::error_ui::capture(e)),
                            }
                        }
                        Err(e) => self.error = Some(crate::error_ui::capture(e)),
                    }
                }
            }
        });
        self.project.stories.insert(self.current.clone(), s);
    }
    fn assets(&mut self, ui: &mut egui::Ui) {
        ui.strong(tr("作品素材"));
        ui.add_space(4.0);
        if ui.button(tr("导入素材…")).clicked() && self.apply_pending_drafts() {
            self.image_import_target = None;
            self.asset_import_dialog = Some(AssetImportDialog {
                extends: String::new(),
                kind: 0,
                name: String::new(),
                paths: vec![],
                error: None,
            });
        }
        if ui
            .button(crate::i18n::key("editor.workspace.add_portrait"))
            .clicked()
            && self.apply_pending_drafts()
        {
            self.image_import_target = None;
            self.asset_import_dialog = Some(AssetImportDialog {
                extends: "player".into(),
                kind: 1,
                name: String::new(),
                paths: vec![],
                error: None,
            });
        }
        let records: Vec<_> = self
            .project
            .assets
            .iter()
            .filter(|(k, _)| k.ends_with("/content.json"))
            .filter_map(|(key, bytes)| {
                serde_json::from_slice::<Value>(bytes)
                    .ok()
                    .map(|m| (key.clone(), m))
            })
            .collect();
        let mut content_names: Vec<_> = records
            .iter()
            .map(|(key, meta)| {
                let id = meta["id"].as_str().unwrap_or(key);
                (
                    id.to_owned(),
                    meta["name"].as_str().unwrap_or(id).to_owned(),
                )
            })
            .collect();
        crate::forms::number_duplicate_labels(&mut content_names);
        let content_names: BTreeMap<_, _> = content_names.into_iter().collect();
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("project-content-list")
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (key, meta) in &records {
                    let id = meta["id"].as_str().unwrap_or(key);
                    let name = content_names.get(id).map(String::as_str).unwrap_or(id);
                    let kind = match meta["type"].as_str().unwrap_or("") {
                        "image" => tr("图片"),
                        "character" => tr("角色"),
                        "audio" => tr("音频"),
                        "item" => tr("物品"),
                        "martial" => tr("功法"),
                        "support" => tr("决斗援助"),
                        "skill" => tr("自创招式"),
                        other => other.to_owned(),
                    };
                    let selected = self.asset_selection == *key;
                    let border = if selected {
                        ui.visuals().selection.stroke.color
                    } else {
                        ui.visuals().text_color().gamma_multiply(0.22)
                    };
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::Button::selectable(selected, name)
                                .frame_when_inactive(true)
                                .stroke(egui::Stroke::new(
                                    if selected { 1.5_f32 } else { 1.0_f32 },
                                    border,
                                ))
                                .corner_radius(3.0)
                                .truncate()
                                .right_text(egui::RichText::new(kind).small().weak()),
                        )
                        .on_hover_text(format!("{name}\nuser:{id}"))
                        .clicked()
                    {
                        self.asset_selection = key.clone();
                    }
                }
            });
        if let Some(bytes) = self.project.assets.get(&self.asset_selection) {
            if let Ok(meta) = serde_json::from_slice::<Value>(bytes) {
                if meta["type"] == "audio" {
                    ui.horizontal(|ui| {
                        if ui.button(tr("试听音频")).clicked() {
                            let filename = meta["files"]["main"].as_str().unwrap_or("");
                            let source = format!(
                                "{}{filename}",
                                self.asset_selection
                                    .strip_suffix("content.json")
                                    .unwrap_or("")
                            );
                            if let Some(data) = self.project.assets.get(&source) {
                                let path = user_root()
                                    .join("rust/preview-audio")
                                    .join(Path::new(filename).file_name().unwrap_or_default());
                                if let Err(e) = lom_core::project::atomic_write(&path, data)
                                    .and_then(|_| self.audio.play(&path))
                                {
                                    self.error = Some(crate::error_ui::capture(e));
                                }
                            }
                        }
                        if ui
                            .add_enabled(self.audio.playing(), egui::Button::new(tr("停止试听")))
                            .clicked()
                        {
                            self.audio.stop();
                        }
                    });
                }
            }
            if self
                .content_panel
                .show(ui, &mut self.project, &mut self.asset_selection)
            {
                self.asset_dirty = true;
            }
        }
        ui.separator();
        ui.collapsing(
            crate::i18n::message(
                "workspace.assets.count",
                &[("count", self.project.assets.len().to_string())],
            ),
            |ui| {
                for (name, bytes) in &self.project.assets {
                    ui.label(format!("{} · {:.1} KiB", name, bytes.len() as f64 / 1024.0));
                }
            },
        );
    }
    fn apply_pending_drafts(&mut self) -> bool {
        match self.content_panel.apply_pending(&mut self.project) {
            Ok(true) => self.track(),
            Ok(false) => {}
            Err(error) => {
                self.error = Some(crate::error_ui::capture(error));
                return false;
            }
        }
        match self.advanced.apply_pending(&mut self.project) {
            Ok(true) => self.track(),
            Ok(false) => {}
            Err(error) => {
                self.error = Some(crate::error_ui::capture(error));
                return false;
            }
        }
        true
    }
    fn commit_asset_import(&mut self) -> anyhow::Result<()> {
        let draft = self
            .asset_import_dialog
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("没有待导入的素材"))?;
        let name = draft.name.trim().to_owned();
        anyhow::ensure!(!name.is_empty(), "请填写素材名称。");
        anyhow::ensure!(
            name.chars().count() <= 80 && !name.chars().any(char::is_control),
            "素材名称最多 80 个字，不能包含换行或控制字符。"
        );
        let kind = draft.kind;
        let extends = if kind == 1 {
            draft.extends.clone()
        } else {
            String::new()
        };
        let paths = draft.paths.clone();
        let prefix = match kind {
            0 => "image",
            1 => "character",
            5 => "item",
            6 => "martial",
            7 => "support",
            8 => "skill",
            9 => "item",
            _ => "audio",
        };
        let namespace: String = self.project.manifest["id"]
            .as_str()
            .unwrap_or("project")
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(32)
            .collect();
        let namespace = if namespace.starts_with(|c: char| c.is_ascii_lowercase()) {
            namespace.as_str()
        } else {
            "project"
        };
        let id = (1..)
            .map(|n| format!("{namespace}.{prefix}{n}"))
            .find(|id| {
                !self
                    .project
                    .assets
                    .keys()
                    .any(|key| key.split('/').nth(3) == Some(id.as_str()))
            })
            .unwrap();
        self.content_panel.apply_pending(&mut self.project)?;
        self.flush();
        self.import_assets_with_target(paths, &id, &name, kind, &extends)?;
        if kind == 0 {
            if let Some((story, node)) = self.image_import_target.take() {
                if let Some(node) = self
                    .project
                    .stories
                    .get_mut(&story)
                    .and_then(|story| story["nodes"].as_array_mut())
                    .and_then(|nodes| nodes.iter_mut().find(|n| n["id"] == node))
                {
                    let field = if node["type"] == "cg" && node["kind"] == "event" {
                        "key"
                    } else {
                        "image"
                    };
                    node[field] = json!(format!("user:{id}"));
                }
            }
        } else {
            self.image_import_target = None;
        }
        self.track();
        self.flush();
        self.asset_import_dialog = None;
        Ok(())
    }
    #[cfg(test)]
    fn import_assets(
        &mut self,
        paths: Vec<PathBuf>,
        id: &str,
        name: &str,
        asset_kind: usize,
    ) -> anyhow::Result<()> {
        self.import_assets_with_target(paths, id, name, asset_kind, "")
    }
    fn import_assets_with_target(
        &mut self,
        paths: Vec<PathBuf>,
        id: &str,
        name: &str,
        asset_kind: usize,
        extends: &str,
    ) -> anyhow::Result<()> {
        let id = id.trim().to_owned();
        lom_core::content::validate_content_id(&id)?;
        let kind = match asset_kind {
            0 => "image",
            1 => "character",
            5 => "item",
            6 => "martial",
            7 => "support",
            8 => "skill",
            9 => "item",
            _ => "audio",
        };
        let prefix = format!("assets/user/{kind}/{id}");
        if !extends.is_empty() {
            anyhow::ensure!(kind == "character", "只有人物素材可以扩展原版人物");
            anyhow::ensure!(
                lom_core::validate::editor_data()["characters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["id"] == extends),
                "请选择有效的原版人物"
            );
            anyhow::ensure!(
                !lom_core::content::character_extensions(&self.project.assets)?
                    .contains_key(extends),
                "该人物已有扩展，请在作品素材中选择已有扩展，并添加指定表情"
            );
        }
        anyhow::ensure!(
            kind != "skill" || (1..=120).contains(&paths.len()),
            "招式动画必须选择1~120个PNG帧"
        );
        anyhow::ensure!(
            !self
                .project
                .assets
                .keys()
                .any(|p| p.starts_with(&format!("{prefix}/"))),
            "内容 ID 已存在，请使用其他 ID"
        );
        let mut payload = BTreeMap::new();
        let mut portraits = serde_json::Map::new();
        let mut main = String::new();
        let mut frames = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            let original_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("文件名无效"))?
                .to_owned();
            anyhow::ensure!(
                !original_name.contains(':') && !original_name.contains('\\'),
                "文件名不合法"
            );
            // Different directories may contain normal.png. Keep the first file,
            // and allocate a package filename without stealing another selection's name.
            let name =
                if payload.keys().any(|key: &String| {
                    key.eq_ignore_ascii_case(&format!("{prefix}/{original_name}"))
                }) {
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("portrait");
                    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    (1..)
                        .map(|n| format!("{stem}_{n}.{extension}"))
                        .find(|candidate| {
                            !payload.keys().any(|key| {
                                key.eq_ignore_ascii_case(&format!("{prefix}/{candidate}"))
                            }) && !paths.iter().any(|selected| {
                                selected
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .is_some_and(|name| name.eq_ignore_ascii_case(candidate))
                            })
                        })
                        .unwrap()
                } else {
                    original_name
                };
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            anyhow::ensure!(
                if kind == "audio" {
                    ["wav", "ogg"].contains(&ext.as_str())
                } else {
                    ["png", "jpg", "jpeg"].contains(&ext.as_str())
                },
                "素材类型与文件格式不符"
            );
            let limit = if kind == "audio" {
                lom_core::content::MAX_AUDIO_BYTES
            } else {
                lom_core::content::MAX_IMAGE_BYTES
            };
            anyhow::ensure!(
                std::fs::metadata(path)?.len() <= limit,
                "素材超过 {} MiB 上限",
                limit / (1024 * 1024)
            );
            let bytes = std::fs::read(path)?;
            anyhow::ensure!(
                !bytes.is_empty() && bytes.len() as u64 <= limit,
                "素材为空或超过大小上限"
            );
            if i == 0 {
                main = name.clone();
                if extends.is_empty() {
                    portraits.insert("normal".into(), name.clone().into());
                }
            }
            if kind == "character" {
                let portrait = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("normal");
                if lom_core::content::validate_portrait_id(portrait).is_ok()
                    && (i == 0 || !portraits.contains_key(portrait))
                {
                    portraits.insert(portrait.into(), name.clone().into());
                } else if i > 0 || !extends.is_empty() {
                    // The first image already has normal. Other arbitrary filenames
                    // need unique IDs without stealing a later file's valid stem.
                    let portrait = (1..)
                        .map(|n| format!("portrait_{n}"))
                        .find(|id| {
                            !portraits.contains_key(id)
                                && !paths.iter().any(|p| {
                                    p.file_stem().and_then(|s| s.to_str()) == Some(id.as_str())
                                })
                        })
                        .unwrap();
                    portraits.insert(portrait, name.clone().into());
                }
            }
            if kind == "skill" {
                anyhow::ensure!(ext == "png", "招式动画帧必须是PNG");
                anyhow::ensure!(
                    image::guess_format(&bytes)? == image::ImageFormat::Png,
                    "招式动画帧必须是PNG编码"
                );
                image::load_from_memory(&bytes)?;
                frames.push(json!(name));
            }
            payload.insert(format!("{prefix}/{name}"), bytes);
        }
        anyhow::ensure!(!main.is_empty(), "未选择素材");
        let mut meta = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":name,"files":{"main":main}});
        if kind == "character" {
            meta["portraits"] = portraits.into();
            if !extends.is_empty() {
                meta["extends"] = json!(extends);
            }
            meta["scale"] = json!(100);
            meta["art_facing"] = json!("left");
        }
        if kind == "audio" {
            meta["audio_kind"] = json!(if asset_kind == 2 { "music" } else { "sound" });
            if asset_kind == 4 {
                meta["character"] = json!("player");
            }
        }
        if let Some(defaults) = lom_core::content::gameplay_defaults(kind).as_object() {
            meta.as_object_mut().unwrap().extend(defaults.clone());
        }
        if kind == "skill" {
            meta["animation"] = json!({"frames":frames,"fps":12,"hit_frame":0,"scale":100});
        }
        if asset_kind == 9 {
            meta["category"] = json!("consumable");
            meta["use_in"] = json!("field");
            meta["effects"] = json!([{"type":"stat","stat":"life","value":10}]);
        }
        let checked = lom_core::content::normalize_content_metadata(&meta)?;
        meta = lom_core::content::content_metadata_payload(&checked);
        let key = format!("{prefix}/content.json");
        payload.insert(key.clone(), lom_core::stable_json(&meta)?);
        self.project.assets.extend(payload);
        self.asset_selection = key;
        self.asset_dirty = true;
        self.status = format!("{}：{name}", tr("已导入"));
        Ok(())
    }
    fn tools(&mut self, ui: &mut egui::Ui) {
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
    fn diagnostic(&mut self) {
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
    fn choose_preview_library(&mut self) {
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
    fn set_legacy_character_rotation(&mut self, legacy: bool, path: &Path) -> anyhow::Result<()> {
        let mut prefs = if path.exists() {
            lom_core::load_json(path)?
        } else {
            json!({})
        };
        anyhow::ensure!(prefs.is_object(), "个人设置格式无效，未修改旋转选项");
        prefs["legacy_character_rotation"] = json!(legacy);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        self.preview.cancel_layout_drag();
        self.preview.legacy_character_rotation = legacy;
        Ok(())
    }
    fn set_startup_update_check(&mut self, enabled: bool, path: &Path) -> anyhow::Result<()> {
        let mut prefs = lom_core::load_json(path)
            .ok()
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        prefs["check_updates_on_startup"] = json!(enabled);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        self.check_updates_on_startup = enabled;
        Ok(())
    }
    fn set_auto_save(&mut self, enabled: bool, seconds: u64, path: &Path) -> anyhow::Result<()> {
        anyhow::ensure!([30, 60, 300, 600].contains(&seconds), "自动保存间隔无效");
        let mut prefs = if path.exists() {
            lom_core::load_json(path)?
        } else {
            json!({})
        };
        anyhow::ensure!(prefs.is_object(), "个人设置格式无效，未修改自动保存设置");
        prefs["auto_save_project"] = json!(enabled);
        prefs["auto_save_seconds"] = json!(seconds);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        self.auto_save_project = enabled;
        self.auto_save_seconds = seconds;
        self.last_auto_save = Instant::now();
        Ok(())
    }
    fn set_interface_locale(&mut self, locale: &str, path: &Path) -> anyhow::Result<()> {
        anyhow::ensure!(crate::i18n::LOCALES.contains(&locale), "不支持的界面语言");
        let mut prefs = lom_core::load_json(path)
            .ok()
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        prefs["ui_locale"] = json!(locale);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        crate::i18n::set_locale(locale);
        crate::macos_window::update_settings_menu_title();
        Ok(())
    }
    fn dialog_keys(&mut self, ctx: &egui::Context) -> (bool, bool) {
        if !self.settings_open
            && !self.updater.open
            && self.chapter_dialog.is_none()
            && self.delete_chapter_dialog.is_none()
            && self.rename_dialog.is_none()
            && self.asset_import_dialog.is_none()
            && self.error.is_none()
            && self.pending.is_none()
            && self.pending_template.is_none()
            && self.recovery_candidates.is_empty()
        {
            self.dialog_ime = false;
            return (false, false);
        }
        let was_composing = self.dialog_ime;
        let events = ctx.input(|i| i.events.clone());
        let mut ime_event = false;
        for event in events {
            if let egui::Event::Ime(event) = event {
                ime_event = true;
                match event {
                    egui::ImeEvent::Enabled => {}
                    egui::ImeEvent::Preedit(text) => self.dialog_ime = !text.is_empty(),
                    egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => self.dialog_ime = false,
                }
            }
        }
        // Return confirms an IME candidate before it can confirm the dialog.
        // Popups likewise own their Return/Escape while a list is open.
        if was_composing || self.dialog_ime || ime_event || egui::Popup::is_any_open(ctx) {
            return (false, false);
        }
        ctx.input_mut(|i| {
            if !i.modifiers.is_none() {
                return (false, false);
            }
            (
                !self.settings_open && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        })
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.tools_panel.dialog_open() {
            return;
        }
        let (return_pressed, escape_pressed) = self.dialog_keys(ctx);
        if let Some(message) = self.error.clone() {
            let mut close = false;
            let mut locate = false;
            let mut issue_location = None;
            let destination = crate::error_location::resolve(&message, &self.project);
            egui::Modal::new(egui::Id::new("operation-error"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(460.0_f32.min(ctx.content_rect().width() - 80.0));
                    close |= shell::dialog_header(ui, &crate::error_ui::title(&message));
                    egui::ScrollArea::vertical()
                        .max_height(ctx.content_rect().height() * 0.5)
                        .show(ui, |ui| {
                            crate::error_ui::show(ui, &message);
                            for issue in &self.error_issues {
                                ui.add_space(10.0);
                                ui.group(|ui| {
                                    ui.strong(crate::error_location::issue_label(
                                        issue,
                                        &self.project,
                                    ));
                                    crate::error_ui::show(ui, &issue.message);
                                    let target = crate::error_location::issue_destination(
                                        issue,
                                        &self.project,
                                    );
                                    if target != crate::error_location::Destination::Current
                                        && ui.button(target.label()).clicked()
                                    {
                                        issue_location = Some((target, issue.message.clone()));
                                    }
                                });
                            }
                            crate::error_ui::copy_button(ui, || {
                                if self.error_issues.is_empty() {
                                    message.clone()
                                } else {
                                    format!(
                                        "{message}\n\n{}",
                                        crate::error_location::issue_report(
                                            &self.error_issues,
                                            &self.project
                                        )
                                    )
                                }
                            });
                        });
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| {
                        if self.error_issues.is_empty()
                            && destination != crate::error_location::Destination::Current
                        {
                            locate = ui
                                .button(crate::error_location::action_label(&message, &destination))
                                .clicked();
                        }
                        close |= ui
                            .add_sized([80.0, 30.0], egui::Button::new(tr("关闭")))
                            .clicked();
                    });
                });
            if close || escape_pressed || locate || issue_location.is_some() {
                self.error = None;
                self.error_issues.clear();
            }
            if let Some((target, detail)) = issue_location {
                crate::error_location::focus_field(ctx, &detail);
                self.go_to_problem(ctx, target);
            } else if locate {
                crate::error_location::focus_field(ctx, &message);
                self.go_to_problem(ctx, destination);
            }
            return;
        }
        if self.updater.open
            && !self.settings_open
            && self.pending.is_none()
            && self.chapter_dialog.is_none()
            && self.delete_chapter_dialog.is_none()
            && self.rename_dialog.is_none()
            && self.asset_import_dialog.is_none()
            && self.recovery_candidates.is_empty()
        {
            if self.updater.dialog(ctx) {
                self.request(Pending::EditorUpdate, ctx);
            }
            return;
        }
        if self.settings_open {
            let mut close = false;
            let mut selected = None;
            let mut startup = self.check_updates_on_startup;
            let mut startup_changed = false;
            let mut auto_save = self.auto_save_project;
            let mut auto_save_seconds = self.auto_save_seconds;
            let mut auto_save_changed = false;
            let mut choose_library = false;
            let mut extract_library = false;
            let mut resolution_changed = false;
            egui::Modal::new(egui::Id::new("application-settings"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(520.0_f32.min(ctx.content_rect().width() - 60.0));
                    close |= shell::dialog_header(ui, &tr("设置"));
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        for (page, label) in ["常规", "素材与路径", "更新"].iter().enumerate() {
                            ui.selectable_value(&mut self.settings_page, page, tr(label));
                        }
                    });
                    ui.separator();
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 180.0).max(80.0))
                        .show(ui, |ui| {
                    match self.settings_page {
                        0 => {
                            ui.strong(tr("界面语言"));
                            for (locale, name) in crate::i18n::LOCALES.into_iter().zip(crate::i18n::NAMES) {
                                if ui.radio(crate::i18n::locale() == locale, name).clicked() { selected = Some(locale); }
                            }
                            ui.small(tr("只改变编辑器界面，不改写作品的台词与语言。"));
                            ui.add_space(12.0);
                            resolution_changed = self.shell.resolution_settings(ui);
                            ui.add_space(12.0);
                            auto_save_changed |= ui.checkbox(&mut auto_save, crate::i18n::key("editor.autosave.enabled")).changed();
                            ui.add_enabled_ui(auto_save, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for (seconds, key) in [(30,"editor.autosave.seconds30"),(60,"editor.autosave.minute1"),(300,"editor.autosave.minutes5"),(600,"editor.autosave.minutes10")] {
                                        auto_save_changed |= ui.selectable_value(&mut auto_save_seconds,seconds,crate::i18n::key(key)).changed();
                                    }
                                });
                            });
                            ui.small(crate::i18n::key("editor.autosave.hint"));
                        }
                        1 => {
                            ui.strong(crate::i18n::key("editor.workspace.preview_library"));
                            if self.preview.library.join("preview_map.json").is_file() { ui.label(self.preview.library.display().to_string()); }
                            else { ui.label(tr("尚未选择已提取的预览素材库")); }
                            choose_library = ui.button(tr("选择预览素材库…")).clicked();
                            extract_library = ui.button(tr("从游戏提取预览素材…")).clicked();
                            ui.add_space(12.0);
                            ui.strong(crate::i18n::key("library.title"));
                            let library = lom_core::content::default_repository_root();
                            ui.label(library.display().to_string());
                            if ui.button(tr("复制用户内容库路径")).clicked() { ctx.copy_text(library.display().to_string()); }
                            ui.small(tr("作品素材随项目保存；更新编辑器会保留内容库和配置。"));
                        }
                        _ => {
                            ui.strong(format!("{} {}", tr("编辑器版本"), env!("CARGO_PKG_VERSION")));
                            startup_changed = ui.checkbox(&mut startup, tr("启动时检查更新")).changed();
                            ui.small(tr("每次启动在后台检查一次，只有发现新版本才弹窗；无新版或网络失败时保持静默。"));
                            ui.add_space(8.0);
                            ui.label(tr("手动检查更新位于“游戏与 MOD”页面。"));
                        }
                    }
                    if let Some(error) = &self.settings_error { crate::error_ui::show_with_actions(ui, error); }
                        });
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| { close |= ui.add_sized([80.0,30.0],egui::Button::new(tr("完成"))).clicked(); });
                });
            let prefs = user_root().join("rust/preferences.json");
            if auto_save_changed {
                self.settings_error = self
                    .set_auto_save(auto_save, auto_save_seconds, &prefs)
                    .err()
                    .map(|error| crate::error_ui::capture(error));
            }
            if resolution_changed {
                let mut preferences = lom_core::load_json(&prefs).unwrap_or(json!({}));
                self.shell.store_resolution_preferences(&mut preferences);
                self.settings_error = lom_core::stable_json(&preferences)
                    .and_then(|bytes| lom_core::project::atomic_write(&prefs, &bytes))
                    .err()
                    .map(|error| crate::error_ui::capture(error));
            }
            if let Some(locale) = selected {
                self.settings_error = self
                    .set_interface_locale(locale, &prefs)
                    .err()
                    .map(|e| crate::error_ui::capture(e));
            }
            if startup_changed {
                self.settings_error = self
                    .set_startup_update_check(startup, &prefs)
                    .err()
                    .map(|e| crate::error_ui::capture(e));
            }
            if close || escape_pressed {
                self.settings_open = false;
            }
            if choose_library {
                self.settings_open = false;
                self.choose_preview_library();
            }
            if extract_library {
                self.settings_open = false;
                self.tools_panel.start_extraction();
            }
            return;
        }
        if let Some(draft) = &mut self.asset_import_dialog {
            let mut submit = false;
            let mut cancel = false;
            let mut choose_files = false;
            let mut choose_folder = false;
            let prefix_id = egui::Id::new("portrait-folder-prefix");
            let mut folder_prefix = ctx
                .data_mut(|data| data.get_temp::<String>(prefix_id))
                .unwrap_or_default();
            egui::Modal::new(egui::Id::new("asset-import"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    cancel |= shell::dialog_header(ui, &tr("导入素材"));
                    ui.add_space(16.0);
                    let old_kind = draft.kind;
                    ui.label(format!("{} *", tr("素材类型")));
                    if self.image_import_target.is_some() {
                        ui.label(tr("图片"));
                        draft.kind = 0;
                    } else {
                        egui::ComboBox::from_id_salt("import-kind")
                            .selected_text(tr([
                                "图片",
                                "角色",
                                "音乐",
                                "音效",
                                "配音",
                                "物品",
                                "功法",
                                "决斗援助",
                                "自创招式",
                                "消耗品",
                            ][draft.kind]))
                            .show_ui(ui, |ui| {
                                for (i, kind) in [
                                    "图片",
                                    "角色",
                                    "音乐",
                                    "音效",
                                    "配音",
                                    "物品",
                                    "功法",
                                    "决斗援助",
                                    "自创招式",
                                    "消耗品",
                                ]
                                .iter()
                                .enumerate()
                                {
                                    ui.selectable_value(&mut draft.kind, i, tr(kind));
                                }
                            });
                    }
                    if old_kind != draft.kind {
                        draft.paths.clear();
                        draft.name.clear();
                        draft.error = None;
                        draft.extends.clear();
                    }
                    if draft.kind == 1 {
                        let mut target = if draft.extends.is_empty() {
                            json!({})
                        } else {
                            json!({"extends":draft.extends})
                        };
                        self.catalog.field(
                            ui,
                            &mut target,
                            "extends",
                            "扩展原版人物",
                            "official_character",
                            true,
                            &[],
                            &[],
                        );
                        draft.extends = target["extends"].as_str().unwrap_or("").to_owned();
                        ui.small(crate::i18n::key("editor.workspace.character_hint"));
                    }
                    choose_files = ui
                        .button(format!(
                            "{} *",
                            tr(if draft.kind == 8 {
                                "选择 PNG 动画帧…"
                            } else if draft.kind == 1 {
                                if draft.paths.is_empty() {
                                    "选择立绘文件…"
                                } else {
                                    "继续添加立绘…"
                                }
                            } else {
                                "选择文件…"
                            })
                        ))
                        .clicked();
                    if draft.kind == 1 {
                        ui.small(tr("可按住 Ctrl 多选，Ctrl+A 全选。"));
                        ui.add(
                            egui::TextEdit::singleline(&mut folder_prefix)
                                .hint_text(tr("文件名前缀（可选）")),
                        );
                        choose_folder = ui.button(tr("从文件夹选择全部立绘…")).clicked();
                    }
                    if !draft.paths.is_empty() {
                        for path in &draft.paths {
                            ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                        }
                        ui.label(format!("{} *", tr("素材名称")));
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.name)
                                .id(egui::Id::new("asset-import-name"))
                                .desired_width(f32::INFINITY),
                        );
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    let can_submit = !draft.paths.is_empty() && !draft.name.trim().is_empty();
                    let actions = dialog_actions(ui, "导入", can_submit);
                    submit = actions.0;
                    cancel |= actions.1;
                    submit |= return_pressed && can_submit;
                });
            ctx.data_mut(|data| data.insert_temp(prefix_id, folder_prefix.clone()));
            if cancel || escape_pressed {
                self.asset_import_dialog = None;
                self.image_import_target = None;
            } else if choose_folder {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    match crate::content_panel::portrait_files(&folder, &folder_prefix) {
                        Ok(paths) => draft.select_files(paths),
                        Err(error) => draft.error = Some(crate::error_ui::capture(error)),
                    }
                }
            } else if choose_files {
                let dialog = rfd::FileDialog::new();
                let paths = if draft.kind == 8 {
                    dialog.add_filter("PNG 动画帧", &["png"]).pick_files()
                } else if draft.kind == 1 {
                    dialog
                        .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                        .pick_files()
                } else if draft.kind == 0 || draft.kind >= 5 {
                    dialog
                        .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                        .pick_file()
                        .map(|path| vec![path])
                } else {
                    dialog
                        .add_filter("WAV / OGG", &["wav", "ogg"])
                        .pick_file()
                        .map(|path| vec![path])
                };
                if let Some(paths) = paths {
                    draft.select_files(paths);
                }
            } else if submit {
                if let Err(error) = self.commit_asset_import() {
                    self.asset_import_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some(chapter) = self
            .delete_chapter_dialog
            .as_ref()
            .map(|draft| draft.chapter.clone())
        {
            let story = self.project.stories.get(&chapter);
            let name = story
                .and_then(|story| story["title"].as_str())
                .unwrap_or(&chapter)
                .to_owned();
            let steps = story
                .and_then(|story| story["nodes"].as_array())
                .map(Vec::len)
                .unwrap_or(0);
            let references = self.chapter_reference_count(&chapter);
            let mut choices: Vec<_> = self
                .project
                .stories
                .iter()
                .filter(|(id, _)| *id != &chapter)
                .map(|(id, story)| (id.clone(), story["title"].as_str().unwrap_or(id).to_owned()))
                .collect();
            crate::forms::number_duplicate_labels(&mut choices);
            let draft = self.delete_chapter_dialog.as_mut().unwrap();
            let mut submit = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("chapter-delete"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(440.0_f32.min(ctx.content_rect().width() - 60.0));
                    cancel |= shell::dialog_header(ui, &tr("删除章节"));
                    ui.add_space(12.0);
                    ui.label(format!("{}：{name}", tr("即将删除章节")));
                    ui.label(format!("{}：{steps}", tr("本章步骤数")));
                    ui.label(tr("删除本章及其全部步骤，可撤销；素材仍然保留。"));
                    if references > 0 {
                        ui.add_space(12.0);
                        ui.label(format!(
                            "{}：{references}",
                            tr("将改到替代章节的入口、跳转及事件引用")
                        ));
                        ui.label(tr("替代章节"));
                        egui::ComboBox::from_id_salt("delete-chapter-replacement")
                            .width(ui.available_width())
                            .selected_text(
                                choices
                                    .iter()
                                    .find(|(id, _)| id == &draft.replacement)
                                    .map(|(_, name)| name.as_str())
                                    .unwrap_or(""),
                            )
                            .show_ui(ui, |ui| {
                                for (id, name) in &choices {
                                    ui.selectable_value(&mut draft.replacement, id.clone(), name);
                                }
                            });
                        ui.small(tr("确认删除时，这些引用会一并改到所选章节。"));
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| {
                        let confirm = ui.add_sized(
                            [100.0, 30.0],
                            egui::Button::new(RichText::new(tr("删除章节")).color(Color32::WHITE))
                                .fill(Color32::from_rgb(170, 45, 45)),
                        );
                        submit = confirm.clicked() || (return_pressed && confirm.has_focus());
                        let dismiss = ui.add_sized([80.0, 30.0], egui::Button::new(tr("取消")));
                        cancel |= dismiss.clicked() || (return_pressed && dismiss.has_focus());
                    });
                });
            if cancel || escape_pressed {
                self.delete_chapter_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_delete_chapter() {
                    self.delete_chapter_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some(draft) = &mut self.chapter_dialog {
            let mut submit = false;
            let mut cancel = false;
            let title = match draft.action {
                ChapterAction::New => "新建章节",
                ChapterAction::Duplicate(_) => "复制章节",
                ChapterAction::NewGroup => "新建章节分组",
                ChapterAction::RenameGroup(_) => "重命名章节分组",
            };
            let group = matches!(
                draft.action,
                ChapterAction::NewGroup | ChapterAction::RenameGroup(_)
            );
            egui::Modal::new(egui::Id::new("chapter-create"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    cancel |= shell::dialog_header(ui, &tr(title));
                    ui.add_space(16.0);
                    ui.label(format!(
                        "{} *",
                        tr(if group {
                            "分组名称"
                        } else {
                            "章节名称"
                        })
                    ));
                    let name = ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .id(egui::Id::new("chapter-name"))
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut draft.focus_name) {
                        name.request_focus();
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    let can_submit = !draft.name.trim().is_empty();
                    let actions = dialog_actions(
                        ui,
                        if matches!(draft.action, ChapterAction::RenameGroup(_)) {
                            "保存"
                        } else {
                            "创建"
                        },
                        can_submit,
                    );
                    submit = actions.0;
                    cancel |= actions.1;
                    submit |= return_pressed && can_submit;
                });
            if cancel || escape_pressed {
                self.chapter_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_chapter() {
                    self.chapter_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some(draft) = &mut self.rename_dialog {
            let mut submit = false;
            let mut cancel = false;
            let title = match draft.target {
                RenameTarget::Node { .. } => "修改步骤标识",
                RenameTarget::Chapter(_) => "修改章节标识",
            };
            egui::Modal::new(egui::Id::new("advanced-rename"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    cancel |= shell::dialog_header(ui, &tr(title));
                    ui.add_space(16.0);
                    ui.label(tr("内部标识用于流程引用，修改时会同步关联内容。"));
                    ui.label(format!("{} *", tr("内部标识")));
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut draft.value)
                            .id(egui::Id::new("advanced-rename-value"))
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut draft.focus_name) {
                        field.request_focus();
                    }
                    if let Some(error) = &draft.error {
                        crate::error_ui::show_with_actions(ui, error);
                    }
                    let can_submit = !draft.value.trim().is_empty();
                    let actions = dialog_actions(ui, "应用", can_submit);
                    submit = actions.0;
                    cancel |= actions.1;
                    submit |= return_pressed && can_submit;
                });
            if cancel || escape_pressed {
                self.rename_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_rename() {
                    self.rename_dialog.as_mut().unwrap().error =
                        Some(crate::error_ui::capture(error));
                }
            }
            return;
        }
        if let Some((nodes, warnings)) = self.pending_template.clone() {
            let mut cancel = false;
            let mut insert = false;
            let structured = self.story()["vanilla_import"]["schema"] == 2;
            egui::Modal::new(egui::Id::new("template-boundary"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(460.0);
                    cancel |= shell::dialog_header(ui, &tr("模板边界引用"));
                    ui.label(crate::i18n::key(if structured {
                        "workspace.template.structured"
                    } else {
                        "workspace.template.boundary"
                    }));
                    for warning in warnings {
                        let warning = if structured {
                            warning
                                .replace(
                                    "会保留，请确认目标章节中存在该 ID",
                                    "将接回插入位置后续步骤",
                                )
                                .replace("插入后请重新绑定", "将接回插入位置后续步骤")
                        } else {
                            warning
                        };
                        ui.label(format!("• {}", crate::error_ui::text(&warning)));
                    }
                    (insert, cancel) = {
                        let (submit, footer_cancel) = dialog_actions(
                            ui,
                            if structured {
                                "接回当前流程并插入"
                            } else {
                                "保留引用并插入"
                            },
                            true,
                        );
                        (submit, cancel || footer_cancel)
                    };
                });
            if cancel || escape_pressed {
                self.pending_template = None;
            } else if insert {
                self.clipboard = nodes;
                self.paste();
                self.pending_template = None;
            }
            return;
        }
        if self.pending.is_some() {
            let mut cancel = false;
            let mut save = false;
            let mut discard = false;
            egui::Modal::new(egui::Id::new("unsaved-changes"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    cancel |= shell::dialog_header(ui, &tr("尚有未保存修改"));
                    ui.label(tr("保存整个项目后再继续，或放弃本次修改。"));
                    ui.add_space(20.0);
                    shell::dialog_buttons(ui, |ui| {
                        save = ui
                            .add_sized(
                                [108.0, 30.0],
                                egui::Button::new(
                                    RichText::new(tr("保存并继续")).color(Color32::WHITE),
                                )
                                .fill(ACCENT),
                            )
                            .clicked();
                        cancel |= ui
                            .add_sized([80.0, 30.0], egui::Button::new(tr("取消")))
                            .clicked();
                        discard = ui
                            .add_sized([96.0, 30.0], egui::Button::new(tr("放弃修改")))
                            .clicked();
                    });
                });
            if cancel || escape_pressed {
                self.pending = None;
            } else if discard || (save && self.save(false)) {
                if let Some(action) = self.pending.take() {
                    self.perform(action, ctx);
                }
            }
            return;
        }
        if !self.recovery_candidates.is_empty() && self.screenshot.is_none() {
            let mut close = false;
            egui::Modal::new(egui::Id::new("recovery-candidates"))
                .frame(shell::dialog_frame())
                .show(ctx, |ui| {
                    ui.set_width(500.0_f32.min(ctx.content_rect().width() - 80.0));
                    close |=
                        shell::dialog_header(ui, &crate::i18n::key("editor.workspace.recover"));
                    ui.label(crate::i18n::key("workspace.recovery.hint"));
                    ui.add_space(12.0);
                    egui::ScrollArea::vertical()
                        .max_height(ctx.content_rect().height() * 0.45)
                        .show(ui, |ui| {
                            for path in self.recovery_candidates.clone() {
                                let manifest = lom_core::load_json(path.join("manifest.json"))
                                    .unwrap_or(Value::Null);
                                let title = manifest["name"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or("未命名作品");
                                egui::Frame::group(ui.style())
                                    .inner_margin(12)
                                    .show(ui, |ui| {
                                        ui.strong(title);
                                        ui.label(
                                            RichText::new(format!(
                                                "副本 {}",
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                        ui.add_space(4.0);
                                        shell::dialog_buttons(ui, |ui| {
                                            if ui.button(tr("恢复")).clicked() {
                                                self.request(Pending::Recover(path.clone()), ctx);
                                            }
                                            if ui.button(tr("移除副本")).clicked() {
                                                if recovery_owner_active(&path) {
                                                    self.error = Some(
                                                        "此副本仍有活跃的所有者，不能移除。".into(),
                                                    );
                                                    return;
                                                }
                                                match std::fs::remove_dir_all(&path) {
                                                    Ok(()) => self
                                                        .recovery_candidates
                                                        .retain(|p| p != &path),
                                                    Err(e) => {
                                                        self.error =
                                                            Some(crate::error_ui::capture(e))
                                                    }
                                                }
                                            }
                                        });
                                    });
                            }
                        });
                    ui.add_space(16.0);
                    shell::dialog_buttons(ui, |ui| {
                        close |= ui
                            .add_sized([100.0, 30.0], egui::Button::new(tr("稍后处理")))
                            .clicked();
                    });
                });
            // Dismissing hides the prompt for this session and never deletes a copy.
            if close || escape_pressed {
                self.recovery_candidates.clear();
            }
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
fn recovery_owner_active(path: &Path) -> bool {
    let Some(pid) = path
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse::<u32>().ok())
    else {
        return true;
    };
    #[cfg(not(windows))]
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        return std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .map(|out| !out.stdout.is_empty())
            .unwrap_or(true);
    }
    #[cfg(windows)]
    {
        let Some(stamp) = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split_once('-'))
            .and_then(|(_, stamp)| stamp.parse::<u128>().ok())
        else {
            return true;
        };
        windows_recovery_owner_active(pid, stamp)
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}
#[cfg(windows)]
fn windows_recovery_owner_active(pid: u32, session_created_ms: u128) -> bool {
    use std::ffi::c_void;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn GetProcessTimes(
            handle: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: i32 = 87;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    const WINDOWS_TO_UNIX_EPOCH_MS: u64 = 11_644_473_600_000;

    // PID 0 is the system idle process, never a valid editor owner.
    if pid == 0 {
        return true;
    }
    // SAFETY: We only query this non-inherited process handle. OwnedHandle closes
    // every successful OpenProcess result, and the FILETIME outputs are valid.
    unsafe {
        let raw = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if raw.is_null() {
            // A missing PID is safe to recover; access denied and unknown errors
            // must keep a potentially active editor's recovery files protected.
            return std::io::Error::last_os_error().raw_os_error() != Some(ERROR_INVALID_PARAMETER);
        }
        let handle = OwnedHandle::from_raw_handle(raw);
        match WaitForSingleObject(handle.as_raw_handle(), 0) {
            WAIT_OBJECT_0 => false,
            WAIT_TIMEOUT => {
                let mut creation = FileTime::default();
                let mut exit = FileTime::default();
                let mut kernel = FileTime::default();
                let mut user = FileTime::default();
                if GetProcessTimes(
                    handle.as_raw_handle(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                ) == 0
                {
                    return true;
                }
                let ticks = (u64::from(creation.high) << 32) | u64::from(creation.low);
                // A process created after the session cannot own it: Windows
                // has reused an earlier editor's PID, including our own PID.
                (ticks / 10_000)
                    .checked_sub(WINDOWS_TO_UNIX_EPOCH_MS)
                    .map(|created_ms| u128::from(created_ms) <= session_created_ms)
                    .unwrap_or(true)
            }
            _ => true,
        }
    }
}
fn primary_insertion_edge(node: &mut Value) -> Option<&mut Value> {
    let kind = node["type"].as_str().unwrap_or("").to_owned();
    if node.get("success").is_some() {
        return node.get_mut("success");
    }
    match kind.as_str() {
        "choice" => node
            .get_mut("options")?
            .as_array_mut()?
            .first_mut()?
            .get_mut("goto"),
        "branch" => node
            .get_mut("cases")?
            .as_array_mut()?
            .first_mut()?
            .get_mut("goto"),
        "dice" => {
            if node["bands"].is_array() {
                node.get_mut("bands")?
                    .as_array_mut()?
                    .first_mut()?
                    .get_mut("goto")
            } else {
                node.get_mut("options")?
                    .as_array_mut()?
                    .first_mut()?
                    .get_mut("goto_大成功")
            }
        }
        "end" if node["resume_original"] == true => Some(
            node.as_object_mut()?
                .entry("resume_node")
                .or_insert(Value::Null),
        ),
        _ if lom_core::validate::node_schema()["no_goto_types"]
            .as_array()?
            .iter()
            .any(|value| value == &kind) =>
        {
            None
        }
        _ => Some(node.as_object_mut()?.entry("goto").or_insert(Value::Null)),
    }
}

fn insert_nodes_after(
    story: &mut Value,
    selected: usize,
    mut inserted: Vec<Value>,
) -> anyhow::Result<usize> {
    let original = story["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("章节缺少步骤"))?;
    let at = (selected + 1).min(original.len());
    if story["vanilla_import"]["schema"] == 2 && !inserted.is_empty() && selected < original.len() {
        let mut anchor = original[selected].clone();
        let fallback = lom_core::analysis::successors(story, selected);
        let linear = !lom_core::validate::node_schema()["no_goto_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == &anchor["type"])
            || anchor["type"] == "end" && anchor["resume_original"] == true;
        let edge = primary_insertion_edge(&mut anchor)
            .ok_or_else(|| anyhow::anyhow!("这个步骤结束当前流程，请在它之前或分支内部插入。"))?;
        let continuation = edge
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| (linear && fallback.len() == 1).then(|| fallback[0].clone()));
        *edge = inserted[0]["id"].clone();
        if let Some(continuation) = continuation {
            let ids: BTreeSet<_> = inserted
                .iter()
                .filter_map(|node| node["id"].as_str().map(str::to_owned))
                .collect();
            let block = json!({"nodes":inserted});
            let mut redirects = BTreeMap::from([(String::new(), continuation.clone())]);
            for index in 0..inserted.len() {
                for target in lom_core::analysis::successors(&block, index) {
                    if !ids.contains(&target) {
                        redirects.insert(target, continuation.clone());
                    }
                }
                if let Some(target) = inserted[index]["resume_node"]
                    .as_str()
                    .filter(|target| !ids.contains(*target))
                {
                    redirects.insert(target.to_owned(), continuation.clone());
                }
            }
            for index in 0..inserted.len() {
                let next = inserted
                    .get(index + 1)
                    .map(|node| node["id"].clone())
                    .unwrap_or(json!(continuation));
                let node = &mut inserted[index];
                retarget(node, &redirects);
                if node["type"] == "end"
                    && node["resume_original"] == true
                    && node["resume_node"].is_null()
                {
                    node["resume_node"] = next;
                } else if !lom_core::validate::node_schema()["no_goto_types"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|kind| kind == &node["type"])
                    && node["goto"].is_null()
                {
                    node["goto"] = next;
                }
            }
        }
        story["nodes"][selected] = anchor;
    }
    story["nodes"]
        .as_array_mut()
        .unwrap()
        .splice(at..at, inserted);
    Ok(at)
}
fn reconnect_deleted_structured(
    story: &mut Value,
    old_nodes: &[Value],
    removed: &BTreeSet<String>,
) -> anyhow::Result<()> {
    let mut original = story.clone();
    original["nodes"] = json!(old_nodes);
    let indices: BTreeMap<_, _> = old_nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| Some((node["id"].as_str()?, index)))
        .collect();
    let mut redirects = BTreeMap::new();
    for id in removed {
        let mut current = id.as_str();
        let mut visited = BTreeSet::new();
        while removed.contains(current) {
            anyhow::ensure!(
                visited.insert(current),
                "步骤“{id}”的删除范围形成循环，请先连接保留的后续步骤。"
            );
            let next = lom_core::analysis::successors(&original, indices[current]);
            anyhow::ensure!(
                next.len() <= 1,
                "步骤“{current}”有多个不同后继，请先选择并连接要保留的分支，再删除。"
            );
            let Some(target) = next.first() else { break };
            if !removed.contains(target) {
                redirects.insert(id.clone(), target.clone());
                break;
            }
            current = old_nodes[indices[target.as_str()]]["id"].as_str().unwrap();
        }
    }
    let forbidden: BTreeMap<_, _> = removed
        .iter()
        .map(|id| (id.clone(), String::new()))
        .collect();
    for node in story["nodes"].as_array_mut().unwrap() {
        retarget(node, &redirects);
        let mut probe = node.clone();
        retarget(&mut probe, &forbidden);
        anyhow::ensure!(
            probe == *node,
            "步骤“{}”仍指向没有明确后继的删除步骤，请先连接保留的后续步骤。",
            node["id"].as_str().unwrap_or("")
        );
    }
    if let Some(start) = story["start"]
        .as_str()
        .filter(|start| removed.contains(*start))
    {
        story["start"] = json!(redirects
            .get(start)
            .ok_or_else(|| anyhow::anyhow!("入口步骤没有明确后继，请先设置保留的入口。"))?);
    }
    Ok(())
}
fn reconnect_deleted_offsets(
    story: &mut Value,
    old_nodes: &[Value],
    removed: &BTreeSet<String>,
) -> anyhow::Result<()> {
    let offsets: BTreeMap<_, _> = old_nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node["type"] == "offset")
        .filter_map(|(index, node)| {
            let id = node["id"].as_str()?;
            removed.contains(id).then_some((id.to_owned(), index))
        })
        .collect();
    if offsets.is_empty() {
        return Ok(());
    }
    let original = json!({"nodes": old_nodes});
    let mut redirects = BTreeMap::new();
    let mut terminal = BTreeSet::new();
    for (id, &index) in &offsets {
        let mut current = index;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            // Follow the original links, including non-adjacent targets and
            // multi-deletions. Never pick an arbitrary arm of a deleted branch.
            let next = lom_core::analysis::successors(&original, current);
            let [next] = next.as_slice() else {
                if next.is_empty() {
                    terminal.insert(id.clone());
                }
                break;
            };
            let Some(index) = old_nodes.iter().position(|node| node["id"] == *next) else {
                break;
            };
            if !removed.contains(next) {
                redirects.insert(id.clone(), next.clone());
                break;
            }
            current = index;
        }
    }
    let forbidden: BTreeMap<_, _> = offsets
        .keys()
        .map(|id| (id.clone(), String::new()))
        .collect();
    let nodes = story["nodes"].as_array_mut().unwrap();
    let last = nodes.len().saturating_sub(1);
    for (index, node) in nodes.iter_mut().enumerate() {
        retarget(node, &redirects);
        // An automatically inserted final offset has no successor. Removing
        // its final incoming jump restores the original end of the sequence.
        if index == last
            && node["goto"]
                .as_str()
                .is_some_and(|id| terminal.contains(id))
        {
            node.as_object_mut().unwrap().remove("goto");
        }
        let mut probe = node.clone();
        retarget(&mut probe, &forbidden);
        anyhow::ensure!(
            probe == *node,
            "步骤“{}”的位移后续不明确，请先修改跳转后再删除。",
            node["id"].as_str().unwrap_or("")
        );
    }
    if let Some(next) = story["start"].as_str().and_then(|id| redirects.get(id)) {
        story["start"] = json!(next);
    }
    Ok(())
}
fn repair_deleted_metadata(story: &mut Value, old_ids: &[String], removed: &BTreeSet<String>) {
    fn repair(item: &mut Value, old_ids: &[String], removed: &BTreeSet<String>) -> bool {
        let start = old_ids
            .iter()
            .position(|id| item["start"].as_str() == Some(id));
        let end = old_ids
            .iter()
            .position(|id| item["end"].as_str() == Some(id));
        if let (Some(start), Some(end)) = (start, end) {
            let survivors: Vec<_> = old_ids[start.min(end)..=start.max(end)]
                .iter()
                .filter(|id| !removed.contains(*id))
                .collect();
            if survivors.is_empty() {
                return false;
            }
            if removed.contains(item["start"].as_str().unwrap_or("")) {
                item["start"] = json!(if start <= end {
                    survivors[0]
                } else {
                    survivors[survivors.len() - 1]
                });
            }
            if removed.contains(item["end"].as_str().unwrap_or("")) {
                item["end"] = json!(if start <= end {
                    survivors[survivors.len() - 1]
                } else {
                    survivors[0]
                });
            }
        }
        if let Some(groups) = item.get_mut("groups").and_then(Value::as_array_mut) {
            groups.retain_mut(|group| repair(group, old_ids, removed));
        }
        true
    }
    if let Some(sections) = story
        .get_mut("_editor")
        .and_then(|e| e.get_mut("sections"))
        .and_then(Value::as_array_mut)
    {
        sections.retain_mut(|section| repair(section, old_ids, removed));
    }
    if let Some(translations) = story
        .get_mut("localization")
        .and_then(|l| l.get_mut("translations"))
        .and_then(Value::as_object_mut)
    {
        for catalog in translations.values_mut() {
            if let Some(entries) = catalog.as_object_mut() {
                entries.retain(|key, _| !removed.contains(key.split('.').next().unwrap_or("")));
            }
        }
    }
}
fn template_project(key: &str) -> anyhow::Result<Project> {
    let templates: Value = serde_json::from_str(include_str!("../data/project-templates.json"))?;
    let mut project = Project::new();
    let template = templates
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("未知项目模板 {key}"))?;
    let campaign = project.manifest["campaign_id"]
        .as_str()
        .unwrap_or("my_campaign");
    let encoded = serde_json::to_string(&template["project"])?;
    let data: Value = serde_json::from_str(&encoded.replace("__NEW_CAMPAIGN_ID__", campaign))?;
    project.stories = serde_json::from_value(data["stories"].clone())?;
    for (k, v) in data["manifest"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("模板manifest无效"))?
    {
        project.manifest[k] = v.clone();
    }
    project.manifest["name"] = template["name"].clone();
    project.paths.clear();
    project.source = None;
    Ok(project)
}
fn compile_with_assets(project: &Project, story: &Value) -> anyhow::Result<String> {
    let root = lom_core::workdir::create()?;
    for (name, bytes) in &project.assets {
        lom_core::package::canonical_archive_name(name)?;
        anyhow::ensure!(name.starts_with("assets/"), "项目素材必须位于 assets/");
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    lom_core::codegen::compile_story(story, Some(&project.manifest), None, Some(root.path()))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn short(s: &str, n: usize) -> String {
    let mut out = s.replace('\n', " ").chars().take(n).collect::<String>();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}
pub(crate) fn retarget(value: &mut Value, map: &BTreeMap<String, String>) {
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "goto"
                        | "win"
                        | "lose"
                        | "success"
                        | "failure"
                        | "resume_node"
                        | "start"
                        | "end"
                        | "goto_大成功"
                        | "goto_成功"
                        | "goto_失败"
                ) {
                    if let Some(new) = v.as_str().and_then(|s| map.get(s)) {
                        *v = json!(new);
                    }
                } else {
                    retarget(v, map);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                retarget(v, map)
            }
        }
        _ => {}
    }
}
fn rename_translations(story: &mut Value, old: &str, new: &str) {
    if let Some(translations) = story
        .get_mut("localization")
        .and_then(|v| v.get_mut("translations"))
        .and_then(Value::as_object_mut)
    {
        for catalog in translations.values_mut() {
            if let Some(o) = catalog.as_object_mut() {
                let updates: Vec<_> = o
                    .keys()
                    .filter(|k| k.starts_with(&format!("{old}.")))
                    .cloned()
                    .collect();
                for key in updates {
                    if let Some(v) = o.remove(&key) {
                        o.insert(format!("{new}{}", &key[old.len()..]), v);
                    }
                }
            }
        }
    }
}
fn rename_story_refs(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(k.as_str(), "next_script" | "script" | "story")
                    && v.as_str() == Some(old)
                {
                    *v = json!(new);
                } else {
                    rename_story_refs(v, old, new);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                rename_story_refs(v, old, new)
            }
        }
        _ => {}
    }
}
fn chapter_group(story: &Value) -> &str {
    story["_editor"]["chapter_group"]
        .as_str()
        .unwrap_or("")
        .trim()
}
fn set_chapter_group(story: &mut Value, group: &str) -> anyhow::Result<()> {
    if group.is_empty() && story.get("_editor").is_none() {
        return Ok(());
    }
    let editor = story
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("章节格式错误"))?
        .entry("_editor")
        .or_insert_with(|| json!({}));
    if editor.is_null() {
        *editor = json!({});
    }
    let fields = editor
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("章节 _editor 必须是对象，分组未修改"))?;
    if group.is_empty() {
        fields.remove("chapter_group");
    } else {
        fields.insert("chapter_group".into(), json!(group));
    }
    if fields.is_empty() {
        story.as_object_mut().unwrap().remove("_editor");
    }
    Ok(())
}
fn story_reference_count(value: &Value, chapter: &str) -> usize {
    match value {
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| {
                if matches!(key.as_str(), "next_script" | "script" | "story")
                    && value.as_str() == Some(chapter)
                {
                    1
                } else {
                    story_reference_count(value, chapter)
                }
            })
            .sum(),
        Value::Array(values) => values
            .iter()
            .map(|value| story_reference_count(value, chapter))
            .sum(),
        _ => 0,
    }
}
fn replace_prose(value: &mut Value, find: &str, replace: &str) -> usize {
    let mut count = 0;
    if find.is_empty() {
        return 0;
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "text"
                        | "title"
                        | "description"
                        | "desc"
                        | "header"
                        | "bonus_name"
                        | "bonus_status"
                ) {
                    if let Some(s) = v.as_str() {
                        let n = s.matches(find).count();
                        if n > 0 {
                            *v = json!(s.replace(find, replace));
                            count += n;
                        }
                    }
                } else if k != "localization" && k != "_editor" {
                    count += replace_prose(v, find, replace);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                count += replace_prose(v, find, replace)
            }
        }
        _ => {}
    }
    count
}
fn report(ui: &mut egui::Ui, value: &Value, depth: usize) {
    crate::tool_report::show(ui, value, depth);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_retargets_nested_edges_without_rewriting_prose() {
        let mut v = json!({"id":"copy","text":"old","options":[{"goto":"old","text":"old"}],"_editor":{"sections":[{"start":"old","end":"old"}]}});
        retarget(&mut v, &BTreeMap::from([("old".into(), "new".into())]));
        assert_eq!(v["options"][0]["goto"], "new");
        assert_eq!(v["text"], "old");
        assert_eq!(v["_editor"]["sections"][0]["start"], "new");
    }
    #[test]
    fn bulk_edit_keeps_code_and_identifiers() {
        let mut v = json!({"id":"hello","title":"hello","nodes":[{"id":"hello","type":"say","text":"hello hello"},{"id":"lua","type":"raw","code":"hello"}]});
        assert_eq!(replace_prose(&mut v, "hello", "goodbye"), 3);
        assert_eq!(v["id"], "hello");
        assert_eq!(v["nodes"][1]["code"], "hello");
    }
    #[test]
    fn all_catalog_node_forms_have_defaults() {
        let c = Catalog::new();
        let schemas = c.schema["NODE_SCHEMAS"].as_object().unwrap();
        assert_eq!(schemas.len(), 70);
        for kind in schemas.keys() {
            let node = c.new_node(kind, "n1".into());
            assert_eq!(node["type"], kind.as_str());
            assert_eq!(node["id"], "n1");
            assert!(node.is_object());
        }
    }
}

#[cfg(test)]
mod application_regression_tests {
    use super::*;
    fn app() -> App {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = App::new(&cc, None, None);
        app.recovery_candidates.clear();
        app.auto_save_project = false;
        app.auto_save_seconds = 300;
        app
    }
    fn choice_preview() -> App {
        let mut app = app();
        app.project.stories.insert("main".into(), json!({"id":"main","start":"choice1","nodes":[
            {"id":"choice1","type":"choice","options":[{"text":"继续","goto":"choice2"},{"text":"回看","goto":"choice1"}]},
            {"id":"choice2","type":"choice","options":[{"text":"继续","goto":"say1"},{"text":"结束","goto":"end1"}]},
            {"id":"say1","type":"say","mode":"narrative","text":"选择后的对白。"},
            {"id":"end1","type":"end"}]}));
        app.current = "main".into();
        app.selected = 0;
        app.sync_preview_selection();
        app
    }
    #[test]
    fn preview_choices_resume_playback_through_consecutive_branches() {
        let mut app = choice_preview();
        app.auto = true;
        for target in ["choice2", "say1"] {
            app.last_step = Instant::now() - Duration::from_secs(60);
            app.advance_preview();
            assert!(app.auto);
            assert!(app.preview_waiting_branch);
            let waiting = app.selected;
            app.advance_preview();
            assert_eq!(app.selected, waiting);
            app.follow_preview_target(target);
            app.sync_preview_selection();
            assert!(app.auto, "a choice must not act like manual node selection");
            assert!(!app.preview_waiting_branch);
            assert_eq!(app.preview_elapsed, Some(0.0));
        }
        app.last_step = Instant::now() - Duration::from_secs(60);
        app.advance_preview();
        assert_eq!(app.story()["nodes"][app.selected]["id"], "end1");
        app.last_step = Instant::now() - Duration::from_secs(60);
        app.advance_preview();
        assert!(!app.auto);
    }
    #[test]
    fn preview_choice_preserves_pause_and_manual_navigation_stops_playback() {
        let mut app = choice_preview();
        app.follow_preview_target("say1");
        app.sync_preview_selection();
        assert!(!app.auto);
        app.selected = 0;
        app.sync_preview_selection();
        app.auto = true;
        app.last_step = Instant::now() - Duration::from_secs(60);
        app.advance_preview();
        app.follow_preview_target("missing");
        assert_eq!(app.selected, 0);
        assert!(app.preview_waiting_branch);
        app.follow_preview_target("choice1");
        assert!(app.auto);
        assert!(!app.preview_waiting_branch);
        app.selected = 2;
        app.sync_preview_selection();
        assert!(!app.auto);
        assert!(!app.preview_waiting_branch);
    }
    #[test]
    fn playtest_start_uses_the_chapter_entry_without_changing_selection() {
        let mut app = app();
        app.project.stories.insert(
            "main".into(),
            json!({"id":"main","title":"测试章","start":"intro","nodes":[
            {"id":"later","type":"end"},{"id":"intro","type":"wait","seconds":1}]}),
        );
        app.current = "main".into();
        app.selected = 0;
        assert_eq!(app.playtest_node(false), "later");
        assert_eq!(app.playtest_node(true), "intro");
        assert_eq!(app.selected, 0);
    }
    #[test]
    fn preflight_errors_show_details_and_target_the_owning_chapter() {
        let mut app = app();
        for id in ["main", "other"] {
            app.project.stories.insert(
                id.into(),
                json!({"id":id,"title":id,"start":"effect1","nodes":[
                {"id":"effect1","type":"effect","name":""},{"id":"end","type":"end"}]}),
            );
        }
        assert!(!app.check_before_run_or_export());
        assert!(app.error.as_ref().unwrap().contains('2'));
        assert_eq!(app.error_issues.len(), 2);
        for issue in &app.error_issues {
            assert!(issue.message.contains("特效名称不能为空"));
            assert_eq!(
                crate::error_location::issue_destination(issue, &app.project),
                crate::error_location::Destination::Story(
                    issue.story_id.clone(),
                    Some("effect1".into())
                )
            );
        }
        let report = crate::error_location::issue_report(&app.error_issues, &app.project);
        assert!(report.contains("main") && report.contains("other") && report.contains("effect1"));
    }
    #[test]
    fn error_dialog_shows_an_action_and_preserves_raw_diagnostics() {
        for (message, action) in [
            (
                "原版音频试听组件缺失：C:/Modkit/audio-decoder/vgmstream-cli.exe",
                "重新打开",
            ),
            (
                "无法保存：C:/作品/story/chapter1.json: Permission denied (os error 5)",
                "另存为",
            ),
            (
                "打开失败：unrecognized project metadata in C:/作品/project.json",
                "开发者",
            ),
        ] {
            let mut app = app();
            app.error = Some(message.into());
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1200.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ctx| app.dialogs(ctx),
            );
            let update = output.platform_output.accesskit_update.unwrap();
            let text = update
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().or_else(|| node.value()))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains(action), "{text}");
            assert!(!text.contains(message), "{text}");
            assert_eq!(app.error.as_deref(), Some(message));
            assert!(output.platform_output.commands.is_empty());
        }
    }
    #[test]
    fn lua_copy_and_export_recompile_instead_of_treating_error_prose_as_a_script() {
        for valid in [true, false] {
            let mut app = app();
            if !valid {
                app.project.stories.insert(
                    "main".into(),
                    json!({"id":"main","story_schema":2,"start":"broken","nodes":[
                        {"id":"broken","type":"effect","name":""}, {"id":"end","type":"end"}
                    ]}),
                );
            }
            app.view = View::Lua;
            app.lua = "缓存中的提示文字，不能当作脚本复制或导出".into();
            let before = snapshot(&app.project);
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let input = |events| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let output = ctx.run(input(vec![]), |ctx| app.right(ctx));
            let click = click_named_option(&output, &tr("复制 Lua"));
            let output = ctx.run(input(vec![click]), |ctx| app.right(ctx));
            let copied = output
                .platform_output
                .commands
                .iter()
                .find_map(|cmd| match cmd {
                    egui::OutputCommand::CopyText(text) => Some(text.clone()),
                    _ => None,
                });
            if valid {
                assert_eq!(
                    copied,
                    Some(compile_with_assets(&app.project, &app.story()).unwrap())
                );
                assert!(app.error.is_none());
            } else {
                assert!(copied.is_none());
                assert!(app.error.as_deref().unwrap().contains("特效名称不能为空"));
                app.error = None;
                let click = click_named_option(&output, &tr("导出 Lua…"));
                let _ = ctx.run(input(vec![click]), |ctx| app.right(ctx));
                assert!(app.error.as_deref().unwrap().contains("特效名称不能为空"));
            }
            assert!(snapshot(&app.project) == before);
        }
    }
    #[test]
    fn project_autosave_writes_directory_json_and_package_without_a_dialog() {
        for layout in ["directory", "json", "package"] {
            let dir = tempfile::tempdir().unwrap();
            let target = match layout {
                "json" => dir.path().join("main.json"),
                "package" => dir.path().join("work.lommod"),
                _ => dir.path().join("project"),
            };
            let mut project = Project::new();
            project.save_to(&target).unwrap();
            let mut app = app();
            app.install(Project::open(&target).unwrap());
            app.recovery_dir = dir.path().join("recovery");
            app.auto_save_project = true;
            app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
                json!("自动保存的新进度");
            app.track();
            app.last_auto_save = Instant::now() - Duration::from_secs(301);
            app.autosave();
            assert!(app.error.is_none(), "{layout}: {:?}", app.error);
            assert!(!app.dirty(), "{layout}");
            assert!(app.pending.is_none());
            assert_eq!(
                Project::open(&target).unwrap().stories["main"]["nodes"][0]["text"],
                "自动保存的新进度"
            );
            assert!(app.status.contains(&target.display().to_string()));
        }
    }

    #[test]
    fn project_autosave_keeps_new_and_disabled_projects_in_recovery() {
        for has_source in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = app();
            if has_source {
                app.project.save_to(&dir.path().join("project")).unwrap();
                app.last = snapshot(&app.project);
                app.mark_saved();
            }
            let original = app
                .project
                .source
                .as_deref()
                .map(|path| Project::open(path).unwrap());
            app.auto_save_project = !has_source;
            app.recovery_dir = dir.path().join("recovery");
            app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
                json!("恢复副本中的新进度");
            app.track();
            app.last_auto_save = Instant::now() - Duration::from_secs(301);
            app.last_recovery = Instant::now() - Duration::from_secs(31);
            app.autosave();
            assert!(app.dirty());
            assert!(app.pending.is_none());
            assert_eq!(
                Project::open(&app.recovery_dir).unwrap().stories["main"]["nodes"][0]["text"],
                "恢复副本中的新进度"
            );
            if let Some(original) = original {
                assert_eq!(
                    Project::open(original.source.as_ref().unwrap())
                        .unwrap()
                        .stories,
                    original.stories
                );
            }
        }
    }

    #[test]
    fn project_autosave_preserves_newer_external_progress_and_keeps_its_own_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let mut app = app();
        app.project.save_to(&root).unwrap();
        app.last = snapshot(&app.project);
        app.mark_saved();
        let mut other = Project::open(&root).unwrap();
        other.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("另一个窗口的新进度");
        other.save_to(&root).unwrap();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("本窗口的修改");
        app.track();
        app.auto_save_project = true;
        app.recovery_dir = dir.path().join("recovery");
        app.last_auto_save = Instant::now() - Duration::from_secs(301);
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        assert!(app.dirty());
        assert!(app.error.as_ref().unwrap().contains("编辑器外修改"));
        assert_eq!(
            Project::open(&root).unwrap().stories["main"]["nodes"][0]["text"],
            "另一个窗口的新进度"
        );
        assert_eq!(
            Project::open(&app.recovery_dir).unwrap().stories["main"]["nodes"][0]["text"],
            "本窗口的修改"
        );
    }

    #[test]
    fn project_autosave_preferences_preserve_other_settings_and_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{"ui_locale":"chs","custom":"keep"}"#).unwrap();
        let mut app = app();
        let before = snapshot(&app.project);
        assert_eq!(app.auto_save_seconds, 300);
        app.set_auto_save(true, 60, &path).unwrap();
        let prefs = lom_core::load_json(&path).unwrap();
        assert_eq!(prefs["auto_save_project"], true);
        assert_eq!(prefs["auto_save_seconds"], 60);
        assert_eq!(prefs["custom"], "keep");
        assert_eq!(prefs["ui_locale"], "chs");
        assert!(app.set_auto_save(true, 0, &path).is_err());
        assert!(snapshot(&app.project) == before);
    }

    #[test]
    fn project_autosave_waits_for_the_interval_and_respects_pending_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let mut app = app();
        app.project.save_to(&root).unwrap();
        app.last = snapshot(&app.project);
        app.mark_saved();
        let baseline = app.project.stories.clone();
        app.auto_save_project = true;
        app.recovery_dir = dir.path().join("recovery");
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("待保存的新进度");
        app.track();
        app.last_auto_save = Instant::now();
        app.autosave();
        assert_eq!(Project::open(&root).unwrap().stories, baseline);
        app.pending = Some(Pending::Quit);
        app.last_auto_save = Instant::now() - Duration::from_secs(301);
        app.autosave();
        assert_eq!(Project::open(&root).unwrap().stories, baseline);
        app.pending = None;
        // Recovery deliberately clears the saved baseline until the user confirms it.
        app.saved.stories.clear();
        app.last_auto_save = Instant::now() - Duration::from_secs(301);
        app.autosave();
        assert_eq!(Project::open(&root).unwrap().stories, baseline);
        assert!(app.dirty());
    }
    #[test]
    fn chapter_groups_support_rename_move_undo_and_project_roundtrip() {
        let mut app = app();
        let mut project = Project::new();
        let mut second = project.stories["main"].clone();
        second["id"] = json!("second");
        project.stories.insert("second".into(), second);
        app.install(project);
        let lua = compile_with_assets(&app.project, &app.story()).unwrap();
        app.begin_chapter_group(false);
        app.chapter_dialog.as_mut().unwrap().name = "主线".into();
        app.commit_chapter().unwrap();
        assert_eq!(chapter_group(&app.project.stories["main"]), "主线");
        assert_eq!(
            compile_with_assets(&app.project, &app.story()).unwrap(),
            lua
        );
        app.current = "second".into();
        app.move_chapter_to_group("主线").unwrap();
        app.begin_chapter_group(true);
        app.chapter_dialog.as_mut().unwrap().name = "任务".into();
        app.commit_chapter().unwrap();
        assert!(app
            .project
            .stories
            .values()
            .all(|story| chapter_group(story) == "任务"));
        app.undo();
        assert!(app
            .project
            .stories
            .values()
            .all(|story| chapter_group(story) == "主线"));
        app.redo();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("project");
        app.project.save_to(&path).unwrap();
        assert!(Project::open(&path)
            .unwrap()
            .stories
            .values()
            .all(|story| chapter_group(story) == "任务"));
        app.move_chapter_to_group("").unwrap();
        assert_eq!(chapter_group(&app.project.stories["second"]), "");
        assert_eq!(chapter_group(&app.project.stories["main"]), "任务");
    }
    #[test]
    fn cached_saved_status_follows_reverts_save_undo_and_redo() {
        let mut app = app();
        let clean = app.project.stories.clone();
        assert!(!app.dirty());
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("修改");
        app.track();
        assert!(app.dirty());
        app.project.stories = clean;
        app.track();
        assert!(!app.dirty());
        app.flush();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("保存内容");
        app.track();
        app.flush();
        app.mark_saved();
        assert!(!app.dirty());
        app.undo();
        assert!(app.dirty());
        app.redo();
        assert!(!app.dirty());
    }

    #[test]
    fn step_rows_show_notes_or_ids_alongside_the_existing_summary() {
        let mut app = app();
        let mut project = Project::new();
        project.stories.get_mut("main").unwrap()["nodes"][0]["_editor"] =
            json!({"note":"开场备注"});
        app.install(project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        );
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, node)| node.label().map(str::to_owned))
            .collect();
        assert!(labels.iter().any(|text| text.contains("开场备注")));
        let id = app.story()["nodes"][1]["id"].as_str().unwrap().to_owned();
        assert!(labels
            .iter()
            .any(|text| text.contains(&id) && text.contains("返回自由模式")));
    }
    #[test]
    #[ignore = "manual timing of editor hot paths on a large project"]
    fn manual_editor_performance_check() {
        fn measure(label: &str, runs: usize, mut operation: impl FnMut()) {
            let mut elapsed = Vec::with_capacity(runs);
            for _ in 0..runs {
                let started = Instant::now();
                operation();
                elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            elapsed.sort_by(f64::total_cmp);
            println!(
                "PERF {label} median_ms={:.3} p95_ms={:.3}",
                elapsed[runs / 2],
                elapsed[runs * 95 / 100]
            );
        }
        let mut app = app();
        let mut project = Project::new();
        let story = json!({"id":"main","start":"n0","nodes":(0..2000).map(|index| {
            json!({"id":format!("n{index}"),"type":"say","character":"player","portrait":"normal","text":format!("性能检查对白 {index}")})
        }).collect::<Vec<_>>()});
        project.stories.insert("main".into(), story);
        for index in 1..10 {
            let id = format!("chapter{index}");
            let mut story = project.stories["main"].clone();
            story["id"] = json!(id);
            project.stories.insert(id, story);
        }
        project
            .assets
            .insert("perf-unused.bin".into(), vec![42; 64 * 1024 * 1024]);
        app.install(project);
        measure("story_copy", 30, || {
            std::hint::black_box(app.story());
        });
        measure("saved_status", 30, || {
            std::hint::black_box(app.dirty());
        });
        measure("change_tracking", 30, || app.track());
        let node = app.story()["nodes"][0].clone();
        measure("character_options", 30, || {
            std::hint::black_box(app.catalog.options("character_group", &node, &[], &[]));
        });
        let ctx = egui::Context::default();
        let draw = |app: &mut App| {
            std::hint::black_box(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.sidebar(ctx);
                    app.right(ctx);
                    app.center(ctx);
                },
            ));
        };
        for _ in 0..3 {
            draw(&mut app);
        }
        measure("stage_frame", 30, || {
            draw(&mut app);
            app.track();
            std::hint::black_box(app.dirty());
        });
        app.view = View::Graph;
        for _ in 0..3 {
            draw(&mut app);
        }
        measure("graph_frame", 30, || {
            draw(&mut app);
            app.track();
            std::hint::black_box(app.dirty());
        });
    }
    #[test]
    fn long_step_lists_only_draw_visible_rows_and_follow_offscreen_selection() {
        let mut app = app();
        let mut project = Project::new();
        project.stories.get_mut("main").unwrap()["nodes"] = Value::Array(
            (0..2000)
                .map(|index| {
                    json!({"id":format!("n{index}"),"type":"say","text":format!("滚动节点_{index:04}")})
                })
                .collect(),
        );
        project.stories.get_mut("main").unwrap()["start"] = json!("n0");
        app.install(project);
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        for selected in [0, 1999, 1000] {
            app.selected = selected;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| app.sidebar(ctx),
            );
            let labels: Vec<_> = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .into_iter()
                .filter_map(|(_, node)| node.label().map(str::to_owned))
                .filter(|label| label.contains("滚动节点_"))
                .collect();
            assert!(labels.len() < 30, "offscreen steps must not create widgets");
            assert!(labels
                .iter()
                .any(|label| label.contains(&format!("滚动节点_{selected:04}"))));
            app.track();
            assert!(snapshot(&app.project) == before);
            assert!(!app.dirty());
        }
    }
    #[test]
    fn idle_frames_sleep_until_compile_history_or_recovery_is_due() {
        let mut app = app();
        app.needs_compile = false;
        assert_eq!(app.idle_repaint_delay(), Duration::MAX);
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("修改");
        app.track();
        assert!(app.idle_repaint_delay() <= Duration::from_millis(400));
        app.needs_compile = false;
        assert!(app.idle_repaint_delay() <= Duration::from_millis(650));
        app.flush();
        assert!(app.idle_repaint_delay() <= Duration::from_secs(30));
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        assert_eq!(app.idle_repaint_delay(), Duration::ZERO);
        app.mark_saved();
        assert_eq!(app.idle_repaint_delay(), Duration::MAX);
        app.screenshot = Some(PathBuf::from("unused-screenshot-test.png"));
        assert!(app.idle_repaint_delay() <= Duration::from_millis(70));
    }
    #[test]
    fn original_dialogue_flag_insertion_ui_undoes_and_does_not_save_original_lua() {
        let mut app = app();
        app.install(Project::new_vanilla_patch("DEMO").unwrap());
        let source = "if original then say(\"原对白\") end\nadvance_original()";
        app.vanilla = crate::vanilla_panel::Panel::with_source("DEMO", source);
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 1600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.right(ctx);
                    app.center(ctx);
                },
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        // AccessKit returns nodes by ID, not screen order. Select the checkbox
        // for the before-line insertion by its position, not either label's ID.
        let target = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, node)| node.label() == Some("仅在旗标检定成立时插入"))
            .min_by(|(_, a), (_, b)| a.bounds().unwrap().y0.total_cmp(&b.bounds().unwrap().y0))
            .unwrap()
            .0;
        let click = egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target,
            data: None,
        });
        draw(&mut app, vec![click]);
        app.track();
        app.flush();
        assert_eq!(
            app.story()["vanilla_patch"]["edits"][0]["before_condition"]["source"],
            "condition"
        );
        assert!(!serde_json::to_string(&app.project.stories)
            .unwrap()
            .contains("advance_original"));
        assert!(app.project.assets.is_empty());
        app.undo();
        assert!(snapshot(&app.project) == before);
        app.redo();
        app.project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"][0]["before"] =
            json!("新增对白");
        app.project.stories.get_mut("main").unwrap()["vanilla_patch"]["edits"][0]
            ["before_condition"]["flag"] = json!("DEMO_CHECKPOINT");
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("project");
        app.project.save_to(&path).unwrap();
        assert_eq!(Project::open(&path).unwrap().stories, app.project.stories);
        let pending_before = snapshot(&app.project);
        app.track();
        app.request(Pending::NewVanillaPatch, &ctx);
        assert!(matches!(app.pending, Some(Pending::NewVanillaPatch)));
        assert!(snapshot(&app.project) == pending_before);
    }
    #[test]
    fn full_original_import_button_preserves_source_undo_and_editable_step_defaults() {
        let mut app = app();
        app.install(Project::new_vanilla_patch("DEMO").unwrap());
        let source="setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nlocal amount=3\nif original then\nsay(\"原对白\")\nend\nadvance_original(amount)\n";
        app.vanilla = crate::vanilla_panel::Panel::with_source("DEMO", source);
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 1600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.center(ctx),
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, node)| node.label() == Some("导入整章到步骤编辑器"))
            .unwrap()
            .0;
        draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        assert!(app.story().get("vanilla_patch").is_none());
        assert_eq!(
            lom_core::vanilla_import::parse(&app.story())
                .unwrap()
                .unwrap()
                .source(),
            source
        );
        let imported = snapshot(&app.project);
        app.undo();
        assert!(snapshot(&app.project) == before);
        app.redo();
        assert!(snapshot(&app.project) == imported);
        app.selected = app.story()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["type"] == "say")
            .unwrap();
        draw(&mut app, vec![]);
        assert!(
            snapshot(&app.project) == imported,
            "Opening an imported step must not rewrite its baseline fields"
        );
        app.add_node("say");
        assert_eq!(app.story()["nodes"][app.selected]["inherit_dialog"], true);
        app.project.stories.get_mut("main").unwrap()["nodes"][app.selected]["text"] =
            json!("新增对白");
        app.copy();
        app.paste();
        assert!(app.story()["nodes"][app.selected]["_editor"]
            .get("vanilla_origin")
            .is_none());
        app.add_node("end");
        assert_eq!(app.story()["nodes"][app.selected]["resume_original"], true);
        lom_core::validate::validate_story(&app.story()).unwrap();
    }
    fn structured_branch_app() -> App {
        let mut app = app();
        let mut project = Project::new_vanilla_patch("DEMO").unwrap();
        project.stories.insert("main".into(), lom_core::vanilla_import::import(
            "setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nif original then\nsay(\"真支原文\")\nelse\nsay(\"假支原文\")\nend\nsay(\"汇合原文\")\n",
            "main", "原版分支", "DEMO", &BTreeMap::new(),
        ).unwrap());
        app.install(project);
        app
    }
    #[test]
    fn structured_additions_follow_the_true_branch_and_resume_its_original_target() {
        let mut app = structured_branch_app();
        app.selected = app.story()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["type"] == "native_branch")
            .unwrap();
        let branch_index = app.selected;
        let before = app.story()["nodes"][branch_index].clone();
        app.add_node("say");
        let added = app.story()["nodes"][app.selected].clone();
        assert_eq!(app.story()["nodes"][branch_index]["success"], added["id"]);
        assert_eq!(
            app.story()["nodes"][branch_index]["failure"],
            before["failure"]
        );
        assert_eq!(added["goto"], before["success"]);
        app.project.stories.get_mut("main").unwrap()["nodes"][app.selected]["text"] =
            json!("新增真支对白");

        app.selected = app.story()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["text"] == "真支原文")
            .unwrap();
        let tail = app.selected;
        let original_next = app.story()["nodes"][tail]["goto"].clone();
        app.add_node("end");
        let end = app.story()["nodes"][app.selected].clone();
        assert_eq!(end["resume_original"], true);
        assert_eq!(end["resume_node"], original_next);
        assert_eq!(app.story()["nodes"][tail]["goto"], end["id"]);
        assert_eq!(
            lom_core::analysis::successors(&app.story(), app.selected),
            vec![original_next.as_str().unwrap().to_owned()]
        );
        lom_core::validate::validate_story(&app.story()).unwrap();
        compile_with_assets(&app.project, &app.story()).unwrap();
    }
    #[test]
    fn structured_paste_preserves_internal_links_and_reconnects_the_external_exit() {
        let mut app = structured_branch_app();
        app.selected = app.story()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["text"] == "真支原文")
            .unwrap();
        let anchor = app.selected;
        let continuation = app.story()["nodes"][anchor]["goto"].clone();
        app.clipboard = vec![
            json!({"id":"copy_a","type":"say","text":"新增甲","inherit_dialog":true,"goto":"copy_b","_editor":{"vanilla_origin":1}}),
            json!({"id":"copy_b","type":"say","text":"新增乙","inherit_dialog":true,"goto":"other_chapter_exit"}),
        ];
        app.paste();
        let story = app.story();
        let first = &story["nodes"][app.selected];
        let last = &story["nodes"][app.selected + 1];
        assert_eq!(story["nodes"][anchor]["goto"], first["id"]);
        assert_eq!(first["goto"], last["id"]);
        assert_eq!(last["goto"], continuation);
        assert!(first["_editor"].get("vanilla_origin").is_none());
        lom_core::validate::validate_story(&story).unwrap();
        compile_with_assets(&app.project, &story).unwrap();
    }
    #[test]
    fn native_nodes_are_rejected_outside_structured_original_chapters() {
        let mut app = app();
        let before = snapshot(&app.project);
        for kind in ["native_action", "native_branch"] {
            app.add_node(kind);
            assert!(app.error.is_some());
            assert!(snapshot(&app.project) == before);
        }
        app.clipboard = vec![json!({"id":"native","type":"native_action","operations":[]})];
        app.paste();
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn structured_delete_reconnects_single_successors_and_keeps_ambiguous_branches() {
        let mut app = structured_branch_app();
        app.selected = app.story()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["text"] == "真支原文")
            .unwrap();
        let removed = app.story()["nodes"][app.selected].clone();
        app.delete();
        assert!(app.error.is_none());
        let story = app.story();
        assert!(!story["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == removed["id"]));
        let branch = story["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["type"] == "native_branch")
            .unwrap();
        assert_eq!(story["nodes"][branch]["success"], removed["goto"]);
        lom_core::validate::validate_story(&story).unwrap();
        compile_with_assets(&app.project, &story).unwrap();

        app.selected = branch;
        let before = snapshot(&app.project);
        app.delete();
        assert!(snapshot(&app.project) == before);
        assert!(app.error.as_ref().unwrap().contains("多个不同后继"));
    }
    #[test]
    fn structured_template_insertion_reuses_the_connected_paste_path() {
        let mut app = structured_branch_app();
        app.selected = app.story()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["text"] == "真支原文")
            .unwrap();
        let anchor = app.selected;
        let continuation = app.story()["nodes"][anchor]["goto"].clone();
        app.pending_template = Some((
            vec![
                json!({"id":"template_say","type":"say","text":"模板对白","inherit_dialog":true,"goto":"outside"}),
            ],
            vec!["外部引用 outside 会保留，请确认目标章节中存在该 ID".into()],
        ));
        let context = egui::Context::default();
        context.enable_accesskit();
        let output = context.run(egui::RawInput::default(), |context| app.dialogs(context));
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("接回当前流程并插入"))
            .unwrap()
            .0;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target,
                        data: None,
                    },
                )],
                ..Default::default()
            },
            |context| app.dialogs(context),
        );
        assert!(app.pending_template.is_none());
        let story = app.story();
        assert_eq!(
            story["nodes"][anchor]["goto"],
            story["nodes"][app.selected]["id"]
        );
        assert_eq!(story["nodes"][app.selected]["goto"], continuation);
        lom_core::validate::validate_story(&story).unwrap();
    }
    #[test]
    fn legacy_original_conversion_button_is_transactional_and_undoable() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../lom-core/tests/fixtures/vanilla-import.json"
        ))
        .unwrap();
        for edited_raw in [false, true] {
            let mut app = app();
            let mut project = Project::new_vanilla_patch("DEMO").unwrap();
            let mut story = fixture["story"].clone();
            if edited_raw {
                story["nodes"][0]["code"] = json!(format!(
                    "{}\nuser_change()",
                    story["nodes"][0]["code"].as_str().unwrap()
                ));
            }
            project.stories.clear();
            project.stories.insert("native_import".into(), story);
            project.manifest["entry"] = json!("native_import");
            app.install(project);
            let before = snapshot(&app.project);
            let context = egui::Context::default();
            context.enable_accesskit();
            let draw = |app: &mut App, events| {
                context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 1200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| app.chapter(ui));
                    },
                )
            };
            let output = draw(&mut app, vec![]);
            let target = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("转换原版分支与高级步骤"))
                .unwrap()
                .0;
            draw(
                &mut app,
                vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target,
                        data: None,
                    },
                )],
            );
            if edited_raw {
                assert!(snapshot(&app.project) == before);
                assert!(app.error.as_ref().unwrap().contains("高级 Lua 已修改"));
            } else {
                assert_eq!(app.story()["vanilla_import"]["schema"], 2);
                app.track();
                app.flush();
                let upgraded = snapshot(&app.project);
                app.undo();
                assert!(snapshot(&app.project) == before);
                app.redo();
                assert!(snapshot(&app.project) == upgraded);
            }
        }
    }
    #[test]
    fn legacy_rotation_preference_preserves_project_angles_and_other_settings() {
        let mut app = app();
        let before = snapshot(&app.project);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("preferences.json");
        std::fs::write(&path, r#"{"custom":"keep","ui_locale":"chs"}"#).unwrap();
        app.set_legacy_character_rotation(true, &path).unwrap();
        assert!(app.preview.legacy_character_rotation);
        let prefs = lom_core::load_json(&path).unwrap();
        assert_eq!(prefs["legacy_character_rotation"], true);
        assert_eq!(prefs["custom"], "keep");
        app.set_legacy_character_rotation(false, &path).unwrap();
        assert!(!app.preview.legacy_character_rotation);
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn game_bookmarks_locate_dialogue_preserve_edits_and_use_the_unsaved_project_guard() {
        let mark = lom_core::story_bookmarks::Bookmark {
            schema: 1,
            script: "DEMO".into(),
            text_key: "D_SECOND".into(),
            text: String::new(),
            game_dir: PathBuf::new(),
            year: 1,
            month: 4,
            stage: 1,
            captured_utc: "2026-11-05T12:00:00Z".into(),
        };
        let mut app = app();
        app.install(Project::new_vanilla_patch("DEMO").unwrap());
        app.vanilla = crate::vanilla_panel::Panel::with_source(
            "DEMO",
            "say(luamanager.GetStoryText(\"D_FIRST\"))\nsay(luamanager.GetStoryText(\"D_SECOND\"))",
        );
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        app.open_story_bookmark(mark.clone(), &ctx);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1600.0),
                )),
                ..Default::default()
            },
            |ctx| app.center(ctx),
        );
        assert_eq!(app.vanilla.selected_key("DEMO"), Some("D_SECOND"));
        assert!(snapshot(&app.project) == before);
        let mut other = mark.clone();
        other.script = "OTHER".into();
        app.open_story_bookmark(other, &ctx);
        app.track();
        app.flush();
        assert_eq!(app.project.stories.len(), 2);
        assert_eq!(app.project.stories["main"], before.stories["main"]);
        app.undo();
        assert_eq!(app.project.stories, before.stories);
        app.install(Project::new());
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("未保存修改");
        app.track();
        let dirty = snapshot(&app.project);
        app.open_story_bookmark(mark, &ctx);
        assert!(matches!(app.pending, Some(Pending::OpenVanillaBookmark(_))));
        assert!(snapshot(&app.project) == dirty);
    }
    #[test]
    fn error_button_opens_and_focuses_the_manifest_field_without_changing_the_project() {
        let mut app = app();
        app.project.manifest["play_mode"] = json!("sandbox");
        app.project.manifest["campaign_id"] = json!("");
        let before = snapshot(&app.project);
        app.error = Some("导出失败\nmanifest.json: 存档身份未填写。".into());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let output = ctx.run(input(vec![]), |ctx| app.dialogs(ctx));
        let click = click_named_option(&output, "去填写存档身份");
        let _ = ctx.run(input(vec![click]), |ctx| app.dialogs(ctx));
        assert!(app.error.is_none());
        assert_eq!(app.center, Center::Manifest);
        let _ = ctx.run(input(vec![]), |ctx| app.center(ctx));
        assert!(ctx.memory(|memory| memory.focused().is_some()));
        assert!(ctx
            .data(|data| data.get_temp::<String>(egui::Id::new("problem-field")))
            .is_none());
        assert!(snapshot(&app.project) == before);
    }

    #[test]
    fn error_navigation_selects_the_owned_node_and_asset() {
        let mut app = app();
        let ctx = egui::Context::default();
        let story = json!({"id":"main","nodes":[{"id":"shared","type":"wait","seconds":1}]});
        app.project.stories.insert("main".into(), story.clone());
        app.project.stories.insert("other".into(), story);
        let raw = "story/other.json: 节点 \"shared\"(wait): 字段 \"seconds\" 必须是数字";
        app.go_to_problem(&ctx, crate::error_location::resolve(raw, &app.project));
        assert_eq!(app.current, "other");
        assert_eq!(app.selected, 0);
        assert_eq!(app.center, Center::Node);
        let path = "assets/user/image/demo.picture/content.json";
        app.project.assets.insert(path.into(), b"{}".to_vec());
        app.go_to_problem(
            &ctx,
            crate::error_location::resolve("用户内容 demo.picture 的图片不存在", &app.project),
        );
        assert_eq!(app.center, Center::Assets);
        assert_eq!(app.asset_selection, path);
    }
    #[test]
    fn dragging_a_mood_updates_the_same_node_as_the_form() {
        assert_mood_drag_updates(None, false);
    }
    #[test]
    fn dragging_a_group_mood_updates_only_its_action() {
        for action in [0, 1] {
            assert_mood_drag_updates(Some(action), false);
        }
        assert_mood_drag_updates(Some(0), true);
    }
    fn assert_mood_drag_updates(action: Option<usize>, second_mood: bool) {
        let mut app = app();
        app.project.stories.insert("main".into(), json!({"story_schema":2,"id":"main","title":"气泡拖动","start":"show","nodes":[
            {"id":"show","type":"show","character":"user:test.actor","position":"M","facing":"left"},
            {"id":"bubble","type":"mood","character":"user:test.actor","enabled":true,"icon":"laugh1"}]}));
        if let Some(action) = action {
            let mood = app.project.stories["main"]["nodes"][1].clone();
            let mut actions =
                vec![json!({"type":"shock","character":"user:test.actor","duration":0.5})];
            let mut mood = mood;
            mood.as_object_mut().unwrap().remove("id");
            actions.insert(action, mood);
            app.project.stories.get_mut("main").unwrap()["nodes"][1] =
                json!({"id":"bubble","type":"parallel","actions":actions});
        }
        if second_mood {
            let story = app.project.stories.get_mut("main").unwrap();
            story["nodes"][0] = json!({"id":"show","type":"parallel","actions":[
                {"type":"show","character":"user:test.actor","position":"L1","facing":"left"},
                {"type":"show","character":"user:test.other","position":"R1","facing":"left"} ]});
            story["nodes"][1]["actions"][1] = json!({"type":"mood","character":"user:test.other","enabled":true,"icon":"shock","offset_x":77,"offset_y":-31});
        }
        let original = app.story()["nodes"][1].clone();
        let mood = |story: Value| -> Value {
            action
                .map(|index| story["nodes"][1]["actions"][index].clone())
                .unwrap_or_else(|| story["nodes"][1].clone())
        };
        app.project.assets.insert(
            "assets/user/character/test.actor/content.json".into(),
            serde_json::to_vec(
                &json!({"id":"test.actor","type":"character","portraits":{"normal":"normal.png"}}),
            )
            .unwrap(),
        );
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(80, 100, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        app.project.assets.insert(
            "assets/user/character/test.actor/normal.png".into(),
            png.into_inner(),
        );
        if second_mood {
            for name in ["content.json", "normal.png"] {
                let bytes =
                    app.project.assets[&format!("assets/user/character/test.actor/{name}")].clone();
                app.project
                    .assets
                    .insert(format!("assets/user/character/test.other/{name}"), bytes);
            }
        }
        app.selected = 1;
        app.view = View::Stage;
        app.center = Center::Node;
        let ctx = egui::Context::default();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1200.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.right(ctx);
                    app.center(ctx);
                },
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let center = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.stroke.color == Color32::from_rgb(45, 145, 220) => {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .expect("selected bubble has a drag handle");
        let pointer = |position, pressed| egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center), pointer(center, true)],
        );
        let target = center + Vec2::new(30.0, -20.0);
        draw(&mut app, vec![egui::Event::PointerMoved(target)]);
        draw(&mut app, vec![pointer(target, false)]);
        let offset = mood(app.story());
        assert!(offset["offset_x"].as_f64().unwrap() > 20.0, "{offset}");
        assert!(offset["offset_y"].as_f64().unwrap() > 20.0, "{offset}");
        draw(&mut app, vec![]);
        assert_eq!(mood(app.story()), offset);
        assert!(app.story()["nodes"][0].get("offset_x").is_none());
        if let Some(action) = action {
            assert!(app.story()["nodes"][1].get("offset_x").is_none());
            assert_eq!(
                app.story()["nodes"][1]["actions"][1 - action],
                original["actions"][1 - action]
            );
            let saved = tempfile::tempdir().unwrap();
            app.project.save_to(saved.path()).unwrap();
            let reopened = Project::open(saved.path()).unwrap();
            assert_eq!(mood(reopened.stories["main"].clone()), offset);
        }
        let output = draw(&mut app, vec![]);
        let center = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.stroke.color == Color32::from_rgb(45, 145, 220) => {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .unwrap();
        let target = center + Vec2::new(20.0, -10.0);
        draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(center),
                pointer(center, true),
                egui::Event::PointerMoved(target),
                pointer(target, false),
            ],
        );
        assert!(
            mood(app.story())["offset_x"].as_f64().unwrap() > offset["offset_x"].as_f64().unwrap()
        );
    }
    #[test]
    fn preview_drag_and_corner_resize_commit_one_undo_each() {
        let mut app = app();
        app.project.stories.insert("main".into(), json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"user:test.actor","position":"M","facing":"left","goto":"early"},
            {"id":"early","type":"say","text":"前面的对白保持原来位置。","mode":"narrative","goto":"line"},
            {"id":"line","type":"say","text":"摆好的人物会继续留在这句对白里。","mode":"narrative","goto":"end"},
            {"id":"end","type":"end"}]}));
        app.project.assets.insert("assets/user/character/test.actor/content.json".into(),
            serde_json::to_vec(&json!({"content_schema":1,"id":"test.actor","type":"character","name":"测试人物","portraits":{"normal":"normal.png"}})).unwrap());
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(80, 160, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        app.project.assets.insert(
            "assets/user/character/test.actor/normal.png".into(),
            png.into_inner(),
        );
        app.selected = 2;
        app.preview.layout_mode = true;
        app.view = View::Stage;
        app.center = Center::Node;
        app.last = snapshot(&app.project);
        let original = snapshot(&app.project);
        let earlier = preview::simulate(&app.story(), "early")["actors"].clone();
        let ctx = egui::Context::default();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1200.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.right(ctx);
                    app.center(ctx);
                },
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let bounds = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::Managed(0) => {
                    Some(mesh.calc_bounds())
                }
                _ => None,
            })
            .expect("actor portrait is drawn");
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        };
        let center = bounds.center();
        let moved = center + Vec2::new(30.0, -20.0);
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center), pointer(center, true)],
        );
        draw(&mut app, vec![egui::Event::PointerMoved(moved)]);
        assert_eq!(
            app.story()["nodes"].as_array().unwrap().len(),
            4,
            "live dragging is a draft"
        );
        draw(&mut app, vec![pointer(moved, false)]);
        let offset = app.story()["nodes"][3].clone();
        assert_eq!(offset["type"], "offset");
        assert_eq!(offset["absolute"], true);
        assert!(offset["x"].as_f64().unwrap() > 30.0);
        assert!(offset["y"].as_f64().unwrap() > 20.0);
        assert_eq!(app.story()["nodes"][app.selected]["id"], "offset1");
        assert_eq!(preview::simulate(&app.story(), "early")["actors"], earlier);
        assert_eq!(app.undo.len(), 1);
        app.undo();
        assert!(snapshot(&app.project) == original);
        app.redo();
        let moved_snapshot = snapshot(&app.project);
        app.selected = 3;
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let corner = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape)
                    if shape.stroke.color == Color32::from_rgb(25, 130, 220)
                        && shape.rect.width() < 20.0 =>
                {
                    Some(shape.rect.center())
                }
                _ => None,
            })
            .expect("selected portrait has resize handles");
        let larger = corner + Vec2::new(-24.0, -30.0);
        // Native input can deliver an entire short gesture in one frame.
        draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(corner),
                pointer(corner, true),
                egui::Event::PointerMoved(larger),
                pointer(larger, false),
            ],
        );
        let scaled = app.story()["nodes"][3].clone();
        assert_eq!(
            app.story()["nodes"].as_array().unwrap().len(),
            5,
            "a second gesture reuses the layout step"
        );
        assert!(scaled["scale"].as_f64().unwrap() > 100.0, "{scaled}");
        assert_eq!(scaled["x"], offset["x"]);
        assert_eq!(scaled["y"], offset["y"]);
        assert_eq!(app.undo.len(), 2);
        app.undo();
        assert!(snapshot(&app.project) == moved_snapshot);
        app.selected = 3;
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let center = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::Managed(0) => {
                    Some(mesh.calc_bounds().center())
                }
                _ => None,
            })
            .unwrap();
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center), pointer(center, true)],
        );
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center + Vec2::splat(20.0))],
        );
        draw(
            &mut app,
            vec![key_event(egui::Key::Escape, Default::default())],
        );
        draw(&mut app, vec![pointer(center + Vec2::splat(20.0), false)]);
        assert!(
            snapshot(&app.project) == moved_snapshot,
            "Esc discards the uncommitted gesture"
        );
        assert_eq!(app.undo.len(), 1);
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center), pointer(center, true)],
        );
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center + Vec2::splat(20.0))],
        );
        app.view = View::Graph;
        draw(&mut app, vec![pointer(center + Vec2::splat(20.0), false)]);
        app.view = View::Stage;
        draw(&mut app, vec![pointer(center + Vec2::splat(40.0), false)]);
        assert!(
            snapshot(&app.project) == moved_snapshot,
            "leaving the stage discards a gesture"
        );
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center), pointer(center, true)],
        );
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center + Vec2::splat(20.0))],
        );
        app.install(app.project.clone());
        app.selected = 3;
        draw(&mut app, vec![pointer(center + Vec2::splat(40.0), false)]);
        assert!(
            snapshot(&app.project) == moved_snapshot,
            "a new project cannot inherit a pending drag"
        );
        app.selected = 3;
        app.delete();
        app.track();
        let deleted = app.story();
        assert_eq!(deleted["nodes"][0]["goto"], "early");
        assert_eq!(deleted["nodes"][1]["goto"], "line");
        assert!(preview::route(&deleted, "line").is_some());
        assert!(preview::route(&deleted, "end").is_some());
        app.undo();
        assert!(snapshot(&app.project) == moved_snapshot);
        app.redo();
        assert_eq!(app.story(), deleted);
    }
    #[test]
    fn editor_update_uses_the_existing_unsaved_project_decision() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] =
            json!("unsaved update test");
        app.track();
        app.flush();
        app.request(Pending::EditorUpdate, &ctx);
        assert!(matches!(app.pending, Some(Pending::EditorUpdate)));
        assert!(!app.updater.open);
        app.pending = None; // Cancelling must retain the unsaved project.
        assert!(app.dirty());
        assert_eq!(app.story()["nodes"][0]["text"], "unsaved update test");
    }

    #[test]
    fn startup_update_preference_preserves_other_settings_and_the_project() {
        let mut app = app();
        let before = snapshot(&app.project);
        let directory = tempfile::tempdir().unwrap();
        let prefs = directory.path().join("preferences.json");
        std::fs::write(
            &prefs,
            r#"{"ui_locale":"chs","preview_library_dir":"keep previews","custom":"keep data"}"#,
        )
        .unwrap();
        app.set_startup_update_check(false, &prefs).unwrap();
        let value = lom_core::load_json(&prefs).unwrap();
        assert_eq!(value["check_updates_on_startup"], false);
        assert_eq!(value["ui_locale"], "chs");
        assert_eq!(value["preview_library_dir"], "keep previews");
        assert_eq!(value["custom"], "keep data");
        assert!(!app.check_updates_on_startup);
        assert!(snapshot(&app.project) == before);
    }
    fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn focused_text_input_keeps_project_undo_redo_and_delete_shortcuts() {
        let mut app = app();
        app.project.manifest["name"] = json!("keep this project edit");
        app.track();
        app.flush();
        let before = snapshot(&app.project);
        let history = (app.undo.len(), app.redo.len());
        let ctx = egui::Context::default();
        let id = egui::Id::new("typing-dialogue");
        let mut text = "已有台词".to_owned();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::multiline(&mut text).id(id))
                    .request_focus();
            });
        });
        for (key, modifiers) in [
            (egui::Key::Z, egui::Modifiers::COMMAND),
            (
                egui::Key::Z,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            ),
            (egui::Key::Y, egui::Modifiers::COMMAND),
            (egui::Key::Backspace, egui::Modifiers::NONE),
            (egui::Key::Delete, egui::Modifiers::NONE),
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    modifiers,
                    events: vec![key_event(key, modifiers)],
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    assert!(
                        ctx.input(|input| input.key_pressed(key)),
                        "the input must receive {key:?}"
                    );
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add(egui::TextEdit::multiline(&mut text).id(id));
                    });
                },
            );
            assert!(snapshot(&app.project) == before);
            assert_eq!((app.undo.len(), app.redo.len()), history);
        }
        ctx.memory_mut(|memory| memory.surrender_focus(id));
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(
            snapshot(&app.project) != before,
            "project undo still works outside text inputs"
        );
    }

    #[test]
    fn file_menu_groups_secondary_actions_without_changing_the_project() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = navigation_frame(&mut app, &ctx, vec![]);
        let event = click_named_option(&output, &tr("文件"));
        navigation_frame(&mut app, &ctx, vec![event]);
        let output = navigation_frame(&mut app, &ctx, vec![]);
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect();
        for key in [
            "editor.workspace.new_project",
            "editor.workspace.original_tools",
            "editor.workspace.export_menu",
            "editor.workspace.preview_assets",
        ] {
            assert!(
                labels
                    .iter()
                    .any(|label| label.starts_with(&crate::i18n::key(key))),
                "missing menu {key}: {labels:?}"
            );
        }
        assert!(!labels.contains(&crate::i18n::key("editor.workspace.import_original").as_str()));
        assert!(!labels.contains(&tr("从游戏提取预览素材…").as_str()));
        assert!(!labels.contains(&tr("导出诊断报告…").as_str()));
        let event = click_named_option(
            &output,
            &crate::i18n::key("editor.workspace.original_tools"),
        );
        navigation_frame(&mut app, &ctx, vec![event]);
        let output = navigation_frame(&mut app, &ctx, vec![]);
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect();
        for key in [
            "editor.workspace.import_original",
            "editor.workspace.routes",
            "editor.workspace.bookmark",
        ] {
            assert!(
                labels.contains(&crate::i18n::key(key).as_str()),
                "missing action {key}: {labels:?}"
            );
        }
        assert!(snapshot(&app.project) == before);
    }

    #[test]
    fn system_clipboard_events_copy_and_paste_steps_once() {
        let mut app = app();
        let original_count = app.story()["nodes"].as_array().unwrap().len();
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::Copy,
                    key_event(egui::Key::C, egui::Modifiers::COMMAND),
                ],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert_eq!(app.clipboard.len(), 1);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::Paste(String::new()),
                    key_event(egui::Key::V, egui::Modifiers::COMMAND),
                ],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert_eq!(
            app.story()["nodes"].as_array().unwrap().len(),
            original_count + 1
        );
    }
    fn navigation_frame(
        app: &mut App,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1200.0, 850.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                app.toolbar(ctx);
                app.center(ctx);
            },
        )
    }
    #[test]
    fn primary_tabs_are_direct_click_targets_and_preserve_story_selection() {
        let mut app = app();
        let before = snapshot(&app.project);
        let selection = (app.current.clone(), app.selected);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = navigation_frame(&mut app, &ctx, vec![]);
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        for (section, label) in [
            (Section::Materials, "素材管理"),
            (Section::Game, "游戏与 MOD"),
            (Section::Release, "发布"),
            (Section::Story, "剧情编辑"),
        ] {
            let target = nodes
                .iter()
                .find_map(|(id, node)| {
                    (node.label() == Some(tr(label).as_str())
                        && node.role() == egui::accesskit::Role::Button)
                        .then_some(*id)
                })
                .expect("task tab must be directly visible");
            navigation_frame(
                &mut app,
                &ctx,
                vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target,
                        data: None,
                    },
                )],
            );
            assert_eq!(app.section(), section, "{label}");
            assert_eq!(app.story_panels_visible(), section == Section::Story);
            assert_eq!((app.current.clone(), app.selected), selection);
            assert!(snapshot(&app.project) == before);
        }
        app.open_tool_page(ToolPage::Editing);
        app.navigate(Section::Story);
        assert_eq!(
            app.center,
            Center::Node,
            "story tab returns from editing tools"
        );
    }
    #[test]
    fn non_story_pages_do_not_accept_hidden_node_delete_or_paste() {
        let mut app = app();
        app.copy();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        for section in [Section::Materials, Section::Game, Section::Release] {
            app.navigate(section);
            navigation_frame(
                &mut app,
                &ctx,
                vec![
                    key_event(egui::Key::Delete, egui::Modifiers::NONE),
                    key_event(egui::Key::V, egui::Modifiers::COMMAND),
                ],
            );
            assert!(
                snapshot(&app.project) == before,
                "hidden story mutated from {section:?}"
            );
        }
    }
    #[test]
    fn navigation_keeps_invalid_material_input_until_the_user_resolves_it() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.import_assets(vec![image], "test.hero", "hero", 0)
            .unwrap();
        app.center = Center::Assets;
        let mut draft: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        draft["files"]["main"] = json!("../outside.png");
        app.content_panel = crate::content_panel::ContentPanel::pending_fixture(
            &app.project,
            &app.asset_selection,
            draft,
        );
        let before = snapshot(&app.project);
        for section in [Section::Game, Section::Release, Section::Story] {
            app.navigate(section);
            assert_eq!(app.center, Center::Assets);
            assert!(app.content_panel.has_pending());
            assert!(app.error.is_some());
            assert!(snapshot(&app.project) == before);
        }
        app.open_tool_page(ToolPage::Library);
        assert_eq!(app.center, Center::Assets);
        let ctx = egui::Context::default();
        for event in [
            key_event(
                egui::Key::F,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            ),
            key_event(egui::Key::F7, egui::Modifiers::NONE),
        ] {
            navigation_frame(&mut app, &ctx, vec![event]);
            assert_eq!(app.center, Center::Assets);
            assert!(app.content_panel.has_pending());
            assert!(snapshot(&app.project) == before);
        }
    }
    #[test]
    fn every_task_page_can_be_read_without_modifying_the_project() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        for page in [
            ToolPage::Editing,
            ToolPage::Previews,
            ToolPage::Library,
            ToolPage::Game,
            ToolPage::Release,
            ToolPage::Maintenance,
        ] {
            app.open_tool_page(page);
            navigation_frame(&mut app, &ctx, vec![]);
            assert!(snapshot(&app.project) == before, "{page:?}");
        }
    }
    #[test]
    fn visible_help_button_and_f1_open_the_guide_without_changing_the_project() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1200.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    app.toolbar(ctx);
                    app.tools_panel.show_dialog(ctx, &app.project);
                },
            )
        };
        let output = draw(&mut app, vec![]);
        let help = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(crate::i18n::key("menu.help_item").as_str()))
                .then_some(id)
            })
            .expect("Help must be directly visible on the toolbar");
        let output = draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: help,
                    data: None,
                },
            )],
        );
        assert!(app.tools_panel.dialog_open());
        let text = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(&crate::i18n::key("help.guide_tab")), "{text}");
        assert!(
            text.contains(&crate::i18n::key("help.reference_tab")),
            "{text}"
        );
        assert!(
            text.contains("F6"),
            "The guide must be open without another expansion: {text}"
        );
        draw(
            &mut app,
            vec![key_event(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        assert!(app.pending.is_none());
        draw(
            &mut app,
            vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(!app.tools_panel.dialog_open());
        draw(
            &mut app,
            vec![key_event(egui::Key::F1, egui::Modifiers::NONE)],
        );
        assert!(app.tools_panel.dialog_open());
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
    }
    #[test]
    fn node_description_button_opens_its_own_reference_without_editing() {
        let mut app = app();
        app.selected = 1;
        assert_eq!(app.story()["nodes"][1]["type"], "end");
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1200.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.center(ctx);
                    app.tools_panel.show_dialog(ctx, &app.project);
                },
            )
        };
        let output = draw(&mut app, vec![]);
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(tr("说明").as_str()))
                .then_some(id)
            })
            .expect("Description must be a clickable button, not just a hover label");
        let output = draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        assert!(app.tools_panel.dialog_open());
        let text = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, n)| n.label().or_else(|| n.value()).map(str::to_owned))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("结束后回到哪里"),
            "Opened the wrong article: {text}"
        );
        assert!(
            text.contains("下一章节"),
            "Missing end-node guidance: {text}"
        );
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
    }
    fn click_named_option(output: &egui::FullOutput, prefix: &str) -> egui::Event {
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let target = nodes
            .iter()
            .find_map(|(id, node)| {
                (node.label().is_some_and(|label| label.starts_with(prefix))
                    || node.value().is_some_and(|value| value.starts_with(prefix)))
                .then_some(*id)
            })
            .unwrap_or_else(|| panic!("Missing option {prefix:?}: {nodes:?}"));
        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target,
            data: None,
        })
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires installed game settings; run the test executable from the complete Windows bundle"]
    fn old_project_audio_failure_does_not_block_existing_or_new_nodes() {
        fn draw(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1600.0, 1200.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.center(ctx);
                    app.dialogs(ctx);
                },
            )
        }
        fn start(app: &mut App) {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let output = draw(app, &ctx, vec![]);
            let event = click_named_option(&output, &tr("试听音频"));
            draw(app, &ctx, vec![event]);
            assert!(app.error.is_none(), "{:?}", app.error);
            assert!(app.audio.playing());
        }
        fn prepare(app: &mut App) -> anyhow::Result<()> {
            let deadline = Instant::now() + Duration::from_secs(30);
            while app.audio.preparing() {
                app.audio.update()?;
                assert!(Instant::now() < deadline, "试听准备超时");
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }

        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("旧工程.json");
        // Older nodes omit the optional op/kind fields. Loading must preserve
        // those fields and a failed old reference must not poison later previews.
        let original = lom_core::stable_json(&json!({
            "story_schema":2,"id":"main","title":"旧工程","start":"sound1",
            "nodes":[
                {"id":"sound1","type":"sound","name":"Sword_Fight_001"},
                {"id":"music1","type":"music","name":"主題_001"},
                {"id":"sound2","type":"sound","name":"Cymbal_001"},
                {"id":"env1","type":"sound","kind":"env","name":"DrumRoll_001"},
                {"id":"end1","type":"end"}
            ]
        }))
        .unwrap();
        std::fs::write(&legacy, &original).unwrap();
        let package =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/全节点样例3.0.lommod");
        for path in [&legacy, &package] {
            let mut app = app();
            let game = app.tools_panel.game_directory();
            assert!(game.join("Mortal_Data").is_dir(), "游戏目录未配置");
            app.install(Project::open(path).unwrap());
            app.recovery_dir = temp.path().join("recovery");
            let existing: Vec<_> = app.story()["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|node| {
                    matches!(node["type"].as_str(), Some("music" | "sound"))
                        && node["name"] != "Sword_Fight_001"
                })
                .map(|node| node["id"].as_str().unwrap().to_owned())
                .collect();
            let before = snapshot(&app.project);
            app.add_node("sound");
            let selected = app.selected;
            app.project.stories.get_mut("main").unwrap()["nodes"][selected]["name"] =
                json!("Sword_Fight_001");
            start(&mut app);
            let error = prepare(&mut app).unwrap_err().to_string();
            assert!(error.contains("Sword_Fight_001"), "{error}");
            assert!(!error.contains('�'), "{error}");
            assert!(!app.audio.playing());
            app.error = Some(error);
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let output = draw(&mut app, &ctx, vec![]);
            let event = click_named_option(&output, &tr("关闭"));
            draw(&mut app, &ctx, vec![event]);
            assert!(app.error.is_none());
            for id in existing {
                app.selected = app.story()["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|node| node["id"] == id)
                    .unwrap();
                start(&mut app);
                prepare(&mut app).unwrap();
                app.audio.stop();
            }
            for (kind, name) in [("music", "主題_001"), ("sound", "Cymbal_001")] {
                app.add_node(kind);
                let selected = app.selected;
                app.project.stories.get_mut("main").unwrap()["nodes"][selected]["name"] =
                    json!(name);
                start(&mut app);
                prepare(&mut app).unwrap();
                app.audio.stop();
            }
            assert_eq!(app.tools_panel.game_directory(), game);
            assert_eq!(std::fs::read(&legacy).unwrap(), original);
            // Playback must leave all existing nodes and assets intact.
            let after = snapshot(&app.project);
            assert_eq!(before.assets, after.assets);
            for node in before.stories["main"]["nodes"].as_array().unwrap() {
                assert!(after.stories["main"]["nodes"]
                    .as_array()
                    .unwrap()
                    .contains(node));
            }
        }
    }
    #[test]
    fn duplicate_chapter_names_select_distinct_ids_without_renaming_stories() {
        let mut app = app();
        app.project.stories.get_mut("main").unwrap()["title"] = json!("同名章节");
        let mut second = app.project.stories["main"].clone();
        second["id"] = json!("second");
        app.project.stories.insert("second".into(), second);
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut events = vec![];
        for option in ["同名章节 1", "同名章节 2"] {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1280.0, 900.0),
                    )),
                    events: std::mem::take(&mut events),
                    ..Default::default()
                },
                |ctx| app.sidebar(ctx),
            );
            events.push(click_named_option(&output, option));
        }
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.sidebar(ctx),
        );
        assert_eq!(app.current, "second");
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn duplicate_content_names_select_distinct_assets_without_renaming_metadata() {
        let mut app = app();
        for id in ["demo.a", "demo.b"] {
            app.project.assets.insert(
                format!("assets/user/image/{id}/content.json"),
                serde_json::to_vec(
                    &json!({"id":id,"type":"image","name":"同名素材","files":{"main":"main.png"}}),
                )
                .unwrap(),
            );
        }
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(700.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.assets(ui));
            },
        );
        let _ = click_named_option(&output, "同名素材 1 ");
        let event = click_named_option(&output, "同名素材 2 ");
        let _ = ctx.run(
            egui::RawInput {
                events: vec![event],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.assets(ui));
            },
        );
        assert_eq!(app.asset_selection, "assets/user/image/demo.b/content.json");
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn settings_shortcut_and_language_preferences_do_not_modify_the_project() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::COMMAND,
                events: vec![key_event(egui::Key::Comma, egui::Modifiers::COMMAND)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(app.settings_open);
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::COMMAND,
                events: vec![key_event(egui::Key::N, egui::Modifiers::COMMAND)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(snapshot(&app.project) == before);
        let dir = tempfile::tempdir().unwrap();
        let prefs = dir.path().join("preferences.json");
        std::fs::write(&prefs, br#"{"preview_library":"keep-me"}"#).unwrap();
        let locale = crate::i18n::locale();
        app.set_interface_locale(locale, &prefs).unwrap();
        let saved = lom_core::load_json(&prefs).unwrap();
        assert_eq!(saved["ui_locale"], locale);
        assert_eq!(saved["preview_library"], "keep-me");
        assert!(app.set_interface_locale("unsupported", &prefs).is_err());
        assert_eq!(lom_core::load_json(&prefs).unwrap(), saved);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        assert!(!app.settings_open);
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
    }
    #[test]
    fn compiler_diagnostic_link_selects_the_error_node_across_chapters() {
        let mut app = app();
        let mut second = app.story();
        second["id"] = json!("second");
        app.project.stories.insert("second".into(), second);
        app.project.stories.insert("main".into(), json!({
            "story_schema": 2, "id": "main", "start": "first",
            "nodes": [
                {"id":"first", "type":"message", "text":"第一步"},
                {"id":"next", "type":"message", "text":"第二步"},
                {"id":"say_left", "type":"say", "mode":"narrative", "text":"错误在第三步", "goto":"missing"},
                {"id":"end", "type":"end"}
            ]
        }));
        app.current = "second".into();
        app.selected = 0;
        app.multiselect.insert(0);
        app.search = "隐藏目标的旧过滤".into();
        app.compile();
        assert_eq!(app.diagnostics.len(), 1);
        assert_eq!(app.diagnostics[0].story_id, "main");
        assert_eq!(app.diagnostics[0].node_id, "say_left");
        assert!(app.diagnostics[0].message.contains("missing"));
        let before = snapshot(&app.project);
        app.view = View::Checks;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1280.0, 900.0),
            )),
            ..Default::default()
        };
        let output = ctx.run(raw.clone(), |ctx| app.right(ctx));
        let _ = ctx.run(
            egui::RawInput {
                events: vec![click_named_option(&output, "main/say_left")],
                ..raw
            },
            |ctx| app.right(ctx),
        );
        assert_eq!(app.current, "main");
        assert_eq!(app.selected, 2);
        assert!(app.center == Center::Node);
        assert!(app.multiselect.is_empty());
        assert!(app.search.is_empty());
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn compiler_diagnostic_does_not_guess_nodes_for_chapter_errors() {
        let mut app = app();
        app.project.stories.get_mut("main").unwrap()["start"] = json!("missing");
        app.compile();
        assert_eq!(app.diagnostics.len(), 1);
        assert_eq!(app.diagnostics[0].story_id, "main");
        assert!(app.diagnostics[0].node_id.is_empty());
        assert!(app.diagnostics[0].message.contains("missing"));
        let preflight = lom_core::release::run_preflight(
            &app.project,
            lom_core::release::Profile::Editing,
            "1.1.3",
        );
        assert!(preflight.contains(&app.diagnostics[0]));
    }
    #[test]
    #[cfg(windows)]
    fn windows_settings_shortcut_accepts_observed_release_without_keydown() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                // The real Windows event retained Ctrl even though the frame's
                // final modifier state was already NONE.
                modifiers: egui::Modifiers::NONE,
                events: vec![egui::Event::Key {
                    key: egui::Key::Comma,
                    physical_key: Some(egui::Key::Comma),
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(app.settings_open);
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    #[cfg(windows)]
    fn windows_settings_shortcut_does_not_repeat_normal_keydown_or_plain_comma() {
        let mut app = app();
        let ctx = egui::Context::default();
        let command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
        for initial_modifiers in [command, egui::Modifiers::NONE] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![key_event(egui::Key::Comma, initial_modifiers)],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            assert_eq!(app.settings_open, initial_modifiers.command);
            // Closing before key-up must not reopen settings. A plain comma
            // followed by Ctrl must not become a shortcut either.
            app.settings_open = false;
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Comma,
                        physical_key: Some(egui::Key::Comma),
                        pressed: false,
                        repeat: false,
                        modifiers: command,
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            assert!(!app.settings_open);
        }
    }
    #[test]
    #[cfg(windows)]
    fn windows_settings_release_respects_modal_focus_and_exact_modifiers() {
        let mut app = app();
        let ctx = egui::Context::default();
        let command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
        let before = snapshot(&app.project);
        for (modal, focused, modifiers) in [
            (true, true, command),
            (false, false, command),
            (false, true, egui::Modifiers::NONE),
            (false, true, command | egui::Modifiers::ALT),
            (false, true, command | egui::Modifiers::SHIFT),
        ] {
            if modal {
                app.begin_chapter(false);
            }
            let _ = ctx.run(
                egui::RawInput {
                    focused,
                    events: vec![egui::Event::Key {
                        key: egui::Key::Comma,
                        physical_key: Some(egui::Key::Comma),
                        pressed: false,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            assert!(!app.settings_open);
            assert!(snapshot(&app.project) == before);
            app.chapter_dialog = None;
        }
    }
    #[test]
    fn modal_blocks_background_clicks_and_project_shortcuts() {
        let mut app = app();
        app.project.manifest["name"] = json!("pending title");
        app.track();
        app.flush();
        app.begin_chapter(false);
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        let mut button_rect = egui::Rect::NOTHING;
        let mut clicked = false;
        let mut draw = |ctx: &egui::Context| {
            app.shortcuts(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = ui.button("background action");
                button_rect = response.rect;
                clicked |= response.clicked();
            });
            app.dialogs(ctx);
        };
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(900.0, 800.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(raw.clone(), &mut draw);
        let pos = egui::pos2(20.0, 20.0);
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..raw.clone()
                },
                &mut draw,
            );
        }
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::COMMAND,
                events: vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
                ..raw
            },
            &mut draw,
        );
        assert!(button_rect.contains(pos));
        assert!(!clicked);
        assert!(app.chapter_dialog.is_some());
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn tool_result_modal_blocks_edit_shortcuts_and_preserves_project() {
        let mut app = app();
        app.project.manifest["name"] = json!("pending title");
        app.track();
        app.flush();
        let before = snapshot(&app.project);
        app.tools_panel
            .present("安装检查", json!({"healthy":true,"game_tested":false}));
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::COMMAND,
                events: vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                app.dialogs(ctx);
                app.tools_panel.show_dialog(ctx, &app.project);
            },
        );
        assert!(app.tools_panel.dialog_open());
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn ime_confirmation_keeps_dialog_open_then_return_creates_the_chapter() {
        let mut app = app();
        app.begin_chapter(false);
        app.chapter_dialog.as_mut().unwrap().name.clear();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.dialogs(ctx));
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::Ime(egui::ImeEvent::Enabled),
                    egui::Event::Ime(egui::ImeEvent::Preedit("chun".into())),
                ],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::Ime(egui::ImeEvent::Commit("春日".into())),
                    key_event(egui::Key::Enter, egui::Modifiers::NONE),
                ],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        assert_eq!(app.project.stories.len(), 1);
        assert_eq!(app.chapter_dialog.as_ref().unwrap().name, "春日");
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        assert!(app.chapter_dialog.is_none());
        assert_eq!(app.story()["title"], "春日");
        app.begin_chapter(false);
        app.chapter_dialog.as_mut().unwrap().name = "Next chapter".into();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Enabled)],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        assert!(app.chapter_dialog.is_none());
        assert_eq!(app.story()["title"], "Next chapter");
    }
    #[test]
    fn chapter_controls_are_buttons_and_deletion_can_be_cancelled() {
        let mut app = app();
        app.begin_chapter(false);
        app.commit_chapter().unwrap();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1200.0, 850.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    app.sidebar(ctx);
                    app.center(ctx);
                    app.dialogs(ctx);
                },
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let settings = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(tr("章节设置").as_str()))
                .then_some(id)
            })
            .expect("chapter settings is a visible button");
        draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: settings,
                    data: None,
                },
            )],
        );
        assert_eq!(app.center, Center::Chapter);
        let output = draw(&mut app, vec![]);
        let delete = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(tr("删除章节…").as_str()))
                .then_some(id)
            })
            .expect("delete is visible beside the chapter settings heading");
        draw(
            &mut app,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: delete,
                    data: None,
                },
            )],
        );
        assert!(app.delete_chapter_dialog.is_some());
        draw(
            &mut app,
            vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert!(snapshot(&app.project) == before, "the modal blocks undo");
        draw(
            &mut app,
            vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(app.delete_chapter_dialog.is_none());
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn chapter_deletion_retargets_references_undoes_and_stays_deleted_after_save() {
        let mut app = app();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for id in ["keep", "other"] {
            let mut story = app.story();
            story["id"] = json!(id);
            story["title"] = json!(id);
            app.project.stories.insert(id.into(), story);
        }
        let other = app.project.stories.get_mut("other").unwrap();
        other["nodes"][1]["next_script"] = json!("main");
        other["_editor"] = json!({"tests":[{"story":"main"}]});
        app.project
            .paths
            .insert("main".into(), "owned-opening.json".into());
        app.project.save_to(root).unwrap();
        let before = snapshot(&app.project);
        app.last = before.clone();
        app.saved = before.clone();
        assert_eq!(app.chapter_reference_count("main"), 4);
        app.begin_delete_chapter();
        app.delete_chapter_dialog.as_mut().unwrap().replacement = "main".into();
        assert!(app.commit_delete_chapter().is_err());
        assert!(snapshot(&app.project) == before);
        app.delete_chapter_dialog.as_mut().unwrap().replacement = "keep".into();
        app.commit_delete_chapter().unwrap();
        assert_eq!(app.current, "keep");
        assert!(!app.project.stories.contains_key("main"));
        assert!(!app.project.paths.contains_key("main"));
        assert_eq!(app.project.manifest["entry"], "keep");
        assert_eq!(
            app.project.scenario.as_ref().unwrap()["events"][0]["script"],
            "keep"
        );
        assert_eq!(
            app.project.stories["other"]["nodes"][1]["next_script"],
            "keep"
        );
        assert_eq!(
            app.project.stories["other"]["_editor"]["tests"][0]["story"],
            "keep"
        );
        lom_core::scenario::validate(
            app.project.scenario.as_ref().unwrap(),
            &app.project.manifest,
            &app.project.stories,
        )
        .unwrap();
        assert!(
            root.join("story/owned-opening.json").exists(),
            "deletion waits for save"
        );
        assert!(app.dirty());
        assert_eq!(app.undo.len(), 1);
        let deleted = snapshot(&app.project);
        app.undo();
        assert!(snapshot(&app.project) == before);
        app.redo();
        assert!(snapshot(&app.project) == deleted);
        crate::persistence::save_project(
            &mut app.project,
            root,
            &before.assets,
            &before.stories,
            &before.paths,
        )
        .unwrap();
        let reopened = Project::open(root).unwrap();
        assert!(!reopened.stories.contains_key("main"));
        assert_eq!(reopened.stories.len(), 2);
        assert_eq!(reopened.manifest["entry"], "keep");
        assert!(!root.join("story/owned-opening.json").exists());
        assert_eq!(reopened.assets, *before.assets);
    }
    #[test]
    fn chapter_deletion_keeps_the_last_chapter() {
        let mut app = app();
        let before = snapshot(&app.project);
        app.begin_delete_chapter();
        assert!(app.delete_chapter_dialog.is_none());
        assert!(app.error.as_deref().unwrap().contains("至少"));
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
    }
    #[test]
    fn chapter_creation_uses_unique_identity_and_preserves_owned_filenames() {
        let mut app = app();
        app.project
            .paths
            .insert("main".into(), "CHAPTER1.json".into());
        let before = snapshot(&app.project);
        app.last = before.clone();
        app.saved = before.clone();
        app.begin_chapter(false);
        app.chapter_dialog.as_mut().unwrap().name = "  春日初遇  ".into();
        app.commit_chapter().unwrap();
        assert_eq!(app.current, "chapter2");
        assert_eq!(app.story()["title"], "春日初遇");
        lom_core::validate::validate_story(&app.story()).unwrap();
        assert_eq!(app.project.paths["main"], PathBuf::from("CHAPTER1.json"));
        assert!(!app.project.paths.contains_key("chapter2"));
        assert!(app.chapter_dialog.is_none());
        assert!(app.dirty());
        app.undo();
        assert!(snapshot(&app.project) == before);
        app.redo();
        assert_eq!(app.project.stories["chapter2"]["title"], "春日初遇");
    }
    #[test]
    fn copied_chapter_preserves_all_content_and_does_not_redirect_explicit_links() {
        let mut app = app();
        let mut source = app.story();
        source["nodes"][1]["next_script"] = json!("main");
        source["localization"] = json!({"translations":{"cht":{"say1.text":"春日"}}});
        source["_editor"] = json!({"tests":[{"story":"main","name":"本章测试"}],"sections":[{"id":"a","start":"say1","end":"end1"}]});
        app.project.stories.insert("main".into(), source.clone());
        let mut occupied = source.clone();
        occupied["id"] = json!("chapter1");
        app.project.stories.insert("chapter1".into(), occupied);
        app.begin_chapter(true);
        app.chapter_dialog.as_mut().unwrap().name = "另一段故事".into();
        app.commit_chapter().unwrap();
        assert_eq!(app.current, "chapter2");
        let mut expected = source.clone();
        expected["id"] = json!("chapter2");
        expected["title"] = json!("另一段故事");
        assert_eq!(app.story(), expected);
        assert_eq!(app.project.stories["main"], source);
        assert_eq!(app.project.manifest["entry"], "main");
        app.begin_chapter(true);
        app.commit_chapter().unwrap();
        assert_eq!(app.current, "chapter3");
    }
    #[test]
    fn chapter_name_input_survives_frames_and_cancel_or_invalid_name_never_changes_project() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        app.begin_chapter(false);
        app.chapter_dialog.as_mut().unwrap().name.clear();
        assert!(app.commit_chapter().is_err());
        assert!(app.chapter_dialog.is_some());
        assert!(snapshot(&app.project) == before);
        for events in [vec![], vec![egui::Event::Text("春日".into())], vec![]] {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| app.dialogs(ctx),
            );
        }
        assert_eq!(app.chapter_dialog.as_ref().unwrap().name, "春日");
        assert!(snapshot(&app.project) == before);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        assert!(app.chapter_dialog.is_none());
        assert!(snapshot(&app.project) == before);
        assert!(!app.dirty());
    }
    #[test]
    fn advanced_node_rename_preserves_links_and_localizations() {
        let mut app = app();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["goto"] = json!("end1");
        app.project.stories.get_mut("main").unwrap()["localization"] =
            json!({"translations":{"cht":{"say1.text":"春日"}}});
        app.rename_dialog = Some(RenameDialog {
            target: RenameTarget::Node {
                story: "main".into(),
                node: "say1".into(),
            },
            value: "greeting".into(),
            error: None,
            focus_name: false,
        });
        app.commit_rename().unwrap();
        assert_eq!(app.story()["start"], "greeting");
        assert_eq!(app.story()["nodes"][0]["id"], "greeting");
        assert_eq!(app.story()["nodes"][0]["goto"], "end1");
        assert_eq!(
            app.story()["localization"]["translations"]["cht"]["greeting.text"],
            "春日"
        );
        assert!(app.story()["localization"]["translations"]["cht"]
            .get("say1.text")
            .is_none());
    }
    #[test]
    fn advanced_chapter_rename_keeps_owned_filename_and_repairs_entry_and_links() {
        let mut app = app();
        app.project
            .paths
            .insert("main".into(), "original.json".into());
        app.project.stories.get_mut("main").unwrap()["nodes"][1]["next_script"] = json!("main");
        app.project.stories.get_mut("main").unwrap()["_editor"] =
            json!({"tests":[{"story":"main"}]});
        let mut other = app.story();
        other["id"] = json!("other");
        app.project.stories.insert("other".into(), other);
        let before = snapshot(&app.project);
        app.last = before.clone();
        app.saved = before.clone();
        app.rename_dialog = Some(RenameDialog {
            target: RenameTarget::Chapter("main".into()),
            value: "other".into(),
            error: None,
            focus_name: false,
        });
        assert!(app.commit_rename().is_err());
        assert!(snapshot(&app.project) == before);
        app.rename_dialog.as_mut().unwrap().value = "opening".into();
        app.commit_rename().unwrap();
        assert_eq!(app.current, "opening");
        assert_eq!(app.project.paths["opening"], PathBuf::from("original.json"));
        assert_eq!(app.project.manifest["entry"], "opening");
        assert_eq!(app.story()["nodes"][1]["next_script"], "opening");
        assert_eq!(
            app.project.stories["other"]["nodes"][1]["next_script"],
            "opening"
        );
        assert_eq!(app.story()["_editor"]["tests"][0]["story"], "opening");
        app.undo();
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn repeated_recovery_updates_only_owned_files_without_polluting_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app();
        app.recovery_dir = dir.path().join("owned-recovery");
        app.project.source = Some(dir.path().join("original-project"));
        app.project
            .paths
            .insert("main".into(), "authored-name.json".into());
        app.project.story_subdir = false;
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("first");
        app.track();
        let original_source = app.project.source.clone();
        let original_paths = app.project.paths.clone();
        for text in ["first", "second"] {
            app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!(text);
            app.track();
            app.last_recovery = Instant::now() - Duration::from_secs(31);
            app.autosave();
            let restored = Project::open(&app.recovery_dir).unwrap();
            assert_eq!(restored.stories["main"]["nodes"][0]["text"], text);
            assert_eq!(app.project.source, original_source);
            assert_eq!(app.project.paths, original_paths);
            assert!(!app.project.story_subdir);
            assert_eq!(app.recovery_paths["main"], PathBuf::from("main.json"));
            let session = lom_core::load_json(app.recovery_dir.join("session.json")).unwrap();
            assert_eq!(session["paths"]["main"], "authored-name.json");
            assert_eq!(session["story_subdir"], false);
        }
        app.begin_chapter(false);
        app.commit_chapter().unwrap();
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        assert!(app.recovery_dir.join("story/chapter1.json").exists());
        app.undo();
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        assert!(!app.recovery_dir.join("story/chapter1.json").exists());
        assert_eq!(Project::open(&app.recovery_dir).unwrap().stories.len(), 1);
        app.clear_recovery();
        assert!(app.recovery_paths.is_empty());
        assert!(!app.recovery_dir.exists());
    }
    #[test]
    fn recovery_never_claims_a_foreign_collision_as_owned() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app();
        app.recovery_dir = dir.path().join("foreign-recovery");
        std::fs::create_dir_all(app.recovery_dir.join("story")).unwrap();
        let collision = app.recovery_dir.join("story/main.json");
        std::fs::write(&collision, b"unowned data").unwrap();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("unsaved");
        app.track();
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        assert_eq!(std::fs::read(&collision).unwrap(), b"unowned data");
        assert!(app.recovery_paths.is_empty());
        assert!(app.status.starts_with("自动恢复副本写入失败"));
        assert!(app.project.source.is_none());
        assert!(app.project.paths.is_empty());
    }
    #[test]
    fn recovery_removes_deleted_owned_assets_without_removing_unknown_files() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.recovery_dir = dir.path().join("asset-recovery");
        app.import_assets(vec![image], "test.hero", "hero", 0)
            .unwrap();
        app.track();
        let owned: BTreeSet<_> = app.project.assets.keys().cloned().collect();
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        assert_eq!(app.recovery_assets, owned);
        let unknown = app.recovery_dir.join("assets/unknown.bin");
        std::fs::write(&unknown, b"unowned data").unwrap();
        lom_core::content_edit::remove(&mut app.project, &app.asset_selection).unwrap();
        app.track();
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        let restored = Project::open(&app.recovery_dir).unwrap();
        for name in owned {
            assert!(!restored.assets.contains_key(&name));
            assert!(!app.recovery_dir.join(name).exists());
        }
        assert_eq!(std::fs::read(&unknown).unwrap(), b"unowned data");
        assert!(app.recovery_assets.is_empty());
        app.clear_recovery();
        assert!(app.recovery_assets.is_empty());
    }
    #[test]
    fn recovery_includes_valid_pending_content_without_changing_live_input() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.recovery_dir = dir.path().join("draft-recovery");
        app.import_assets(vec![image], "test.hero", "hero", 0)
            .unwrap();
        app.track();
        let before = snapshot(&app.project);
        let mut draft: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        draft["name"] = json!("new title ");
        app.content_panel = crate::content_panel::ContentPanel::pending_fixture(
            &app.project,
            &app.asset_selection,
            draft,
        );
        app.last_recovery = Instant::now() - Duration::from_secs(31);
        app.autosave();
        let restored = Project::open(&app.recovery_dir).unwrap();
        let meta: Value = serde_json::from_slice(&restored.assets[&app.asset_selection]).unwrap();
        assert_eq!(meta["name"], "new title");
        assert!(snapshot(&app.project) == before);
        assert!(app.content_panel.has_pending());
    }
    #[test]
    fn undo_and_redo_keep_invalid_content_drafts_and_existing_history() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.import_assets(vec![image], "test.hero", "hero", 0)
            .unwrap();
        app.track();
        app.flush();
        app.project.manifest["name"] = json!("new title");
        app.track();
        app.flush();
        app.undo();
        assert!(!app.undo.is_empty());
        assert!(!app.redo.is_empty());
        let mut draft: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        draft["files"]["main"] = json!("../outside.png");
        app.content_panel = crate::content_panel::ContentPanel::pending_fixture(
            &app.project,
            &app.asset_selection,
            draft,
        );
        let project = snapshot(&app.project);
        let history = (app.undo.len(), app.redo.len());
        app.undo();
        assert!(snapshot(&app.project) == project);
        assert!(app.content_panel.has_pending());
        assert!(app.error.is_some());
        app.redo();
        assert!(snapshot(&app.project) == project);
        assert!(app.content_panel.has_pending());
        assert_eq!((app.undo.len(), app.redo.len()), history);
    }
    #[test]
    fn character_import_accepts_timestamp_filename_and_independent_display_name() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("26_04_23_17_53_52.png");
        image::RgbaImage::new(1, 1).save(&image).unwrap();
        for name in ["normal", "我的角色"] {
            let mut app = app();
            app.asset_import_dialog = Some(AssetImportDialog {
                extends: String::new(),
                kind: 1,
                name: name.into(),
                paths: vec![image.clone()],
                error: None,
            });
            app.commit_asset_import().unwrap();
            let meta: Value =
                serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
            assert_eq!(meta["name"], name);
            assert_eq!(meta["portraits"], json!({"normal":"26_04_23_17_53_52.png"}));
            assert_eq!(meta["files"]["main"], "26_04_23_17_53_52.png");
            assert_eq!(app.project.assets.len(), 2);
        }
    }
    #[test]
    fn official_character_import_lists_new_expressions_under_the_original_identity() {
        let directory = tempfile::tempdir().unwrap();
        let paths: Vec<_> = ["tired.png", "sleep.png"]
            .into_iter()
            .map(|name| {
                let path = directory.path().join(name);
                image::RgbaImage::new(2, 3).save(&path).unwrap();
                path
            })
            .collect();
        let mut app = app();
        app.import_assets_with_target(paths, "demo.xia", "Extra expressions", 1, "special825")
            .unwrap();
        let meta: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        assert_eq!(meta["extends"], "special825");
        assert_eq!(
            meta["portraits"],
            json!({"tired":"tired.png", "sleep":"sleep.png"})
        );
        app.catalog.sync_assets(&app.project.assets);
        let options = app
            .catalog
            .options("portrait", &json!({"character":"special825"}), &[], &[]);
        for name in ["tired", "sleep", "normal"] {
            assert!(options.iter().any(|(id, _)| id == name));
        }
        assert!(!app.catalog.data["characters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "user:demo.xia"));
        app.project.stories.get_mut("main").unwrap()["nodes"] = json!([
            {"id":"say1","type":"show","character":"special825","position":"M","portrait":"tired"},
            {"id":"end1","type":"end"}]);
        let lua = compile_with_assets(&app.project, &app.project.stories["main"]).unwrap();
        assert!(lua.contains("characters.LoadCharacterPortrait(\"special825\", \"tired\")"));
    }
    #[test]
    fn character_import_appends_selections_and_preserves_same_named_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = [
            ("a/normal.png", b"first".as_slice()),
            ("b/normal.png", b"second".as_slice()),
            ("c/normal_1.png", b"reserved".as_slice()),
            ("d/NORMAL.PNG", b"fourth".as_slice()),
        ]
        .into_iter()
        .map(|(name, bytes)| {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            path
        })
        .collect();
        let mut draft = AssetImportDialog {
            extends: String::new(),
            kind: 1,
            name: String::new(),
            paths: vec![],
            error: None,
        };
        draft.select_files(vec![paths[0].clone()]);
        draft.name = "自定人物".into();
        draft.select_files(vec![paths[0].clone(), paths[1].clone()]);
        draft.select_files(paths[2..].to_vec());
        assert_eq!(draft.paths, paths);
        assert_eq!(draft.name, "自定人物");
        let mut app = app();
        app.asset_import_dialog = Some(draft);
        app.commit_asset_import().unwrap();
        let meta: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        let prefix = app.asset_selection.strip_suffix("content.json").unwrap();
        assert_eq!(meta["files"]["main"], "normal.png");
        assert_eq!(meta["portraits"]["normal"], "normal.png");
        assert_eq!(meta["portraits"]["portrait_1"], "normal_2.png");
        assert_eq!(meta["portraits"]["normal_1"], "normal_1.png");
        assert_eq!(meta["portraits"]["NORMAL"], "NORMAL_3.PNG");
        for (name, bytes) in [
            ("normal.png", b"first".as_slice()),
            ("normal_2.png", b"second".as_slice()),
            ("normal_1.png", b"reserved".as_slice()),
            ("NORMAL_3.PNG", b"fourth".as_slice()),
        ] {
            assert_eq!(app.project.assets[&format!("{prefix}{name}")], bytes);
        }
    }
    #[test]
    fn character_import_keeps_every_image_and_never_overwrites_normal_or_named_portraits() {
        let dir = tempfile::tempdir().unwrap();
        let names = [
            "26_04_23_17_53_52.png",
            "中文.png",
            "portrait_1.png",
            "happy.png",
            "happy.jpg",
            "normal.png",
            "abcdefghijklmnopqrstuvwxyzabcdefg.png",
            "face-smile.png",
        ];
        let paths = names
            .iter()
            .map(|name| {
                let path = dir.path().join(name);
                std::fs::write(&path, b"image fixture").unwrap();
                path
            })
            .collect();
        let mut app = app();
        app.import_assets(paths, "test.hero", "角色", 1).unwrap();
        let meta: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        assert_eq!(meta["portraits"]["normal"], names[0]);
        assert_eq!(meta["portraits"]["portrait_1"], "portrait_1.png");
        assert_eq!(meta["portraits"]["happy"], "happy.png");
        let portraits = meta["portraits"].as_object().unwrap();
        assert_eq!(portraits.len(), names.len());
        for name in names {
            assert!(
                portraits.values().any(|value| value == name),
                "missing {name}"
            );
        }
        lom_core::content::normalize_content_metadata(&meta).unwrap();
    }
    #[test]
    fn import_confirmation_generates_unique_identity_and_keeps_display_name() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("春日.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.asset_import_dialog = Some(AssetImportDialog {
            extends: String::new(),
            kind: 0,
            name: "".into(),
            paths: vec![image.clone()],
            error: None,
        });
        let before = snapshot(&app.project);
        assert!(app.commit_asset_import().is_err());
        assert!(snapshot(&app.project) == before);
        app.asset_import_dialog.as_mut().unwrap().name = "春日背景".into();
        app.commit_asset_import().unwrap();
        let first = app.project.assets.clone();
        let meta: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        assert_eq!(meta["id"], "my_mod.image1");
        assert_eq!(meta["name"], "春日背景");
        app.asset_import_dialog = Some(AssetImportDialog {
            extends: String::new(),
            kind: 0,
            name: "另一张背景".into(),
            paths: vec![image],
            error: None,
        });
        app.commit_asset_import().unwrap();
        let meta: Value =
            serde_json::from_slice(&app.project.assets[&app.asset_selection]).unwrap();
        assert_eq!(meta["id"], "my_mod.image2");
        for (key, bytes) in first {
            assert_eq!(app.project.assets[&key], bytes);
        }
        let committed = snapshot(&app.project);
        app.asset_import_dialog = Some(AssetImportDialog {
            extends: String::new(),
            kind: 1,
            name: "取消".into(),
            paths: vec![],
            error: None,
        });
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.dialogs(ctx),
        );
        assert!(app.asset_import_dialog.is_none());
        assert!(snapshot(&app.project) == committed);
    }
    #[test]
    fn command_shift_z_redoes_instead_of_consuming_undo() {
        let mut app = app();
        let original = app.project.manifest["name"].clone();
        app.project.manifest["name"] = json!("edited");
        app.track();
        app.flush();
        app.undo();
        assert_eq!(app.project.manifest["name"], original);
        let ctx = egui::Context::default();
        let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let _ = ctx.run(
            egui::RawInput {
                modifiers,
                events: vec![egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert_eq!(app.project.manifest["name"], "edited");
        assert!(app.redo.is_empty());
    }
    #[test]
    fn gameplay_content_imports_defaults_and_exposes_typed_selectors() {
        let dir = tempfile::tempdir().unwrap();
        let icon = dir.path().join("icon.png");
        std::fs::write(&icon, b"sample icon").unwrap();
        let mut app = app();
        for (kind, index, id) in [
            ("item", 5, "demo.token"),
            ("martial", 6, "demo.manual"),
            ("support", 7, "demo.aid"),
        ] {
            app.import_assets(vec![icon.clone()], id, "测试内容", index)
                .unwrap();
            let key = format!("assets/user/{kind}/{id}/content.json");
            let meta: Value = serde_json::from_slice(&app.project.assets[&key]).unwrap();
            assert_eq!(meta["type"], kind);
            lom_core::content::normalize_content_metadata(&meta).unwrap();
        }
        app.catalog.sync_assets(&app.project.assets);
        assert!(app
            .catalog
            .options("item", &json!({"kind":"misc"}), &[], &[])
            .iter()
            .any(|(id, _)| id == "user:demo.token"));
        assert!(app
            .catalog
            .options("item", &json!({"kind":"book"}), &[], &[])
            .iter()
            .any(|(id, _)| id == "user:demo.manual"));
        assert!(app
            .catalog
            .options("support", &Value::Null, &[], &[])
            .iter()
            .any(|(id, _)| id == "user:demo.aid"));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(&icon, bytes.into_inner()).unwrap();
        app.import_assets(vec![icon.clone(), icon.clone()], "demo.strike", "试招", 8)
            .unwrap();
        app.import_assets(vec![icon], "demo.tonic", "丹药", 9)
            .unwrap();
        app.catalog.sync_assets(&app.project.assets);
        let skill: Value = serde_json::from_slice(
            &app.project.assets["assets/user/skill/demo.strike/content.json"],
        )
        .unwrap();
        assert_eq!(skill["animation"]["frames"].as_array().unwrap().len(), 2);
        assert_ne!(
            skill["animation"]["frames"][0],
            skill["animation"]["frames"][1]
        );
        assert!(app
            .catalog
            .options("skill", &Value::Null, &[], &[])
            .iter()
            .any(|(id, _)| id == "user:demo.strike"));
        assert!(app
            .catalog
            .options("field_consumable", &Value::Null, &[], &[])
            .iter()
            .any(|(id, _)| id == "user:demo.tonic"));
    }
    #[test]
    fn asset_import_is_atomic_and_undo_redo_restores_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        assert!(app
            .import_assets(vec![image.clone()], "Invalid-ID", "hero", 0)
            .is_err());
        assert!(app.project.assets.is_empty());
        app.import_assets(vec![image], "my_mod.hero", "hero", 0)
            .unwrap();
        app.track();
        app.flush();
        let imported = app.project.assets.clone();
        assert_eq!(imported.len(), 2);
        assert!(app.dirty());
        app.undo();
        assert!(app.project.assets.is_empty());
        assert!(!app.dirty());
        app.redo();
        assert_eq!(app.project.assets, imported);
        assert!(app.dirty());
    }
    #[test]
    fn multi_node_copy_across_chapters_rewrites_legacy_dice_targets_and_undoes() {
        let mut app = app();
        let mut second = Project::new().stories["main"].clone();
        second["id"] = json!("second");
        app.project.stories.insert("second".into(), second);
        app.project.stories.get_mut("main").unwrap()["nodes"] = json!([{"id":"d","type":"dice","options":[{"goto_大成功":"end","goto_成功":"end","goto_失败":"end"}]},{"id":"end","type":"end"}]);
        app.last = snapshot(&app.project);
        app.mark_saved();
        app.multiselect = BTreeSet::from([0, 1]);
        app.copy();
        app.current = "second".into();
        app.selected = 0;
        app.multiselect.clear();
        app.paste();
        app.track();
        app.flush();
        let nodes = app.project.stories["second"]["nodes"].as_array().unwrap();
        let goto = nodes[1]["options"][0]["goto_成功"].as_str().unwrap();
        assert_eq!(goto, nodes[2]["id"].as_str().unwrap());
        assert_ne!(goto, "end");
        app.undo();
        assert_eq!(
            app.project.stories["second"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        app.redo();
        assert_eq!(
            app.project.stories["second"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
    #[test]
    fn moving_front_follows_only_a_front_entry() {
        let mut app = app();
        let first = app.story()["nodes"][0]["id"].clone();
        let last = app.story()["nodes"][1]["id"].clone();
        app.selected = 1;
        app.move_node(0);
        assert_eq!(app.story()["start"], last);
        app.project.stories.get_mut("main").unwrap()["start"] = first.clone();
        app.selected = 1;
        app.move_node(0);
        assert_eq!(app.story()["start"], first);
    }
    #[test]
    fn deleting_dragged_offsets_preserves_branches_parallel_actions_and_nonadjacent_targets() {
        for parallel in [false, true] {
            let mut app = app();
            let show = if parallel {
                json!({"id":"a","type":"parallel","goto":"line","actions":[
                    {"type":"show","character":"player","position":"L1"},
                    {"type":"show","character":"girl1","position":"R1"}
                ]})
            } else {
                json!({"id":"a","type":"show","character":"player","goto":"line"})
            };
            let original = json!({"story_schema":2,"id":"main","start":"choice","nodes":[
                {"id":"choice","type":"choice","options":[{"goto":"a","text":"人物"},{"goto":"b","text":"其他分支"}]},
                show,
                {"id":"b","type":"wait","seconds":1,"goto":"line"},
                {"id":"line","type":"say","mode":"narrative","text":"汇合对白"},
                {"id":"end","type":"end"}
            ]});
            let mut story = original.clone();
            let mut selected = if parallel { 1 } else { 3 };
            for character in if parallel {
                vec!["player", "girl1"]
            } else {
                vec!["player"]
            } {
                selected = crate::preview_layout::apply(
                    &mut story,
                    selected,
                    &crate::preview_layout::Change {
                        target: crate::preview_layout::Target::Character(character.into()),
                        x: 120.0,
                        y: 60.0,
                        scale: 100.0,
                        effect_scale: None,
                    },
                )
                .unwrap();
            }
            app.project.stories.insert("main".into(), story);
            app.multiselect = app.story()["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(i, node)| (node["type"] == "offset").then_some(i))
                .collect();
            app.delete();
            assert_eq!(app.story(), original);
            assert!(preview::route(&app.story(), "end").is_some());
        }
    }
    #[test]
    fn deleting_offsets_reconnects_incoming_choice_dice_and_check_targets() {
        let mut app = app();
        let original = json!({"story_schema":2,"id":"main","start":"offset1","nodes":[
            {"id":"choice","type":"choice","options":[{"text":"offset1","goto":"offset1"},{"text":"保留","goto":"end"}]},
            {"id":"dice","type":"dice","options":[{"goto_大成功":"offset1","goto_成功":"offset1","goto_失败":"end"}]},
            {"id":"check","type":"stat_check","success":"offset1","failure":"end"},
            {"id":"offset1","type":"offset","character":"player","goto":"offset2"},
            {"id":"offset2","type":"offset","character":"player"},
            {"id":"line","type":"say","mode":"narrative","text":"offset1"},
            {"id":"end","type":"end"}
        ]});
        app.project.stories.insert("main".into(), original.clone());
        app.multiselect = BTreeSet::from([3, 4]);
        app.delete();
        let result = app.story();
        assert_eq!(result["start"], "line");
        assert_eq!(result["nodes"][0]["options"][0]["goto"], "line");
        assert_eq!(result["nodes"][0]["options"][0]["text"], "offset1");
        assert_eq!(
            result["nodes"][0]["options"][1],
            original["nodes"][0]["options"][1]
        );
        assert_eq!(result["nodes"][1]["options"][0]["goto_大成功"], "line");
        assert_eq!(result["nodes"][1]["options"][0]["goto_成功"], "line");
        assert_eq!(result["nodes"][1]["options"][0]["goto_失败"], "end");
        assert_eq!(result["nodes"][2]["success"], "line");
        assert_eq!(result["nodes"][2]["failure"], "end");
        assert_eq!(result["nodes"][3]["text"], "offset1");
    }
    #[test]
    fn deleting_a_final_drag_offset_restores_the_end_and_ambiguous_deletion_changes_nothing() {
        let mut app = app();
        let original = json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"player"}
        ]});
        let mut story = original.clone();
        crate::preview_layout::apply(
            &mut story,
            0,
            &crate::preview_layout::Change {
                target: crate::preview_layout::Target::Character("player".into()),
                x: 120.0,
                y: 60.0,
                scale: 100.0,
                effect_scale: None,
            },
        )
        .unwrap();
        app.project.stories.insert("main".into(), story);
        app.selected = 1;
        app.delete();
        assert_eq!(app.story(), original);

        let ambiguous = json!({"story_schema":2,"id":"main","start":"show","nodes":[
            {"id":"show","type":"show","character":"player","goto":"offset1"},
            {"id":"offset1","type":"offset","character":"player","goto":"choice"},
            {"id":"choice","type":"choice","options":[{"goto":"a"},{"goto":"b"}]},
            {"id":"a","type":"end"}, {"id":"b","type":"end"}
        ]});
        app.project.stories.insert("main".into(), ambiguous.clone());
        app.multiselect = BTreeSet::from([1, 2]);
        app.delete();
        assert_eq!(app.story(), ambiguous);
        assert!(app.error.as_ref().unwrap().contains("位移后续不明确"));
    }
    #[test]
    fn deleting_dragged_offsets_keeps_the_rest_of_the_chapter_reachable() {
        for explicit in [false, true] {
            let mut app = app();
            let mut story = json!({"story_schema":2,"id":"main","start":"show","nodes":[
                {"id":"show","type":"show","character":"player","position":"M","facing":"left"},
                {"id":"line","type":"say","mode":"narrative","text":"对白"},
                {"id":"bubble","type":"mood","character":"player","enabled":true,"icon":"laugh1"},
                {"id":"end","type":"end"}
            ]});
            if explicit {
                story["nodes"][0]["goto"] = json!("line");
            }
            app.project.stories.insert("main".into(), story.clone());
            app.last = snapshot(&app.project);
            let change = crate::preview_layout::Change {
                target: crate::preview_layout::Target::Character("player".into()),
                x: 120.0,
                y: 60.0,
                scale: 100.0,
                effect_scale: None,
            };
            for _ in 0..2 {
                crate::preview_layout::apply(
                    app.project.stories.get_mut("main").unwrap(),
                    1,
                    &change,
                )
                .unwrap();
                app.track();
                app.flush();
                let dragged = app.story();
                assert_eq!(dragged["nodes"][2]["type"], "offset");
                assert_eq!(
                    preview::simulate(&dragged, "bubble")["actors"]["player"]["offset_x"],
                    120.0
                );
                app.selected = 2;
                app.delete();
                app.track();
                app.flush();
                let deleted = app.story();
                assert_eq!(deleted, story);
                for id in ["line", "bubble", "end"] {
                    assert!(preview::route(&deleted, id).is_some(), "{id}: {deleted}");
                }
                assert_eq!(
                    deleted["nodes"].as_array().unwrap()[1..],
                    story["nodes"].as_array().unwrap()[1..]
                );
                assert_eq!(
                    preview::simulate(&deleted, "line")["actors"],
                    preview::simulate(&story, "line")["actors"]
                );
                lom_core::validate::validate_story(&deleted).unwrap();
                compile_with_assets(&app.project, &deleted).unwrap();
                app.undo();
                assert_eq!(app.story(), dragged);
                app.redo();
                assert_eq!(app.story(), deleted);
            }
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("project");
            app.project.save_to(&path).unwrap();
            let reopened = Project::open(&path).unwrap();
            assert_eq!(reopened.stories["main"], app.story());
            assert!(preview::route(&reopened.stories["main"], "end").is_some());
        }
    }
    #[test]
    fn deleting_nodes_repairs_sections_groups_and_localizations_without_rewriting_links() {
        let mut app = app();
        let story = json!({"id":"main","story_schema":2,"start":"a","nodes":[{"id":"a","type":"say","text":"a"},{"id":"b","type":"say","text":"b"},{"id":"c","type":"end"}],"_editor":{"sections":[{"id":"s","start":"a","end":"c","groups":[{"id":"g","start":"a","end":"a"}]}]},"localization":{"default_locale":"chs","translations":{"cht":{"a.text":"甲","b.text":"乙"}}}});
        app.project.stories.insert("main".into(), story);
        app.selected = 0;
        app.delete();
        let result = app.story();
        assert_eq!(result["start"], "b");
        assert_eq!(result["_editor"]["sections"][0]["start"], "b");
        assert!(result["_editor"]["sections"][0]["groups"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(result["localization"]["translations"]["cht"]
            .get("a.text")
            .is_none());
        assert_eq!(
            result["localization"]["translations"]["cht"]["b.text"],
            "乙"
        );
    }
    #[test]
    fn dirty_project_requires_a_decision_before_recovery() {
        let mut app = app();
        app.project.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("unsaved");
        app.track();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        app.request(Pending::Recover(PathBuf::from("/missing")), &ctx);
        assert!(matches!(app.pending, Some(Pending::Recover(_))));
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn project_intro_compilation_uses_project_assets_not_global_repository() {
        let mut project = Project::new();
        let prefix = "assets/user/character/rust_fixture.hero";
        let meta = json!({"schema":1,"content_schema":1,"id":"rust_fixture.hero","type":"character","name":"hero","files":{"main":"normal.png"},"portraits":{"normal":"normal.png"},"intro":{"name":"项目内人物","title":"项目内标题","text":"来自项目素材"}});
        project.assets.insert(
            format!("{prefix}/content.json"),
            serde_json::to_vec(&meta).unwrap(),
        );
        project
            .assets
            .insert(format!("{prefix}/normal.png"), vec![1]);
        let story = json!({"id":"main","story_schema":2,"start":"intro","nodes":[{"id":"intro","type":"intro","intro_source":"character","character":"user:rust_fixture.hero"},{"id":"end","type":"end"}]});
        let lua = compile_with_assets(&project, &story).unwrap();
        assert!(lua.contains("项目内人物"));
        assert!(lua.contains("来自项目素材"));
    }
    #[test]
    fn all_six_original_starter_templates_have_valid_story_schemas() {
        let catalog: Value =
            serde_json::from_str(include_str!("../data/project-templates.json")).unwrap();
        assert_eq!(catalog.as_object().unwrap().len(), 6);
        for key in catalog.as_object().unwrap().keys() {
            let project = template_project(key).unwrap();
            assert!(project.manifest["campaign_id"]
                .as_str()
                .unwrap()
                .starts_with("campaign_"));
            for story in project.stories.values() {
                lom_core::validate::validate_story(story).unwrap();
            }
        }
    }
    #[test]
    fn imported_step_image_is_applied_and_undone_with_its_asset() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("image.png");
        std::fs::write(&file, b"fixture image").unwrap();
        let mut app = app();
        let id = app.story()["nodes"][0]["id"].as_str().unwrap().to_owned();
        app.project.stories.get_mut(&app.current).unwrap()["nodes"][0] =
            json!({"id":id,"type":"overlay","slot":"main","action":"show"});
        app.track();
        app.flush();
        let before = snapshot(&app.project);
        app.image_import_target = Some((app.current.clone(), id));
        app.asset_import_dialog = Some(AssetImportDialog {
            extends: String::new(),
            kind: 0,
            name: "图片".into(),
            paths: vec![file],
            error: None,
        });
        app.commit_asset_import().unwrap();
        let image = app.story()["nodes"][0]["image"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(image.starts_with("user:"));
        assert!(!app.project.assets.is_empty());
        app.undo();
        assert!(snapshot(&app.project) == before);
    }
    #[test]
    fn reading_every_main_panel_preserves_project() {
        let mut app = app();
        let ctx = egui::Context::default();
        let original = snapshot(&app.project);
        for panel in [
            Center::Node,
            Center::Chapter,
            Center::Initial,
            Center::Manifest,
            Center::Events,
            Center::Tasks,
            Center::Localization,
            Center::Assets,
            Center::Tools,
            Center::Advanced,
        ] {
            app.center = panel;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(900.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| app.center(ctx),
            );
            assert!(
                snapshot(&app.project) == original,
                "Opening a panel modified the project"
            );
        }
    }
    #[test]
    fn event_configuration_uses_existing_undo_and_read_only_panel_lifecycle() {
        let mut app = app();
        let ctx = egui::Context::default();
        for mode in ["addon", "sandbox"] {
            app.install(Project::new_scenario(mode));
            let original = snapshot(&app.project);
            for center in [Center::Manifest, Center::Events, Center::Tasks] {
                app.center = center;
                let _ = ctx.run(Default::default(), |ctx| app.center(ctx));
                assert!(
                    snapshot(&app.project) == original,
                    "painting event panels must preserve source"
                );
            }
            app.project.scenario.as_mut().unwrap()["events"][0]["once"] = json!(false);
            app.track();
            app.flush();
            app.undo();
            assert!(
                snapshot(&app.project) == original,
                "undo must include independent configuration"
            );
        }
    }
    #[test]
    fn scenario_navigation_follows_relationships_without_changing_the_project() {
        let mut app = app();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/event-addon");
        app.install(Project::open(&root).unwrap());
        let original = snapshot(&app.project);
        let ctx = egui::Context::default();
        for link in [
            crate::scenario_panel::Navigate::Event("july_letter".into()),
            crate::scenario_panel::Navigate::Task("delivery".into()),
            crate::scenario_panel::Navigate::Story("meeting".into()),
        ] {
            app.follow_scenario_link(link);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1280.0, 760.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.sidebar(ctx);
                    app.right(ctx);
                    app.center(ctx);
                },
            );
            assert!(
                snapshot(&app.project) == original,
                "navigation must not rewrite scenario or chapter data"
            );
        }
        assert_eq!(app.center, Center::Node);
        assert_eq!(app.current, "meeting");
        app.center = Center::Tasks;
        let original = snapshot(&app.project);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::Delete, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(
            snapshot(&app.project) == original,
            "task navigation must not accept hidden chapter deletion"
        );
    }
    #[test]
    fn closing_dialogs_cancels_only_the_dialog_and_keeps_work_and_recovery_files() {
        let root = std::env::temp_dir().join(format!(
            "lom-dialog-close-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("session.json");
        std::fs::write(&marker, b"retained recovery").unwrap();
        for kind in ["chapter", "pending", "error", "recovery"] {
            for escape in [false, true] {
                let mut app = app();
                app.project.manifest["name"] = json!("unsaved work");
                match kind {
                    "chapter" => app.begin_chapter(false),
                    "pending" => app.pending = Some(Pending::New),
                    "error" => app.error = Some("example failure".into()),
                    _ => app.recovery_candidates = vec![root.clone()],
                }
                let before = snapshot(&app.project);
                let ctx = egui::Context::default();
                ctx.enable_accesskit();
                let frame = |app: &mut App, events| {
                    ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(900.0, 560.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |_ui| {});
                            app.dialogs(ctx);
                        },
                    )
                };
                let output = frame(&mut app, vec![]);
                if kind == "recovery" {
                    let nodes = &output
                        .platform_output
                        .accesskit_update
                        .as_ref()
                        .unwrap()
                        .nodes;
                    let bounds = |label: &str| {
                        nodes
                            .iter()
                            .find(|(_, n)| {
                                n.label().or_else(|| n.value()) == Some(tr(label).as_str())
                            })
                            .and_then(|(_, n)| n.bounds())
                            .unwrap()
                    };
                    assert!(
                        bounds("稍后处理").y1 - bounds("恢复创作副本").y0 < 300.0,
                        "a single recovery card must not expand to fill the scroll area"
                    );
                }
                let event = if escape {
                    key_event(egui::Key::Escape, egui::Modifiers::NONE)
                } else {
                    click_named_option(&output, &tr("关闭"))
                };
                let _ = frame(&mut app, vec![event]);
                assert!(
                    app.chapter_dialog.is_none()
                        && app.pending.is_none()
                        && app.error.is_none()
                        && app.recovery_candidates.is_empty(),
                    "{kind}, escape={escape}"
                );
                assert!(
                    snapshot(&app.project) == before,
                    "closing {kind} must preserve unsaved work"
                );
                assert_eq!(std::fs::read(&marker).unwrap(), b"retained recovery");
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn active_instance_recovery_is_never_a_candidate() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        assert!(recovery_owner_active(&PathBuf::from(format!(
            "{}-{stamp}",
            std::process::id()
        ))));
        assert!(recovery_owner_active(Path::new("unknown-owner")));
    }
    #[cfg(windows)]
    #[test]
    fn windows_recovery_distinguishes_live_exited_and_reused_processes() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut child = Command::new("cmd.exe")
            .args(["/D", "/C", "set /p recovery_test="])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let pid = child.id();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let path = PathBuf::from(format!("{pid}-{stamp}"));
        let live_active = recovery_owner_active(&path);
        let reused_active = recovery_owner_active(Path::new(&format!("{pid}-1")));
        // Close only our child's input, allowing it to exit before assertions.
        drop(child.stdin.take());
        child.wait().unwrap();
        let exited_active = recovery_owner_active(&path);
        drop(child);
        let removed_active = recovery_owner_active(&path);

        assert!(live_active);
        assert!(!reused_active);
        assert!(!exited_active);
        assert!(!removed_active);
        assert!(!recovery_owner_active(Path::new(&format!(
            "{}-1",
            std::process::id()
        ))));
        assert!(recovery_owner_active(Path::new("0-1")));
        assert!(recovery_owner_active(Path::new(&format!("{pid}-invalid"))));
    }
    #[cfg(windows)]
    #[test]
    fn windows_recovery_restores_chapters_assets_and_original_source_after_exit() {
        use std::os::windows::process::CommandExt;

        let mut child = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "exit", "0"])
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        drop(child);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("原项目 source");
        let image = dir.path().join("恢复 素材.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut writer = app();
        writer.project.save_to(&original).unwrap();
        writer.saved = snapshot(&writer.project);
        writer.recovery_dir = dir.path().join(format!("{pid}-{stamp}"));
        writer.begin_chapter(false);
        writer.chapter_dialog.as_mut().unwrap().name = "未保存章节".into();
        writer.commit_chapter().unwrap();
        writer
            .import_assets(vec![image], "test.recovered", "恢复素材", 0)
            .unwrap();
        writer.track();
        writer.last_recovery = Instant::now() - Duration::from_secs(31);
        writer.autosave();
        assert!(writer.recovery_dir.join("session.json").is_file());
        assert!(!recovery_owner_active(&writer.recovery_dir));

        let mut restored = app();
        restored.recover(&writer.recovery_dir);
        assert!(restored.error.is_none(), "{:?}", restored.error);
        assert_eq!(restored.project.stories, writer.project.stories);
        assert_eq!(restored.project.assets, writer.project.assets);
        assert_eq!(restored.project.source, writer.project.source);
        assert_eq!(restored.project.paths, writer.project.paths);
        assert_eq!(restored.current, writer.current);
        assert!(restored.dirty());
        let unchanged = Project::open(&original).unwrap();
        assert_eq!(unchanged.stories.len(), 1);
        assert!(unchanged.assets.is_empty());

        let active = dir.path().join(format!("{}-{stamp}", std::process::id()));
        std::fs::create_dir(&active).unwrap();
        let before = snapshot(&restored.project);
        restored.recover(&active);
        assert!(restored.error.as_ref().unwrap().contains("仍在运行"));
        assert!(snapshot(&restored.project) == before);
        assert!(active.exists());
    }
}
