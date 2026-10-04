use crate::i18n::tr;
use crate::{
    authoring,
    forms::{value_editor, Catalog},
    preview::{self, Preview},
};
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
    stories: BTreeMap<String, Value>,
    paths: BTreeMap<String, PathBuf>,
    assets: std::sync::Arc<BTreeMap<String, Vec<u8>>>,
}
#[derive(Clone)]
enum Pending {
    New,
    Template(String),
    Recover(PathBuf),
    Open(PathBuf),
    Quit,
}
#[derive(Clone, Copy, PartialEq)]
enum Center {
    Node,
    Chapter,
    Manifest,
    Localization,
    Assets,
    Tools,
    Advanced,
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
}
struct ChapterDialog {
    action: ChapterAction,
    name: String,
    error: Option<String>,
    focus_name: bool,
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
    kind: usize,
    name: String,
    paths: Vec<PathBuf>,
    error: Option<String>,
}
struct App {
    project: Project,
    catalog: Catalog,
    preview: Preview,
    current: String,
    selected: usize,
    multiselect: BTreeSet<usize>,
    center: Center,
    view: View,
    search: String,
    replace: String,
    status: String,
    error: Option<String>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Snapshot,
    saved: Snapshot,
    pending_history: Option<Snapshot>,
    last_edit: Instant,
    clipboard: Vec<Value>,
    pending: Option<Pending>,
    allow_close: bool,
    asset_dirty: bool,
    lua: String,
    diagnostics: Vec<(String, String)>,
    needs_compile: bool,
    chapter_dialog: Option<ChapterDialog>,
    rename_dialog: Option<RenameDialog>,
    settings_open: bool,
    settings_error: Option<String>,
    dialog_ime: bool,
    recovery_dir: PathBuf,
    recovery_paths: BTreeMap<String, PathBuf>,
    recovery_assets: BTreeSet<String>,
    recovery_candidates: Vec<PathBuf>,
    last_recovery: Instant,
    templates: Value,
    locale: String,
    auto: bool,
    last_step: Instant,
    tools_report: Value,
    screenshot: Option<PathBuf>,
    frame: usize,
    asset_selection: String,
    asset_import_dialog: Option<AssetImportDialog>,
    shell: shell::Shell,
    scroll_selection: (String, usize),
    bulk_key: String,
    bulk_draft: Value,
    template_name: String,
    pending_template: Option<(Vec<Value>, Vec<String>)>,
    section_parent: usize,
    recent: Vec<PathBuf>,
    node_search: String,
    tools_panel: crate::tools_panel::ToolsPanel,
    advanced: crate::advanced::Advanced,
    content_panel: crate::content_panel::ContentPanel,
    audio: crate::audio::Player,
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
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
    .inner
}
fn snapshot(p: &Project) -> Snapshot {
    Snapshot {
        manifest: p.manifest.clone(),
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
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" => {
                i += 1;
                screenshot = args.get(i).map(PathBuf::from);
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
        Box::new(move |cc| Ok(Box::new(App::new(cc, open, screenshot)))),
    )
}
fn smoke(path: &Path) -> anyhow::Result<Value> {
    let p = Project::open(path)?;
    let catalog = Catalog::new();
    let mut nodes = 0;
    let mut previews = 0;
    for (id, story) in &p.stories {
        lom_core::validate::validate_story(story)?;
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
        let mut p = Project::new();
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
            view: View::Stage,
            search: String::new(),
            replace: String::new(),
            status: String::new(),
            error,
            undo: vec![],
            redo: vec![],
            last: snap.clone(),
            saved: snap,
            pending_history: None,
            last_edit: Instant::now(),
            clipboard: vec![],
            pending: None,
            allow_close: false,
            asset_dirty: false,
            lua: String::new(),
            diagnostics: vec![],
            needs_compile: true,
            chapter_dialog: None,
            rename_dialog: None,
            settings_open: false,
            settings_error: None,
            dialog_ime: false,
            recovery_dir,
            recovery_paths: BTreeMap::new(),
            recovery_assets: BTreeSet::new(),
            recovery_candidates,
            last_recovery: Instant::now(),
            templates,
            locale: "cht".into(),
            auto: false,
            last_step: Instant::now(),
            tools_report: json!({}),
            screenshot,
            frame: 0,
            asset_selection: String::new(),
            asset_import_dialog: None,
            shell: shell::Shell::default(),
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
            tools_panel: crate::tools_panel::ToolsPanel::default(),
            advanced: crate::advanced::Advanced::default(),
            content_panel: Default::default(),
            audio: Default::default(),
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
                    self.status = format!("项目已处理；最近列表保存失败：{e}");
                }
            }
        }
    }
    fn persist_templates(&mut self) {
        match lom_core::stable_json(&self.templates).and_then(|b| {
            lom_core::project::atomic_write(&user_root().join("rust/node-templates.json"), &b)
        }) {
            Ok(()) => self.status = "节点模板已保存到本机创作数据目录".into(),
            Err(e) => self.error = Some(e.to_string()),
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
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn dirty(&self) -> bool {
        self.last != self.saved
            || self.asset_dirty
            || self.content_panel.has_pending()
            || self.advanced.has_pending()
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
        let current = Snapshot {
            manifest: self.project.manifest.clone(),
            stories: self.project.stories.clone(),
            paths: self.project.paths.clone(),
            assets: if *self.last.assets == self.project.assets {
                self.last.assets.clone()
            } else {
                std::sync::Arc::new(self.project.assets.clone())
            },
        };
        if current != self.last {
            if current.assets != self.last.assets {
                self.catalog.sync_assets(&self.project.assets);
            }
            if self.pending_history.is_none() {
                self.pending_history = Some(self.last.clone());
                self.redo.clear();
            }
            self.last = current;
            self.last_edit = Instant::now();
            self.needs_compile = true;
        }
        if self.last_edit.elapsed() > Duration::from_millis(650) {
            self.flush();
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.content_panel.reset();
        self.advanced.reset();
        self.project.stories = s.stories;
        self.project.manifest = s.manifest;
        self.project.paths = s.paths;
        self.project.assets = (*s.assets).clone();
        self.catalog.sync_assets(&self.project.assets);
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
            Pending::Quit => {
                self.allow_close = true;
                self.clear_recovery();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Pending::New => self.install(Project::new()),
            Pending::Template(key) => match template_project(&key) {
                Ok(project) => {
                    self.install(project);
                    self.saved.manifest = json!({});
                    self.status =
                        "已从模板创建项目；user:template.* 占位资源需要在内容库中替换".into();
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            Pending::Recover(path) => self.recover(&path),
            Pending::Open(path) => match Project::open(&path) {
                Ok(p) => self.install(p),
                Err(e) => self.error = Some(format!("打开失败\n{e:#}")),
            },
        }
    }
    fn install(&mut self, p: Project) {
        self.clear_recovery();
        self.project = p;
        self.advanced = Default::default();
        self.content_panel.reset();
        self.chapter_dialog = None;
        self.rename_dialog = None;
        self.asset_import_dialog = None;
        self.audio.stop();
        self.remember_source();
        self.catalog.sync_assets(&self.project.assets);
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
        self.saved = self.last.clone();
        self.asset_dirty = false;
        self.needs_compile = true;
        self.status = format!("已载入 {} 个章节", self.project.stories.len());
    }
    fn save(&mut self, as_new: bool) -> bool {
        if !self.apply_pending_drafts() {
            return false;
        }
        self.flush();
        let path = if !as_new {
            self.project.source.clone().filter(|p| {
                p.extension().and_then(|x| x.to_str()) != Some("json")
                    || (self.project.stories.len() == 1
                        && self.project.manifest == self.saved.manifest)
            })
        } else {
            None
        };
        let path = path.or_else(|| {
            rfd::FileDialog::new()
                .set_title("选择项目保存目录（保存全部章节与素材）")
                .pick_folder()
        });
        let Some(path) = path else { return false };
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
                self.saved = self.last.clone();
                self.asset_dirty = false;
                self.clear_recovery();
                self.status = format!(
                    "已保存全部 {} 个章节 · {}",
                    self.project.stories.len(),
                    path.display()
                );
                true
            }
            Err(e) => {
                self.error = Some(format!("保存失败；修改仍保留在编辑器中。\n{e:#}"));
                false
            }
        }
    }
    fn export(&mut self) {
        if !self.apply_pending_drafts() {
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
    fn compile(&mut self) {
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
            match lom_core::validate::validate_story(s) {
                Ok(warnings) => {
                    for w in warnings {
                        self.diagnostics.push((id.clone(), w));
                    }
                }
                Err(e) => self.diagnostics.push((id.clone(), format!("错误：{e:#}"))),
            }
        }
        match compile_with_assets(&self.project, &self.story()) {
            Ok(lua) => self.lua = lua,
            Err(e) => self.lua = format!("编译未通过：\n{e:#}"),
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
        self.flush();
        let node = self.catalog.new_node(kind, self.next_id(kind));
        if let Some(a) = self
            .project
            .stories
            .get_mut(&self.current)
            .and_then(|s| s["nodes"].as_array_mut())
        {
            let at = (self.selected + 1).min(a.len());
            a.insert(at, node);
            self.selected = at;
            self.multiselect.clear();
            self.center = Center::Node;
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
        let mut used: BTreeSet<String> = self.story()["nodes"]
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
        }
        for n in &mut clones {
            retarget(n, &map);
        }
        if let Some(a) = self
            .project
            .stories
            .get_mut(&self.current)
            .and_then(|s| s["nodes"].as_array_mut())
        {
            let at = (self.selected + 1).min(a.len());
            let count = clones.len();
            a.splice(at..at, clones);
            self.selected = at;
            self.multiselect = (at..at + count).collect();
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
        self.flush();
        if let Some(story) = self.project.stories.get_mut(&self.current) {
            let old_start = story["start"].as_str().unwrap_or("").to_owned();
            if let Some(nodes) = story["nodes"].as_array_mut() {
                nodes.retain(|n| !removed.contains(n["id"].as_str().unwrap_or("")));
                if removed.contains(&old_start) {
                    story["start"] = nodes.first().map(|n| n["id"].clone()).unwrap_or(json!(""));
                }
            }
            repair_deleted_metadata(story, &old_ids, &removed);
        }
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
                    stories: BTreeMap::new(),
                    paths: BTreeMap::new(),
                    assets: std::sync::Arc::new(BTreeMap::new()),
                };
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
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn clear_recovery(&mut self) {
        if self.recovery_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.recovery_dir);
        }
        self.recovery_paths.clear();
        self.recovery_assets.clear();
    }
    fn autosave(&mut self) {
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
            self.status = format!("自动恢复副本写入失败：{e:#}");
        } else if let Some(error) = pending_warning {
            self.status = format!("恢复副本已保存其他修改；尚未包含未完成的素材输入：{error}");
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.chapter_dialog.is_some()
            || self.rename_dialog.is_some()
            || self.asset_import_dialog.is_some()
            || self.settings_open
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
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
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            )
        }) {
            self.redo();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)) {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.request(Pending::New, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.play_current();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F6)) {
            self.compile();
            self.view = View::Checks;
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::F,
            )
        }) {
            self.center = Center::Advanced;
            self.advanced.page = 0;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.center = Center::Tools;
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::C)) {
                self.copy();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::V)) {
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
                .add_filter("剧情或 Mod 包", &["json", "lommod"])
                .pick_file()
            {
                self.request(Pending::Open(path), ctx);
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) {
            self.center = Center::Assets;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7)) {
            self.view = View::Graph;
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("commands")
            .frame(surface(false))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.menu_button(tr("编辑器"), |ui| {
                        if ui
                            .add(egui::Button::new(tr("设置…")).shortcut_text(
                                if cfg!(target_os = "macos") {
                                    "⌘,"
                                } else {
                                    "Ctrl+,"
                                },
                            ))
                            .clicked()
                        {
                            self.settings_open = true;
                            self.settings_error = None;
                            ui.close();
                        }
                    });
                    ui.menu_button(tr("文件"), |ui| {
                        if ui.button(tr("新建项目")).clicked() {
                            ui.close();
                            self.request(Pending::New, ctx);
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
                        if ui.button(tr("打开目录…")).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.request(Pending::Open(path), ctx);
                            }
                        }
                        if ui.button(tr("打开 JSON / Mod 包…")).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("剧情或 Mod 包", &["json", "lommod"])
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
                        if ui.button(tr("保存全部章节  ⌘S")).clicked() {
                            ui.close();
                            self.save(false);
                        }
                        if ui.button(tr("另存项目目录…")).clicked() {
                            ui.close();
                            self.save(true);
                        }
                        if ui.button(tr("导出 .lommod…")).clicked() {
                            ui.close();
                            self.export();
                        }
                        if ui.button(tr("选择预览素材库…")).clicked() {
                            ui.close();
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                match self.preview.set_library(&path) {
                                    Ok(()) => {
                                        self.status = format!("预览素材库：{}", path.display());
                                        let mut prefs = lom_core::load_json(
                                            user_root().join("rust/preferences.json"),
                                        )
                                        .unwrap_or(json!({}));
                                        prefs["preview_library_dir"] = json!(path);
                                        let _ = lom_core::project::atomic_write(
                                            &user_root().join("rust/preferences.json"),
                                            &lom_core::stable_json(&prefs).unwrap_or_default(),
                                        );
                                    }
                                    Err(e) => self.error = Some(e.to_string()),
                                }
                            }
                        }
                        if ui.button(tr("导出诊断报告…")).clicked() {
                            ui.close();
                            self.diagnostic();
                        }
                    });

                    ui.menu_button(tr("编辑"), |ui| {
                        if ui
                            .add_enabled(
                                !self.undo.is_empty() || self.pending_history.is_some(),
                                egui::Button::new(tr("撤销")),
                            )
                            .clicked()
                        {
                            self.undo();
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.redo.is_empty(), egui::Button::new(tr("重做")))
                            .clicked()
                        {
                            self.redo();
                            ui.close();
                        }
                        if ui.button(tr("复制")).clicked() {
                            self.copy();
                            ui.close();
                        }
                        if ui.button(tr("粘贴")).clicked() {
                            self.paste();
                            ui.close();
                        }
                        if ui.button(tr("作品设置")).clicked() {
                            self.center = Center::Manifest;
                            ui.close();
                        }
                        if ui.button(tr("多语言")).clicked() {
                            self.center = Center::Localization;
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
                    ui.menu_button(tr("创作工具"), |ui| {
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
                                self.advanced.page = page;
                                self.center = Center::Advanced;
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(tr("创作工具")).clicked() {
                            self.center = Center::Tools;
                            ui.close();
                        }
                        if ui.button(tr("检查项目")).clicked() {
                            self.compile();
                            self.view = View::Checks;
                            ui.close();
                        }
                        if ui.button(tr("统计")).clicked() {
                            self.view = View::Statistics;
                            ui.close();
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                cfg!(windows),
                                egui::Button::new(crate::i18n::key("toolbar.play")),
                            )
                            .clicked()
                        {
                            self.play_current();
                        }
                        if ui.button(crate::i18n::key("toolbar.library")).clicked() {
                            self.center = Center::Assets;
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
                    });
                });
            });
    }
    fn play_current(&mut self) {
        self.flush();
        let story = self.story();
        let node = story["nodes"][self.selected]["id"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        match self
            .tools_panel
            .play_from_current(&self.project, &self.current, &node)
        {
            Ok(status) => self.status = status,
            Err(e) => self.error = Some(format!("{e:#}")),
        }
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("chapters")
            .frame(surface(true))
            .default_width(shell::NAVIGATION_WIDTH)
            .min_width(220.0)
            .max_width((ctx.available_rect().width() - 620.0).max(220.0))
            .resizable(true)
            .show(ctx, |ui| {
                if self.screenshot.is_some() && self.frame == 12 {
                    println!("Layout navigation width: {}", ui.max_rect().width());
                }
                egui::TopBottomPanel::bottom("add-step-bottom")
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
                    .show_inside(ui, |ui| {
                        ui.menu_button(tr("＋ 添加步骤"), |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.node_search)
                                    .hint_text(tr("查找步骤名称 / 类型")),
                            );
                            egui::ScrollArea::vertical()
                                .max_height(560.0)
                                .show(ui, |ui| {
                                    let groups = self.catalog.schema["NODE_GROUPS_SRC"]
                                        .as_array()
                                        .cloned()
                                        .unwrap_or_default();
                                    for (group_i, g) in groups.iter().enumerate() {
                                        ui.label(
                                            RichText::new(
                                                [
                                                    "演出与画面",
                                                    "属性与旗标",
                                                    "玩法与结果",
                                                    "流程控制",
                                                ][group_i.min(3)],
                                            )
                                            .color(ACCENT)
                                            .strong(),
                                        );
                                        for kind in g[1]
                                            .as_array()
                                            .into_iter()
                                            .flatten()
                                            .filter_map(Value::as_str)
                                        {
                                            if !self.node_search.is_empty()
                                                && !format!("{} {kind}", self.catalog.label(kind))
                                                    .to_lowercase()
                                                    .contains(&self.node_search.to_lowercase())
                                            {
                                                continue;
                                            }
                                            if ui
                                                .button(self.catalog.label(kind))
                                                .on_hover_text(format!(
                                                    "{kind} · 添加后可在属性页查看全部字段"
                                                ))
                                                .clicked()
                                            {
                                                self.add_node(kind);
                                                ui.close();
                                            }
                                        }
                                        ui.separator();
                                    }
                                });
                        });
                    });
                let old = self.current.clone();
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::key("nav.story"));
                    egui::ComboBox::from_id_salt("chapters")
                        .truncate()
                        .width((ui.available_width() - 112.0).max(75.0))
                        .selected_text(
                            self.project
                                .stories
                                .get(&self.current)
                                .and_then(|s| s["title"].as_str())
                                .unwrap_or(&self.current),
                        )
                        .show_ui(ui, |ui| {
                            for (id, story) in &self.project.stories {
                                ui.selectable_value(
                                    &mut self.current,
                                    id.clone(),
                                    story["title"].as_str().unwrap_or(id),
                                );
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
                            self.center = Center::Tools;
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
                if ui
                    .selectable_label(self.center == Center::Chapter, tr("章节设置"))
                    .clicked()
                {
                    self.center = Center::Chapter;
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
                let nodes = story["nodes"].as_array().cloned().unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_salt("steps")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                        ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                        if !self.search.is_empty() {
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
                            return;
                        }
                        if let Err(error) = authoring::validate_sections(&story) {
                            ui.colored_label(Color32::from_rgb(179, 55, 49), error.to_string());
                        }
                        let mut drop_to = None;
                        for row in authoring::section_rows(&story) {
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
                                        let s =
                                            self.project.stories.get_mut(&self.current).unwrap();
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
                            let summary = if node["type"] == "end" {
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
                            } else if let Some(text) =
                                node["text"].as_str().filter(|v| !v.is_empty())
                            {
                                text.to_owned()
                            } else if let Some(character) =
                                node["character"].as_str().filter(|v| !v.is_empty())
                            {
                                let name = self.catalog.data["characters"]
                                    .as_array()
                                    .and_then(|a| a.iter().find(|v| v["id"] == character))
                                    .and_then(|v| v["name"].as_str())
                                    .unwrap_or(character);
                                let position = node["position"]
                                    .as_str()
                                    .or_else(|| node["to"].as_str())
                                    .unwrap_or("");
                                if position.is_empty() {
                                    name.to_owned()
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
                                    let preview =
                                        summary.split_whitespace().collect::<Vec<_>>().join(" ");
                                    if !preview.is_empty() {
                                        label.append("\n", 0.0, secondary.clone());
                                        label.append(&short(&preview, 26), 23.0, secondary);
                                    }
                                    let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
                                    ui.add_sized(
                                        [
                                            ui.available_width(),
                                            if summary.is_empty() { 32.0 } else { 44.0 },
                                        ],
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
                            if self.selected == index
                                && self.scroll_selection != (self.current.clone(), self.selected)
                            {
                                response.scroll_to_me(Some(egui::Align::Center));
                                self.scroll_selection = (self.current.clone(), self.selected);
                            }
                            if response.clicked() {
                                let modifiers = ui.input(|i| i.modifiers);
                                if modifiers.shift {
                                    self.multiselect = (self.selected.min(index)
                                        ..=self.selected.max(index))
                                        .collect();
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
    fn right(&mut self, ctx: &egui::Context) {
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
                            self.view = view;
                        }
                    }
                });
                ui.separator();
                let story = self.story();
                match self.view {
                    View::Stage => {
                        egui::TopBottomPanel::bottom("stage-transport")
                            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
                            .show_inside(ui, |ui| {
                                ui.horizontal(|ui| {
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
                                        self.auto = !self.auto;
                                        self.last_step = Instant::now();
                                    }
                                });
                            });
                        if let Some(target) =
                            self.preview.show(ui, &self.project, &story, self.selected)
                        {
                            if let Some(i) = story["nodes"].as_array().and_then(|a| {
                                a.iter().position(|n| n["id"].as_str() == Some(&target))
                            }) {
                                self.selected = i;
                            }
                        }
                    }
                    View::Portraits => {
                        self.preview.show_portrait(
                            ui,
                            &self.project,
                            &story["nodes"][self.selected],
                        );
                    }
                    View::Graph => {
                        egui::ScrollArea::both().show(ui, |ui| {
                            if let Some(i) = preview::graph(ui, &story, self.selected) {
                                self.selected = i;
                                self.center = Center::Node;
                            }
                        });
                    }
                    View::Lua | View::Checks => {
                        ui.horizontal(|ui| {
                            if ui.button(tr("检查项目")).clicked() {
                                self.compile();
                            }
                            if ui.button(tr("复制 Lua")).clicked() {
                                ctx.copy_text(self.lua.clone());
                            }
                            if ui.button(tr("导出 Lua…")).clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name(format!("{}.lua", self.current))
                                    .save_file()
                                {
                                    if self.lua.starts_with("编译未通过") {
                                        self.error = Some("编译尚未通过，不能导出 Lua。".into());
                                    } else if let Err(e) =
                                        lom_core::project::atomic_write(&path, self.lua.as_bytes())
                                    {
                                        self.error = Some(e.to_string());
                                    }
                                }
                            }
                        });
                        if !self.diagnostics.is_empty() {
                            egui::ScrollArea::vertical()
                                .id_salt("compile-errors")
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    for (id, message) in self.diagnostics.clone() {
                                        if ui.link(&id).clicked() {
                                            self.current = id;
                                            self.needs_compile = true;
                                        }
                                        ui.colored_label(Color32::from_rgb(179, 55, 49), message);
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
        egui::CentralPanel::default()
            .frame(surface(true))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("properties")
                    .show(ui, |ui| match self.center {
                        Center::Node => {
                            let mut story = self.story();
                            let ids: Vec<_> = story["nodes"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|n| n["id"].as_str().map(str::to_owned))
                                .collect();
                            let stories = self.project.stories.keys().cloned().collect::<Vec<_>>();
                            if let Some(mut node) = story["nodes"].get(self.selected).cloned() {
                                if self.catalog.node_form(ui, &mut node, &ids, &stories) {
                                    story["nodes"][self.selected] = node.clone();
                                    self.project
                                        .stories
                                        .insert(self.current.clone(), story.clone());
                                }
                            } else {
                                ui.heading(tr("还没有步骤"));
                                ui.label(tr("从左侧“添加步骤”开始创作。"));
                            }
                        }
                        Center::Chapter => self.chapter(ui),
                        Center::Manifest => {
                            crate::manifest_panel::show(
                                ui,
                                &mut self.project.manifest,
                                &self.catalog,
                                &self.project.stories.keys().cloned().collect::<Vec<_>>(),
                            );
                        }
                        Center::Localization => self.localization(ui),
                        Center::Assets => self.assets(ui),
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
        ui.heading(tr("章节设置"));
        let mut story = self.story();
        let mut title = story["title"].as_str().unwrap_or_default().to_owned();
        ui.label(tr("章节名称"));
        if ui
            .add(egui::TextEdit::singleline(&mut title).desired_width(f32::INFINITY))
            .changed()
        {
            story["title"] = json!(title);
        }
        let ids = story["nodes"].as_array().cloned().unwrap_or_default();
        let mut start = story["start"].as_str().unwrap_or("").to_owned();
        ui.label(tr("入口步骤"));
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
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        ui.horizontal(|ui| {
            let sections = story["_editor"]["sections"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("parent-section")
                .selected_text(
                    sections
                        .get(self.section_parent)
                        .and_then(|s| s["title"].as_str())
                        .unwrap_or("所属分区"),
                )
                .show_ui(ui, |ui| {
                    for (i, s) in sections.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.section_parent,
                            i,
                            s["title"].as_str().unwrap_or("分区"),
                        );
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
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        });
        if let Some(result) = authoring::section_editor(ui, &story) {
            match result {
                Ok(next) => story = next,
                Err(e) => self.error = Some(format!("分区/分组修改未应用：{e}")),
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
        ui.label(tr("逐句编辑翻译；人物 ID、资源路径和 Lua 保持原值。"));
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
                ui.label(tr(label));
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
                        self.error = Some(e.to_string());
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
                                Err(e) => self.error = Some(e.to_string()),
                            }
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
        });
        self.project.stories.insert(self.current.clone(), s);
    }
    fn assets(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("作品内容库"));
        ui.add_space(4.0);
        if ui.button(tr("导入素材…")).clicked() && self.apply_pending_drafts() {
            self.asset_import_dialog = Some(AssetImportDialog {
                kind: 0,
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
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("project-content-list")
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (key, meta) in &records {
                    let id = meta["id"].as_str().unwrap_or("");
                    let kind = match meta["type"].as_str().unwrap_or("") {
                        "image" => tr("图片"),
                        "character" => tr("角色"),
                        "audio" => tr("音频"),
                        other => other.to_owned(),
                    };
                    if ui
                        .add_sized(
                            [ui.available_width(), 26.0],
                            egui::Button::selectable(
                                self.asset_selection == *key,
                                format!("{} · {kind}", meta["name"].as_str().unwrap_or(id)),
                            )
                            .frame(self.asset_selection == *key)
                            .truncate()
                            .right_text(""),
                        )
                        .on_hover_text(format!("user:{id}"))
                        .clicked()
                    {
                        self.asset_selection = key.clone();
                    }
                }
            });
        if let Some(bytes) = self.project.assets.get(&self.asset_selection).cloned() {
            if let Ok(meta) = serde_json::from_slice::<Value>(&bytes) {
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
                                    self.error = Some(e.to_string());
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
            format!("全部包内资源 · {} 个", self.project.assets.len()),
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
                self.error = Some(error.to_string());
                return false;
            }
        }
        match self.advanced.apply_pending(&mut self.project) {
            Ok(true) => self.track(),
            Ok(false) => {}
            Err(error) => {
                self.error = Some(error.to_string());
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
        let paths = draft.paths.clone();
        let prefix = match kind {
            0 => "image",
            1 => "character",
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
        self.import_assets(paths, &id, &name, kind)?;
        self.track();
        self.flush();
        self.asset_import_dialog = None;
        Ok(())
    }
    fn import_assets(
        &mut self,
        paths: Vec<PathBuf>,
        id: &str,
        name: &str,
        asset_kind: usize,
    ) -> anyhow::Result<()> {
        let id = id.trim().to_owned();
        lom_core::content::validate_content_id(&id)?;
        let kind = match asset_kind {
            0 => "image",
            1 => "character",
            _ => "audio",
        };
        let prefix = format!("assets/user/{kind}/{id}");
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
        for (i, path) in paths.iter().enumerate() {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("文件名无效"))?
                .to_owned();
            anyhow::ensure!(!name.contains(':') && !name.contains('\\'), "文件名不合法");
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
                portraits.insert("normal".into(), name.clone().into());
            }
            if kind == "character" {
                let portrait = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("normal");
                anyhow::ensure!(
                    portrait
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                    "角色表情文件名只能含字母、数字和下划线"
                );
                portraits.insert(portrait.into(), name.clone().into());
            }
            anyhow::ensure!(
                !payload
                    .keys()
                    .any(|key: &String| key.eq_ignore_ascii_case(&format!("{prefix}/{name}"))),
                "所选文件存在同名素材，请分别导入。"
            );
            payload.insert(format!("{prefix}/{name}"), bytes);
        }
        anyhow::ensure!(!main.is_empty(), "未选择素材");
        let mut meta = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":name,"files":{"main":main}});
        if kind == "character" {
            meta["portraits"] = portraits.into();
            meta["scale"] = json!(100);
            meta["art_facing"] = json!("left");
        }
        if kind == "audio" {
            meta["audio_kind"] = json!(if asset_kind == 2 { "music" } else { "sound" });
            if asset_kind == 4 {
                meta["character"] = json!("player");
            }
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
        ui.heading(tr("创作工具"));
        self.tools_panel.show(ui, &mut self.project);
        if let Some((sid, nid)) = self.tools_panel.take_location() {
            if let Some(story) = self.project.stories.get(&sid) {
                if let Some(index) = nid.and_then(|nid| {
                    story["nodes"]
                        .as_array()
                        .and_then(|ns| ns.iter().position(|n| n["id"] == nid))
                }) {
                    self.selected = index;
                    self.center = Center::Node;
                } else {
                    self.center = Center::Chapter;
                }
                self.current = sid;
                self.multiselect.clear();
                self.search.clear();
                self.needs_compile = true;
            } else {
                self.center = Center::Manifest;
            }
        }
        ui.collapsing(tr("批量修改选中步骤字段"), |ui| {
            let story = self.story();
            let indices: BTreeSet<usize> = self.selected_indices().into_iter().collect();
            let selected: Vec<Value> = indices
                .iter()
                .filter_map(|i| story["nodes"].get(*i).cloned())
                .collect();
            ui.label(format!(
                "已选中 {} 个步骤。左侧 Shift 连选，⌘/Ctrl 多选。",
                selected.len()
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
                    match authoring::bulk_edit(
                        &story,
                        &indices,
                        &self.catalog.schema,
                        &field.key,
                        &self.bulk_draft[&field.key],
                    ) {
                        Ok(next) => {
                            self.flush();
                            self.project.stories.insert(self.current.clone(), next);
                            self.status = format!("已批改 {} 个步骤，可撤销", selected.len());
                        }
                        Err(e) => self.error = Some(format!("{e:#}")),
                    }
                }
            }
        });
        ui.collapsing(tr("批量修改对白"), |ui| {
            ui.label(tr(
                "只替换台词、标题与描述文本；不会改动 ID、引用、路径或 Lua。",
            ));
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
        ui.collapsing(tr("节点模板"), |ui| {
            ui.label(tr(
                "模板保存连续选中的步骤，并在插入时生成新 ID、重连内部跳转。",
            ));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.template_name).hint_text(tr("模板名称")),
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
                    ui.label(format!("{name} · {} 步", nodes.len()));
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
                            Err(e) => self.error = Some(e.to_string()),
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
            if ui.button(tr("变量 / 条件 / 引用分析")).clicked(){self.tools_report=lom_core::analysis::analyze_project(&self.project.stories,&self.project.manifest);}
            if ui.button(tr("运行项目离线测试")).clicked(){let cases:Vec<Value>=self.project.stories.values().flat_map(|s|s["_editor"]["tests"].as_array().cloned().unwrap_or_default()).collect();if cases.is_empty(){self.error=Some("还没有定义离线测试。请打开章节设置，添加测试并填写初始变量、选项和断言。".into());}else{match lom_core::analysis::run_story_tests(&self.project.stories,&Value::Array(cases)){Ok(result)=>self.tools_report=result,Err(e)=>self.error=Some(e.to_string())}}}
            if ui.button(tr("从入口模拟当前章节")).clicked(){let case=json!([{"name":"入口路径模拟","story":self.current,"initial":{"variables":{},"flags":{}},"actions":{"choices":[]},"assert":{}}]);match lom_core::analysis::run_story_tests(&self.project.stories,&case){Ok(r)=>self.tools_report=r,Err(e)=>self.error=Some(e.to_string())}}
        });
        ui.label(
            RichText::new(tr(
                "离线模拟遇到未知游戏状态会报告 unsupported，不会伪造战斗胜负。",
            ))
            .small()
            .color(Color32::GRAY),
        );
        ui.separator();
        report(ui, &self.tools_report, 0);
    }
    fn diagnostic(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("lom-editor-diagnostics.zip")
            .save_file()
        {
            let issues = lom_core::release::run_preflight(
                &self.project,
                lom_core::release::Profile::Editing,
                "1.1.2",
            );
            match lom_core::release::export_diagnostic_bundle(
                &path,
                &self.project,
                &issues,
                &self.tools_panel.diagnostic_options(),
            ) {
                Ok(saved) => self.status = format!("脱敏诊断包已保存：{}", saved.display()),
                Err(e) => self.error = Some(e.to_string()),
            }
        }
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
        Ok(())
    }
    fn dialog_keys(&mut self, ctx: &egui::Context) -> (bool, bool) {
        if !self.settings_open
            && self.chapter_dialog.is_none()
            && self.rename_dialog.is_none()
            && self.asset_import_dialog.is_none()
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
        let (return_pressed, escape_pressed) = self.dialog_keys(ctx);
        if self.settings_open {
            let mut close = false;
            let mut selected = None;
            egui::Modal::new(egui::Id::new("application-settings")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading(tr("设置"));
                ui.add_space(16.0);
                ui.label(tr("界面语言"));
                for (locale, name) in crate::i18n::LOCALES.into_iter().zip(crate::i18n::NAMES) {
                    if ui.radio(crate::i18n::locale() == locale, name).clicked() {
                        selected = Some(locale);
                    }
                }
                if let Some(error) = &self.settings_error {
                    ui.colored_label(Color32::from_rgb(179, 55, 49), error);
                }
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui
                        .add_sized([80.0, 30.0], egui::Button::new(tr("完成")))
                        .clicked();
                });
            });
            if let Some(locale) = selected {
                let prefs = user_root().join("rust/preferences.json");
                self.settings_error = self
                    .set_interface_locale(locale, &prefs)
                    .err()
                    .map(|e| e.to_string());
            }
            if close || escape_pressed {
                self.settings_open = false;
            }
        }
        if let Some(draft) = &mut self.asset_import_dialog {
            let mut submit = false;
            let mut cancel = false;
            let mut choose_files = false;
            egui::Modal::new(egui::Id::new("asset-import")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(tr("导入素材"));
                ui.add_space(16.0);
                let old_kind = draft.kind;
                ui.label(tr("素材类型"));
                egui::ComboBox::from_id_salt("import-kind")
                    .selected_text(tr(["图片", "角色", "音乐", "音效", "配音"][draft.kind]))
                    .show_ui(ui, |ui| {
                        for (i, kind) in ["图片", "角色", "音乐", "音效", "配音"].iter().enumerate()
                        {
                            ui.selectable_value(&mut draft.kind, i, tr(kind));
                        }
                    });
                if old_kind != draft.kind {
                    draft.paths.clear();
                    draft.name.clear();
                    draft.error = None;
                }
                choose_files = ui
                    .button(tr(if draft.kind == 1 {
                        "选择立绘文件…"
                    } else {
                        "选择文件…"
                    }))
                    .clicked();
                if !draft.paths.is_empty() {
                    for path in &draft.paths {
                        ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                    }
                    ui.label(tr("素材名称"));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .id(egui::Id::new("asset-import-name"))
                            .desired_width(f32::INFINITY),
                    );
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(Color32::from_rgb(179, 55, 49), tr(error));
                }
                let can_submit = !draft.paths.is_empty() && !draft.name.trim().is_empty();
                (submit, cancel) = dialog_actions(ui, "导入", can_submit);
                submit |= return_pressed && can_submit;
            });
            if cancel || escape_pressed {
                self.asset_import_dialog = None;
            } else if choose_files {
                let dialog = rfd::FileDialog::new();
                let paths = if draft.kind == 1 {
                    dialog
                        .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                        .pick_files()
                } else if draft.kind == 0 {
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
                    draft.name = paths
                        .first()
                        .and_then(|path| path.file_stem())
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    draft.paths = paths;
                    draft.error = None;
                }
            } else if submit {
                if let Err(error) = self.commit_asset_import() {
                    self.asset_import_dialog.as_mut().unwrap().error = Some(error.to_string());
                }
            }
        }
        if let Some(draft) = &mut self.chapter_dialog {
            let mut submit = false;
            let mut cancel = false;
            let title = match draft.action {
                ChapterAction::New => "新建章节",
                ChapterAction::Duplicate(_) => "复制章节",
            };
            egui::Modal::new(egui::Id::new("chapter-create")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading(tr(title));
                ui.add_space(16.0);
                ui.label(tr("章节名称"));
                let name = ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .id(egui::Id::new("chapter-name"))
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut draft.focus_name) {
                    name.request_focus();
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(Color32::from_rgb(179, 55, 49), tr(error));
                }
                let can_submit = !draft.name.trim().is_empty();
                (submit, cancel) = dialog_actions(ui, "创建", can_submit);
                submit |= return_pressed && can_submit;
            });
            if cancel || escape_pressed {
                self.chapter_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_chapter() {
                    self.chapter_dialog.as_mut().unwrap().error = Some(error.to_string());
                }
            }
        }
        if let Some(draft) = &mut self.rename_dialog {
            let mut submit = false;
            let mut cancel = false;
            let title = match draft.target {
                RenameTarget::Node { .. } => "修改步骤标识",
                RenameTarget::Chapter(_) => "修改章节标识",
            };
            egui::Modal::new(egui::Id::new("advanced-rename")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(tr(title));
                ui.add_space(16.0);
                ui.label(tr("内部标识用于流程引用，修改时会同步关联内容。"));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut draft.value)
                        .id(egui::Id::new("advanced-rename-value"))
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut draft.focus_name) {
                    field.request_focus();
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(Color32::from_rgb(179, 55, 49), tr(error));
                }
                let can_submit = !draft.value.trim().is_empty();
                (submit, cancel) = dialog_actions(ui, "应用", can_submit);
                submit |= return_pressed && can_submit;
            });
            if cancel || escape_pressed {
                self.rename_dialog = None;
            } else if submit {
                if let Err(error) = self.commit_rename() {
                    self.rename_dialog.as_mut().unwrap().error = Some(error.to_string());
                }
            }
        }
        if let Some((nodes, warnings)) = self.pending_template.clone() {
            egui::Window::new(tr("模板边界引用"))
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(tr("插入前请核对这些跨出模板范围的流程："));
                    for warning in warnings {
                        ui.label(format!("• {warning}"));
                    }
                    ui.horizontal(|ui| {
                        if ui.button(tr("保留边界引用并插入")).clicked() {
                            self.clipboard = nodes;
                            self.paste();
                            self.pending_template = None;
                        }
                        if ui.button(tr("取消")).clicked() {
                            self.pending_template = None;
                        }
                    });
                });
        }
        if self.pending.is_some() {
            egui::Window::new(tr("尚有未保存修改"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(tr("保存整个项目后再继续，或放弃本次修改。"));
                    ui.horizontal(|ui| {
                        if ui.button(tr("保存并继续")).clicked() && self.save(false) {
                            if let Some(action) = self.pending.take() {
                                self.perform(action, ctx);
                            }
                        }
                        if ui.button(tr("放弃修改")).clicked() {
                            if let Some(action) = self.pending.take() {
                                self.perform(action, ctx);
                            }
                        }
                        if ui.button(tr("取消")).clicked() {
                            self.pending = None;
                        }
                    });
                });
        }
        if let Some(message) = self.error.clone() {
            egui::Window::new(tr("操作未完成"))
                .collapsible(false)
                .resizable(true)
                .default_width(520.0)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(message);
                    if ui.button(tr("知道了")).clicked() {
                        self.error = None;
                    }
                });
        }
        if !self.recovery_candidates.is_empty() && self.screenshot.is_none() {
            egui::Window::new(tr("发现未正常关闭的创作副本"))
                .default_width(520.0)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(tr("这些恢复副本不会覆盖原项目。恢复后请检查并保存。"));
                    let candidates = self.recovery_candidates.clone();
                    for path in candidates {
                        ui.horizontal(|ui| {
                            ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                            if ui.button(tr("恢复")).clicked() {
                                self.request(Pending::Recover(path.clone()), ctx);
                            }
                            if ui.button(tr("移除副本")).clicked() {
                                if recovery_owner_active(&path) {
                                    self.error = Some("此副本仍有活跃的所有者，不能移除。".into());
                                    return;
                                }
                                match std::fs::remove_dir_all(&path) {
                                    Ok(()) => self.recovery_candidates.retain(|p| p != &path),
                                    Err(e) => self.error = Some(e.to_string()),
                                }
                            }
                        });
                    }
                    if ui.button(tr("稍后处理")).clicked() {
                        self.recovery_candidates.clear();
                    }
                });
        }
    }
}
impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.shell.clear_color()
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frame += 1;
        self.shell.install(frame);
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
        if self.auto && self.last_step.elapsed() > Duration::from_millis(1800) {
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
                self.auto = false;
                self.status = "流程在条件或选项处分叉，自动浏览已暂停".into();
            } else {
                let targets = lom_core::analysis::successors(&story, self.selected);
                if targets.len() == 1 {
                    if let Some(index) = story["nodes"]
                        .as_array()
                        .and_then(|nodes| nodes.iter().position(|n| n["id"] == targets[0]))
                    {
                        self.selected = index;
                    } else {
                        self.auto = false;
                    }
                } else {
                    self.auto = false;
                }
            }
            self.last_step = Instant::now();
        }
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
                });
            });
        self.sidebar(ctx);
        self.right(ctx);
        self.center(ctx);
        self.track();
        self.dialogs(ctx);
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
                    eprintln!("Screenshot failed: {e}");
                    self.error = Some(e.to_string());
                }
            }
            self.screenshot = None;
        }
        ctx.request_repaint_after(Duration::from_millis(if self.screenshot.is_some() {
            70
        } else {
            200
        }));
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
    #[cfg(not(unix))]
    {
        true
    }
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
    let root = tempfile::tempdir()?;
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
fn retarget(value: &mut Value, map: &BTreeMap<String, String>) {
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
    if depth > 10 {
        return;
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if v.is_object() || v.is_array() {
                    egui::CollapsingHeader::new(k)
                        .default_open(depth < 1)
                        .show(ui, |ui| report(ui, v, depth + 1));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(k);
                        ui.label(v.as_str().map(str::to_owned).unwrap_or(v.to_string()));
                    });
                }
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new(format!("#{}", i + 1)).small().color(ACCENT));
                    report(ui, v, depth + 1);
                });
            }
        }
        _ => {
            ui.label(
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or(value.to_string()),
            );
        }
    }
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
        assert_eq!(schemas.len(), 63);
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
        app
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
    fn import_confirmation_generates_unique_identity_and_keeps_display_name() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("春日.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.asset_import_dialog = Some(AssetImportDialog {
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
        app.saved = app.last.clone();
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
    fn reading_every_main_panel_preserves_project() {
        let mut app = app();
        let ctx = egui::Context::default();
        let original = snapshot(&app.project);
        for panel in [
            Center::Node,
            Center::Chapter,
            Center::Manifest,
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
    fn active_instance_recovery_is_never_a_candidate() {
        assert!(recovery_owner_active(&PathBuf::from(format!(
            "{}-123",
            std::process::id()
        ))));
        assert!(recovery_owner_active(Path::new("unknown-owner")));
    }
}
