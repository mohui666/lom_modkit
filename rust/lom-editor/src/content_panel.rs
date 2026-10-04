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
    files: BTreeMap<String, Vec<u8>>,
    slot: usize,
    portrait: String,
    message: String,
    preview: Option<(String, egui::TextureHandle)>,
    replace_shared: bool,
}
impl ContentPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, project: &mut Project, key: &str) -> bool {
        let Some(bytes) = project.assets.get(key) else {
            return false;
        };
        if self.key != key || self.source != *bytes {
            self.key = key.into();
            self.source = bytes.clone();
            self.draft = serde_json::from_slice(bytes).unwrap_or(Value::Null);
            self.files.clear();
            self.preview = None;
            self.message.clear();
        }
        if !self.draft.is_object() {
            ui.label(tr("内容元数据无效"));
            return false;
        }
        let mut changed = false;
        ui.separator();
        let symbol = format!("user:{}", self.draft["id"].as_str().unwrap_or(""));
        if ui.button(tr("复制引用")).clicked() {
            ui.ctx().copy_text(symbol.clone());
        }
        let kind = self.draft["type"].as_str().unwrap_or("").to_owned();
        if kind == "character" {
            if self.draft.get("intro").is_none() && ui.button(tr("添加角色介绍卡")).clicked()
            {
                self.draft["intro"] = json!({"name":self.draft["name"],"title":"","text":"","image_scale":100,"image_x":0,"image_y":0});
            }
            if self.draft.get("intro").is_some() && ui.button(tr("移除介绍卡")).clicked() {
                self.draft.as_object_mut().unwrap().remove("intro");
            }
            for (key, default) in [
                ("title", json!("")),
                ("scale", json!(100)),
                ("art_facing", json!("right")),
            ] {
                if self.draft.get(key).is_none() {
                    self.draft[key] = default;
                }
            }
        }
        if kind == "audio" {
            let mut character = self.draft["character"].as_str().unwrap_or("").to_string();
            ui.horizontal(|ui| {
                ui.label(tr("配音绑定人物（空值解除绑定）"));
                if ui.text_edit_singleline(&mut character).changed() {
                    if character.is_empty() {
                        self.draft.as_object_mut().unwrap().remove("character");
                    } else {
                        self.draft["character"] = json!(character);
                    }
                }
            });
        }
        let catalog = crate::forms::Catalog::new();
        catalog.field(ui, &mut self.draft, "name", "名称", "str", false, &[], &[]);
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
            .selected_text(slots[self.slot])
            .show_ui(ui, |ui| {
                for (i, label) in slots.iter().enumerate() {
                    ui.selectable_value(&mut self.slot, i, *label);
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
                    .map(|_| "已载入草稿，应用后生效".into())
                    .unwrap_or_else(|e| e.to_string());
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("应用修改")).clicked() {
                match content_edit::update(project, key, &self.draft, &self.files) {
                    Ok(()) => {
                        changed = true;
                        self.key.clear();
                        self.message = "已应用".into();
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            if ui.button(tr("放弃内容草稿")).clicked() {
                self.key.clear();
            }
            if ui.button(tr("删除项目内容（可撤销）")).clicked() {
                match content_edit::remove(project, key) {
                    Ok(()) => {
                        changed = true;
                        self.key.clear();
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
            if ui.button(tr("保存当前已应用内容到共享库")).clicked() {
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
            ui.label(&self.message);
        }
        changed
    }
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
