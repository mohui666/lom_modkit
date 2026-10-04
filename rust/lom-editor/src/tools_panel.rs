//! Auxiliary native workflows share the same in-memory project as authoring.
use eframe::egui;
use lom_core::{content_library, game_tools, project::Project, release};
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct ToolsPanel {
    report: Value,
    library: PathBuf,
    game: PathBuf,
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
    reference_search: String,
    read_reset: Vec<PathBuf>,
    job: Option<std::sync::mpsc::Receiver<Result<Value, String>>>,
}
impl Default for ToolsPanel {
    fn default() -> Self {
        let settings = lom_core::load_json(settings_path()).unwrap_or(json!({}));
        Self {
            report: Value::Null,
            library: lom_core::content::default_repository_root(),
            game: PathBuf::from(settings["game_dir"].as_str().unwrap_or("")),
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
            reference_search: String::new(),
            read_reset: vec![],
            job: None,
        }
    }
}
impl ToolsPanel {
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
            "请先在创作工具 → Windows 游戏接入中选择游戏目录和 C# 宿主目录"
        );
        let issues = release::run_preflight(project, release::Profile::Editing, "1.1.2");
        anyhow::ensure!(
            !issues.iter().any(|i| i.severity == "error"),
            "项目存在体检错误，请先检查项目"
        );
        let preview = prepare_preview(project, script, node)?;
        let temp = tempfile::tempdir()?;
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
    fn save_setting(&mut self, key: &str, value: Value) {
        let path = settings_path();
        let result = (|| {
            let mut settings = lom_core::load_json(&path).unwrap_or(json!({}));
            anyhow::ensure!(settings.is_object(), "设置文件不是对象");
            settings[key] = value;
            lom_core::project::atomic_write(&path, &lom_core::stable_json(&settings)?)
        })();
        if let Err(e) = result {
            self.result::<Value>(Err(e));
        }
    }
    fn background(&mut self, f: impl FnOnce() -> anyhow::Result<Value> + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
                .map_err(|_| "检测任务意外退出".to_owned())
                .and_then(|r| r.map_err(|e| format!("{e:#}")));
            let _ = tx.send(result);
        });
    }
    fn result<T: serde::Serialize>(&mut self, result: anyhow::Result<T>) {
        self.report = match result {
            Ok(v) => serde_json::to_value(v).unwrap_or(Value::Null),
            Err(e) => json!({"error":format!("{e:#}")}),
        };
    }
    pub fn show(&mut self, ui: &mut egui::Ui, project: &mut Project) {
        ui.collapsing(crate::i18n::tr("节点参考与帮助"), |ui| {
            ui.text_edit_singleline(&mut self.reference_search);
            let reference: Value = serde_json::from_str(include_str!("../data/reference.json"))
                .expect("embedded reference");
            let schema: Value = serde_json::from_str(include_str!("../data/authoring.json"))
                .expect("embedded schema");
            let query = self.reference_search.to_lowercase();
            for (kind, definition) in schema["NODE_SCHEMAS"].as_object().unwrap() {
                let label = crate::i18n::key(&format!("node.{kind}"));
                let help = crate::i18n::key(&format!("help.{kind}"));
                let api = reference["RUNTIME_API"][kind].as_str().unwrap_or("");
                if !query.is_empty()
                    && !format!("{kind} {label} {help} {api}")
                        .to_lowercase()
                        .contains(&query)
                {
                    continue;
                }
                ui.collapsing(format!("{label} · {kind}"), |ui| {
                    ui.label(help);
                    ui.monospace(api);
                    show_report(ui, definition, 0);
                });
            }
            ui.collapsing(crate::i18n::tr("帮助"), |ui| {
                ui.label(crate::i18n::help_text());
            });
        });
        if let Some(receiver) = &self.job {
            match receiver.try_recv() {
                Ok(result) => {
                    self.report = result.unwrap_or_else(|error| json!({"error":error}));
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.report = json!({"error":"检测任务连接已中断"});
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.spinner();
                    ui.label(crate::i18n::tr("正在检测，可继续编辑项目…"));
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        ui.collapsing(crate::i18n::tr("发布体检与打包"), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("编辑体检")).clicked() {
                    self.report = json!(release::run_preflight(
                        project,
                        release::Profile::Editing,
                        "1.1.2"
                    ));
                }
                if ui.button(crate::i18n::tr("发布体检")).clicked() {
                    self.report = json!(release::run_preflight(
                        project,
                        release::Profile::Release,
                        "1.1.2"
                    ));
                }
                if ui.button(crate::i18n::tr("生成安全修复建议")).clicked() {
                    let fixes = release::propose_safe_fixes(&project.stories);
                    self.report = json!({"changes":fixes.changes});
                    self.fixes = Some(fixes);
                }
                if ui.button(crate::i18n::tr("生成发布目录")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("发布包、校验和与安装说明的输出目录")
                        .pick_folder()
                    {
                        self.result(release::build_release_directory(project, &path, "1.1.2"));
                    }
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
                }
            }
        });
        ui.collapsing(crate::i18n::tr("统计与配音覆盖"), |ui| {
            if ui
                .button(crate::i18n::tr("统计类型、引用素材和未使用资源"))
                .clicked()
            {
                let assets = project.assets.keys().cloned().collect::<Vec<_>>();
                self.report =
                    release::calculate_project_statistics(&project.stories, Some(&assets));
            }
            if ui
                .button(crate::i18n::tr("按章节和人物检查缺少配音的对白"))
                .clicked()
            {
                self.report = release::calculate_voice_coverage(&project.stories);
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
                    self.result(inspect_package(&path));
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
                    self.background(move || {
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
                    self.background(move || {
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
        ui.collapsing(crate::i18n::tr("共享内容库与 .lomcontent"), |ui| {
            ui.label(self.library.display().to_string());
            if ui.button(crate::i18n::tr("选择内容库目录")).clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.library = path;
                }
            }
            if ui.button(crate::i18n::tr("列出内容")).clicked() {
                self.result(content_library::list_contents(&self.library));
            }
            if ui.button(crate::i18n::tr("检查待导入的内容包")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("内容包", &["lomcontent"])
                    .pick_file()
                {
                    match content_library::inspect_content_pack(&self.library, &path) {
                        Ok(info) => {
                            self.report = json!(info);
                            self.pending_info = Some(info);
                            self.pending_pack = Some(path);
                        }
                        Err(e) => self.result::<Value>(Err(e)),
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
                        self.result(content_library::import_content_pack(&self.library, &path));
                        self.pending_info = None;
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("内容 ID"));
                ui.text_edit_singleline(&mut self.content_id);
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("载入共享默认信息")).clicked() {
                    match content_library::content_pack_defaults(&self.library, &self.content_id) {
                        Ok(v) => {
                            self.content_version = v["version"].as_str().unwrap_or("1.0.0").into();
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
                        }
                        Err(e) => self.result::<Value>(Err(e)),
                    }
                }
                if ui.button(crate::i18n::tr("复制此内容到当前项目")).clicked() {
                    self.result(copy_content_to_project(
                        &self.library,
                        &self.content_id,
                        project,
                    ));
                }
            });
            for (label, value) in [
                ("版本", &mut self.content_version),
                ("作者", &mut self.author),
                ("许可", &mut self.license),
                ("依赖 ID（逗号分隔）", &mut self.dependencies),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
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
                    self.result(content_library::export_content_pack(
                        &self.library,
                        &path,
                        &self.content_id,
                        &self.content_version,
                        &self.author,
                        &self.license,
                        &deps,
                    ));
                }
            }
        });
        ui.collapsing(crate::i18n::tr("迁移与诊断"),|ui|{
            ui.label(crate::i18n::tr("迁移遵循原版本规则：无法可靠升级的旧游戏节点会明确拒绝。写入前保留原始字节备份。"));
            if ui.button(crate::i18n::tr("检查并迁移所选 JSON")).clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("JSON",&["json"]).pick_file(){let kind=match path.file_name().and_then(|s|s.to_str()){Some("manifest.json")=>"manifest",Some("content.json")=>"content",_=>"story"};self.result(lom_core::migration::migrate_json_file(&path,kind,None).map(|(result,backup)|json!({"changed":result.changed,"steps":result.steps,"backup":backup})));}}
            if ui.button(crate::i18n::tr("导出脱敏诊断 ZIP")).clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("lom-editor-diagnostics.zip").save_file(){let issues=release::run_preflight(project,release::Profile::Editing,"1.1.2");self.result(release::export_diagnostic_bundle(&path,project,&issues,&self.diagnostic_options()));}}
        });
        ui.collapsing(crate::i18n::tr("Windows 游戏接入与 MOD 管理"), |ui| {
            ui.label(crate::i18n::tr(
                "沿用现有 C# MortalModHost。当前平台未执行 Windows 或游戏实机验证。",
            ));
            ui.add_enabled_ui(cfg!(windows), |ui| {
                if ui.button(crate::i18n::tr("选择游戏目录")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        match game_tools::validate_root(&path) {
                            Ok(()) => {
                                self.game = path;
                                self.save_setting("game_dir", json!(self.game));
                            }
                            Err(e) => self.result::<Value>(Err(e)),
                        }
                    }
                }
                ui.label(self.game.display().to_string());
                if ui.button(crate::i18n::tr("安装检查")).clicked() {
                    self.result(game_tools::diagnose_installation(&self.game, &self.runtime));
                }
                if ui.button(crate::i18n::tr("修复宿主安装")).clicked() {
                    self.result(game_tools::apply_installation_doctor_fixes(
                        &self.game,
                        &self.runtime,
                    ));
                }
                if ui.button(crate::i18n::tr("修复 Steam 启动")).clicked() {
                    self.result(steam_launch_fix(&self.game));
                }
                if ui.button(crate::i18n::tr("选择 C# 宿主所在目录")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.runtime = path;
                        self.save_setting("rust_runtime_dir", json!(self.runtime));
                    }
                }
                if ui.button(crate::i18n::tr("安装 / 更新 C# 宿主")).clicked() {
                    self.result(game_tools::install_runtime(&self.game, &self.runtime));
                }
                if ui.button(crate::i18n::tr("恢复上一版 C# 宿主")).clicked() {
                    self.result(game_tools::restore_runtime(&self.game));
                }
                if ui
                    .button(crate::i18n::tr("安装官方 BepInEx 压缩包"))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("ZIP", &["zip"])
                        .pick_file()
                    {
                        self.result(game_tools::install_bepinex_archive(&self.game, &path));
                    }
                }
                if ui.button(crate::i18n::tr("导入 MOD")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Mod", &["lommod"])
                        .pick_file()
                    {
                        self.result((|| {
                            lom_core::package::read_package(&path)?;
                            game_tools::install_runtime(&self.game, &self.runtime)?;
                            game_tools::install_mod(&self.game, &path, true)
                        })());
                    }
                }
                if ui.button(crate::i18n::tr("刷新 MOD 列表")).clicked() {
                    self.result(game_tools::list_mods(&self.game));
                }
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
                        self.result(result);
                        self.read_reset.clear();
                    }
                }
                let rows = self.report.as_array().cloned().unwrap_or_default();
                for item in rows {
                    if let (Some(path), Some(enabled)) =
                        (item["path"].as_str(), item["enabled"].as_bool())
                    {
                        ui.horizontal(|ui| {
                            ui.label(item["manifest"]["name"].as_str().unwrap_or(path));
                            if ui.button(if enabled { "停用" } else { "启用" }).clicked() {
                                self.result(game_tools::set_enabled(
                                    &self.game,
                                    std::path::Path::new(path),
                                    !enabled,
                                ));
                            }
                        });
                    }
                }
            });
        });
        if !self.report.is_null() {
            ui.separator();
            show_report(ui, &self.report, 0);
        }
    }
}
fn steam_launch_fix(root: &std::path::Path) -> anyhow::Result<Vec<String>> {
    #[cfg(windows)]
    {
        anyhow::ensure!(
            game_tools::game_architecture(root)? == "x86",
            "当前补丁仅适用于 x86 游戏"
        );
        let windows = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or_else(|| anyhow::anyhow!("缺少 SystemRoot"))?,
        );
        let version = [
            windows.join("SysWOW64/version.dll"),
            windows.join("System32/version.dll"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| anyhow::anyhow!("找不到系统 VERSION.dll，未更改游戏文件"))?;
        let temp = tempfile::tempdir()?;
        let doorstop = temp.path().join("win-x86-doorstop.dll");
        std::fs::write(
            &doorstop,
            include_bytes!("../../../editor/assets/doorstop/win-x86-doorstop.dll"),
        )?;
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
    let entries = lom_core::package::read_package(path)?;
    let manifest: Value = serde_json::from_slice(&entries["manifest.json"])?;
    let project = Project::open(path)?;
    let assets = project.assets.keys().cloned().collect::<Vec<_>>();
    Ok(
        json!({"manifest":manifest,"integrity_verified":true,"source_lua_match":true,"entries":entries.iter().map(|(p,b)|json!({"path":p,"size":b.len(),"kind":if p.starts_with("story/"){"source"}else if p.starts_with("assets/"){"asset"}else if p.ends_with(".lua"){"compiled"}else{"metadata"}})).collect::<Vec<_>>(),"preflight":release::run_preflight(&project,release::Profile::Release,"1.1.2"),"unused_assets":release::unused_asset_paths(&project.stories,&assets)}),
    )
}
fn show_report(ui: &mut egui::Ui, value: &Value, depth: usize) {
    if depth > 8 {
        ui.label("…");
        return;
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if v.is_object() || v.is_array() {
                    ui.collapsing(k, |ui| show_report(ui, v, depth + 1));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(k);
                        ui.label(
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string()),
                        );
                    });
                }
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().take(1000).enumerate() {
                ui.push_id(i, |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| show_report(ui, v, depth + 1));
                });
            }
        }
        _ => {
            ui.label(value.to_string());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
