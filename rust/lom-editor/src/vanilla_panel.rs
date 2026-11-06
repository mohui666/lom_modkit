use crate::forms::{self, Catalog};
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

#[derive(Default)]
pub struct Panel {
    game: PathBuf,
    chapters: Vec<String>,
    sources: BTreeMap<String, Source>,
    attempted: BTreeSet<String>,
    catalog_attempted: bool,
    job: Option<mpsc::Receiver<Result<Value, String>>>,
    error: Option<String>,
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
        }
        self.focus = Some(bookmark);
        self.filter.clear();
    }
    pub fn take_bookmark(&mut self) -> Option<Bookmark> {
        self.requested_bookmark.take()
    }
    fn load_navigation(&mut self) {
        self.navigation_loaded = true;
        self.navigation_error = None;
        self.routes = None;
        self.graph = None;
        self.routes_attempted = false;
        let root = story_bookmarks::default_root();
        match story_bookmarks::read(&root) {
            Ok(marks) => self.bookmarks = marks,
            Err(error) => self.navigation_error = Some(error.to_string()),
        }
        match story_bookmarks::read_timeline(&root) {
            Ok(timeline) => self.timeline = timeline,
            Err(error) => self.navigation_error = Some(error.to_string()),
        }
    }
    fn navigation(&mut self, ui: &mut egui::Ui) {
        if !self.navigation_loaded {
            self.load_navigation();
        }
        egui::CollapsingHeader::new("游戏标记与路线图")
            .default_open(true).open(self.timeline_tab.then_some(true))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.timeline_tab, false, "游戏标记");
                    ui.selectable_value(&mut self.timeline_tab, true, "路线图");
                    if ui.add_enabled(self.job.is_none(), egui::Button::new("刷新定位信息")).clicked() { self.load_navigation(); }
                });
                ui.add(egui::TextEdit::singleline(&mut self.navigation_filter).hint_text("搜索对白、任务、地点、旗标或章节"));
                let query=self.navigation_filter.to_lowercase();
                if let Some(error)=&self.navigation_error {ui.colored_label(egui::Color32::RED,error);}
                if self.timeline_tab {
                    if let (Some(timeline),Some(graph))=(&self.timeline,&self.graph) {
                        let marks:Vec<_>=self.bookmarks.iter().filter(|mark|mark.game_dir==timeline.game_dir).cloned().collect();
                        if let Some(action)=self.route_view.show(ui,graph,&marks,&query) {
                            self.requested_bookmark=match action {
                                crate::route_map::Action::OpenMark=>marks.first().cloned(),
                                crate::route_map::Action::Open(script)=>Some(Bookmark {
                                    schema:1,script,text_key:String::new(),text:String::new(),game_dir:timeline.game_dir.clone(),year:0,month:0,stage:0,captured_utc:String::new(),
                                }),
                            };
                        }
                    } else if self.timeline.is_some() {
                        if self.job.is_some(){ui.spinner();ui.label("正在读取本机剧情路线…");}
                        if let Some(error)=&self.error {ui.colored_label(egui::Color32::RED,error);}
                        if self.job.is_none()&&ui.button("读取路线图").clicked(){self.routes_attempted=false;}
                    } else {
                        ui.label("在游戏原版剧情中按 Ctrl+F6 标记一次，再刷新，即可读取当前游戏的路线图。");
                    }
                } else {
                    ui.small("游戏中按 Ctrl+F6 标记当前章节与对白，回来刷新后点击定位。");
                    egui::ScrollArea::vertical().id_salt("original-bookmarks").max_height(180.0).show(ui,|ui| {
                        for mark in &self.bookmarks {
                            if !format!("{} {} {}",mark.script,mark.text_key,mark.text).to_lowercase().contains(&query) {continue;}
                            let excerpt:String=mark.text.chars().take(56).collect();
                            if ui.button(format!("{}\n{excerpt}",mark.caption())).clicked(){self.requested_bookmark=Some(mark.clone());}
                        }
                        if self.bookmarks.is_empty(){ui.label("还没有游戏标记。");}
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
        panel.attempted.insert(target.into());
        panel.sources.insert(
            target.into(),
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
            .get(target)?
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
            self.attempted.insert(target.into());
        } else {
            self.catalog_attempted = true;
        }
        let args = target
            .map(|target| vec!["--story-script".into(), target.into()])
            .unwrap_or_else(|| vec!["--story-list".into()]);
        self.read_request(ctx, self.game.clone(), args);
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
        );
    }
    fn read_request(&mut self, ctx: &egui::Context, game: PathBuf, args: Vec<String>) {
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
                        anyhow::anyhow!(
                            "读取组件没有返回章节结果：{}",
                            String::from_utf8_lossy(&output.stderr)
                        )
                    })?;
                anyhow::ensure!(
                    output.status.success() && event["event"] == "result",
                    "{}",
                    event["message"].as_str().unwrap_or("读取原版章节失败")
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
            Err(mpsc::TryRecvError::Disconnected) => Err("原版章节读取中断".into()),
        };
        self.job = None;
        match result {
            Err(error) => self.error = Some(error),
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
                        Err(error) => self.error = Some(format!("路线读取结果无效：{error}")),
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
                        target.into(),
                        Source {
                            text: source.into(),
                            sites: vanilla_patch::text_sites(source),
                            translations,
                        },
                    );
                } else {
                    self.error = Some("原版章节读取结果不完整".into());
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
        self.poll();
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
        ui.heading("原版剧情补丁");
        ui.label("修改原对白、选项文字，或通过旗标检定在原句前后加入对白。未修改的流程继续执行。");
        if let Some(status) = &self.navigation_status {
            ui.label(status);
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.job.is_none(), egui::Button::new("选择游戏目录…"))
                .clicked()
            {
                if let Some(game) = rfd::FileDialog::new().pick_folder() {
                    self.game = game;
                    self.chapters.clear();
                    self.sources.clear();
                    self.attempted.clear();
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
        ui.label("原版章节");
        let display = patch.target.clone();
        ui.add_enabled_ui(patch.edits.is_empty() && self.job.is_none(), |ui| {
            forms::resource_combo(ui, "vanilla-chapters", &display, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.chapter_filter).hint_text("搜索章节编号"),
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
                            !used.iter().any(|id| id.eq_ignore_ascii_case(target)),
                            egui::Button::selectable(patch.target == *target, target),
                        )
                        .clicked()
                    {
                        patch.target = target.clone();
                        self.selected = 0;
                        ui.close();
                    }
                }
            });
        });
        if patch.target != display && story["title"] == format!("原版章节 · {display}") {
            story["title"] = json!(format!("原版章节 · {}", patch.target));
        }
        if !patch.edits.is_empty() {
            ui.small("本章已有修改。编辑其他原版章节时，请在左侧添加补丁章节。");
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.job.is_none() && !self.game.as_os_str().is_empty(),
                    egui::Button::new("重新读取原版章节"),
                )
                .clicked()
            {
                self.read(ui.ctx(), Some(&patch.target));
            }
            if self.job.is_some() {
                ui.spinner();
                ui.label("正在读取本机游戏…");
            }
        });
        if self.error.is_none()
            && self.job.is_none()
            && !self.game.as_os_str().is_empty()
            && !self.attempted.contains(&patch.target)
        {
            self.read(ui.ctx(), Some(&patch.target));
        }
        story["vanilla_patch"] = json!(patch);
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(180, 50, 40), error);
        }
        let Some(source) = self.sources.get(&patch.target) else {
            return;
        };
        if ui.button("导入整章到步骤编辑器").clicked() {
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
        ui.small("导入后可添加演出、音乐、物品、数值和分支。原生复杂逻辑保留为高级步骤。");
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
                    "已定位到标记的原版对白。".into()
                } else {
                    "同一句在本章出现多次，请在原文列表中选择对应位置。".into()
                });
            } else {
                self.selected = 0;
                self.navigation_status =
                    Some(if mark.text_key.is_empty() && mark.text.is_empty() {
                        "已定位原版章节。".into()
                    } else {
                        "已定位原版章节，当前文字未匹配到原句，请在列表中核对。".into()
                    });
            }
        }
        self.selected = self.selected.min(source.sites.len().saturating_sub(1));
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("搜索原文、选项或文字编号"));
        ui.small(format!(
            "{} 处对白与选项 · 已修改 {} 处",
            source.sites.len(),
            patch.edits.len()
        ));
        egui::ScrollArea::vertical()
            .id_salt("vanilla-dialogue-list")
            .max_height(220.0)
            .show(ui, |ui| {
                for (index, site) in source.sites.iter().enumerate() {
                    let text = source.display(site);
                    if !format!("{text} {}", site.original)
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
                    {
                        continue;
                    }
                    let edited = patch
                        .edits
                        .iter()
                        .any(|edit| edit.find == site.find && edit.occurrence == site.occurrence);
                    let excerpt: String = text.chars().take(54).collect();
                    if ui
                        .selectable_label(
                            self.selected == index,
                            format!(
                                "{} {}{}",
                                if site.kind == "say" {
                                    "对白"
                                } else {
                                    "选项"
                                },
                                if edited { "● " } else { "" },
                                excerpt
                            ),
                        )
                        .clicked()
                    {
                        self.selected = index;
                    }
                }
            });
        let Some(site) = source.sites.get(self.selected) else {
            ui.label("这个章节没有可识别的对白或选项。");
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
        ui.strong("原文");
        ui.label(&original);
        let mut replace = edit.text.is_some();
        if ui.checkbox(&mut replace, "修改这处原文").changed() {
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
                &mut edit.before,
                &mut edit.before_condition,
                catalog,
            );
            insertion(
                ui,
                "原句后新增对白",
                &mut edit.after,
                &mut edit.after_condition,
                catalog,
            );
            ui.small("新增对白沿用此处人物与画面；条件不成立时，继续执行原对白。");
        }
        let restore = ui.button("还原此处修改").clicked();
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
            ui.colored_label(
                egui::Color32::RED,
                "有一处旧修改在当前原章中无法定位，请核对后重新添加。",
            );
            ui.label(format!(
                "{}\n{}\n{}",
                orphan.before,
                orphan.text.as_deref().unwrap_or(""),
                orphan.after
            ));
            let remove = ui
                .push_id((&orphan.find, orphan.occurrence), |ui| {
                    ui.button("移除此处旧定位").clicked()
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
        ui.heading("补丁预览");
        ui.label("正式游戏沿用原章节的触发条件；试玩从原章开头运行。");
        let Ok(Some(patch)) = vanilla_patch::parse(story) else {
            return;
        };
        ui.label(format!(
            "目标：{} · {} 处修改",
            patch.target,
            patch.edits.len()
        ));
        let Some(source) = self.sources.get(&patch.target) else {
            return;
        };
        if let Some(site) = source.sites.get(self.selected) {
            let edit = patch
                .edits
                .iter()
                .find(|edit| edit.find == site.find && edit.occurrence == site.occurrence);
            if let Some(edit) = edit.filter(|edit| !edit.before.is_empty()) {
                preview_insertion(ui, "原句前", &edit.before, &edit.before_condition);
            }
            ui.separator();
            ui.strong("原对白 / 选项");
            ui.label(
                edit.and_then(|edit| edit.text.clone())
                    .unwrap_or_else(|| source.display(site)),
            );
            if let Some(edit) = edit.filter(|edit| !edit.after.is_empty()) {
                preview_insertion(ui, "原句后", &edit.after, &edit.after_condition);
            }
        }
        ui.separator();
        egui::CollapsingHeader::new("查看修改后的原章 Lua").show(
            ui,
            |ui| match vanilla_patch::apply(&source.text, &patch.edits) {
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
            },
        );
    }
}

fn insertion(
    ui: &mut egui::Ui,
    label: &str,
    text: &mut String,
    condition: &mut Option<Condition>,
    catalog: &Catalog,
) {
    ui.push_id(label, |ui| {
        ui.strong(label);
        ui.add(
            egui::TextEdit::multiline(text)
                .desired_rows(3)
                .hint_text("不填写则不插入")
                .desired_width(f32::INFINITY),
        );
        let mut enabled = condition.is_some();
        if ui
            .checkbox(&mut enabled, "仅在旗标检定成立时插入")
            .changed()
        {
            *condition = enabled.then(Condition::default);
        }
        if let Some(condition) = condition {
            let mut node = serde_json::to_value(&*condition).unwrap();
            node["type"] = json!("flag_check");
            for (key, label, kind) in [
                ("source", "旗标来源", "enum:flag_check_source"),
                ("flag", "旗标或检查点", "line"),
            ] {
                catalog.field(ui, &mut node, key, label, kind, false, &[], &[]);
            }
            if node["source"] == "flag_value" {
                catalog.field(
                    ui,
                    &mut node,
                    "op",
                    "比较方式",
                    "enum:check_op",
                    false,
                    &[],
                    &[],
                );
                catalog.field(ui, &mut node, "value", "目标值", "int", false, &[], &[]);
            } else {
                catalog.field(
                    ui,
                    &mut node,
                    "invert",
                    "取反：条件不成立时插入",
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
fn preview_insertion(ui: &mut egui::Ui, label: &str, text: &str, condition: &Option<Condition>) {
    ui.separator();
    ui.strong(label);
    if let Some(condition) = condition {
        ui.small(if condition.source == "flag_value" {
            format!(
                "{} {} {} 时显示",
                condition.flag, condition.op, condition.value
            )
        } else {
            format!(
                "{} {}时显示",
                condition.flag,
                if condition.invert {
                    "不成立"
                } else {
                    "成立"
                }
            )
        });
    } else {
        ui.small("无条件显示");
    }
    ui.label(text);
}
