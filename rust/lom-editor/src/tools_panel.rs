//! Auxiliary native workflows share the same in-memory project as authoring.
use eframe::egui;
use lom_core::{content_library, game_tools, project::Project, release};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

enum JobEvent {
    Progress(Value),
    Finished(Result<Value, String>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolPage {
    #[default]
    Editing,
    Previews,
    Library,
    Game,
    Release,
    Maintenance,
}
impl ToolPage {
    pub fn title(self) -> &'static str {
        match self {
            Self::Editing => "批量修改与模板",
            Self::Previews | Self::Library => "素材管理",
            Self::Game => "游戏与 MOD",
            Self::Release => "发布",
            Self::Maintenance => "迁移与诊断",
        }
    }
}

pub struct ToolsPanel {
    pub page: ToolPage,
    export_requested: bool,
    update_check_requested: bool,
    installation: Option<Value>,
    mods: Vec<Value>,
    mods_error: Option<String>,
    game_loaded: bool,
    refresh_after_job: bool,
    report: Value,
    location: Option<crate::advanced::Location>,
    result_filter: String,
    package_report: Value,
    package_entry: String,
    library: PathBuf,
    game: PathBuf,
    game_candidates: Vec<PathBuf>,
    runtime: PathBuf,
    ffmpeg: Option<PathBuf>,
    content_id: String,
    content_version: String,
    author: String,
    license: String,
    dependencies: String,
    pending_pack: Option<PathBuf>,
    pending_info: Option<content_library::ContentPackInfo>,
    fixes: Option<release::FixProposal>,
    read_reset: Vec<PathBuf>,
    job: Option<mpsc::Receiver<JobEvent>>,
    report_title: String,
    job_title: String,
    report_open: bool,
    help_open: bool,
    help: crate::help::HelpDocument,
    progress: Value,
    cancel: Arc<AtomicBool>,
    preview_ready: Option<PathBuf>,
}
impl Default for ToolsPanel {
    fn default() -> Self {
        let settings = lom_core::load_json(settings_path()).unwrap_or(json!({}));
        Self {
            page: ToolPage::default(),
            export_requested: false,
            update_check_requested: false,
            installation: None,
            mods: vec![],
            mods_error: None,
            game_loaded: false,
            refresh_after_job: false,
            report: Value::Null,
            location: None,
            result_filter: String::new(),
            package_report: Value::Null,
            package_entry: String::new(),
            library: lom_core::content::default_repository_root(),
            game: PathBuf::from(settings["game_dir"].as_str().unwrap_or("")),
            game_candidates: Vec::new(),
            runtime: settings["rust_runtime_dir"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|p| p.join("runtime")))
                        .filter(|p| p.join("MortalModHost.dll").is_file())
                })
                .unwrap_or_default(),
            ffmpeg: settings["ffmpeg_path"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"]
                        .into_iter()
                        .map(PathBuf::from)
                        .find(|p| p.is_file())
                }),
            content_id: String::new(),
            content_version: "1.0.0".into(),
            author: String::new(),
            license: String::new(),
            dependencies: String::new(),
            pending_pack: None,
            pending_info: None,
            fixes: None,
            read_reset: vec![],
            job: None,
            report_title: String::new(),
            job_title: String::new(),
            report_open: false,
            help_open: false,
            help: Default::default(),
            progress: Value::Null,
            cancel: Arc::new(AtomicBool::new(false)),
            preview_ready: None,
        }
    }
}
impl ToolsPanel {
    pub fn take_location(&mut self) -> Option<crate::advanced::Location> {
        self.location.take()
    }
    pub fn diagnostic_options(&self) -> release::DiagnosticOptions {
        release::DiagnosticOptions {
            game_root: (!self.game.as_os_str().is_empty()).then(|| self.game.clone()),
            bundled_runtime: (!self.runtime.as_os_str().is_empty())
                .then(|| self.runtime.join("MortalModHost.dll")),
            ..Default::default()
        }
    }
    pub fn play_from_current(
        &mut self,
        project: &Project,
        script: &str,
        node: &str,
    ) -> anyhow::Result<String> {
        anyhow::ensure!(
            cfg!(windows),
            "游戏接入仅支持 Windows；当前 Mac 可编辑、编译和预览"
        );
        anyhow::ensure!(
            !self.game.as_os_str().is_empty() && !self.runtime.as_os_str().is_empty(),
            "请先打开“游戏与 MOD”，选择游戏目录并完成一键配置"
        );
        let issues = release::run_preflight(project, release::Profile::Editing, "1.1.3");
        anyhow::ensure!(
            !issues.iter().any(|i| i.severity == "error"),
            "项目存在体检错误，请先检查项目"
        );
        let preview = prepare_preview(project, script, node)?;
        let temp = lom_core::workdir::create()?;
        let package = temp.path().join("__lom_modkit_preview.lommod");
        preview.export(&package)?;
        let was_running = game_tools::is_game_running()?;
        let runtime = game_tools::install_runtime(&self.game, &self.runtime)?;
        game_tools::remove_preview_packages(&self.game)?;
        game_tools::install_mod(&self.game, &package, true)?;
        game_tools::request_preview(&self.game, script, node)?;
        if was_running && runtime["changed"] == true {
            return Ok("试玩已准备：C# 宿主刚刚更新，请完整退出并重新启动游戏。".into());
        }
        game_tools::launch_game()?;
        Ok(format!("试玩请求已发送：{script}/{node}"))
    }
    fn save_setting(&mut self, key: &str, value: Value) -> bool {
        let path = settings_path();
        let result = (|| {
            let mut settings = lom_core::load_json(&path).unwrap_or(json!({}));
            anyhow::ensure!(settings.is_object(), "设置文件不是对象");
            settings[key] = value;
            lom_core::project::atomic_write(&path, &lom_core::stable_json(&settings)?)
        })();
        if let Err(e) = result {
            self.result::<Value>("操作未完成", Err(e));
            false
        } else {
            true
        }
    }
    fn background(
        &mut self,
        title: &str,
        f: impl FnOnce() -> anyhow::Result<Value> + Send + 'static,
    ) {
        self.background_progress(title, move |_| f());
    }
    fn background_progress(
        &mut self,
        title: &str,
        f: impl FnOnce(mpsc::Sender<JobEvent>) -> anyhow::Result<Value> + Send + 'static,
    ) {
        if self.job.is_some() {
            self.report_title = self.job_title.clone();
            self.report_open = true;
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.cancel.store(false, Ordering::Relaxed);
        self.report_title = title.into();
        self.job_title = title.into();
        self.report = Value::Null;
        self.progress = Value::Null;
        self.report_open = true;
        self.job = Some(rx);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(tx.clone())))
                .map_err(|_| "任务意外退出".to_owned())
                .and_then(|r| r.map_err(|e| format!("{e:#}")));
            let _ = tx.send(JobEvent::Finished(result));
        });
    }
    fn result<T: serde::Serialize>(&mut self, title: &str, result: anyhow::Result<T>) {
        let value = match result {
            Ok(v) => serde_json::to_value(v).unwrap_or(Value::Null),
            Err(e) => json!({"error":format!("{e:#}")}),
        };
        self.present(title, value);
    }
    pub fn present(&mut self, title: &str, value: Value) {
        self.report = value;
        self.report_title = title.into();
        self.report_open = true;
    }
    pub fn dialog_open(&self) -> bool {
        self.report_open || self.help_open
    }
    pub fn open_help(&mut self) {
        self.help.open_guide();
        self.help_open = true;
    }
    pub fn status_ui(&mut self, ui: &mut egui::Ui) {
        if self.job.is_some() {
            ui.separator();
            ui.spinner();
            ui.label(crate::i18n::tr(&self.job_title));
            if ui.small_button(crate::i18n::tr("查看进度或结果")).clicked() {
                self.report_title = self.job_title.clone();
                self.report_open = true;
            }
        }
    }
    pub fn take_update_check(&mut self) -> bool {
        std::mem::take(&mut self.update_check_requested)
    }

    pub fn take_preview_library(&mut self) -> Option<PathBuf> {
        self.preview_ready.take()
    }
    pub fn start_extraction_from(&mut self, path: PathBuf) {
        if self.job.is_some() {
            self.report_open = true;
            return;
        }
        let path = if path.file_name().is_some_and(|name| name == "Mortal_Data") {
            path.parent().unwrap_or(&path).to_path_buf()
        } else {
            path
        };
        if let Err(error) = crate::asset_extraction::validate_game(&path) {
            self.result::<Value>("选择游戏目录", Err(error));
            return;
        }
        self.game = path;
        self.game_loaded = false;
        if self.save_setting("game_dir", json!(self.game)) {
            self.start_extraction();
        }
    }
    pub fn start_extraction(&mut self) {
        if self.job.is_some() {
            self.report_open = true;
            return;
        }
        // Validate the installed helper before asking the user to select a game.
        let helper = match crate::asset_extraction::helper_path() {
            Ok(path) => path,
            Err(e) => {
                self.result::<Value>("一键提取游戏资产", Err(e));
                return;
            }
        };
        if crate::asset_extraction::validate_game(&self.game).is_err() {
            let Some(path) = rfd::FileDialog::new()
                .set_title("选择包含 Mortal_Data 的游戏目录")
                .pick_folder()
            else {
                return;
            };
            if let Err(e) = crate::asset_extraction::validate_game(&path) {
                self.result::<Value>("一键提取游戏资产", Err(e));
                return;
            }
            self.game = path;
            self.game_loaded = false;
            if !self.save_setting("game_dir", json!(self.game)) {
                return;
            }
        }
        let game = self.game.clone();
        let cache = settings_path().parent().unwrap().join("game-previews");
        let cancel = self.cancel.clone();
        self.background_progress("一键提取游戏资产", move |tx| {
            crate::asset_extraction::extract(&helper, &game, &cache, cancel, move |v| {
                let _ = tx.send(JobEvent::Progress(v));
            })
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        loop {
            let Some(receiver) = &self.job else {
                break;
            };
            match receiver.try_recv() {
                Ok(JobEvent::Progress(value)) => self.progress = value,
                Ok(JobEvent::Finished(result)) => {
                    self.report_title = self.job_title.clone();
                    self.report = result.unwrap_or_else(|error| json!({"error":error}));
                    if let Some(path) = self.report["output_dir"].as_str() {
                        self.preview_ready = Some(PathBuf::from(path));
                    }
                    self.job = None;
                    if self.refresh_after_job {
                        self.refresh_after_job = false;
                        self.refresh_game();
                    }
                    self.report_open = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.report_title = self.job_title.clone();
                    self.report = json!({"error":"任务连接已中断，请重新执行"});
                    self.job = None;
                    if self.refresh_after_job {
                        self.refresh_after_job = false;
                        self.refresh_game();
                    }
                    self.report_open = true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    pub fn show_dialog(&mut self, ctx: &egui::Context, project: &Project) {
        if self.help_open {
            self.show_help(ctx);
            return;
        }
        if !self.report_open {
            return;
        }
        let running = self.job.is_some() && self.report_title == self.job_title;
        let response = egui::Modal::new(egui::Id::new("creation-tool-result"))
            .frame(crate::shell::dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(560.0_f32.min(ctx.content_rect().width() - 64.0).max(240.0));
                if crate::shell::dialog_header(ui, &crate::i18n::tr(&self.report_title)) {
                    self.report_open = false;
                }
                ui.separator();
                if running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(crate::i18n::tr("正在处理，请稍候…"));
                    });
                    if let Some(message) = self.progress["message"].as_str() {
                        ui.label(crate::i18n::tr(message));
                    }
                    if let (Some(done), Some(total)) = (
                        self.progress["done"].as_u64(),
                        self.progress["total"].as_u64(),
                    ) {
                        if total > 0 {
                            ui.add(
                                egui::ProgressBar::new(done as f32 / total as f32)
                                    .text(format!("{done} / {total}")),
                            );
                        }
                    }
                    ui.label(crate::i18n::tr("可收起窗口继续编辑，完成后会再次提示。"));
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("creation-result-scroll")
                        .max_height((ctx.content_rect().height() - 180.0).max(100.0))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_max_width(ui.available_width());
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::tool_report::summary(&self.report))
                                        .strong(),
                                )
                                .wrap(),
                            );
                            ui.add_space(8.0);
                            if self.report["error"].is_null() {
                                crate::tool_report::show(ui, &self.report, 0);
                            }
                            let mut locations = Vec::new();
                            collect_locations(&self.report, project, &mut locations);
                            if !locations.is_empty() {
                                ui.separator();
                                ui.label(crate::i18n::tr("点击可回到对应剧情步骤"));
                                for (sid, nid, label) in locations.iter().take(100) {
                                    if ui
                                        .link(format!(
                                            "{sid}/{} · {label}",
                                            nid.as_deref().unwrap_or("作品设置")
                                        ))
                                        .clicked()
                                    {
                                        self.location = Some((sid.clone(), nid.clone()));
                                        self.report_open = false;
                                    }
                                }
                            }
                            ui.collapsing(
                                crate::i18n::tr("技术详情（排查问题时使用）"),
                                |ui| {
                                    if ui.button(crate::i18n::tr("复制原始报告")).clicked() {
                                        ui.ctx().copy_text(
                                            serde_json::to_string_pretty(&self.report)
                                                .unwrap_or_default(),
                                        );
                                    }
                                    let mut text = serde_json::to_string_pretty(&self.report)
                                        .unwrap_or_default();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut text)
                                            .code_editor()
                                            .desired_width(f32::INFINITY),
                                    );
                                },
                            );
                        });
                }
                ui.separator();
                crate::shell::dialog_buttons(ui, |ui| {
                    if ui
                        .add_sized(
                            [100.0, 30.0],
                            egui::Button::new(crate::i18n::tr(if running {
                                "收起，继续编辑"
                            } else {
                                "知道了"
                            })),
                        )
                        .clicked()
                    {
                        self.report_open = false;
                    }
                    if running
                        && self.report_title == "一键提取游戏资产"
                        && ui
                            .add_enabled(
                                !self.cancel.load(Ordering::Relaxed),
                                egui::Button::new(crate::i18n::tr("取消提取")),
                            )
                            .clicked()
                    {
                        self.cancel.store(true, Ordering::Relaxed);
                    }
                });
            });
        if response.should_close() {
            self.report_open = false;
        }
    }
    pub fn open_node_help(&mut self, kind: &str) {
        self.help.open_node(kind);
        self.help_open = true;
    }
    fn show_help(&mut self, ctx: &egui::Context) {
        if self.help.show(ctx) {
            self.help_open = false;
        }
    }
    pub fn refresh_game(&mut self) {
        self.game_loaded = true;
        self.mods.clear();
        self.mods_error = None;
        if self.game.as_os_str().is_empty() {
            self.installation = None;
            return;
        }
        self.installation = Some(
            match crate::game_setup::diagnose(&self.game, &self.runtime) {
                Ok(report) => report,
                Err(error) => json!({"error":format!("{error:#}")}),
            },
        );
        match game_tools::list_mods(&self.game) {
            Ok(rows) => self.mods = rows.as_array().cloned().unwrap_or_default(),
            Err(error) => self.mods_error = Some(format!("{error:#}")),
        }
    }
    pub fn take_export_request(&mut self) -> bool {
        std::mem::take(&mut self.export_requested)
    }
    pub fn ready_to_play(&self) -> bool {
        self.game_ready()
    }
    fn game_ready(&self) -> bool {
        self.installation.as_ref().is_some_and(|report| {
            report["error"].is_null()
                && report["steam_ready"] == true
                && report["findings"].as_array().is_some_and(|findings| {
                    !findings.is_empty()
                        && !findings
                            .iter()
                            .any(|f| f["severity"] == "error" || f["fixable"] == true)
                })
        })
    }
    fn runtime_available(&self) -> bool {
        game_tools::HOST_FILES
            .iter()
            .all(|name| self.runtime.join(name).is_file())
    }
    fn choose_game(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title(crate::i18n::tr("选择包含 Mortal.exe 的游戏目录"))
            .pick_folder()
        {
            self.select_game(path);
        }
    }
    fn select_game(&mut self, path: PathBuf) {
        if let Err(error) = game_tools::validate_root(&path)
            .and_then(|_| game_tools::game_architecture(&path).map(|_| ()))
        {
            self.result::<Value>("选择游戏目录", Err(error));
            return;
        }
        if self.save_setting("game_dir", json!(path)) {
            self.game = path;
            self.game_candidates.clear();
            self.refresh_game();
        }
    }
    fn locate_game(&mut self) {
        self.game_candidates.clear();
        match lom_core::game_discovery::installed_games() {
            Ok(paths) if paths.len() == 1 => self.select_game(paths[0].clone()),
            Ok(paths) if paths.is_empty() => self.present(
                "一键定位游戏目录",
                json!({
                    "error": crate::i18n::tr("未找到已安装的活侠传，请使用“选择游戏目录”手动指定。")
                }),
            ),
            Ok(paths) => self.game_candidates = paths,
            Err(error) => self.result::<Value>("一键定位游戏目录", Err(error)),
        }
    }
    fn start_setup(&mut self, archive: Option<PathBuf>) {
        if self.job.is_some() {
            self.report_open = true;
            return;
        }
        let game = self.game.clone();
        let runtime = self.runtime.clone();
        self.refresh_after_job = true;
        self.background_progress("一键配置游戏", move |tx| {
            crate::game_setup::configure(&game, &runtime, archive.as_deref(), |message, done| {
                let _ = tx.send(JobEvent::Progress(
                    json!({"message":message,"done":done,"total":4}),
                ));
            })
        });
    }
    fn show_game(&mut self, ui: &mut egui::Ui, project: &Project) {
        ui.horizontal_wrapped(|ui| {
            ui.strong(crate::i18n::tr("编辑器更新"));
            ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
            if ui
                .add_enabled(
                    self.job.is_none(),
                    egui::Button::new(crate::i18n::tr("检查更新")),
                )
                .clicked()
            {
                self.update_check_requested = true;
            }
        });
        ui.separator();
        use crate::i18n::tr;
        if !cfg!(windows) {
            ui.group(|ui| {
                ui.strong(tr("请在 Windows 版中配置游戏和管理 MOD"));
                ui.label(tr(
                    "Mac 可编辑、预览和导出作品；将导出的 .lommod 带到 Windows 安装。",
                ));
            });
            return;
        }
        self.show_game_controls(ui, project);
    }
    fn show_game_controls(&mut self, ui: &mut egui::Ui, project: &Project) {
        use crate::i18n::tr;
        if !self.game_loaded {
            self.refresh_game();
        }
        let idle = self.job.is_none();
        let has_game = !self.game.as_os_str().is_empty();
        let ready = self.game_ready();
        ui.group(|ui| {
            ui.set_min_width((ui.available_width() - 20.0).max(0.0));
            ui.strong(tr("游戏配置"));
            if has_game {
                ui.add(
                    egui::Label::new(self.game.display().to_string())
                        .wrap()
                        .selectable(true),
                );
                if let Ok(architecture) = game_tools::game_architecture(&self.game) {
                    ui.label(format!("{}: {architecture}", tr("游戏架构")));
                }
            } else {
                ui.label(tr("尚未选择游戏目录"));
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(idle, egui::Button::new(tr("一键定位游戏目录")))
                    .clicked()
                {
                    self.locate_game();
                }
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(tr(if has_game {
                            "更换游戏目录"
                        } else {
                            "选择游戏目录"
                        })),
                    )
                    .clicked()
                {
                    self.choose_game();
                }
                if has_game
                    && ui
                        .add_enabled(idle, egui::Button::new(tr("重新检查")))
                        .clicked()
                {
                    self.refresh_game();
                }
            });
            if !self.game_candidates.is_empty() {
                ui.label(tr("找到多个游戏目录，请选择要使用的目录："));
                for path in self.game_candidates.clone() {
                    if ui
                        .add_enabled(idle, egui::Button::new(path.display().to_string()).wrap())
                        .clicked()
                    {
                        self.select_game(path);
                    }
                }
            }
        });
        ui.add_space(10.0);
        if has_game {
            ui.group(|ui| {
                if ready {
                    ui.strong(tr("配置就绪，可以安装 MOD 或返回剧情试玩"));
                    if self
                        .installation
                        .as_ref()
                        .is_some_and(|r| r["healthy"] != true)
                    {
                        ui.label(tr("有提醒事项，可查看安装检查结果。"));
                    }
                } else {
                    ui.strong(tr("下一步：完成游戏配置"));
                    ui.label(tr(
                        "自动安装匹配的加载器、配置 Steam 启动并安装 MOD 支持组件。",
                    ));
                    if !self.runtime_available() {
                        ui.colored_label(
                            egui::Color32::from_rgb(179, 55, 49),
                            tr("安装包缺少宿主文件，请在高级维护中选择完整的宿主目录。"),
                        );
                    }
                    if let Some(error) =
                        self.installation.as_ref().and_then(|r| r["error"].as_str())
                    {
                        ui.colored_label(egui::Color32::from_rgb(179, 55, 49), error);
                    }
                    if ui
                        .add_enabled(
                            idle && self.runtime_available(),
                            egui::Button::new(tr("一键配置游戏"))
                                .fill(ui.visuals().selection.bg_fill),
                        )
                        .clicked()
                    {
                        self.start_setup(None);
                    }
                }
                if ready
                    && ui
                        .add_enabled(
                            idle && self.runtime_available(),
                            egui::Button::new(tr("重新配置游戏环境")),
                        )
                        .clicked()
                {
                    self.start_setup(None);
                }
                if ui
                    .add_enabled(idle, egui::Button::new(tr("查看安装检查结果")))
                    .clicked()
                {
                    self.refresh_game();
                    if let Some(report) = self.installation.clone() {
                        self.present("安装检查", report);
                    }
                }
            });
        }
        ui.add_space(16.0);
        ui.heading(tr("已安装 MOD"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(idle && ready, egui::Button::new(tr("安装当前作品")))
                .clicked()
            {
                self.result(
                    "安装当前作品",
                    crate::game_setup::install_project(&self.game, project),
                );
                self.refresh_game();
            }
            if ui
                .add_enabled(idle && ready, egui::Button::new(tr("安装 .lommod 文件…")))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("MOD", &["lommod"])
                    .pick_file()
                {
                    self.result("导入 MOD", game_tools::install_mod(&self.game, &path, true));
                    self.refresh_game();
                }
            }
            if ui
                .add_enabled(idle && has_game, egui::Button::new(tr("刷新 MOD 列表")))
                .clicked()
            {
                self.refresh_game();
            }
        });
        if !ready {
            ui.small(tr("完成上方游戏配置后即可安装作品。"));
        }
        if let Some(error) = &self.mods_error {
            ui.colored_label(egui::Color32::from_rgb(179, 55, 49), error);
        } else if has_game && self.mods.is_empty() {
            ui.label(tr("还没有安装 MOD。"));
        }
        // This list is persistent view state, never the last operation's report.
        for item in self.mods.clone() {
            let Some(path) = item["path"].as_str() else {
                continue;
            };
            let enabled = item["enabled"] == true;
            let name = item["manifest"]["name"].as_str().unwrap_or(path);
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(name);
                    ui.label(tr(if enabled { "已启用" } else { "已停用" }));
                    if ui
                        .add_enabled(
                            idle && (enabled || item["error"].is_null()),
                            egui::Button::new(tr(if enabled { "停用" } else { "启用" })),
                        )
                        .clicked()
                    {
                        self.result(
                            "更改 MOD 启用状态",
                            game_tools::set_enabled(
                                &self.game,
                                std::path::Path::new(path),
                                !enabled,
                            ),
                        );
                        self.refresh_game();
                    }
                });
                if let Some(error) = item["error"].as_str() {
                    ui.colored_label(egui::Color32::from_rgb(179, 55, 49), error);
                }
            });
        }
        ui.add_space(16.0);
        ui.collapsing(tr("高级维护"), |ui| {
            ui.add_enabled_ui(idle, |ui| {
                ui.label(self.runtime.display().to_string());
                if ui.button(tr("选择 C# 宿主所在目录")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        if !game_tools::HOST_FILES.iter().all(|name| path.join(name).is_file()) {
                            self.present("选择 C# 宿主所在目录", json!({"error":"所选目录必须同时包含 MortalModHost.dll 和 NVorbis.dll"}));
                        } else {
                            self.runtime = path;
                            self.save_setting("rust_runtime_dir", json!(self.runtime));
                            self.refresh_game();
                        }
                    }
                }
                ui.add_enabled_ui(has_game, |ui| {
                    if ui.add_enabled(self.runtime_available(), egui::Button::new(tr("重新配置游戏"))).clicked() {
                        self.start_setup(None);
                    }
                    if ui.add_enabled(self.runtime_available(), egui::Button::new(tr("使用官方离线压缩包配置…"))).clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("ZIP", &["zip"]).pick_file() {
                            self.start_setup(Some(path));
                        }
                    }
                    if let Ok(architecture) = game_tools::game_architecture(&self.game) {
                        if let Ok(distribution) = game_tools::bepinex_distribution(&architecture) {
                            ui.hyperlink_to(tr("下载官方离线压缩包"), distribution.url);
                        }
                    }
                    if ui.button(tr("修复宿主安装")).clicked() {
                        self.result("修复宿主安装", game_tools::apply_installation_doctor_fixes(&self.game, &self.runtime));
                        self.refresh_game();
                    }
                    if ui.button(tr("修复 Steam 启动")).clicked() {
                        self.result("修复 Steam 启动", steam_launch_fix(&self.game));
                        self.refresh_game();
                    }
                    if ui.button(tr("安装 / 更新 C# 宿主")).clicked() {
                        self.result("安装 / 更新 C# 宿主", (|| {
                            anyhow::ensure!(!game_tools::is_game_running()?, "请先退出游戏，再更新宿主");
                            game_tools::install_runtime(&self.game, &self.runtime)
                        })());
                        self.refresh_game();
                    }
                    let rollback = game_tools::plugin_dir(&self.game).join(".runtime_rollback/previous.json").is_file();
                    if ui.add_enabled(rollback, egui::Button::new(tr("恢复上一版 C# 宿主"))).clicked() {
                        self.result("恢复上一版 C# 宿主", (|| {
                            anyhow::ensure!(!game_tools::is_game_running()?, "请先退出游戏，再恢复宿主");
                            game_tools::restore_runtime(&self.game)
                        })());
                        self.refresh_game();
                    }
                });
            });
        });
        ui.collapsing(tr("剧情已读记录"), |ui| {
            ui.add_enabled_ui(idle && has_game, |ui| {
                if ui
                    .button(crate::i18n::tr("选择要重置本作品已读记录的存档"))
                    .clicked()
                {
                    if let Some(paths) = rfd::FileDialog::new()
                        .set_title("选择 Save_universe.dat；将自动保留原始备份")
                        .add_filter("Universe", &["dat"])
                        .pick_files()
                    {
                        self.read_reset = paths;
                    }
                }
                if !self.read_reset.is_empty() {
                    ui.label(
                        "只清理当前作品及临时试玩包的已读文本记录，保留其它存档内容。先建立备份。",
                    );
                    for path in &self.read_reset {
                        ui.label(path.display().to_string());
                    }
                    if ui
                        .button(crate::i18n::tr("确认备份并重置已读记录"))
                        .clicked()
                    {
                        let result = (|| {
                            let id = project.manifest["id"]
                                .as_str()
                                .ok_or_else(|| anyhow::anyhow!("作品 ID 未填写"))?;
                            let ids = vec![game_tools::PREVIEW_ID.to_owned()];
                            let keys = std::collections::BTreeMap::from([
                                (
                                    id.to_owned(),
                                    game_tools::build_story_read_keys(id, &project.stories)?,
                                ),
                                (
                                    game_tools::PREVIEW_ID.to_owned(),
                                    game_tools::build_story_read_keys(
                                        game_tools::PREVIEW_ID,
                                        &project.stories,
                                    )?,
                                ),
                            ]);
                            game_tools::reset_story_read_state(id, &self.read_reset, &ids, &keys)
                        })();
                        self.result("重置已读记录", result);
                        self.read_reset.clear();
                    }
                }
            });
        });
    }
    pub fn show(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        match self.page {
            ToolPage::Editing => {}
            ToolPage::Previews => self.show_previews(ui),
            ToolPage::Library => self.show_library(ui, project),
            ToolPage::Game => self.show_game(ui, project),
            ToolPage::Release => self.show_release(ui, project),
            ToolPage::Maintenance => self.show_maintenance(ui, project),
        }
        self.show_latest_result(ui, project);
    }
    fn show_previews(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(crate::i18n::tr("游戏预览素材"))
            .default_open(true)
            .show(ui, |ui| {
                ui.label(crate::i18n::tr(
                    "从自己的游戏中提取立绘、表情和背景，完成后自动用于右侧预览。",
                ));
                if !self.game.as_os_str().is_empty() {
                    ui.label(self.game.display().to_string());
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            self.job.is_none(),
                            egui::Button::new(crate::i18n::tr("一键提取游戏资产")),
                        )
                        .clicked()
                    {
                        self.start_extraction();
                    }
                    if ui
                        .add_enabled(
                            self.job.is_none(),
                            egui::Button::new(crate::i18n::tr("更换游戏目录")),
                        )
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("选择包含 Mortal_Data 的游戏目录")
                            .pick_folder()
                        {
                            match crate::asset_extraction::validate_game(&path) {
                                Ok(()) => {
                                    self.game = path;
                                    self.game_loaded = false;
                                    self.save_setting("game_dir", json!(self.game));
                                }
                                Err(e) => self.result::<Value>("选择游戏目录", Err(e)),
                            }
                        }
                    }
                });
                ui.small(crate::i18n::tr(
                    "首次选择游戏目录即可。只读取游戏文件，图片保存到本地素材缓存。",
                ));
            });
    }
    fn show_release(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        egui::CollapsingHeader::new(crate::i18n::tr("发布体检与打包"))
            .default_open(true)
            .show(ui, |ui| {
                ui.strong(crate::i18n::tr("1. 检查作品"));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(crate::i18n::tr("编辑体检")).clicked() {
                        self.present(
                            "编辑体检",
                            json!(release::run_preflight(
                                project,
                                release::Profile::Editing,
                                "1.1.3"
                            )),
                        );
                    }
                    if ui.button(crate::i18n::tr("发布体检")).clicked() {
                        self.present(
                            "发布体检",
                            json!(release::run_preflight(
                                project,
                                release::Profile::Release,
                                "1.1.3"
                            )),
                        );
                    }
                    if ui.button(crate::i18n::tr("生成安全修复建议")).clicked() {
                        let fixes = release::propose_safe_fixes(&project.stories);
                        self.present("生成安全修复建议", json!({"changes":fixes.changes}));
                        self.fixes = Some(fixes);
                    }
                });
                if let Some(fixes) = &self.fixes {
                    let changes = fixes.changes.clone();
                    let before = fixes.before.clone();
                    let after = fixes.after.clone();
                    for change in changes {
                        ui.label(change);
                    }
                    if ui
                        .add_enabled(
                            project.stories == before,
                            egui::Button::new(crate::i18n::tr("应用以上安全修复（可撤销）")),
                        )
                        .clicked()
                    {
                        project.stories = after;
                        self.fixes = None;
                        self.present(
                            "应用安全修复",
                            json!({"message":"安全修复已应用，可在编辑菜单中撤销"}),
                        );
                    }
                }
                ui.separator();
                ui.strong(crate::i18n::tr("2. 导出作品"));
                ui.small(crate::i18n::tr(
                    "导出单个 MOD 文件，或生成包含校验和与说明的发布目录。导出时会再次检查。",
                ));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(crate::i18n::tr("导出 .lommod…")).clicked() {
                        self.export_requested = true;
                    }
                    if ui.button(crate::i18n::tr("生成发布目录")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("发布包、校验和与安装说明的输出目录")
                            .pick_folder()
                        {
                            self.result(
                                "生成发布目录",
                                release::build_release_directory(project, &path, "1.1.3"),
                            );
                        }
                    }
                });
            });
        ui.collapsing(crate::i18n::tr("统计与配音覆盖"), |ui| {
            if ui
                .button(crate::i18n::tr("统计类型、引用素材和未使用资源"))
                .clicked()
            {
                let assets = project.assets.keys().cloned().collect::<Vec<_>>();
                self.present(
                    "项目统计",
                    release::calculate_project_statistics(&project.stories, Some(&assets)),
                );
            }
            if ui
                .button(crate::i18n::tr("按章节和人物检查缺少配音的对白"))
                .clicked()
            {
                self.present(
                    "配音覆盖检查",
                    release::calculate_voice_coverage(&project.stories),
                );
            }
        });
        ui.collapsing(crate::i18n::tr("包检查与水印检测"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    self.ffmpeg
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "FFmpeg: PATH".into()),
                );
                if ui.button(crate::i18n::tr("选择 FFmpeg 程序")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("选择 ffmpeg 可执行程序")
                        .pick_file()
                    {
                        self.ffmpeg = Some(path.clone());
                        self.save_setting("ffmpeg_path", json!(path));
                    }
                }
            });
            if ui.button(crate::i18n::tr("检查 .lommod 包")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Mod", &["lommod"])
                    .pick_file()
                {
                    self.package_report = Value::Null;
                    match inspect_package(&path) {
                        Ok(report) => {
                            self.present("检查 .lommod 包", report.clone());
                            self.package_report = report;
                            self.package_entry.clear();
                        }
                        Err(e) => self.result::<Value>("操作未完成", Err(e)),
                    }
                }
            }
            if self.package_report.is_object() {
                ui.label(crate::i18n::tr(if self.package_report["ok"] == true {
                    "检查通过"
                } else {
                    "检查未通过"
                }));
                if let Some(error) = self.package_report["validation_error"].as_str() {
                    ui.colored_label(egui::Color32::from_rgb(179, 55, 49), error);
                }
                ui.collapsing("SHA-256", |ui| {
                    ui.monospace(self.package_report["package_sha256"].as_str().unwrap_or(""));
                });
                let entries = self.package_report["entries"].as_array().unwrap();
                if self.package_entry.is_empty() {
                    self.package_entry = entries
                        .iter()
                        .find(|entry| entry["path"] == "manifest.json")
                        .or_else(|| entries.first())
                        .and_then(|entry| entry["path"].as_str())
                        .unwrap_or("")
                        .into();
                }
                egui::ComboBox::from_id_salt("package-entry")
                    .truncate()
                    .width(ui.available_width())
                    .selected_text(&self.package_entry)
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        for e in entries {
                            if let Some(name) = e["path"].as_str() {
                                ui.selectable_value(&mut self.package_entry, name.into(), name);
                            }
                        }
                    });
                if let Some(entry) = entries.iter().find(|e| e["path"] == self.package_entry) {
                    ui.label(format!("{} bytes", entry["size"]));
                    ui.collapsing(crate::i18n::tr("文件校验值"), |ui| {
                        ui.monospace(entry["sha256"].as_str().unwrap_or(""));
                    });
                    if let Some(preview) = entry["preview"].as_str() {
                        let mut preview = preview;
                        egui::ScrollArea::both()
                            .id_salt("entry-preview")
                            .max_height(320.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut preview)
                                        .code_editor()
                                        .desired_width(f32::INFINITY),
                                );
                            });
                        if entry["preview_truncated"] == true {
                            ui.label(crate::i18n::tr("预览已截断"));
                        }
                    } else {
                        ui.label(crate::i18n::tr("二进制文件，仅显示大小与哈希"));
                    }
                }
                if !self.package_report["preflight"].is_null() {
                    ui.collapsing(crate::i18n::tr("发布检查"), |ui| {
                        show_report(ui, &self.package_report["preflight"], 0);
                    });
                    ui.collapsing(crate::i18n::tr("未使用资源"), |ui| {
                        show_report(ui, &self.package_report["unused_assets"], 0);
                    });
                }
            }
            if ui
                .add_enabled(
                    self.job.is_none(),
                    egui::Button::new(crate::i18n::tr("从图片检测水印")),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("图片", &["png", "jpg", "jpeg"])
                    .pick_file()
                {
                    self.background("从图片检测水印", move || {
                        lom_core::watermark::detect_image(
                            &path,
                            lom_core::watermark::DEFAULT_SCALE_FACTORS,
                        )
                    });
                }
            }
            if ui
                .add_enabled(
                    self.job.is_none(),
                    egui::Button::new(crate::i18n::tr("从视频检测水印（FFmpeg）")),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("视频", &["mp4", "mov", "mkv", "webm", "avi"])
                    .pick_file()
                {
                    let ffmpeg = self.ffmpeg.clone();
                    self.background("从视频检测水印", move || {
                        lom_core::watermark::detect_video(
                            &path,
                            ffmpeg.as_deref(),
                            2.0,
                            12,
                            lom_core::watermark::DEFAULT_SCALE_FACTORS,
                        )
                    });
                }
            }
        });
    }
    fn show_library(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        egui::CollapsingHeader::new(crate::i18n::tr("共享内容库与 .lomcontent"))
            .default_open(true)
            .show(ui, |ui| {
                ui.label(self.library.display().to_string());
                if ui.button(crate::i18n::tr("选择内容库目录")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.library = path;
                    }
                }
                if ui.button(crate::i18n::tr("列出内容")).clicked() {
                    self.result("列出内容", content_library::list_contents(&self.library));
                }
                if ui.button(crate::i18n::tr("检查待导入的内容包")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("内容包", &["lomcontent"])
                        .pick_file()
                    {
                        match content_library::inspect_content_pack(&self.library, &path) {
                            Ok(info) => {
                                self.present("检查待导入的内容包", json!(info));
                                self.pending_info = Some(info);
                                self.pending_pack = Some(path);
                            }
                            Err(e) => self.result::<Value>("操作未完成", Err(e)),
                        }
                    }
                }
                if let Some(info) = &self.pending_info {
                    ui.label(format!(
                        "{} · {} · 作者：{} · 许可：{}",
                        info.name, info.version, info.author, info.license
                    ));
                    if !info.missing_dependencies.is_empty() {
                        ui.label(format!(
                            "缺少依赖：{}",
                            info.missing_dependencies.join("、")
                        ));
                    }
                    let permitted = info.collision_type.is_none();
                    if !permitted {
                        ui.label(crate::i18n::tr(
                            "内容库已有同 ID 内容；请换用空目录或更改来源内容 ID，避免覆盖。",
                        ));
                    }
                    if ui
                        .add_enabled(
                            permitted,
                            egui::Button::new(crate::i18n::tr("导入此内容包")),
                        )
                        .clicked()
                    {
                        if let Some(path) = self.pending_pack.take() {
                            self.result(
                                "导入此内容包",
                                content_library::import_content_pack(&self.library, &path),
                            );
                            self.pending_info = None;
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(format!("{} *", crate::i18n::tr("内容 ID")));
                    ui.text_edit_singleline(&mut self.content_id);
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button(crate::i18n::tr("载入共享默认信息")).clicked() {
                        match content_library::content_pack_defaults(
                            &self.library,
                            &self.content_id,
                        ) {
                            Ok(v) => {
                                self.content_version =
                                    v["version"].as_str().unwrap_or("1.0.0").into();
                                self.author = v["author"].as_str().unwrap_or("").into();
                                self.license = v["license"].as_str().unwrap_or("").into();
                                self.dependencies = v["dependencies"]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(Value::as_str)
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    })
                                    .unwrap_or_default();
                                self.present(
                                    "载入共享默认信息",
                                    json!({"message":"版本、作者、许可和依赖信息已填入表单"}),
                                );
                            }
                            Err(e) => self.result::<Value>("操作未完成", Err(e)),
                        }
                    }
                    if ui.button(crate::i18n::tr("复制此内容到当前项目")).clicked() {
                        self.result(
                            "复制此内容到当前项目",
                            copy_content_to_project(&self.library, &self.content_id, project),
                        );
                    }
                });
                for (label, value) in [
                    ("版本", &mut self.content_version),
                    ("作者", &mut self.author),
                    ("许可", &mut self.license),
                    ("依赖 ID（逗号分隔）", &mut self.dependencies),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{}{}",
                            crate::i18n::tr(label),
                            if label == "依赖 ID（逗号分隔）" {
                                ""
                            } else {
                                " *"
                            }
                        ));
                        ui.text_edit_singleline(value);
                    });
                }
                if ui.button(crate::i18n::tr("导出共享内容包")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("{}.lomcontent", self.content_id))
                        .save_file()
                    {
                        let deps = json!(self
                            .dependencies
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>());
                        self.result(
                            "导出共享内容包",
                            content_library::export_content_pack(
                                &self.library,
                                &path,
                                &self.content_id,
                                &self.content_version,
                                &self.author,
                                &self.license,
                                &deps,
                            ),
                        );
                    }
                }
            });
    }
    fn show_maintenance(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        egui::CollapsingHeader::new(crate::i18n::tr("迁移与诊断")).default_open(true).show(ui,|ui|{
            ui.label(crate::i18n::tr("迁移遵循原版本规则：无法可靠升级的旧游戏节点会明确拒绝。写入前保留原始字节备份。"));
            if ui.button(crate::i18n::tr("检查并迁移所选 JSON")).clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("JSON",&["json"]).pick_file(){let kind=match path.file_name().and_then(|s|s.to_str()){Some("manifest.json")=>"manifest",Some("content.json")=>"content",_=>"story"};self.result("迁移旧项目", lom_core::migration::migrate_json_file(&path,kind,None).map(|(result,backup)|json!({"changed":result.changed,"steps":result.steps,"backup":backup})));}}
            if ui.button(crate::i18n::tr("导出脱敏诊断 ZIP")).clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("lom-editor-diagnostics.zip").save_file(){let issues=release::run_preflight(project,release::Profile::Editing,"1.1.3");self.result("导出诊断资料", release::export_diagnostic_bundle(&path,project,&issues,&self.diagnostic_options()));}}
        });
    }
    fn show_latest_result(&mut self, ui: &mut egui::Ui, project: &Project) {
        if self.job.is_some() || !self.report_title.is_empty() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if self.job.is_some() {
                    ui.spinner();
                }
                ui.strong(crate::i18n::tr(&self.report_title));
                if ui.button(crate::i18n::tr("查看进度或结果")).clicked() {
                    if self.job.is_some() {
                        self.report_title = self.job_title.clone();
                    }
                    self.report_open = true;
                }
            });
            let mut locations = Vec::new();
            collect_locations(&self.report, project, &mut locations);
            if !locations.is_empty() {
                ui.heading(crate::i18n::tr("定位到剧情"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.result_filter)
                        .hint_text(crate::i18n::tr("筛选章节、节点或内容")),
                );
                for (sid, nid, label) in locations {
                    if !self.result_filter.is_empty()
                        && !format!("{sid} {nid:?} {label}")
                            .to_lowercase()
                            .contains(&self.result_filter.to_lowercase())
                    {
                        continue;
                    }
                    if ui
                        .link(format!(
                            "{sid}/{} · {label}",
                            nid.as_deref().unwrap_or("作品设置")
                        ))
                        .clicked()
                    {
                        self.location = Some((sid, nid));
                    }
                }
            }
            if let Some(error) = self.report["error"].as_str() {
                ui.colored_label(egui::Color32::from_rgb(179, 55, 49), error);
            } else if self.report.as_array().is_some_and(|a| a.is_empty()) {
                ui.label(crate::i18n::tr("未发现问题"));
            }
            egui::CollapsingHeader::new(crate::i18n::tr("完整报告"))
                .default_open(false)
                .show(ui, |ui| show_report(ui, &self.report, 0));
        }
    }
}
impl Drop for ToolsPanel {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub(crate) fn steam_launch_fix(root: &std::path::Path) -> anyhow::Result<Vec<String>> {
    #[cfg(windows)]
    {
        let architecture = game_tools::game_architecture(root)?;
        let windows = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or_else(|| anyhow::anyhow!("缺少 SystemRoot"))?,
        );
        let version = game_tools::system_version_dll(&windows, &architecture)?;
        let temp = lom_core::workdir::create()?;
        let doorstop = temp.path().join(format!("win-{architecture}-doorstop.dll"));
        std::fs::write(&doorstop, game_tools::bundled_doorstop(&architecture)?)?;
        game_tools::apply_steam_launch_fix(root, Some(&doorstop), Some(&version))
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        anyhow::bail!("Steam 游戏启动修复仅适用于 Windows")
    }
}
fn settings_path() -> PathBuf {
    lom_core::content::default_repository_root()
        .parent()
        .unwrap()
        .join("settings.json")
}
fn prepare_preview(project: &Project, script: &str, node: &str) -> anyhow::Result<Project> {
    let mut preview = project.clone();
    preview.scenario = None; // F5 previews presentation only; it must not advance authored tasks.
    let story = preview
        .stories
        .get_mut(script)
        .ok_or_else(|| anyhow::anyhow!("章节不存在"))?;
    anyhow::ensure!(
        story["nodes"]
            .as_array()
            .is_some_and(|ns| ns.iter().any(|n| n["id"] == node)),
        "节点不存在"
    );
    let prelude = crate::preview::build_playtest_prelude(story, node);
    story["start"] = json!(prelude
        .first()
        .and_then(|n| n["id"].as_str())
        .unwrap_or(node));
    story["nodes"].as_array_mut().unwrap().extend(prelude);
    preview.manifest = json!({"format":3,"package_format":3,"story_schema":2,"content_schema":1,"id":game_tools::PREVIEW_ID,"campaign_id":game_tools::PREVIEW_ID,"name":format!("编辑器临时试玩：{script}"),"version":"0.0.0-preview","author":"lom_modkit","description":format!("从 {script}/{node} 开始的临时测试包"),"entry":script,"campaign":{"new_game":true,"disable_official_events":true}});
    Ok(preview)
}
fn copy_content_to_project(
    root: &std::path::Path,
    id: &str,
    project: &mut Project,
) -> anyhow::Result<Value> {
    let record = content_library::get_content(root, id)?;
    for (path, bytes) in &project.assets {
        if path.ends_with("/content.json") {
            let existing: Value = serde_json::from_slice(bytes)?;
            anyhow::ensure!(
                existing["id"] != id || existing["type"] == record.content_type,
                "当前项目已有不同类型的同 ID 内容：{id}"
            );
        }
    }
    let files = lom_core::content::listed_content_files(&record.metadata);
    let prefix = lom_core::content::package_content_dir(&record.content_type, id)?;
    let mut additions = std::collections::BTreeMap::new();
    additions.insert(
        format!("{prefix}/content.json"),
        lom_core::stable_json(&record.metadata)?,
    );
    for file in files {
        let path = record.folder.join(&file);
        lom_core::package::resolve_confined_file(&record.folder, &path)?;
        additions.insert(format!("{prefix}/{file}"), std::fs::read(path)?);
    }
    for (name, bytes) in &additions {
        anyhow::ensure!(
            project.assets.get(name).is_none_or(|old| old == bytes),
            "项目已有不同内容：{name}"
        );
    }
    project.assets.extend(additions);
    Ok(json!({"added":id,"type":record.content_type}))
}
fn inspect_package(path: &std::path::Path) -> anyhow::Result<Value> {
    let mut report = lom_core::package::inspect_package(path)?;
    // Preserve release checks for valid packages while allowing damaged packages
    // to be inspected without opening them as authoring projects.
    if report["ok"] == true {
        let project = Project::open(path)?;
        let assets = project.assets.keys().cloned().collect::<Vec<_>>();
        report["preflight"] = serde_json::to_value(release::run_preflight(
            &project,
            release::Profile::Release,
            "1.1.3",
        ))?;
        report["unused_assets"] =
            serde_json::to_value(release::unused_asset_paths(&project.stories, &assets))?;
    }
    Ok(report)
}
fn collect_locations(
    value: &Value,
    project: &Project,
    out: &mut Vec<(String, Option<String>, String)>,
) {
    if let Some(o) = value.as_object() {
        let sid = o
            .get("story_id")
            .or_else(|| o.get("story"))
            .and_then(Value::as_str);
        let nid = o
            .get("node_id")
            .or_else(|| o.get("node"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        if let Some(sid) = sid {
            let label = o
                .get("message")
                .or_else(|| o.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if sid.is_empty() && nid.is_none() {
                out.push((String::new(), None, label));
            } else if let Some(story) = project.stories.get(sid) {
                if nid.is_none_or(|n| {
                    story["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|v| v["id"] == n)
                }) {
                    out.push((sid.into(), nid.map(str::to_owned), label));
                }
            }
        }
        for v in o.values() {
            collect_locations(v, project, out);
        }
    } else if let Some(a) = value.as_array() {
        for v in a {
            collect_locations(v, project, out);
        }
    }
}
fn show_report(ui: &mut egui::Ui, value: &Value, depth: usize) {
    crate::tool_report::show(ui, value, depth);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_game_tools_work_with_an_unusable_system_temp() {
        const SANDBOX: &str = "LOM_MODKIT_TEMP_REGRESSION_ROOT";
        let Some(root) = std::env::var_os(SANDBOX).map(PathBuf::from) else {
            // Isolate process-wide TEMP settings from the rest of the test suite.
            let dir = tempfile::tempdir().unwrap();
            let blocked = dir.path().join("unusable-system-temp");
            std::fs::write(&blocked, b"not a directory").unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tools_panel::tests::windows_game_tools_work_with_an_unusable_system_temp",
                    "--nocapture",
                ])
                .env(SANDBOX, dir.path())
                .env("TEMP", &blocked)
                .env("TMP", &blocked)
                .env("LOCALAPPDATA", dir.path().join("user-cache"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let error =
            tempfile::NamedTempFile::new().expect_err("system TEMP must actually be unusable");
        println!("system TEMP failure: {error}");
        let workspace = lom_core::workdir::create().unwrap();
        assert!(workspace.path().starts_with(root.join("user-cache")));
        let game = root.join("synthetic-game");
        std::fs::create_dir_all(game.join("Mortal_Data/Managed")).unwrap();
        let mut pe = vec![0u8; 70];
        pe[..2].copy_from_slice(b"MZ");
        pe[60..64].copy_from_slice(&64u32.to_le_bytes());
        pe[64..68].copy_from_slice(b"PE\0\0");
        pe[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        std::fs::write(game.join("Mortal.exe"), pe).unwrap();
        for name in game_tools::BEPINEX_FILES {
            let path = game.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"synthetic loader dependency").unwrap();
        }
        let runtime = root.join("runtime-fixture");
        std::fs::create_dir(&runtime).unwrap();
        for name in game_tools::HOST_FILES {
            std::fs::write(runtime.join(name), b"synthetic host").unwrap();
        }
        assert_eq!(
            game_tools::install_runtime(&game, &runtime).unwrap()["changed"],
            true
        );
        assert_eq!(
            game_tools::install_runtime(&game, &runtime).unwrap()["changed"],
            false
        );
        steam_launch_fix(&game).unwrap();
        assert!(game_tools::steam_launch_fix_applied(&game).unwrap());
        let preview = prepare_preview(&Project::new(), "main", "say1").unwrap();
        let package = root.join("preview.lommod");
        preview.export(&package).unwrap();
        let installed = game_tools::install_mod(&game, &package, true).unwrap();
        assert!(installed.is_file());
        game_tools::request_preview(&game, "main", "say1").unwrap();
        println!("user-cache workspace, host install/no-op, Steam fix, preview export, package validation/install and preview request succeeded without system TEMP");
    }
    #[test]
    fn mod_rows_survive_other_reports_and_refresh_after_toggle() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        std::fs::create_dir_all(game.join("Mortal_Data/Managed")).unwrap();
        std::fs::write(game.join("Mortal.exe"), b"fixture").unwrap();
        let project = Project::new();
        crate::game_setup::install_project(&game, &project).unwrap();
        let mut panel = ToolsPanel::default();
        panel.game = game;
        panel.refresh_game();
        assert_eq!(panel.mods.len(), 1);
        let row = panel.mods[0].clone();
        panel.present("项目统计", json!({"nodes":2}));
        assert_eq!(panel.mods[0], row);
        let path = std::path::Path::new(row["path"].as_str().unwrap());
        game_tools::set_enabled(&panel.game, path, false).unwrap();
        panel.refresh_game();
        assert_eq!(panel.mods.len(), 1);
        assert_eq!(panel.mods[0]["enabled"], false);
        assert_eq!(panel.report_title, "项目统计");
        assert_eq!(panel.report["nodes"], 2);
    }
    #[test]
    fn game_readiness_requires_complete_configuration_and_distinguishes_warnings() {
        let mut panel = ToolsPanel::default();
        assert!(!panel.game_ready());
        panel.installation =
            Some(json!({"steam_ready":true,"findings":[{"severity":"ok","fixable":false}]}));
        assert!(panel.game_ready());
        panel.installation.as_mut().unwrap()["findings"][0] =
            json!({"severity":"warning","fixable":false});
        assert!(
            panel.game_ready(),
            "nonfixable warnings must remain visible without an endless setup loop"
        );
        panel.installation.as_mut().unwrap()["findings"][0]["fixable"] = json!(true);
        assert!(!panel.game_ready());
        panel.installation.as_mut().unwrap()["findings"][0] =
            json!({"severity":"error","fixable":false});
        assert!(!panel.game_ready());
        panel.installation.as_mut().unwrap()["findings"][0] =
            json!({"severity":"ok","fixable":false});
        panel.installation.as_mut().unwrap()["steam_ready"] = json!(false);
        assert!(!panel.game_ready());
    }
    #[test]
    fn failed_setup_reloads_current_installation_without_overwriting_its_error() {
        let mut panel = ToolsPanel::default();
        panel.game = PathBuf::new();
        panel.mods = vec![json!({"path":"old-game/mod.lommod"})];
        panel.refresh_after_job = true;
        let (tx, rx) = mpsc::channel();
        panel.job = Some(rx);
        panel.job_title = "一键配置游戏".into();
        tx.send(JobEvent::Finished(Err("fixture download failure".into())))
            .unwrap();
        panel.poll(&egui::Context::default());
        assert_eq!(panel.report["error"], "fixture download failure");
        assert!(panel.mods.is_empty());
        assert!(panel.installation.is_none());
        assert!(!panel.refresh_after_job);
        assert!(panel.report_open);
    }
    #[test]
    fn background_completion_reopens_result_after_progress_was_dismissed() {
        let ctx = egui::Context::default();
        let mut panel = ToolsPanel::default();
        let (tx, rx) = mpsc::channel();
        panel.job = Some(rx);
        panel.job_title = "一键提取游戏资产".into();
        panel.report_open = false;
        tx.send(JobEvent::Progress(json!({"done":10,"total":30})))
            .unwrap();
        panel.poll(&ctx);
        assert_eq!(panel.progress["done"], 10);
        assert!(!panel.report_open);
        // Another tool may have displayed a result while the job was minimized.
        panel.present("项目统计", json!({"nodes":20}));
        panel.report_open = false;
        tx.send(JobEvent::Finished(Ok(
            json!({"exported":29,"failed":1,"output_dir":"fixture-cache"}),
        )))
        .unwrap();
        panel.poll(&ctx);
        assert!(panel.report_open);
        assert!(panel.job.is_none());
        assert_eq!(panel.report_title, "一键提取游戏资产");
        assert_eq!(
            panel.take_preview_library(),
            Some(PathBuf::from("fixture-cache"))
        );
        assert!(panel.take_preview_library().is_none());
    }
    #[test]
    fn failed_jobs_and_disconnections_never_replace_preview_library() {
        let mut panel = ToolsPanel::default();
        let ctx = egui::Context::default();
        let (tx, rx) = mpsc::channel();
        panel.job = Some(rx);
        tx.send(JobEvent::Finished(Err("已取消".into()))).unwrap();
        panel.poll(&ctx);
        assert_eq!(panel.report["error"], "已取消");
        assert!(panel.take_preview_library().is_none());
        let (tx, rx) = mpsc::channel();
        panel.job = Some(rx);
        drop(tx);
        panel.poll(&ctx);
        assert!(panel.report["error"].is_string());
        assert!(panel.take_preview_library().is_none());
    }
    #[test]
    fn report_navigation_resolves_only_existing_targets() {
        let p = Project::new();
        let node = p.stories["main"]["nodes"][0]["id"].as_str().unwrap();
        let report = json!([
            {"story_id":"main","node_id":node,"message":"missing voice"},
            {"story":"main","node":node,"text":"line"},
            {"story_id":"main","message":"chapter issue"},
            {"story_id":"","message":"manifest issue"},
            {"story_id":"missing","node_id":node},
            {"story_id":"main","node_id":"missing"}
        ]);
        let mut result = Vec::new();
        collect_locations(&report, &p, &mut result);
        assert_eq!(
            result,
            vec![
                ("main".into(), Some(node.into()), "missing voice".into()),
                ("main".into(), Some(node.into()), "line".into()),
                ("main".into(), None, "chapter issue".into()),
                (String::new(), None, "manifest issue".into()),
            ]
        );
    }
    #[test]
    fn preview_package_reconstructs_stage_without_mutating_author_project() {
        let mut project = Project::new();
        project.stories.insert("main".into(),json!({"id":"main","story_schema":2,"start":"show1","nodes":[{"id":"show1","type":"show","character":"player","position":"M"},{"id":"move1","type":"move","character":"player","from":"M","to":"L","duration":1},{"id":"end1","type":"end"}]}));
        let before = project.clone();
        let preview = prepare_preview(&project, "main", "move1").unwrap();
        assert_eq!(project, before);
        assert_eq!(preview.manifest["id"], game_tools::PREVIEW_ID);
        let story = &preview.stories["main"];
        assert!(story["start"].as_str().unwrap().starts_with("zz_playtest_"));
        let prelude = story["nodes"].as_array().unwrap().last().unwrap();
        assert_eq!(prelude["type"], "show");
        assert_eq!(prelude["position"], "M");
        assert_eq!(prelude["goto"], "move1");
        lom_core::validate::validate_story(story).unwrap();
    }
    #[test]
    fn content_copy_checks_all_collisions_before_applying_any_asset() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("assets/user/image/test.pic");
        std::fs::create_dir_all(&folder).unwrap();
        let meta = json!({"schema":1,"content_schema":1,"id":"test.pic","type":"image","name":"Test","files":{"main":"main.png"}});
        std::fs::write(
            folder.join("content.json"),
            lom_core::stable_json(&meta).unwrap(),
        )
        .unwrap();
        std::fs::write(folder.join("main.png"), b"fixture image bytes").unwrap();
        let mut p = Project::new();
        p.assets.insert(
            "assets/user/image/test.pic/main.png".into(),
            b"original".to_vec(),
        );
        let before = p.clone();
        assert!(copy_content_to_project(dir.path(), "test.pic", &mut p).is_err());
        assert_eq!(p, before);
        p.assets.clear();
        copy_content_to_project(dir.path(), "test.pic", &mut p).unwrap();
        assert_eq!(p.assets.len(), 2);
    }
}
