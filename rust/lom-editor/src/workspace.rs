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

const ACCENT: Color32 = Color32::from_rgb(10, 132, 255);
const TEXT: Color32 = Color32::from_rgb(242, 242, 247);
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
    rename: String,
    new_story_id: String,
    recovery_dir: PathBuf,
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
    asset_id: String,
    asset_kind: usize,
    glass_backend: Option<String>,
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
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 760.0])
        .with_min_inner_size([900.0, 560.0])
        .with_transparent(true)
        .with_title("活侠传剧情编辑器 · Rust");
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
fn style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "C:/Windows/Fonts/msyh.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("chinese".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("chinese".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
    let mut s = (*ctx.style()).clone();
    s.visuals = egui::Visuals::dark();
    s.visuals.panel_fill = Color32::TRANSPARENT;
    s.visuals.window_fill = Color32::from_rgb(30, 32, 44);
    s.visuals.extreme_bg_color = Color32::from_rgba_unmultiplied(13, 15, 23, 190);
    s.visuals.faint_bg_color = Color32::from_white_alpha(8);
    s.visuals.override_text_color = Some(TEXT);
    s.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(10, 132, 255, 92);
    s.visuals.selection.stroke.color = Color32::from_rgb(96, 168, 255);
    s.visuals.widgets.active.bg_fill = Color32::from_white_alpha(38);
    s.visuals.widgets.active.weak_bg_fill = Color32::from_white_alpha(38);
    s.visuals.widgets.hovered.bg_fill = Color32::from_white_alpha(26);
    s.visuals.widgets.hovered.weak_bg_fill = Color32::from_white_alpha(26);
    s.visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(13, 15, 23, 190);
    s.visuals.widgets.inactive.weak_bg_fill = Color32::from_white_alpha(14);
    for widget in [
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
        &mut s.visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_white_alpha(34));
        widget.fg_stroke.color = TEXT;
    }
    s.visuals.widgets.active.bg_stroke.color = Color32::from_rgba_unmultiplied(96, 168, 255, 200);
    s.visuals.window_corner_radius = egui::CornerRadius::same(12);
    s.visuals.window_stroke = egui::Stroke::new(1.0_f32, Color32::from_white_alpha(34));
    s.spacing.item_spacing = Vec2::new(7.0, 6.0);
    s.spacing.button_padding = Vec2::new(10.0, 5.0);
    s.text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    s.text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    s.text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(23.0));
    ctx.set_style(s);
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
            status: "原生 Rust 编辑器 · JSON / Lua / .lommod 格式兼容".into(),
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
            rename: String::new(),
            new_story_id: String::new(),
            recovery_dir,
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
            asset_id: String::new(),
            asset_kind: 0,
            glass_backend: None,
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
        self.last != self.saved || self.asset_dirty
    }
    fn story(&self) -> Value {
        self.project
            .stories
            .get(&self.current)
            .cloned()
            .unwrap_or(json!({"nodes":[]}))
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
        self.flush();
        if let Some(s) = self.undo.pop() {
            self.redo.push(snapshot(&self.project));
            self.restore(s);
            self.status = "已撤销".into();
        }
    }
    fn redo(&mut self) {
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
        self.content_panel = Default::default();
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
        match self.project.save_to(&path) {
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
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("活侠传 Mod", &["lommod"])
            .set_file_name(format!(
                "{}.lommod",
                self.project.manifest["id"].as_str().unwrap_or("my_mod")
            ))
            .save_file()
        {
            match self.project.export(&path) {
                Ok(p) => self.status = format!("已用 Rust 编译并导出 {}", p.display()),
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
    fn clear_recovery(&self) {
        if self.recovery_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.recovery_dir);
        }
    }
    fn autosave(&mut self) {
        if !self.dirty() || self.last_recovery.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.last_recovery = Instant::now();
        let mut p = self.project.clone();
        p.source = Some(self.recovery_dir.clone());
        let result = (|| -> anyhow::Result<()> {
            p.save_to(&self.recovery_dir)?;
            let session = json!({"source":self.project.source,"current":self.current,"selected":self.selected});
            lom_core::project::atomic_write(
                &self.recovery_dir.join("session.json"),
                &lom_core::stable_json(&session)?,
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            self.status = format!("自动恢复副本写入失败：{e:#}");
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("commands")
            .frame(surface(false))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
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
                    if ui.button(crate::i18n::key("toolbar.export")).clicked() {
                        self.export();
                    }
                    ui.separator();
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
                        ui.menu_button(tr("界面语言 / Language"), |ui| {
                            for (locale, name) in
                                crate::i18n::LOCALES.into_iter().zip(crate::i18n::NAMES)
                            {
                                if ui
                                    .selectable_label(crate::i18n::locale() == locale, name)
                                    .clicked()
                                {
                                    crate::i18n::set_locale(locale);
                                    let mut prefs = lom_core::load_json(
                                        user_root().join("rust/preferences.json"),
                                    )
                                    .unwrap_or(json!({}));
                                    prefs["ui_locale"] = locale.into();
                                    if let Err(e) = lom_core::stable_json(&prefs).and_then(|b| {
                                        lom_core::project::atomic_write(
                                            &user_root().join("rust/preferences.json"),
                                            &b,
                                        )
                                    }) {
                                        self.error = Some(e.to_string());
                                    }
                                    ui.close();
                                }
                            }
                        });
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
            .default_width(280.0)
            .min_width(220.0)
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
                                    format!("{} · {}", story["title"].as_str().unwrap_or(id), id),
                                );
                            }
                        });
                    ui.menu_button("＋", |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_story_id)
                                .hint_text(tr("章节 ID，如 chapter2")),
                        );
                        if ui.button(tr("创建空章节")).clicked() {
                            let id = self.new_story_id.trim().to_owned();
                            if valid_id(&id) && !self.project.stories.contains_key(&id) {
                                let mut s = Project::new().stories["main"].clone();
                                s["id"] = id.clone().into();
                                s["title"] = id.clone().into();
                                self.project.stories.insert(id.clone(), s);
                                self.current = id;
                                self.selected = 0;
                                self.new_story_id.clear();
                                ui.close();
                            } else {
                                self.error = Some(
                                    "章节 ID 应为 1–64 位字母、数字、_ 或 -，且项目内唯一。".into(),
                                );
                            }
                        }
                        if ui.button(tr("复制当前章节")).clicked() {
                            let id = self.new_story_id.trim().to_owned();
                            if valid_id(&id) && !self.project.stories.contains_key(&id) {
                                let mut s = self.story();
                                s["id"] = id.clone().into();
                                self.project.stories.insert(id.clone(), s);
                                self.current = id;
                                self.new_story_id.clear();
                                ui.close();
                            }
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
                                                    "{sid} / {} · {}",
                                                    node["id"].as_str().unwrap_or(""),
                                                    short(node["text"].as_str().unwrap_or(""), 22)
                                                ),
                                            )
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
                            ui.colored_label(Color32::LIGHT_RED, error.to_string());
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
                            let summary = if let Some(text) =
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
                                    let text = format!(
                                        "第 {} 步 · {}\n{}",
                                        index + 1,
                                        self.catalog.label(node["type"].as_str().unwrap_or("")),
                                        short(&summary, 22)
                                    );
                                    let mut label = egui::text::LayoutJob::simple(
                                        text,
                                        egui::TextStyle::Button.resolve(ui.style()),
                                        ui.visuals().text_color(),
                                        (ui.available_width() - 20.0).max(40.0),
                                    );
                                    // A single-line truncating button hides the entire summary.
                                    label.wrap.max_rows = 2;
                                    let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
                                    ui.add_sized(
                                        [ui.available_width(), 48.0],
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
                                self.rename.clear();
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
            .default_width(560.0)
            .min_width(320.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (view, key) in [
                        (View::Stage, "tab.preview"),
                        (View::Portraits, "portrait.preview"),
                        (View::Graph, "tab.flow"),
                        (View::Lua, "tab.compile"),
                    ] {
                        let selected =
                            self.view == view || (view == View::Lua && self.view == View::Checks);
                        if ui
                            .selectable_label(selected, crate::i18n::key(key))
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
                                        ui.colored_label(Color32::LIGHT_RED, message);
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
                                ui.collapsing(tr("重命名节点"), |ui| {
                                    ui.label(tr("节点 ID"));
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.rename)
                                                .hint_text(node["id"].as_str().unwrap_or("")),
                                        );
                                        if ui.button(tr("重命名")).clicked() {
                                            let new = self.rename.trim().to_owned();
                                            let old = node["id"].as_str().unwrap_or("").to_owned();
                                            if !new.is_empty()
                                                && new
                                                    .bytes()
                                                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                                                && !ids.contains(&new)
                                            {
                                                let map =
                                                    BTreeMap::from([(old.clone(), new.clone())]);
                                                if let Some(s) =
                                                    self.project.stories.get_mut(&self.current)
                                                {
                                                    s["nodes"][self.selected]["id"] = new.into();
                                                    retarget(s, &map);
                                                    rename_translations(
                                                        s,
                                                        &old,
                                                        map[&old].as_str(),
                                                    );
                                                }
                                                self.rename.clear();
                                            } else {
                                                self.error = Some(
                                            "节点 ID 必须只含字母、数字和下划线，且章节内唯一。"
                                                .into(),
                                        );
                                            }
                                        }
                                    });
                                });
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
        ui.label(format!("章节 ID：{}", self.current));
        if let Some(s) = story.get_mut("title") {
            value_editor(ui, "标题", s, 0);
        } else if ui.button(tr("填写章节标题")).clicked() {
            story["title"] = json!(self.current);
        }
        let ids = story["nodes"].as_array().cloned().unwrap_or_default();
        let mut start = story["start"].as_str().unwrap_or("").to_owned();
        ui.label(tr("入口步骤"));
        egui::ComboBox::from_id_salt("start")
            .selected_text(&start)
            .show_ui(ui, |ui| {
                for n in &ids {
                    if let Some(id) = n["id"].as_str() {
                        ui.selectable_value(&mut start, id.into(), id);
                    }
                }
            });
        story["start"] = start.into();
        let mut mood = story["mood"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut mood, tr("显示官方心情气泡")).changed() {
            story["mood"] = mood.into();
        }
        ui.separator();
        ui.label(tr("重命名章节（同步入口和跨章节引用，保留原文件名）"));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_story_id).hint_text(tr("新的章节 ID")));
            if ui.button(tr("应用")).clicked() {
                let id = self.new_story_id.trim().to_owned();
                if valid_id(&id) && !self.project.stories.contains_key(&id) {
                    let old = self.current.clone();
                    story["id"] = id.clone().into();
                    self.project.stories.remove(&old);
                    if let Some(path) = self.project.paths.remove(&old) {
                        self.project.paths.insert(id.clone(), path);
                    }
                    if self.project.manifest["entry"] == old {
                        self.project.manifest["entry"] = id.clone().into();
                    }
                    for s in self.project.stories.values_mut() {
                        rename_story_refs(s, &old, &id);
                    }
                    rename_story_refs(&mut self.project.manifest, &old, &id);
                    rename_story_refs(&mut story, &old, &id);
                    self.current = id;
                    self.new_story_id.clear();
                } else {
                    self.error = Some("章节 ID 格式不合法或已存在。".into());
                }
            }
        });
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
        ui.label(tr("自定义角色、背景、插图和音频随项目保存与导出。"));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("asset-kind")
                .selected_text(["图片", "角色", "音乐", "音效", "配音"][self.asset_kind])
                .show_ui(ui, |ui| {
                    for (i, k) in ["图片", "角色", "音乐", "音效", "配音"].iter().enumerate()
                    {
                        ui.selectable_value(&mut self.asset_kind, i, *k);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.asset_id)
                    .hint_text(tr("内容 ID，如 mymod.hero")),
            );
        });
        if ui.button(tr("导入素材文件…")).clicked() {
            let paths = if self.asset_kind == 1 {
                rfd::FileDialog::new()
                    .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                    .pick_files()
            } else {
                rfd::FileDialog::new()
                    .add_filter("图片或音频", &["png", "jpg", "jpeg", "ogg", "wav"])
                    .pick_file()
                    .map(|p| vec![p])
            };
            if let Some(paths) = paths {
                if let Err(e) = self.import_assets(paths) {
                    self.error = Some(e.to_string());
                }
            }
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
        for (key, meta) in &records {
            let id = meta["id"].as_str().unwrap_or("");
            if ui
                .selectable_label(
                    self.asset_selection == *key,
                    format!(
                        "{} · {}\nuser:{}",
                        meta["name"].as_str().unwrap_or(id),
                        meta["type"].as_str().unwrap_or(""),
                        id
                    ),
                )
                .clicked()
            {
                self.asset_selection = key.clone();
            }
        }
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
                .show(ui, &mut self.project, &self.asset_selection)
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
    fn import_assets(&mut self, paths: Vec<PathBuf>) -> anyhow::Result<()> {
        let id = self.asset_id.trim().to_owned();
        lom_core::content::validate_content_id(&id)?;
        let kind = match self.asset_kind {
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
            payload.insert(format!("{prefix}/{name}"), bytes);
        }
        anyhow::ensure!(!main.is_empty(), "未选择素材");
        let mut meta = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":id,"files":{"main":main}});
        if kind == "character" {
            meta["portraits"] = portraits.into();
            meta["scale"] = json!(100);
            meta["art_facing"] = json!("left");
        }
        if kind == "audio" {
            meta["audio_kind"] = json!(if self.asset_kind == 2 {
                "music"
            } else {
                "sound"
            });
            if self.asset_kind == 4 {
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
        self.status = format!("已导入 user:{id}");
        Ok(())
    }
    fn tools(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("创作工具"));
        self.tools_panel.show(ui, &mut self.project);
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
    fn dialogs(&mut self, ctx: &egui::Context) {
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
        if self
            .glass_backend
            .as_deref()
            .is_some_and(|b| b.starts_with("NS"))
        {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [0.063, 0.071, 0.10, 1.0]
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame += 1;
        if self.glass_backend.is_none() {
            let backend = match crate::macos_glass::install(_frame) {
                Ok(name) => name,
                Err(error) => {
                    eprintln!("Native glass unavailable: {error}");
                    String::from("portable")
                }
            };
            println!("Native glass backend: {backend}");
            self.glass_backend = Some(backend);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending = Some(Pending::Quit);
            } else {
                self.clear_recovery();
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            )
        }) {
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
                        RichText::new(if self.dirty() {
                            "● 未保存"
                        } else {
                            "○ 已保存"
                        })
                        .color(if self.dirty() {
                            ACCENT
                        } else {
                            Color32::GRAY
                        }),
                    );
                    ui.separator();
                    ui.label(RichText::new(&self.status).small());
                });
            });
        self.sidebar(ctx);
        self.right(ctx);
        self.center(ctx);
        self.track();
        self.dialogs(ctx);
        self.autosave();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{} · 活侠传剧情编辑器（Rust）",
            if self.dirty() { "* " } else { "" },
            self.project.manifest["name"].as_str().unwrap_or("新作品")
        )));
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
fn surface(content: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(if content {
            Color32::from_rgba_unmultiplied(39, 40, 45, 65)
        } else {
            Color32::from_rgba_unmultiplied(42, 43, 48, 85)
        })
        .inner_margin(if content {
            egui::Margin::symmetric(8, 7)
        } else {
            egui::Margin::symmetric(8, 3)
        })
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
    #[test]
    fn asset_import_is_atomic_and_undo_redo_restores_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("hero.png");
        std::fs::write(&image, b"image fixture").unwrap();
        let mut app = app();
        app.asset_kind = 0;
        app.asset_id = "Invalid-ID".into();
        assert!(app.import_assets(vec![image.clone()]).is_err());
        assert!(app.project.assets.is_empty());
        app.asset_id = "my_mod.hero".into();
        app.import_assets(vec![image]).unwrap();
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
