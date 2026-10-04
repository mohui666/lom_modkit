use crate::i18n::tr;
use eframe::egui;
use lom_core::{content_edit, editing, project::Project};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct ContentPanel {
    key: String,
    source: Vec<u8>,
    draft: Value,
    baseline: Value,
    files: BTreeMap<String, Vec<u8>>,
    slot: usize,
    portrait: String,
    message: String,
    preview: Option<(String, egui::TextureHandle)>,
    replace_shared: bool,
}
impl ContentPanel {
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
        self.key.clear();
        self.files.clear();
        self.preview = None;
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
                    self.message = format!("修改尚未完成：{error}");
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
                self.message = "素材已被其他操作修改；请撤销未完成的输入后重试".into();
                return changed;
            }
            self.key = key.into();
            self.source = bytes.clone();
            self.draft = draft_from_source(bytes);
            self.baseline = self.draft.clone();
            self.files.clear();
            self.preview = None;
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
            for (key, label, field_kind) in [
                ("title", "称号", "str"),
                ("scale", "立绘缩放", "int"),
                ("art_facing", "原图朝向", "facing"),
            ] {
                catalog.field(ui, &mut self.draft, key, label, field_kind, false, &[], &[]);
            }
            if self.draft["intro"].is_object() {
                ui.collapsing(tr("角色介绍卡"), |ui| {
                    for (key, label, field_kind) in [
                        ("name", "名称", "str"),
                        ("title", "称号", "str"),
                        ("text", "简介", "multiline"),
                        ("image_scale", "图片缩放", "int"),
                        ("image_x", "横向偏移", "int"),
                        ("image_y", "纵向偏移", "int"),
                    ] {
                        catalog.field(
                            ui,
                            &mut self.draft["intro"],
                            key,
                            label,
                            field_kind,
                            false,
                            &[],
                            &[],
                        );
                    }
                });
            }
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
            .selected_text(tr(slots[self.slot]))
            .show_ui(ui, |ui| {
                for (i, label) in slots.iter().enumerate() {
                    ui.selectable_value(&mut self.slot, i, tr(label));
                }
            });
        if self.slot == 1 {
            ui.horizontal(|ui| {
                ui.label(tr("表情 ID"));
                ui.text_edit_singleline(&mut self.portrait);
            });
        }
        if ui.button(tr("添加或替换素材文件…")).clicked() {
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
                                anyhow::ensure!(
                                    !self.portrait.is_empty()
                                        && self
                                            .portrait
                                            .bytes()
                                            .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                                    "表情 ID 不合法"
                                );
                                self.draft["portraits"][&self.portrait] = json!(name);
                            }
                            2 => {
                                anyhow::ensure!(self.draft["intro"].is_object(), "请先添加介绍卡");
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
                self.message = result
                    .map(|_| String::new())
                    .unwrap_or_else(|e| e.to_string());
            }
        }
        // Keep in-progress typing intact (including spaces). Leaving the input,
        // choosing a file, switching resources, or saving commits the draft.
        if !ui.ctx().wants_keyboard_input() || !self.files.is_empty() {
            match self.apply_pending(project) {
                Ok(applied) => changed |= applied,
                Err(error) => self.message = format!("修改尚未完成：{error}"),
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
                    Err(e) => self.message = e.to_string(),
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
        ui.collapsing(format!("{} 处内容引用", refs.len()), |ui| {
            for r in refs {
                ui.label(format!(
                    "{}/{} · {}",
                    r.story_id,
                    r.node_id.unwrap_or_default(),
                    r.field
                ));
            }
        });
        if kind != "audio" {
            let file = if kind == "character" {
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
                self.message = content_edit::store_shared(&root, project, key, self.replace_shared)
                    .map(|b| {
                        format!(
                            "已保存到共享库；备份：{}",
                            b.map(|p| p.display().to_string())
                                .unwrap_or_else(|| "首次创建".into())
                        )
                    })
                    .unwrap_or_else(|e| e.to_string());
            }
            if ui.button(tr("从共享库移入回收目录")).clicked() {
                self.message =
                    content_edit::remove_shared(&root, self.draft["id"].as_str().unwrap_or(""))
                        .map(|p| format!("已保留于 {}", p.display()))
                        .unwrap_or_else(|e| e.to_string());
            }
        });
        if !self.message.is_empty() {
            if self.has_pending() {
                ui.colored_label(egui::Color32::from_rgb(179, 55, 49), &self.message);
            } else {
                ui.label(&self.message);
            }
        }
        changed
    }
}
fn draft_from_source(bytes: &[u8]) -> Value {
    let mut draft: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
    if draft["type"] == "character" {
        for (key, default) in [
            ("title", json!("")),
            ("scale", json!(100)),
            ("art_facing", json!("left")),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(saved["scale"], 130);
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
