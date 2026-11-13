//! One entry for original-chapter events, anchored insertions and full editing.
use super::*;

impl App {
    pub(super) fn open_original_content(&mut self, ctx: &egui::Context) {
        if self.project.manifest["play_mode"] != "addon" {
            self.request(Pending::NewVanillaPatch, ctx);
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        let existing = self
            .project
            .stories
            .get(&self.current)
            .filter(|story| story.get("vanilla_patch").is_some())
            .map(|_| self.current.clone())
            .or_else(|| {
                self.project
                    .stories
                    .iter()
                    .find(|(_, story)| story.get("vanilla_patch").is_some())
                    .map(|(id, _)| id.clone())
            });
        if let Some(id) = existing {
            self.current = id;
        } else {
            let used: BTreeSet<_> = self
                .project
                .stories
                .values()
                .filter_map(lom_core::vanilla_import::target)
                .map(str::to_owned)
                .collect();
            let target = if used
                .iter()
                .all(|target| !target.eq_ignore_ascii_case("ch1_1"))
            {
                Some("ch1_1".to_owned())
            } else {
                self.vanilla.next_target(&used)
            };
            let Some(target) = target else {
                self.status = crate::i18n::key("vanilla.content_already_imported");
                return;
            };
            let id = (1..)
                .map(|index| format!("patch{index}"))
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
            match lom_core::vanilla_patch::new_story(&id, &target) {
                Ok(story) => {
                    self.project.stories.insert(id.clone(), story);
                    self.current = id;
                    self.track();
                    self.flush();
                }
                Err(error) => {
                    self.error = Some(crate::error_ui::capture(error));
                    return;
                }
            }
        }
        self.selected = 0;
        self.multiselect.clear();
        self.search.clear();
        self.center = Center::Node;
        self.vanilla.open_content();
    }

    pub(super) fn add_before_original_event(
        &mut self,
        target: &str,
        chapter: &str,
    ) -> anyhow::Result<()> {
        let source = self
            .project
            .stories
            .get(chapter)
            .context("选择的剧情章节已不存在")?;
        anyhow::ensure!(
            source.get("vanilla_patch").is_none() && source.get("vanilla_import").is_none(),
            "请选择本作品的普通剧情章节"
        );
        let mut config = self
            .project
            .scenario
            .clone()
            .context("请先创建原版附加剧情作品")?;
        let events = config["events"]
            .as_array_mut()
            .context("事件配置格式无效")?;
        let id = crate::scenario_panel::unique(events, "before_original");
        let mut event = crate::scenario_panel::new_event(&id, chapter);
        event["name"] = json!(crate::i18n::message(
            "vanilla.content_event_name",
            &[("target", target.to_owned())]
        ));
        event["trigger"] = json!("story_entry");
        event["target"] = json!(target);
        event.as_object_mut().unwrap().remove("position");
        events.push(event);
        self.flush();
        self.project.scenario = Some(config);
        self.scenario_selection.event = Some(id);
        self.current = chapter.to_owned();
        self.selected = 0;
        self.multiselect.clear();
        self.center = Center::Events;
        self.track();
        self.flush();
        self.status = crate::i18n::key("vanilla.content_event_done");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(project: Project) -> App {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = App::new(&cc, None, None);
        app.recovery_candidates.clear();
        app.auto_save_project = false;
        app.install(project);
        app
    }

    #[test]
    fn original_content_launcher_keeps_authored_chapters_and_adds_an_undoable_patch() {
        let mut project = Project::new_scenario("addon");
        project.stories.get_mut("main").unwrap()["title"] = json!("写好的一整段");
        let mut app = app(project);
        let original = snapshot(&app.project);
        let source = app.project.stories["main"].clone();
        app.open_original_content(&egui::Context::default());
        assert_eq!(app.project.stories["main"], source);
        assert!(app.story().get("vanilla_patch").is_some());
        assert!(app.pending.is_none());
        let current = app.current.clone();
        app.open_original_content(&egui::Context::default());
        assert_eq!(app.current, current);
        assert_eq!(app.project.stories.len(), 2);
        app.undo();
        assert!(snapshot(&app.project) == original);
    }

    #[test]
    fn chapter_before_event_keeps_the_patch_and_segment_and_roundtrips() {
        let mut project = Project::new_vanilla_patch("DEMO").unwrap();
        let mut segment = Project::new().stories["main"].clone();
        segment["id"] = json!("extra");
        project.stories.insert("extra".into(), segment.clone());
        let mut app = app(project);
        let original = snapshot(&app.project);
        app.add_before_original_event("DEMO", "extra").unwrap();
        assert_eq!(app.project.stories["extra"], segment);
        assert!(app.project.stories["main"].get("vanilla_patch").is_some());
        let event = &app.project.scenario.as_ref().unwrap()["events"][0];
        assert_eq!(event["trigger"], "story_entry");
        assert_eq!(event["target"], "DEMO");
        assert_eq!(event["script"], "extra");
        assert!(event.get("position").is_none());
        let inserted = snapshot(&app.project);
        app.undo();
        assert!(snapshot(&app.project) == original);
        app.redo();
        assert!(snapshot(&app.project) == inserted);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("project");
        app.project.save_to(&path).unwrap();
        let reopened = Project::open(&path).unwrap();
        let package = temp.path().join("chapter-before.lommod");
        reopened.export(&package).unwrap();
        assert_eq!(Project::open(&package).unwrap().scenario, reopened.scenario);
    }

    #[test]
    fn original_content_ui_creates_a_segment_and_connects_it_before_the_chosen_chapter() {
        let mut app = app(Project::new_vanilla_patch("DEMO").unwrap());
        app.vanilla = crate::vanilla_panel::Panel::with_source(
            "DEMO",
            "setsaydialog(saydialogs.narrative)\nsay(\"原文\")\n",
        );
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1500.0, 1800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.center(ctx),
            )
        };
        let click = |output: egui::FullOutput, key: &str| {
            let label = crate::i18n::key(key);
            let target = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .into_iter()
                .find(|(_, node)| node.label() == Some(label.as_str()))
                .unwrap()
                .0;
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )]
        };
        let output = draw(&mut app, vec![]);
        draw(&mut app, click(output, "vanilla.segment_new"));
        assert!(app.chapter_dialog.is_some());
        app.chapter_dialog.as_mut().unwrap().name = "整段草稿".into();
        app.commit_chapter().unwrap();
        let segment = app.current.clone();
        let source = app.story();
        assert!(source.get("vanilla_patch").is_none());
        app.open_original_content(&ctx);
        let output = draw(&mut app, vec![]);
        draw(&mut app, click(output, "vanilla.content_before"));
        let output = draw(&mut app, vec![]);
        draw(&mut app, click(output, "vanilla.content_create_event"));
        assert_eq!(app.center, Center::Events);
        let event = &app.project.scenario.as_ref().unwrap()["events"][0];
        assert_eq!(event["target"], "DEMO");
        assert_eq!(event["script"], segment);
        assert_eq!(app.project.stories[&segment], source);
    }
}
