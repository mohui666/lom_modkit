//! Project assets, pending drafts and asset imports.
use super::*;

impl App {
    pub(super) fn assets(&mut self, ui: &mut egui::Ui) {
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
            .button(crate::i18n::key("usability.authoring.talent"))
            .clicked()
        {
            self.open_custom_talent();
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
                        "talent" => tr("自定义天赋"),
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
    pub(super) fn apply_pending_drafts(&mut self) -> bool {
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
    pub(super) fn commit_asset_import(&mut self) -> anyhow::Result<()> {
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
            10 => "talent",
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
                    let field = if node["type"] == "cg" { "key" } else { "image" };
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
    pub(super) fn import_assets(
        &mut self,
        paths: Vec<PathBuf>,
        id: &str,
        name: &str,
        asset_kind: usize,
    ) -> anyhow::Result<()> {
        self.import_assets_with_target(paths, id, name, asset_kind, "")
    }
    pub(super) fn import_assets_with_target(
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
            10 => "talent",
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
        anyhow::ensure!(kind == "talent" || !main.is_empty(), "未选择素材");
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
}
