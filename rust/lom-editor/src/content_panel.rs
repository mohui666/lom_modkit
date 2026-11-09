use crate::i18n::tr;
use eframe::egui;
use lom_core::{content_edit, editing, project::Project};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[path = "content_alignment.rs"]
mod alignment;
#[derive(Default)]
pub struct ContentPanel {
    key: String,
    source: Vec<u8>,
    draft: Value,
    baseline: Value,
    files: BTreeMap<String, Vec<u8>>,
    slot: usize,
    portrait: String,
    portrait_prefix: String,
    message: String,
    message_is_error: bool,
    preview: Option<(String, egui::TextureHandle)>,
    replace_shared: bool,
    animation_frame: Option<usize>,
    animation_playing: bool,
    animation_started: f64,
    alignment_mood: bool,
    alignment_dragging: bool,
    alignment_textures: BTreeMap<String, (egui::TextureHandle, [u32; 2])>,
}
impl ContentPanel {
    fn add_portraits(&mut self, paths: &[std::path::PathBuf]) -> anyhow::Result<usize> {
        anyhow::ensure!(self.draft["type"] == "character", "仅人物素材支持新增表情");
        let mut draft = self.draft.clone();
        let mut files = self.files.clone();
        let mut last = None;
        for path in paths {
            let ext = path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            anyhow::ensure!(
                ["png", "jpg", "jpeg"].contains(&ext.as_str()),
                "立绘需要 PNG/JPG 图片"
            );
            anyhow::ensure!(
                std::fs::metadata(path)?.len() <= lom_core::content::MAX_IMAGE_BYTES,
                "图片超过 8 MiB"
            );
            let bytes = std::fs::read(path)?;
            image::load_from_memory(&bytes)?;
            let stem = path
                .file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("portrait");
            let stem = if lom_core::content::validate_portrait_id(stem).is_ok() {
                stem
            } else {
                "portrait"
            };
            let portrait = if draft["portraits"].get(stem).is_none() {
                stem.to_owned()
            } else {
                (2..)
                    .map(|n| format!("{}_{n}", &stem[..stem.len().min(24)]))
                    .find(|name| draft["portraits"].get(name).is_none())
                    .unwrap()
            };
            let name = format!("{}.{ext}", uuid_name());
            draft["portraits"][&portrait] = json!(name);
            files.insert(name, bytes);
            last = Some(portrait);
        }
        self.draft = draft;
        self.files = files;
        if let Some(portrait) = last {
            self.portrait = portrait;
            self.slot = 1;
        }
        self.preview = None;
        Ok(paths.len())
    }
    fn set_message(&mut self, result: anyhow::Result<String>) {
        self.message_is_error = result.is_err();
        self.message = result.unwrap_or_else(crate::error_ui::capture);
    }
    fn show_message(&self, ui: &mut egui::Ui) {
        if !self.message.is_empty() {
            if self.message_is_error {
                crate::error_ui::show_with_actions(ui, &self.message);
            } else {
                ui.label(&self.message);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn pending_fixture(project: &Project, key: &str, draft: Value) -> Self {
        let source = project.assets[key].clone();
        Self {
            key: key.into(),
            baseline: draft_from_source(&source),
            source,
            draft,
            ..Default::default()
        }
    }
    pub fn has_pending(&self) -> bool {
        !self.key.is_empty() && (self.draft != self.baseline || !self.files.is_empty())
    }
    pub fn reset(&mut self) {
        self.alignment_textures.clear();
        self.alignment_mood = false;
        self.alignment_dragging = false;
        self.key.clear();
        self.files.clear();
        self.preview = None;
        self.animation_frame = None;
        self.animation_playing = false;
        self.message.clear();
    }
    pub fn apply_pending(&mut self, project: &mut Project) -> anyhow::Result<bool> {
        if !self.apply_pending_to_snapshot(project)? {
            return Ok(false);
        }
        self.source = project.assets[&self.key].clone();
        self.draft = draft_from_source(&self.source);
        self.baseline = self.draft.clone();
        self.files.clear();
        self.message.clear();
        Ok(true)
    }
    /// Include valid in-progress input in recovery without changing the editing buffer.
    pub(crate) fn apply_pending_to_snapshot(&self, project: &mut Project) -> anyhow::Result<bool> {
        if !self.has_pending() {
            return Ok(false);
        }
        anyhow::ensure!(
            project.assets.get(&self.key) == Some(&self.source),
            "素材已被其他操作修改；请撤销未完成的输入后重试"
        );
        content_edit::update(project, &self.key, &self.draft, &self.files)?;
        Ok(true)
    }
    fn select(&mut self, project: &mut Project, selection: &mut String) -> bool {
        let mut changed = false;
        if self.key != *selection && self.has_pending() {
            match self.apply_pending(project) {
                Ok(applied) => changed = applied,
                Err(error) => {
                    *selection = self.key.clone();
                    self.set_message(Err(error.context("修改尚未完成")));
                    return false;
                }
            }
        }
        let Some(bytes) = project.assets.get(selection) else {
            return changed;
        };
        let key = selection.as_str();
        if self.key != key || self.source != *bytes {
            // External project changes must not silently replace unfinished input.
            if self.has_pending() {
                self.set_message(Err(anyhow::anyhow!(
                    "素材已被其他操作修改；请撤销未完成的输入后重试"
                )));
                return changed;
            }
            self.key = key.into();
            self.source = bytes.clone();
            self.draft = draft_from_source(bytes);
            self.portrait = self.draft["portraits"]
                .as_object()
                .and_then(|p| {
                    if p.contains_key("normal") {
                        Some("normal".to_owned())
                    } else {
                        p.keys().next().cloned()
                    }
                })
                .unwrap_or_default();
            self.baseline = self.draft.clone();
            self.files.clear();
            self.preview = None;
            self.alignment_textures.clear();
            self.animation_frame = None;
            self.animation_playing = false;
            self.message.clear();
        }
        changed
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        selection: &mut String,
    ) -> bool {
        let mut changed = self.select(project, selection);
        self.alignment_dragging = false;
        let key = self.key.clone();
        let key = key.as_str();
        if key.is_empty() || !project.assets.contains_key(key) {
            return changed;
        }
        if !self.draft.is_object() {
            ui.label(tr("内容元数据无效"));
            return changed;
        }
        ui.separator();
        let symbol = format!("user:{}", self.draft["id"].as_str().unwrap_or(""));
        let kind = self.draft["type"].as_str().unwrap_or("").to_owned();
        if kind == "character" {
            if self.draft.get("intro").is_none() && ui.button(tr("添加角色介绍卡")).clicked()
            {
                self.draft["intro"] = json!({"name":self.draft["name"],"title":"","text":"","image_scale":100,"image_x":0,"image_y":0});
            }
            if self.draft.get("intro").is_some() && ui.button(tr("移除介绍卡")).clicked() {
                self.draft.as_object_mut().unwrap().remove("intro");
            }
        }
        let mut catalog = crate::forms::Catalog::new();
        catalog.sync_assets(&project.assets);
        catalog.field(ui, &mut self.draft, "name", "名称", "str", false, &[], &[]);
        if kind == "audio" {
            catalog.field(
                ui,
                &mut self.draft,
                "character",
                "绑定人物",
                "character",
                true,
                &[],
                &[],
            );
        }
        if kind == "character" {
            catalog.field(
                ui,
                &mut self.draft,
                "extends",
                "扩展原版人物",
                "official_character",
                true,
                &[],
                &[],
            );
            ui.small(crate::i18n::key("workspace.content.extension_hint"));
            for (key, label, field_kind) in [
                ("title", "称号", "str"),
                ("scale", "立绘缩放（%）", "float"),
                ("art_facing", "原图朝向", "facing"),
                ("mood_x", "气泡横向位置（%）", "int"),
                ("mood_y", "气泡高度（%）", "int"),
            ] {
                if matches!(key, "mood_x" | "mood_y")
                    && !self.draft["extends"].as_str().unwrap_or("").is_empty()
                {
                    continue;
                }
                catalog.field(ui, &mut self.draft, key, label, field_kind, false, &[], &[]);
            }
            if self.draft["intro"].is_object() {
                ui.collapsing(tr("角色介绍卡"), |ui| {
                    for (key, label, field_kind) in [
                        ("name", "名称", "str"),
                        ("title", "称号", "str"),
                        ("text", "简介", "multiline"),
                        ("image_scale", "图片缩放（%）", "float"),
                        ("image_x", "横向偏移", "int"),
                        ("image_y", "纵向偏移", "int"),
                    ] {
                        catalog.field(
                            ui,
                            &mut self.draft["intro"],
                            key,
                            label,
                            field_kind,
                            !matches!(key, "name" | "text"),
                            &[],
                            &[],
                        );
                    }
                });
            }
        }
        if lom_core::content::GAMEPLAY_TYPES.contains(&kind.as_str()) {
            gameplay_fields(ui, &mut self.draft, &catalog);
        }
        if kind == "skill" {
            self.animation_controls(ui, project);
        }
        // Preserve the complete metadata editor, including future fields.
        ui.collapsing(tr("高级属性"), |ui| {
            if ui.button(tr("复制引用")).clicked() {
                ui.ctx().copy_text(symbol.clone());
            }
            let mut values = self.draft.clone();
            values.as_object_mut().map(|o| {
                o.remove("id");
                o.remove("type");
            });
            if crate::forms::value_editor(ui, "内容属性", &mut values, 0) {
                values["id"] = self.draft["id"].clone();
                values["type"] = self.draft["type"].clone();
                self.draft = values;
            }
        });
        if kind != "talent" {
            ui.add_space(4.0);
            let slots = if kind == "character" {
                vec![
                    "默认立绘",
                    "指定表情",
                    "介绍卡图片",
                    "战斗待机",
                    "战斗攻击",
                    "战斗受伤",
                    "战斗防御",
                ]
            } else {
                vec!["主文件"]
            };
            self.slot = self.slot.min(slots.len() - 1);
            egui::ComboBox::from_id_salt("content-file-slot")
                .selected_text(format!(
                    "{}{}",
                    tr(slots[self.slot]),
                    if self.slot == 0 { " *" } else { "" }
                ))
                .show_ui(ui, |ui| {
                    for (i, label) in slots.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.slot,
                            i,
                            format!("{}{}", tr(label), if i == 0 { " *" } else { "" }),
                        );
                    }
                });
            if self.slot == 1 {
                egui::ComboBox::from_id_salt("content-portrait")
                    .selected_text(if self.portrait.is_empty() {
                        crate::i18n::key("editor.content.choose_portrait")
                    } else {
                        self.portrait.clone()
                    })
                    .show_ui(ui, |ui| {
                        for portrait in self.draft["portraits"]
                            .as_object()
                            .into_iter()
                            .flat_map(|p| p.keys())
                        {
                            ui.selectable_value(&mut self.portrait, portrait.clone(), portrait);
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label(format!("{} *", tr("表情 ID")));
                    ui.text_edit_singleline(&mut self.portrait);
                });
                ui.small(crate::i18n::key("workspace.content.portrait_hint"));
            }
            if kind == "character" && ui.button(tr("新增表情立绘…")).clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("立绘图片", &["png", "jpg", "jpeg"])
                    .pick_files()
                {
                    let result = self
                        .add_portraits(&paths)
                        .map(|count| format!("已新增 {count} 张表情立绘"));
                    self.set_message(result);
                }
            }
            if kind == "character" {
                ui.small(tr(
                    "按住 Ctrl 多选图片，Ctrl+A 全选；新增不会覆盖已有表情。",
                ));
                ui.add(
                    egui::TextEdit::singleline(&mut self.portrait_prefix)
                        .hint_text(tr("文件名前缀（可选，用于文件夹导入）")),
                );
                if ui.button(tr("从文件夹批量新增表情…")).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        let result = portrait_files(&folder, &self.portrait_prefix)
                            .and_then(|paths| self.add_portraits(&paths))
                            .map(|count| format!("已新增 {count} 张表情立绘"));
                        self.set_message(result);
                    }
                }
            }
            if ui.button(tr("替换当前素材文件…")).clicked() {
                let extensions = if kind == "audio" {
                    vec!["wav", "ogg"]
                } else {
                    vec!["png", "jpg", "jpeg"]
                };
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("素材", &extensions)
                    .pick_file()
                {
                    let result = (|| -> anyhow::Result<()> {
                        let bytes = std::fs::read(&path)?;
                        let ext = path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        let name = format!("{}.{ext}", uuid_name());
                        let limit = if kind == "audio" {
                            20 * 1024 * 1024
                        } else {
                            8 * 1024 * 1024
                        };
                        anyhow::ensure!(
                            !bytes.is_empty() && bytes.len() <= limit,
                            "素材为空或超过大小限制"
                        );
                        if kind != "audio" {
                            image::load_from_memory(&bytes)?;
                        }
                        if kind == "character" {
                            match self.slot {
                                0 => {
                                    self.draft["files"]["main"] = json!(name);
                                    self.draft["portraits"]["normal"] = json!(name);
                                }
                                1 => {
                                    lom_core::content::validate_portrait_id(&self.portrait)?;
                                    self.draft["portraits"][&self.portrait] = json!(name);
                                }
                                2 => {
                                    anyhow::ensure!(
                                        self.draft["intro"].is_object(),
                                        "请先添加介绍卡"
                                    );
                                    self.draft["intro"]["image"] = json!(name);
                                }
                                i => {
                                    self.draft[[
                                        "combat_idle",
                                        "combat_attack",
                                        "combat_hurt",
                                        "combat_defence",
                                    ][i - 3]] = json!(name)
                                }
                            }
                        } else {
                            self.draft["files"]["main"] = json!(name);
                        }
                        self.files.insert(name, bytes);
                        self.preview = None;
                        Ok(())
                    })();
                    self.set_message(result.map(|_| String::new()));
                }
            }
        }
        if kind == "character" {
            self.alignment_controls(ui, project, key);
        }
        // Keep in-progress typing intact (including spaces). Leaving the input,
        // choosing a file, switching resources, or saving commits the draft.
        if !self.alignment_dragging && (!ui.ctx().wants_keyboard_input() || !self.files.is_empty())
        {
            match self.apply_pending(project) {
                Ok(applied) => changed |= applied,
                Err(error) => self.set_message(Err(error.context("修改尚未完成"))),
            }
        }
        ui.horizontal_wrapped(|ui| {
            if self.has_pending() && ui.button(tr("撤销未完成的输入")).clicked() {
                self.reset();
            }
            if ui.button(tr("删除项目内容（可撤销）")).clicked() {
                match content_edit::remove(project, key) {
                    Ok(()) => {
                        changed = true;
                        self.reset();
                    }
                    Err(e) => self.set_message(Err(e)),
                }
            }
        });
        let refs = editing::references(
            &project.stories,
            &project.manifest,
            "content",
            &symbol,
            None,
        );
        ui.collapsing(
            crate::i18n::message(
                "workspace.content.references",
                &[("count", refs.len().to_string())],
            ),
            |ui| {
                for r in refs {
                    ui.label(format!(
                        "{}/{} · {}",
                        r.story_id,
                        r.node_id.unwrap_or_default(),
                        r.field
                    ));
                }
            },
        );
        if kind != "audio" {
            let file = if kind == "skill" && self.animation_frame.is_some() {
                self.draft["animation"]["frames"]
                    .get(self.animation_frame.unwrap())
                    .and_then(Value::as_str)
            } else if kind == "character" {
                match self.slot {
                    1 => self.draft["portraits"][&self.portrait].as_str(),
                    2 => self.draft["intro"]["image"].as_str(),
                    i if i >= 3 => self.draft[[
                        "combat_idle",
                        "combat_attack",
                        "combat_hurt",
                        "combat_defence",
                    ][i - 3]]
                        .as_str(),
                    _ => self.draft["files"]["main"].as_str(),
                }
            } else {
                self.draft["files"]["main"].as_str()
            };
            if let Some(name) = file {
                let asset = format!("{}{name}", key.strip_suffix("content.json").unwrap_or(""));
                if let Some(bytes) = self.files.get(name).or_else(|| project.assets.get(&asset)) {
                    let cache = format!("{asset}:{}", bytes.len());
                    if self.preview.as_ref().is_none_or(|(k, _)| *k != cache) {
                        if let Ok(image) = image::load_from_memory(bytes) {
                            let pixels = image.thumbnail(1024, 1024).to_rgba8();
                            let texture = ui.ctx().load_texture(
                                &cache,
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [pixels.width() as usize, pixels.height() as usize],
                                    pixels.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            );
                            self.preview = Some((cache, texture));
                        }
                    }
                    if let Some((_, texture)) = &self.preview {
                        ui.add(
                            egui::Image::new(texture)
                                .max_width(ui.available_width())
                                .max_height(300.0),
                        );
                    }
                }
            }
        }
        ui.collapsing(tr("共享库与导出"), |ui| {
            let root = lom_core::content::default_repository_root();
            ui.checkbox(
                &mut self.replace_shared,
                tr("允许替换共享库同 ID 内容（旧内容移入 .trash）"),
            );
            if ui
                .add_enabled(!self.has_pending(), egui::Button::new(tr("保存到共享库")))
                .clicked()
            {
                let result = content_edit::store_shared(&root, project, key, self.replace_shared)
                    .map(|b| {
                        format!(
                            "已保存到共享库；备份：{}",
                            b.map(|p| p.display().to_string())
                                .unwrap_or_else(|| "首次创建".into())
                        )
                    });
                self.set_message(result);
            }
            if ui.button(tr("从共享库移入回收目录")).clicked() {
                let result =
                    content_edit::remove_shared(&root, self.draft["id"].as_str().unwrap_or(""))
                        .map(|p| format!("已保留于 {}", p.display()));
                self.set_message(result);
            }
        });
        self.show_message(ui);
        changed
    }
    fn append_animation_frames(
        &mut self,
        project: &Project,
        paths: &[std::path::PathBuf],
    ) -> anyhow::Result<()> {
        let mut frames = self.draft["animation"]["frames"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        anyhow::ensure!(
            !paths.is_empty() && frames.len() + paths.len() <= 120,
            "招式动画最多120帧"
        );
        let prefix = self
            .key
            .strip_suffix("content.json")
            .ok_or_else(|| anyhow::anyhow!("内容路径无效"))?;
        let mut files = BTreeMap::new();
        for (index, path) in paths.iter().enumerate() {
            anyhow::ensure!(
                path.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png")),
                "动画帧必须是PNG"
            );
            let bytes = std::fs::read(path)?;
            anyhow::ensure!(
                !bytes.is_empty() && bytes.len() <= lom_core::content::MAX_IMAGE_BYTES as usize,
                "动画帧为空或超过8 MiB"
            );
            anyhow::ensure!(
                image::guess_format(&bytes)? == image::ImageFormat::Png,
                "动画帧必须是PNG编码"
            );
            image::load_from_memory(&bytes)?;
            let original = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("文件名无效"))?;
            let name = if self.files.contains_key(original)
                || files.contains_key(original)
                || project.assets.contains_key(&format!("{prefix}{original}"))
            {
                format!("{}_{index}.png", uuid_name())
            } else {
                original.into()
            };
            frames.push(json!(name));
            files.insert(name, bytes);
        }
        let mut next = self.draft.clone();
        if !next["animation"].is_object() {
            next["animation"] = json!({"fps":12,"hit_frame":0,"scale":100});
        }
        next["animation"]["frames"] = json!(frames);
        lom_core::content::normalize_content_metadata(&next)?;
        self.draft = next;
        self.files.extend(files);
        self.preview = None;
        self.animation_frame = Some(0);
        self.animation_playing = false;
        Ok(())
    }
    fn animation_controls(&mut self, ui: &mut egui::Ui, project: &Project) {
        ui.separator();
        ui.strong(tr("招式帧动画"));
        if ui.button(tr("导入／追加 PNG 动画帧…")).clicked() {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter("PNG动画帧", &["png"])
                .pick_files()
            {
                let result = self
                    .append_animation_frames(project, &paths)
                    .map(|_| String::new());
                self.set_message(result);
            }
        }
        if !self.draft["animation"].is_object() {
            ui.weak(tr("未添加帧动画，使用原版动作演出。"));
            return;
        }
        let mut clear = false;
        ui.horizontal(|ui| {
            if ui
                .button(tr(if self.animation_playing {
                    "暂停动画"
                } else {
                    "播放动画"
                }))
                .clicked()
            {
                self.animation_playing = !self.animation_playing;
                self.animation_started = ui.input(|i| i.time);
            }
            if ui.button(tr("预览图标")).clicked() {
                self.animation_frame = None;
                self.animation_playing = false;
            }
            clear = ui.button(tr("移除动画，使用原版动作")).clicked();
        });
        if clear {
            self.draft.as_object_mut().unwrap().remove("animation");
            self.animation_frame = None;
            self.animation_playing = false;
            return;
        }
        integer(ui, &mut self.draft["animation"], "fps", "每秒帧数", 1, 60);
        integer(
            ui,
            &mut self.draft["animation"],
            "scale",
            "动画缩放（%）",
            50,
            200,
        );
        let animation = &mut self.draft["animation"];
        let mut hit = animation["hit_frame"].as_u64().unwrap_or(0) as usize;
        let Some(frames) = animation["frames"].as_array_mut() else {
            return;
        };
        let count = frames.len();
        let mut remove = None;
        let mut swap = None;
        egui::ScrollArea::vertical()
            .id_salt("skill-animation-frames")
            .max_height(220.0)
            .show(ui, |ui| {
                for (index, file) in frames.iter().enumerate() {
                    let name = file.as_str().unwrap_or("");
                    let short = if name.chars().count() > 20 {
                        format!("{}…", name.chars().take(19).collect::<String>())
                    } else {
                        name.to_owned()
                    };
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    self.animation_frame == Some(index),
                                    format!("{:03} · {}", index + 1, short),
                                )
                                .on_hover_text(name)
                                .clicked()
                            {
                                self.animation_frame = Some(index);
                                self.animation_playing = false;
                            }
                            ui.radio_value(&mut hit, index, tr("命中帧"));
                            if ui.add_enabled(index > 0, egui::Button::new("↑")).clicked() {
                                swap = Some((index, index - 1));
                            }
                            if ui
                                .add_enabled(index + 1 < count, egui::Button::new("↓"))
                                .clicked()
                            {
                                swap = Some((index, index + 1));
                            }
                            if ui
                                .add_enabled(count > 1, egui::Button::new(tr("移除")).small())
                                .clicked()
                            {
                                remove = Some(index);
                            }
                        });
                    });
                }
            });
        if let Some((a, b)) = swap {
            frames.swap(a, b);
            if hit == a {
                hit = b;
            } else if hit == b {
                hit = a;
            }
            self.animation_frame = Some(b);
        }
        if let Some(index) = remove {
            frames.remove(index);
            if hit > index {
                hit -= 1;
            }
            hit = hit.min(frames.len().saturating_sub(1));
            self.animation_frame = Some(hit);
        }
        let count = frames.len();
        animation["hit_frame"] = json!(hit);
        if self.animation_playing && count > 0 {
            let fps = animation["fps"].as_f64().unwrap_or(12.0);
            self.animation_frame = Some(
                ((ui.input(|i| i.time) - self.animation_started).max(0.0) * fps).floor() as usize
                    % count,
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(1.0 / fps));
        }
    }
}
fn choice(ui: &mut egui::Ui, row: &mut Value, field: &str, label: &str, options: &[(&str, &str)]) {
    let mut selected = row[field].as_str().unwrap_or("").to_owned();
    ui.horizontal(|ui| {
        ui.label(tr(label));
        egui::ComboBox::from_id_salt(field)
            .selected_text(tr(options
                .iter()
                .find(|(id, _)| *id == selected)
                .map(|(_, label)| *label)
                .unwrap_or(&selected)))
            .show_ui(ui, |ui| {
                for (id, label) in options {
                    ui.selectable_value(&mut selected, (*id).into(), tr(label));
                }
            });
    });
    row[field] = json!(selected);
}
fn integer(ui: &mut egui::Ui, row: &mut Value, field: &str, label: &str, min: i64, max: i64) {
    let mut number = row[field].as_i64().unwrap_or(min);
    ui.horizontal(|ui| {
        ui.label(tr(label));
        if ui
            .add(egui::DragValue::new(&mut number).range(min..=max))
            .changed()
        {
            row[field] = json!(number);
        }
    });
}
fn stat_rows(
    ui: &mut egui::Ui,
    level: &mut Value,
    field: &str,
    value_field: &str,
    label: &str,
    catalog: &crate::forms::Catalog,
) {
    ui.push_id(field, |ui| {
        ui.label(tr(label));
        let Some(rows) = level[field].as_array_mut() else {
            return;
        };
        let mut remove = None;
        for (index, row) in rows.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{}", index + 1));
                    if ui.small_button(tr("移除")).clicked() {
                        remove = Some(index);
                    }
                });
                catalog.field(ui, row, "stat", "属性", "stat", false, &[], &[]);
                integer(
                    ui,
                    row,
                    value_field,
                    if value_field == "min" {
                        "至少达到"
                    } else {
                        "增加数值"
                    },
                    -999999,
                    999999,
                );
            });
        }
        if let Some(index) = remove {
            rows.remove(index);
        }
        if rows.len() < 32 && ui.small_button(tr("添加属性")).clicked() {
            rows.push(json!({"stat":"internal",value_field:if value_field=="min" {0} else {1}}));
        }
    });
}
fn skill_effect_fields(ui: &mut egui::Ui, draft: &mut Value) {
    ui.strong(tr("命中后附加效果（可选）"));
    let Some(effects) = draft["effects"].as_array_mut() else {
        return;
    };
    let mut remove = None;
    for (index, effect) in effects.iter_mut().enumerate() {
        ui.push_id(("skill-effect", index), |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} {}",
                    crate::i18n::key("editor.content.effect"),
                    index + 1
                ));
                if ui.small_button(tr("移除")).clicked() {
                    remove = Some(index);
                }
            });
            choice(
                ui,
                effect,
                "type",
                "效果类型",
                &[
                    ("damage", "造成伤害"),
                    ("heal", "恢复血量"),
                    ("stamina", "恢复气力"),
                ],
            );
            choice(
                ui,
                effect,
                "target",
                "效果目标",
                &[("self", "自身"), ("enemy", "对手")],
            );
            integer(ui, effect, "value", "数值", 1, 999999);
        });
    }
    if let Some(index) = remove {
        effects.remove(index);
    }
    if effects.len() < 16 && ui.button(tr("添加效果")).clicked() {
        effects.push(json!({"type":"damage","target":"enemy","value":10}));
    }
}
fn consumable_effect_fields(ui: &mut egui::Ui, draft: &mut Value, catalog: &crate::forms::Catalog) {
    let previous = draft["use_in"].clone();
    choice(
        ui,
        draft,
        "use_in",
        "使用场景",
        &[("field", "野外／剧情"), ("combat", "决斗中")],
    );
    ui.weak(tr("切换使用场景会重建效果列表。决斗道具占用一次行动。"));
    let field = draft["use_in"] == "field";
    if previous != draft["use_in"] {
        draft["effects"] = if field {
            json!([{"type":"stat","stat":"life","value":10}])
        } else {
            json!([{"type":"heal","value":10}])
        };
    }
    let Some(effects) = draft["effects"].as_array_mut() else {
        return;
    };
    let can_remove = effects.len() > 1;
    let mut remove = None;
    for (index, effect) in effects.iter_mut().enumerate() {
        ui.push_id(("consumable-effect", index), |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} {}", tr("使用效果"), index + 1));
                if ui
                    .add_enabled(can_remove, egui::Button::new(tr("移除")).small())
                    .clicked()
                {
                    remove = Some(index);
                }
            });
            if field {
                catalog.field(ui, effect, "stat", "玩家属性", "stat", false, &[], &[]);
                integer(ui, effect, "value", "增加数值（不能为0）", -999999, 999999);
            } else {
                choice(
                    ui,
                    effect,
                    "type",
                    "效果类型",
                    &[("heal", "恢复血量"), ("stamina", "恢复气力")],
                );
                integer(ui, effect, "value", "数值", 1, 999999);
            }
        });
    }
    if let Some(index) = remove {
        effects.remove(index);
    }
    if effects.len() < 16 && ui.button(tr("添加使用效果")).clicked() {
        effects.push(if field {
            json!({"type":"stat","stat":"life","value":10})
        } else {
            json!({"type":"heal","value":10})
        });
    }
}
fn gameplay_fields(ui: &mut egui::Ui, draft: &mut Value, catalog: &crate::forms::Catalog) {
    ui.add_space(8.0);
    catalog.field(
        ui,
        draft,
        "description",
        "说明",
        "multiline",
        false,
        &[],
        &[],
    );
    match draft["type"].as_str().unwrap_or("") {
        "talent" => {
            ui.small(crate::i18n::key("editor.content.talent_no_effect"));
        }
        "item" => {
            let old_category = draft["category"].clone();
            choice(
                ui,
                draft,
                "category",
                "物品类别",
                &[
                    ("misc", "杂物"),
                    ("special", "贵重品"),
                    ("consumable", "消耗品"),
                ],
            );
            integer(ui, draft, "cost", "基础价格", 0, 999999);
            if draft["category"] == "consumable" {
                if old_category != "consumable" {
                    draft["use_in"] = json!("field");
                    draft["effects"] = json!([{"type":"stat","stat":"life","value":10}]);
                }
                consumable_effect_fields(ui, draft, catalog);
            }
            ui.weak(tr("通过物品节点发放或消耗，也可加入自定义商店。"));
        }
        "martial" => {
            integer(ui, draft, "cost", "秘籍价格", 0, 999999);
            ui.weak(tr(
                "按等级配置修炼点数、属性条件和升级加成；通过书籍类别的物品节点发放。",
            ));
            let Some(levels) = draft["levels"].as_array_mut() else {
                return;
            };
            let can_remove = levels.len() > 1;
            let mut remove = None;
            for (index, level) in levels.iter_mut().enumerate() {
                egui::CollapsingHeader::new(format!(
                    "{} {}",
                    crate::i18n::key("table.level"),
                    index + 1
                ))
                .id_salt(("martial-level", index))
                .default_open(index == 0)
                .show(ui, |ui| {
                    integer(ui, level, "points", "修炼所需点数", 1, 999999);
                    stat_rows(ui, level, "requirements", "min", "修炼条件", catalog);
                    ui.add_space(6.0);
                    stat_rows(ui, level, "bonuses", "delta", "升级加成", catalog);
                    if ui
                        .add_enabled(can_remove, egui::Button::new(tr("移除此等级")).small())
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                if levels.len() > 1 {
                    levels.remove(index);
                }
            }
            if levels.len() < 20 && ui.button(tr("添加等级")).clicked() {
                levels.push(json!({"points":10,"requirements":[],"bonuses":[]}));
            }
        }
        "support" => {
            choice(
                ui,
                draft,
                "trigger",
                "触发时机",
                &[
                    ("round_start", "回合开始"),
                    ("after_action", "行动之后"),
                    ("after_hit", "受击之后"),
                ],
            );
            integer(ui, draft, "chance", "触发概率（%）", 0, 100);
            integer(ui, draft, "uses", "可用次数", 1, 99);
            ui.weak(tr(
                "在决斗节点的我方／对手自定义援助中选择。一次触发会执行下列全部效果并消耗一次。",
            ));
            let Some(effects) = draft["effects"].as_array_mut() else {
                return;
            };
            let can_remove = effects.len() > 1;
            let mut remove = None;
            for (index, effect) in effects.iter_mut().enumerate() {
                ui.push_id(("support-effect", index), |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!(
                            "{} {}",
                            crate::i18n::key("editor.content.effect"),
                            index + 1
                        ));
                        if ui
                            .add_enabled(can_remove, egui::Button::new(tr("移除")).small())
                            .clicked()
                        {
                            remove = Some(index);
                        }
                    });
                    choice(
                        ui,
                        effect,
                        "type",
                        "效果类型",
                        &[
                            ("damage", "造成伤害"),
                            ("heal", "恢复血量"),
                            ("stamina", "恢复气力"),
                        ],
                    );
                    choice(
                        ui,
                        effect,
                        "target",
                        "效果目标",
                        &[("self", "持有援助的一方"), ("enemy", "对手")],
                    );
                    integer(ui, effect, "value", "数值", 1, 999999);
                });
            }
            if let Some(index) = remove {
                if effects.len() > 1 {
                    effects.remove(index);
                }
            }
            if effects.len() < 16 && ui.button(tr("添加效果")).clicked() {
                effects.push(json!({"type":"heal","target":"self","value":10}));
            }
        }
        "skill" => {
            choice(
                ui,
                draft,
                "action",
                "动作类型",
                &[("attack", "攻击"), ("weapon", "暗器"), ("ultimate", "绝招")],
            );
            for (key, label, min, max) in [
                ("power", "招式威力（基础值）", 0, 999999),
                ("stamina_cost", "气力消耗", 0, 9999),
                ("accuracy", "命中率（%）", 0, 100),
                ("cooldown", "冷却回合", 0, 99),
                ("ai_chance", "敌方选用概率（%）", 0, 100),
            ] {
                integer(ui, draft, key, label, min, max);
            }
            skill_effect_fields(ui, draft);
        }
        _ => {}
    }
}
fn draft_from_source(bytes: &[u8]) -> Value {
    let mut draft: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
    if let Some(kind) = draft["type"]
        .as_str()
        .filter(|kind| lom_core::content::GAMEPLAY_TYPES.contains(kind))
        .map(str::to_owned)
    {
        let defaults = match kind.as_str() {
            "item" => json!({"description":"","cost":0,"category":"misc"}),
            "martial" => json!({"description":"","cost":0}),
            "support" => json!({"description":"","trigger":"round_start","chance":100,"uses":1}),
            "talent" => json!({"description":""}),
            _ => {
                json!({"description":"","action":"attack","power":0,"stamina_cost":0,"accuracy":100,"cooldown":0,"ai_chance":100,"effects":[]})
            }
        };
        for (key, value) in defaults.as_object().unwrap() {
            draft
                .as_object_mut()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        if draft["category"] == "consumable" {
            draft
                .as_object_mut()
                .unwrap()
                .entry("use_in")
                .or_insert_with(|| json!("field"));
        }
        if let Some(animation) = draft["animation"].as_object_mut() {
            for (key, value) in [("fps", 12), ("hit_frame", 0), ("scale", 100)] {
                animation.entry(key).or_insert_with(|| json!(value));
            }
        }
        if let Some(levels) = draft["levels"].as_array_mut() {
            for level in levels {
                if let Some(object) = level.as_object_mut() {
                    for field in ["requirements", "bonuses"] {
                        object.entry(field).or_insert_with(|| json!([]));
                    }
                }
            }
        }
    }
    if draft["type"] == "character" {
        for (key, default) in [
            ("title", json!("")),
            ("scale", json!(100)),
            ("art_facing", json!("left")),
            ("mood_x", json!(50)),
            ("mood_y", json!(90)),
        ] {
            if draft.get(key).is_none() {
                draft[key] = default;
            }
        }
    }
    draft
}
fn uuid_name() -> String {
    format!(
        "asset_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

pub(crate) fn portrait_files(
    folder: &std::path::Path,
    prefix: &str,
) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with(prefix)
            && path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| ["png", "jpg", "jpeg"].contains(&s.to_ascii_lowercase().as_str()))
        {
            paths.push(path);
        }
    }
    paths.sort();
    anyhow::ensure!(!paths.is_empty(), "文件夹中没有匹配前缀的 PNG/JPG 图片");
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adding_portraits_keeps_default_and_existing_expressions() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("happy.png");
        image::RgbaImage::from_pixel(16, 24, image::Rgba([255, 80, 30, 255]))
            .save(&path)
            .unwrap();
        let mut panel = ContentPanel::default();
        panel.draft = json!({"type":"character","files":{"main":"normal.png"},"portraits":{"normal":"normal.png","happy":"old-happy.png"}});
        let original = panel.draft.clone();
        panel.add_portraits(&[path.clone(), path]).unwrap();
        assert_eq!(panel.draft["files"], original["files"]);
        assert_eq!(panel.draft["portraits"]["normal"], "normal.png");
        assert_eq!(panel.draft["portraits"]["happy"], "old-happy.png");
        assert_eq!(panel.draft["portraits"].as_object().unwrap().len(), 4);
        assert!(panel.draft["portraits"].get("happy_2").is_some());
        assert!(panel.draft["portraits"].get("happy_3").is_some());
        let saved = (panel.draft.clone(), panel.files.clone());
        let bad = temp.path().join("broken.png");
        std::fs::write(&bad, b"broken").unwrap();
        assert!(panel.add_portraits(&[bad]).is_err());
        assert_eq!((panel.draft, panel.files), saved);
    }
    #[test]
    fn folder_portraits_filter_by_prefix_and_ignore_subfolders() {
        let temp = tempfile::tempdir().unwrap();
        for file in [
            "shen_happy.png",
            "shen_angry.JPG",
            "other.png",
            "shen_note.txt",
        ] {
            std::fs::write(temp.path().join(file), b"fixture").unwrap();
        }
        std::fs::create_dir(temp.path().join("shen_subfolder.png")).unwrap();
        let files = portrait_files(temp.path(), "shen_").unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(portrait_files(temp.path(), "").unwrap().len(), 3);
        assert!(portrait_files(temp.path(), "missing_").is_err());
    }
    #[test]
    fn operation_errors_use_the_component_even_without_pending_edits_and_success_keeps_its_path() {
        let mut panel = ContentPanel::default();
        assert!(!panel.has_pending());
        panel.set_message(Err(
            anyhow::anyhow!("Permission denied (os error 5)").context("C:/shared/content.json")
        ));
        let raw = panel.message.clone();
        let read = |panel: &ContentPanel| {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| panel.show_message(ui));
            });
            output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().or_else(|| node.value()))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let shown = read(&panel);
        assert!(
            shown.contains("读写权限") && shown.contains("复制诊断信息"),
            "{shown}"
        );
        assert!(!shown.contains("C:/shared") && !shown.contains("Permission denied"));
        assert_eq!(panel.message, raw);
        let success = "已保存到共享库：C:/shared/MyCustomAsset";
        panel.set_message(Ok(success.into()));
        assert!(read(&panel).contains(success));
        assert!(!panel.message_is_error);
    }

    #[test]
    fn skill_frames_import_in_order_and_a_bad_batch_preserves_the_draft() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let png = bytes.into_inner();
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let a = first.join("帧.png");
        let b = second.join("帧.png");
        std::fs::write(&a, &png).unwrap();
        std::fs::write(&b, &png).unwrap();
        let mut project = Project::new();
        let key = "assets/user/skill/demo.test/content.json".to_owned();
        project.assets.insert(key.clone(),serde_json::to_vec(&json!({"schema":1,"id":"demo.test","type":"skill","name":"试招","files":{"main":"icon.png"}})).unwrap());
        project
            .assets
            .insert("assets/user/skill/demo.test/icon.png".into(), png.clone());
        let mut panel = ContentPanel::default();
        let mut selection = key.clone();
        panel.select(&mut project, &mut selection);
        panel
            .append_animation_frames(&project, &[a.clone(), b])
            .unwrap();
        let frames = panel.draft["animation"]["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 2);
        assert_ne!(frames[0], frames[1]);
        assert_eq!(frames[0], "帧.png");
        let previous_draft = panel.draft.clone();
        let previous_files = panel.files.clone();
        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"not an image").unwrap();
        assert!(panel
            .append_animation_frames(&project, &[a.clone(), bad])
            .is_err());
        assert_eq!(panel.draft, previous_draft);
        assert_eq!(panel.files, previous_files);
        assert!(panel
            .append_animation_frames(&project, &vec![a; 119])
            .is_err());
        panel.apply_pending(&mut project).unwrap();
        let saved: Value = serde_json::from_slice(&project.assets[&key]).unwrap();
        assert_eq!(
            saved["animation"]["frames"],
            previous_draft["animation"]["frames"]
        );
        for file in lom_core::content::listed_content_files(&saved) {
            assert_eq!(
                project.assets[&format!("assets/user/skill/demo.test/{file}")],
                png
            );
        }
    }

    #[test]
    fn gameplay_content_forms_preserve_documents_until_an_edit_is_committed() {
        for kind in lom_core::content::GAMEPLAY_TYPES {
            let mut project = Project::new();
            let key = format!("assets/user/{kind}/demo.test/content.json");
            let mut metadata = json!({"schema":1,"content_schema":1,"type":kind,"id":"demo.test","name":"测试内容","files":{"main":"icon.png"}});
            if *kind == "martial" {
                metadata["levels"] = json!([{"points":10}]);
            }
            if *kind == "support" {
                metadata["effects"] = json!([{"type":"heal","target":"self","value":10}]);
            }
            if *kind == "skill" {
                metadata["animation"] = json!({"frames":["icon.png"]});
            }
            project
                .assets
                .insert(key.clone(), serde_json::to_vec(&metadata).unwrap());
            project
                .assets
                .insert(key.replace("content.json", "icon.png"), b"icon".to_vec());
            let original = project.clone();
            let mut panel = ContentPanel::default();
            let mut selection = key.clone();
            panel.select(&mut project, &mut selection);
            assert!(!panel.has_pending());
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    panel.show(ui, &mut project, &mut selection);
                });
            });
            assert_eq!(project, original, "{kind}");
            panel.draft["description"] = json!("修改后的说明");
            panel.apply_pending(&mut project).unwrap();
            let saved: Value = serde_json::from_slice(&project.assets[&key]).unwrap();
            assert_eq!(saved["description"], "修改后的说明");
            lom_core::content::normalize_content_metadata(&saved).unwrap();
        }
    }

    fn fixture() -> (Project, String, String) {
        let mut project = Project::new();
        let mut keys = Vec::new();
        for (id, kind) in [("demo.hero", "character"), ("demo.scene", "image")] {
            let prefix = format!("assets/user/{kind}/{id}/");
            let key = format!("{prefix}content.json");
            project.assets.insert(
                key.clone(),
                serde_json::to_vec(&json!({
                    "schema":1,"content_schema":1,"id":id,"type":kind,"name":id,
                    "files":{"main":"main.png"}
                }))
                .unwrap(),
            );
            project
                .assets
                .insert(format!("{prefix}main.png"), vec![1, 2, 3]);
            keys.push(key);
        }
        (project, keys[0].clone(), keys[1].clone())
    }

    #[test]
    fn viewing_content_does_not_materialize_defaults_or_mark_dirty() {
        let (mut project, first, second) = fixture();
        let original = project.assets.clone();
        let mut panel = ContentPanel::default();
        let mut selection = first;
        assert!(!panel.select(&mut project, &mut selection));
        assert_eq!(panel.draft["art_facing"], "left");
        assert!(!panel.has_pending());
        selection = second;
        assert!(!panel.select(&mut project, &mut selection));
        assert_eq!(project.assets, original);
    }

    #[test]
    fn switching_content_applies_names_and_files_before_loading_the_next_item() {
        let (mut project, first, second) = fixture();
        let mut panel = ContentPanel::default();
        let mut selection = first.clone();
        panel.select(&mut project, &mut selection);
        panel.draft["name"] = json!("新角色名");
        panel.draft["files"]["main"] = json!("new.png");
        panel.files.insert("new.png".into(), vec![4, 5, 6]);
        assert!(panel.has_pending());
        selection = second.clone();
        assert!(panel.select(&mut project, &mut selection));
        assert_eq!(selection, second);
        assert!(!panel.has_pending());
        let metadata: Value = serde_json::from_slice(&project.assets[&first]).unwrap();
        assert_eq!(metadata["name"], "新角色名");
        assert_eq!(metadata["files"]["main"], "new.png");
        assert_eq!(
            project.assets["assets/user/character/demo.hero/new.png"],
            vec![4, 5, 6]
        );
    }

    #[test]
    fn invalid_content_keeps_input_and_selection_and_cannot_be_saved() {
        let (mut project, first, second) = fixture();
        let original = project.assets.clone();
        let mut panel = ContentPanel::default();
        let mut selection = first.clone();
        panel.select(&mut project, &mut selection);
        panel.draft["files"]["main"] = json!("missing.png");
        let input = panel.draft.clone();
        selection = second;
        assert!(!panel.select(&mut project, &mut selection));
        assert_eq!(selection, first);
        assert_eq!(panel.draft, input);
        assert!(panel.has_pending());
        assert!(panel.apply_pending(&mut project).is_err());
        assert_eq!(project.assets, original);
        panel.reset();
        panel.select(&mut project, &mut selection);
        assert!(!panel.has_pending());
        assert_eq!(panel.draft["files"]["main"], "main.png");
    }

    #[test]
    fn committing_normalized_content_keeps_editor_and_saved_values_in_sync() {
        let (mut project, first, _) = fixture();
        let mut panel = ContentPanel::default();
        let mut selection = first.clone();
        panel.select(&mut project, &mut selection);
        panel.draft["name"] = json!("  林灯 旅人  ");
        panel.draft["scale"] = json!(200);
        assert!(panel.apply_pending(&mut project).unwrap());
        let saved: Value = serde_json::from_slice(&project.assets[&first]).unwrap();
        assert_eq!(saved["name"], "林灯 旅人");
        assert_eq!(saved["scale"], 200);
        assert_eq!(panel.draft["name"], saved["name"]);
        assert_eq!(panel.draft["scale"], saved["scale"]);
        assert!(!panel.has_pending());
    }

    #[test]
    fn recovery_snapshot_includes_pending_content_without_normalizing_active_input() {
        let (mut project, first, _) = fixture();
        let mut panel = ContentPanel::default();
        let mut selection = first.clone();
        panel.select(&mut project, &mut selection);
        panel.draft["name"] = json!("  林灯 旅人  ");
        panel.draft["files"]["main"] = json!("new.png");
        panel.files.insert("new.png".into(), vec![4, 5, 6]);
        let original = project.clone();
        let source = panel.source.clone();
        let baseline = panel.baseline.clone();
        let input = panel.draft.clone();
        let files = panel.files.clone();
        let mut recovery = project.clone();
        assert!(panel.apply_pending_to_snapshot(&mut recovery).unwrap());

        let dir = tempfile::tempdir().unwrap();
        recovery.save_to(dir.path()).unwrap();
        let restored = Project::open(dir.path()).unwrap();
        let metadata: Value = serde_json::from_slice(&restored.assets[&first]).unwrap();
        assert_eq!(metadata["name"], "林灯 旅人");
        assert_eq!(metadata["files"]["main"], "new.png");
        assert_eq!(
            restored.assets["assets/user/character/demo.hero/new.png"],
            vec![4, 5, 6]
        );
        assert_eq!(project, original);
        assert_eq!(panel.source, source);
        assert_eq!(panel.baseline, baseline);
        assert_eq!(panel.draft, input);
        assert_eq!(panel.files, files);
        assert!(panel.has_pending());
    }

    #[test]
    fn invalid_pending_content_cannot_partially_modify_recovery_snapshot() {
        let (mut project, first, _) = fixture();
        let mut panel = ContentPanel::default();
        let mut selection = first;
        panel.select(&mut project, &mut selection);
        panel.draft["name"] = json!("保留输入");
        panel.draft["files"]["main"] = json!("missing.png");
        let input = panel.draft.clone();
        let mut recovery = project.clone();
        assert!(panel.apply_pending_to_snapshot(&mut recovery).is_err());
        assert_eq!(recovery, project);
        assert_eq!(panel.draft, input);
        assert!(panel.has_pending());
    }
}
