//! Project lifecycle and chapter operations.
use super::*;

impl App {
    pub(super) fn resolve_project_source(&mut self, path: &Path) -> Option<PathBuf> {
        match project_io::project_sources(path) {
            Ok(mut candidates) if candidates.len() == 1 => candidates.pop(),
            Ok(candidates) => {
                self.project_choice = Some(project_io::ProjectChoice {
                    directory: path.to_owned(),
                    candidates,
                    selected: 0,
                });
                None
            }
            Err(error) => {
                self.error = Some(format!("打开失败\n{}\n{error:#}", path.display()));
                None
            }
        }
    }

    pub(super) fn project_dialog_directory(&self) -> Option<&Path> {
        crate::file_dialog::project_directory(&self.project).or_else(|| {
            self.recent.iter().find_map(|source| {
                let directory = if source.is_dir() {
                    source.as_path()
                } else {
                    source.parent()?
                };
                directory.is_dir().then_some(directory)
            })
        })
    }

    pub(super) fn project_dialog(&self) -> rfd::FileDialog {
        crate::file_dialog::at(self.project_dialog_directory())
    }

    pub(super) fn export_dialog_directory(&self) -> Option<&Path> {
        self.usability
            .export_directory
            .as_deref()
            .filter(|directory| directory.is_dir())
            .or_else(|| self.project_dialog_directory())
    }

    pub(super) fn export_dialog(&self) -> rfd::FileDialog {
        crate::file_dialog::at(self.export_dialog_directory())
    }

    pub(super) fn merge_project_at(&mut self, path: &Path) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        let result = open_project(path)
            .and_then(|incoming| lom_core::project_merge::merge(&mut self.project, &incoming));
        match result {
            Ok(report) => {
                self.asset_dirty |= report.assets > 0;
                self.track();
                self.status = crate::i18n::message(
                    "editor.merge.done",
                    &[
                        ("chapters", report.chapters.to_string()),
                        ("assets", report.assets.to_string()),
                        ("events", report.events.to_string()),
                    ],
                );
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    pub(super) fn remember_source(&mut self) {
        if let Some(path) = self.project.source.clone() {
            if path.starts_with(user_root().join("rust/recovery")) {
                return;
            }
            self.recent.retain(|p| p != &path);
            self.recent.insert(0, path);
            self.recent.truncate(12);
            if cfg!(test) {
                return;
            }
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
    pub(super) fn begin_chapter(&mut self, duplicate: bool) {
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
            (ChapterAction::Duplicate(story.clone()), name)
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
    pub(super) fn commit_chapter(&mut self) -> anyhow::Result<()> {
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
    pub(super) fn begin_chapter_group(&mut self, rename: bool) {
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
    pub(super) fn move_chapter_to_group(&mut self, group: &str) -> anyhow::Result<()> {
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
    pub(super) fn chapter_reference_count(&self, chapter: &str) -> usize {
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
    pub(super) fn begin_delete_chapter(&mut self) {
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
    pub(super) fn commit_delete_chapter(&mut self) -> anyhow::Result<()> {
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
    pub(super) fn commit_rename(&mut self) -> anyhow::Result<()> {
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
    pub(super) fn request(&mut self, p: Pending, ctx: &egui::Context) {
        if self.managed_tab && !matches!(p, Pending::EditorUpdate | Pending::CloseTab) {
            self.tab_action = Some(p);
            return;
        }
        // Downloading only prepares the update. Ask about unsaved work once,
        // immediately before installation and exit, including any newer edits.
        if matches!(p, Pending::EditorUpdate) {
            if self.updater.ready_to_install() {
                self.request(Pending::InstallEditorUpdate, ctx);
            } else {
                self.updater.download(ctx);
            }
            return;
        }
        self.track();
        self.flush();
        if self.dirty() || self.saving() {
            self.begin_pending_action(p);
        } else {
            self.perform(p, ctx);
        }
    }
    pub(super) fn perform(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::CloseTab => self.tab_action = Some(Pending::CloseTab),
            Pending::EditorUpdate => self.request(Pending::EditorUpdate, ctx),
            Pending::InstallEditorUpdate => {
                match self.updater.install(self.project.source.as_deref()) {
                    Ok(()) => {
                        self.allow_close = true;
                        self.clear_recovery();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        self.error = Some(format!("更新未完成，编辑器尚未退出。\n{error:#}"));
                    }
                }
            }
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
            Pending::OpenCheckpoint(path) => match Project::open(&path) {
                Ok(mut project) => {
                    project.source = None;
                    self.install(project);
                    self.document_dirty = true;
                    self.status = crate::i18n::key("editor.checkpoints.opened");
                }
                Err(error) => {
                    self.error = Some(crate::error_ui::capture(
                        error.context(crate::i18n::key("editor.checkpoints.open_failed")),
                    ))
                }
            },
            Pending::Open(path) => {
                if let Some(source) = self.resolve_project_source(&path) {
                    match Project::open(&source) {
                        Ok(p) => self.install(p),
                        Err(e) => {
                            self.error = Some(format!("打开失败\n{}\n{e:#}", source.display()))
                        }
                    }
                }
            }
        }
    }
    pub(super) fn open_vanilla_routes(&mut self, ctx: &egui::Context) {
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
    pub(super) fn open_story_bookmark(
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
    pub(super) fn install(&mut self, p: Project) {
        self.preview.cancel_layout_drag();
        self.reset_save_project();
        self.clear_recovery();
        self.project = p;
        self.checkpoint_entries = None;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.last_auto_save = Instant::now();
        self.center = Center::Node;
        self.scenario_selection = Default::default();
        self.advanced = Default::default();
        self.content_panel.reset();
        self.vanilla = Default::default();
        self.chapter_dialog = None;
        self.project_choice = None;
        self.delete_chapter_dialog = None;
        self.rename_dialog = None;
        self.asset_import_dialog = None;
        self.video_dialog = None;
        self.font_conversion_dialog = None;
        self.image_import_target = None;
        self.audio.stop();
        self.auto = false;
        self.preview_audio.stop();
        self.preview_buffering = None;
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
        if let Some(parent) = self
            .project
            .source
            .as_ref()
            .filter(|source| {
                source.is_file() && source.extension().and_then(|s| s.to_str()) == Some("json")
            })
            .filter(|_| !self.project.has_manifest)
            .and_then(|source| source.parent())
        {
            self.status = crate::i18n::message(
                "workspace.open.standalone_notice",
                &[("directory", parent.display().to_string())],
            );
        }
        if let Some(notice) = self
            .project
            .stories
            .values()
            .find_map(|story| story["lua_import"]["notice"].as_str())
        {
            self.status = notice.to_owned();
        }
        self.repair_legacy_portrait_overrides();
    }
    pub(super) fn save(&mut self, as_new: bool) -> bool {
        if !self.apply_pending_drafts() {
            return false;
        }
        self.flush();
        let path = if as_new { None } else { self.save_target() };
        let path = path.or_else(|| {
            self.project_dialog()
                .set_title("选择项目保存目录（保存全部章节与素材）")
                .pick_folder()
        });
        let Some(path) = path else { return false };
        self.save_at(&path, false)
    }
    pub(super) fn save_target(&self) -> Option<PathBuf> {
        self.project.source.clone().filter(|path| {
            path.extension().and_then(|x| x.to_str()) != Some("json")
                || (self.project.stories.len() == 1 && self.project.manifest == self.saved.manifest)
        })
    }
    pub(super) fn save_at(&mut self, path: &Path, automatic: bool) -> bool {
        self.queue_project_save(path, automatic)
    }
    pub(super) fn export(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        if !self.check_before_run_or_export() {
            return;
        }
        let dialog = self.export_dialog().add_filter("活侠传 Mod", &["lommod"]);
        if let Some(path) = dialog
            .set_file_name(format!(
                "{}.lommod",
                self.project.manifest["id"].as_str().unwrap_or("my_mod")
            ))
            .save_file()
        {
            match self.project.export(&path) {
                Ok(p) => {
                    self.remember_export_directory(&path);
                    self.status = format!("已导出 {}", p.display());
                }
                Err(e) => self.error = Some(format!("导出失败\n{e:#}")),
            }
        }
    }
    pub(super) fn export_play_bundle(&mut self) {
        self.export_player_package(crate::play_bundle::Format::Zip);
    }
    pub(super) fn export_player_package(&mut self, format: crate::play_bundle::Format) {
        if self.apply_pending_drafts() && self.check_before_run_or_export() {
            self.flush();
            let directory = self.export_dialog_directory().map(Path::to_path_buf);
            if let Some(path) = self
                .tools_panel
                .export_play_bundle_in(&self.project, directory.as_deref(), format)
            {
                self.remember_export_directory(&path);
            }
        }
    }
}
