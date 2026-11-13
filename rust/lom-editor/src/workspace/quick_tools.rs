//! Command search and reviewed, batch dialogue insertion.
use super::*;

const STATE_ID: &str = "workspace-quick-tools";

#[derive(Clone, Default)]
struct QuickToolsState {
    palette: Option<CommandPalette>,
    dialogue: Option<DialogueDraft>,
}

#[derive(Clone)]
struct CommandPalette {
    query: String,
    selected: usize,
    focus: bool,
    composing: bool,
    reveal_selection: bool,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            query: String::new(),
            selected: 0,
            focus: true,
            composing: false,
            reveal_selection: false,
        }
    }
}

#[derive(Clone)]
enum QuickAction {
    Dialogue,
    RenameCharacter,
    CustomTalent,
    Export,
    AddNode(String),
    CharacterSay(String),
    Chapter(String),
    Asset(String),
    Center(Center),
    Tool(ToolPage),
    View(View),
    Save,
    Undo,
    Redo,
    PlayCurrent,
}

struct CommandEntry {
    label: String,
    detail: String,
    category: String,
    search: String,
    action: QuickAction,
}

impl CommandEntry {
    fn new(
        label: String,
        detail: String,
        category: String,
        aliases: &str,
        action: QuickAction,
    ) -> Self {
        let search = format!("{label} {detail} {category} {aliases}").to_lowercase();
        Self {
            label,
            detail,
            category,
            search,
            action,
        }
    }
}

#[derive(Clone, Debug)]
struct DialogueActor {
    id: String,
    label: String,
    aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Speaker {
    Narration,
    Character(String),
    Unresolved,
}

#[derive(Clone, Debug)]
struct DialogueRow {
    line: usize,
    source_speaker: String,
    speaker: Speaker,
    text: String,
    actor_search: String,
}

#[derive(Clone)]
struct DialogueDraft {
    chapter: String,
    anchor: Option<String>,
    source: String,
    parsed_source: String,
    rows: Vec<DialogueRow>,
    focus: bool,
    composing: bool,
    error: Option<String>,
}

fn qt(key: &str) -> String {
    crate::i18n::key(&format!("quick_tools.{key}"))
}

fn qmessage(key: &str, values: &[(&str, String)]) -> String {
    crate::i18n::message(&format!("quick_tools.{key}"), values)
}

fn state(ctx: &egui::Context) -> QuickToolsState {
    ctx.data_mut(|data| data.get_temp(egui::Id::new(STATE_ID)))
        .unwrap_or_default()
}

fn store_state(ctx: &egui::Context, state: QuickToolsState) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(STATE_ID), state));
}

/// IME confirmation and open picker popups own Return and Escape.
fn quick_dialog_keys(ctx: &egui::Context, composing: &mut bool) -> (bool, bool, isize) {
    let was_composing = *composing;
    let mut ime_event = false;
    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Ime(event) = event {
                ime_event = true;
                match event {
                    egui::ImeEvent::Enabled => {}
                    egui::ImeEvent::Preedit(text) => *composing = !text.is_empty(),
                    egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => *composing = false,
                }
            }
        }
    });
    if was_composing || *composing || ime_event || egui::Popup::is_any_open(ctx) {
        return (false, false, 0);
    }
    ctx.input_mut(|input| {
        let enter = input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        let escape = input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        let movement = if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            1
        } else if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            -1
        } else {
            0
        };
        (enter, escape, movement)
    })
}

fn narration_name(name: &str) -> bool {
    name.is_empty()
        || [
            "旁白",
            "旁白/叙述",
            "旁白／敘述",
            "叙述",
            "敘述",
            "narration",
            "narrator",
            "ナレーション",
            "내레이션",
        ]
        .iter()
        .any(|label| name.eq_ignore_ascii_case(label))
        || name == qt("narration")
}

fn match_speaker(name: &str, actors: &[DialogueActor]) -> Speaker {
    if narration_name(name) {
        return Speaker::Narration;
    }
    // An explicit ID is authoritative; a name can identify several appearances.
    if let Some(actor) = actors.iter().find(|actor| actor.id == name) {
        return Speaker::Character(actor.id.clone());
    }
    let matches: Vec<_> = actors
        .iter()
        .filter(|actor| actor.aliases.iter().any(|alias| alias == name))
        .collect();
    if matches.len() == 1 {
        Speaker::Character(matches[0].id.clone())
    } else {
        Speaker::Unresolved
    }
}

fn parse_dialogue(source: &str, actors: &[DialogueActor]) -> Vec<DialogueRow> {
    source
        .lines()
        .enumerate()
        .filter_map(|(line, text)| {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            // Match complete IDs before splitting: custom IDs contain a colon.
            let explicit_id = actors
                .iter()
                .filter_map(|actor| {
                    text.strip_prefix(&actor.id)
                        .and_then(|rest| {
                            let rest = rest.trim_start();
                            rest.strip_prefix('：').or_else(|| rest.strip_prefix(':'))
                        })
                        .map(|rest| (actor.id.as_str(), rest))
                })
                .max_by_key(|(id, _)| id.len());
            let (speaker, text) = explicit_id
                .or_else(|| {
                    text.starts_with("user:")
                        .then(|| text.split_once('：'))
                        .flatten()
                })
                .or_else(|| {
                    let at = text.find(['：', ':'])?;
                    let (speaker, rest) = text.split_at(at);
                    Some((speaker, &rest[rest.chars().next()?.len_utf8()..]))
                })
                .unwrap_or(("", text));
            let source_speaker = speaker.trim().to_owned();
            Some(DialogueRow {
                line: line + 1,
                speaker: match_speaker(&source_speaker, actors),
                source_speaker,
                text: text.trim().to_owned(),
                actor_search: String::new(),
            })
        })
        .collect()
}

fn dialogue_ready(rows: &[DialogueRow], actors: &[DialogueActor]) -> bool {
    !rows.is_empty()
        && rows.iter().all(|row| {
            !row.text.trim().is_empty()
                && match &row.speaker {
                    Speaker::Narration => true,
                    Speaker::Character(id) => actors.iter().any(|actor| &actor.id == id),
                    Speaker::Unresolved => false,
                }
        })
}

fn apply_same_speaker(rows: &mut [DialogueRow], name: &str, speaker: &Speaker) {
    if name.is_empty() || *speaker == Speaker::Unresolved {
        return;
    }
    for row in rows.iter_mut().filter(|row| row.source_speaker == name) {
        row.speaker = speaker.clone();
    }
}

fn command_matches(entry: &CommandEntry, query: &str) -> bool {
    query
        .split_whitespace()
        .all(|term| entry.search.contains(term))
}

impl App {
    /// Call after the existing modal guard in `shortcuts`; true suppresses
    /// document shortcuts while a quick tool owns keyboard input.
    pub(super) fn quick_tools_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let current = state(ctx);
        if current.palette.is_some() || current.dialogue.is_some() {
            return true;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.open_command_palette(ctx);
            return true;
        }
        false
    }

    pub(super) fn open_command_palette(&mut self, ctx: &egui::Context) {
        let mut current = state(ctx);
        if current.dialogue.is_none() {
            current.palette = Some(CommandPalette::default());
            store_state(ctx, current);
        }
    }

    pub(super) fn open_dialogue_authoring(&mut self, ctx: &egui::Context) {
        if self.is_vanilla_patch() || !self.project.stories.contains_key(&self.current) {
            self.status = qt("dialogue_unavailable");
            return;
        }
        if !self.apply_pending_drafts() {
            return;
        }
        let mut current = state(ctx);
        current.palette = None;
        if current.dialogue.is_none() {
            current.dialogue = Some(DialogueDraft {
                chapter: self.current.clone(),
                anchor: self.story()["nodes"][self.selected]["id"]
                    .as_str()
                    .map(str::to_owned),
                source: String::new(),
                parsed_source: String::new(),
                rows: Vec::new(),
                focus: true,
                composing: false,
                error: None,
            });
        }
        store_state(ctx, current);
    }

    /// Draw after the main workspace and before the existing dialog stack.
    pub(super) fn quick_tools(&mut self, ctx: &egui::Context) {
        if self.error.is_some()
            || self.tools_panel.dialog_open()
            || self.settings_open
            || self.updater.open
        {
            return;
        }
        let mut current = state(ctx);
        let mut action = None;
        if let Some(palette) = &mut current.palette {
            let (close, chosen) = self.command_palette_ui(ctx, palette);
            action = chosen;
            if close {
                current.palette = None;
            }
        } else if let Some(draft) = &mut current.dialogue {
            let actors = self.dialogue_actors();
            let (cancel, submit) = dialogue_ui(ctx, draft, &actors);
            if cancel {
                current.dialogue = None;
            } else if submit && self.apply_pending_drafts() {
                match self.insert_dialogue_rows(
                    &draft.chapter,
                    draft.anchor.as_deref(),
                    &draft.rows,
                ) {
                    Ok(count) => {
                        self.status = qmessage("inserted", &[("count", count.to_string())]);
                        current.dialogue = None;
                    }
                    Err(error) => draft.error = Some(crate::error_ui::capture(error)),
                }
            }
        }
        store_state(ctx, current);
        if let Some(action) = action {
            self.execute_quick_action(ctx, action);
        }
    }

    fn dialogue_actors(&self) -> Vec<DialogueActor> {
        let names: BTreeMap<_, _> = self.catalog.data["characters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| {
                Some((
                    row["id"].as_str()?.to_owned(),
                    row["name"].as_str().unwrap_or("").to_owned(),
                ))
            })
            .collect();
        self.catalog
            .options("character", &json!({"type":"say"}), &[], &[])
            .into_iter()
            .map(|(id, label)| {
                let raw = names.get(&id).cloned().unwrap_or_default();
                let localized = crate::i18n::term("characters", &id, &raw);
                let mut aliases = vec![label.clone(), raw, localized];
                aliases.retain(|name| !name.is_empty());
                aliases.sort();
                aliases.dedup();
                DialogueActor { id, label, aliases }
            })
            .collect()
    }

    fn insert_dialogue_rows(
        &mut self,
        chapter: &str,
        anchor: Option<&str>,
        rows: &[DialogueRow],
    ) -> anyhow::Result<usize> {
        anyhow::ensure!(
            dialogue_ready(rows, &self.dialogue_actors()),
            "{}",
            qt("review_required")
        );
        anyhow::ensure!(self.current == chapter, "{}", qt("location_changed"));
        let mut story = self
            .project
            .stories
            .get(chapter)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(qt("location_changed")))?;
        anyhow::ensure!(
            story.get("vanilla_patch").is_none(),
            "{}",
            qt("dialogue_unavailable")
        );
        let existing = story["nodes"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!(qt("location_changed")))?;
        let selected = if let Some(anchor) = anchor {
            existing
                .iter()
                .position(|node| node["id"] == anchor)
                .ok_or_else(|| anyhow::anyhow!(qt("location_changed")))?
        } else {
            anyhow::ensure!(existing.is_empty(), "{}", qt("location_changed"));
            0
        };
        let mut used: BTreeSet<String> = existing
            .iter()
            .filter_map(|node| node["id"].as_str().map(str::to_owned))
            .collect();
        let mut inserted = Vec::with_capacity(rows.len());
        for row in rows {
            let id = (1..)
                .map(|index| format!("say{index}"))
                .find(|id| !used.contains(id))
                .unwrap();
            used.insert(id.clone());
            let mut node = self.catalog.new_node("say", id);
            node["text"] = json!(row.text);
            match &row.speaker {
                Speaker::Narration => {
                    node["mode"] = json!("narrative");
                    node.as_object_mut().unwrap().remove("character");
                    node.as_object_mut().unwrap().remove("portrait");
                }
                Speaker::Character(id) => node["character"] = json!(id),
                Speaker::Unresolved => unreachable!("reviewed above"),
            }
            if story.get("vanilla_import").is_some() {
                // This tool explicitly chooses the speaker and dialogue mode.
                node["inherit_dialog"] = json!(false);
            }
            inserted.push(node);
        }
        let at = steps::insert_nodes_after(&mut story, selected, inserted)?;
        // Finish prior edits, then record this batch as one complete history item.
        self.track();
        self.flush();
        self.project.stories.insert(chapter.to_owned(), story);
        self.selected = at;
        self.multiselect.clear();
        self.center = Center::Node;
        self.track();
        self.flush();
        Ok(rows.len())
    }

    fn command_entries(&self) -> Vec<CommandEntry> {
        let mut entries = Vec::new();
        let command = qt("category.command");
        let mut push = |label: &str, detail: &str, aliases: &str, action| {
            entries.push(CommandEntry::new(
                qt(label),
                detail.to_owned(),
                command.clone(),
                aliases,
                action,
            ));
        };
        if !self.is_vanilla_patch() && self.project.stories.contains_key(&self.current) {
            push(
                "rename_character",
                "",
                "人物改名 自定义名字 姓名 rename character",
                QuickAction::RenameCharacter,
            );
            push(
                "dialogue",
                "",
                "连续对白 dialogue 批量 台词",
                QuickAction::Dialogue,
            );
        }
        push(
            "custom_talent",
            "",
            "自定义天赋 创建天赋 talent",
            QuickAction::CustomTalent,
        );
        push("export", "", "导出作品 export", QuickAction::Export);
        push("save", "Ctrl+S", "save 保存", QuickAction::Save);
        if !self.undo.is_empty() || self.pending_history.is_some() {
            push("undo", "Ctrl+Z", "undo 撤销", QuickAction::Undo);
        }
        if !self.redo.is_empty() {
            push("redo", "Ctrl+Shift+Z", "redo 重做", QuickAction::Redo);
        }
        push(
            "assets",
            "",
            "assets 素材 library",
            QuickAction::Center(Center::Assets),
        );
        push(
            "chapter",
            "",
            "chapter 章节",
            QuickAction::Center(Center::Chapter),
        );
        push(
            "manifest",
            "",
            "project manifest 作品设置",
            QuickAction::Center(Center::Manifest),
        );
        push(
            "events",
            "",
            "events 事件",
            QuickAction::Center(Center::Events),
        );
        push(
            "tasks",
            "",
            "tasks 任务",
            QuickAction::Center(Center::Tasks),
        );
        push(
            "localization",
            "",
            "localization 多语言 翻译",
            QuickAction::Center(Center::Localization),
        );
        push(
            "game",
            "",
            "game 游戏 MOD",
            QuickAction::Tool(ToolPage::Game),
        );
        push(
            "release",
            "",
            "release export 发布 导出",
            QuickAction::Tool(ToolPage::Release),
        );
        push(
            "previews",
            "",
            "preview 素材预览 提取",
            QuickAction::Tool(ToolPage::Previews),
        );
        push(
            "stage",
            "",
            "stage 舞台 预览",
            QuickAction::View(View::Stage),
        );
        push(
            "graph",
            "F7",
            "graph 流程图",
            QuickAction::View(View::Graph),
        );
        push("lua", "", "lua 脚本", QuickAction::View(View::Lua));
        push(
            "checks",
            "F6",
            "check 检查 问题 diagnostics",
            QuickAction::View(View::Checks),
        );
        push("play", "F5", "play 试玩", QuickAction::PlayCurrent);

        let story = self.project.stories.get(&self.current);
        let imported = story.is_some_and(|story| story.get("vanilla_import").is_some());
        let structured = story.is_some_and(|story| story["vanilla_import"]["schema"] == 2);
        if !self.is_vanilla_patch() && story.is_some() {
            if let Some(schemas) = self.catalog.schema["NODE_SCHEMAS"].as_object() {
                for kind in schemas.keys().filter(|kind| {
                    (structured || !matches!(kind.as_str(), "native_action" | "native_branch"))
                        && (!imported || !matches!(kind.as_str(), "combat" | "battle"))
                        && (self.project.scenario.is_none()
                            || imported
                            || lom_core::scenario::performance_node(kind))
                }) {
                    entries.push(CommandEntry::new(
                        qmessage("add_node", &[("name", self.catalog.label(kind))]),
                        kind.clone(),
                        qt("category.node"),
                        "add node 添加 步骤 节点",
                        QuickAction::AddNode(kind.clone()),
                    ));
                }
            }
            for actor in self.dialogue_actors() {
                entries.push(CommandEntry::new(
                    qmessage("actor_dialogue", &[("name", actor.label)]),
                    actor.id.clone(),
                    qt("category.character"),
                    &format!("character 人物 角色 对白 {}", actor.aliases.join(" ")),
                    QuickAction::CharacterSay(actor.id),
                ));
            }
        }
        for (id, story) in &self.project.stories {
            let title = story["title"]
                .as_str()
                .filter(|title| !title.is_empty())
                .unwrap_or(id);
            entries.push(CommandEntry::new(
                title.to_owned(),
                id.clone(),
                qt("category.chapter"),
                "chapter 章节 跳转",
                QuickAction::Chapter(id.clone()),
            ));
        }
        for (path, bytes) in self
            .project
            .assets
            .iter()
            .filter(|(path, _)| path.ends_with("/content.json"))
        {
            let Ok(meta) = serde_json::from_slice::<Value>(bytes) else {
                continue;
            };
            let id = meta["id"].as_str().unwrap_or(path);
            let name = meta["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(id);
            let kind = meta["type"].as_str().unwrap_or("");
            entries.push(CommandEntry::new(
                name.to_owned(),
                format!("user:{id} · {kind}"),
                qt("category.asset"),
                &format!("asset material 素材 {path}"),
                QuickAction::Asset(path.clone()),
            ));
        }
        entries
    }

    fn command_palette_ui(
        &self,
        ctx: &egui::Context,
        palette: &mut CommandPalette,
    ) -> (bool, Option<QuickAction>) {
        let (enter, escape, movement) = quick_dialog_keys(ctx, &mut palette.composing);
        let entries = self.command_entries();
        let mut close = escape;
        let mut chosen = None;
        egui::Modal::new(egui::Id::new("command-palette"))
            .frame(shell::dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(620.0);
                close |= shell::dialog_header(ui, &qt("search"));
                let input = ui.add(
                    egui::TextEdit::singleline(&mut palette.query)
                        .id(egui::Id::new("command-palette-query"))
                        .desired_width(f32::INFINITY)
                        .hint_text(qt("search_hint")),
                );
                if std::mem::take(&mut palette.focus) {
                    input.request_focus();
                }
                if input.changed() {
                    palette.selected = 0;
                    palette.reveal_selection = true;
                }
                let query = palette.query.trim().to_lowercase();
                let mut filtered: Vec<_> = entries
                    .iter()
                    .filter(|entry| command_matches(entry, &query))
                    .collect();
                if !query.is_empty() {
                    filtered.sort_by_key(|entry| !entry.label.to_lowercase().starts_with(&query));
                }
                palette.selected = palette.selected.min(filtered.len().saturating_sub(1));
                if movement != 0 && !filtered.is_empty() {
                    palette.selected = palette
                        .selected
                        .saturating_add_signed(movement)
                        .min(filtered.len() - 1);
                    palette.reveal_selection = true;
                }
                ui.small(qt("search_keys"));
                ui.separator();
                if filtered.is_empty() {
                    ui.label(qt("no_results"));
                } else {
                    let row_height = 44.0;
                    let row_stride = row_height + ui.spacing().item_spacing.y;
                    let max_height = 350.0;
                    let mut scroll = egui::ScrollArea::vertical()
                        .id_salt("command-palette-results")
                        .max_height(max_height)
                        .auto_shrink([false, false]);
                    if std::mem::take(&mut palette.reveal_selection) {
                        scroll = scroll.vertical_scroll_offset(
                            (palette.selected as f32 * row_stride - max_height / 2.0).max(0.0),
                        );
                    }
                    scroll.show_rows(ui, row_height, filtered.len(), |ui, range| {
                        for index in range {
                            let entry = filtered[index];
                            let label = if entry.detail.is_empty() {
                                entry.label.clone()
                            } else {
                                format!("{}\n{}", entry.label, entry.detail)
                            };
                            if ui
                                .add_sized(
                                    [ui.available_width(), row_height],
                                    egui::Button::selectable(index == palette.selected, label)
                                        .right_text(RichText::new(&entry.category).small().weak())
                                        .truncate(),
                                )
                                .on_hover_text(format!("{}\n{}", entry.label, entry.detail))
                                .clicked()
                            {
                                chosen = Some(entry.action.clone());
                            }
                        }
                    });
                    if enter {
                        chosen = Some(filtered[palette.selected].action.clone());
                    }
                }
            });
        (close || chosen.is_some(), chosen)
    }

    fn execute_quick_action(&mut self, ctx: &egui::Context, action: QuickAction) {
        use crate::error_location::Destination;
        match action {
            QuickAction::Dialogue => self.open_dialogue_authoring(ctx),
            QuickAction::RenameCharacter => self.open_name_authoring(),
            QuickAction::CustomTalent => self.open_custom_talent(),
            QuickAction::Export => self.request_export(ctx),
            QuickAction::AddNode(kind) => {
                if self.apply_pending_drafts() {
                    self.add_node(&kind);
                }
            }
            QuickAction::CharacterSay(id) => {
                if !self.apply_pending_drafts() {
                    return;
                }
                let story = self.story();
                let count = story["nodes"].as_array().map_or(0, Vec::len);
                self.add_node("say");
                if self.story()["nodes"].as_array().map_or(0, Vec::len) == count + 1 {
                    let node = &mut self.project.stories.get_mut(&self.current).unwrap()["nodes"]
                        [self.selected];
                    node["character"] = json!(id);
                    if node.get("inherit_dialog").is_some() {
                        node["inherit_dialog"] = json!(false);
                    }
                }
            }
            QuickAction::Chapter(id) => self.go_to_problem(ctx, Destination::Story(id, None)),
            QuickAction::Asset(path) => self.go_to_problem(ctx, Destination::Asset(path)),
            QuickAction::Center(center) => {
                self.open_center(center);
            }
            QuickAction::Tool(page) => self.open_tool_page(page),
            QuickAction::View(view) => {
                if self.open_center(Center::Node) {
                    if view == View::Checks {
                        self.compile();
                        self.problems_open = true;
                    }
                    self.view = view;
                }
            }
            QuickAction::Save => {
                self.save(false);
            }
            QuickAction::Undo => self.undo(),
            QuickAction::Redo => self.redo(),
            QuickAction::PlayCurrent => self.play_current(),
        }
    }
}

fn dialogue_ui(
    ctx: &egui::Context,
    draft: &mut DialogueDraft,
    actors: &[DialogueActor],
) -> (bool, bool) {
    // Return and arrow keys belong to multiline dialogue fields; only Escape
    // closes this editor. In particular, Return never inserts the batch.
    let was_composing = draft.composing;
    let mut ime_event = false;
    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Ime(event) = event {
                ime_event = true;
                match event {
                    egui::ImeEvent::Enabled => {}
                    egui::ImeEvent::Preedit(text) => draft.composing = !text.is_empty(),
                    egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => draft.composing = false,
                }
            }
        }
    });
    let mut cancel = !was_composing
        && !draft.composing
        && !ime_event
        && !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let mut submit = false;
    egui::Modal::new(egui::Id::new("continuous-dialogue"))
        .frame(shell::dialog_frame())
        .show(ctx, |ui| {
            ui.set_width(740.0);
            cancel |= shell::dialog_header(ui, &qt("dialogue"));
            ui.small(qmessage(
                "destination",
                &[
                    ("chapter", draft.chapter.clone()),
                    (
                        "step",
                        draft.anchor.clone().unwrap_or_else(|| qt("chapter_start")),
                    ),
                ],
            ));
            egui::ScrollArea::vertical()
                .id_salt("continuous-dialogue-body")
                .max_height((ctx.content_rect().height() - 210.0).max(240.0))
                .show(ui, |ui| {
                    ui.label(qt("source_hint"));
                    let source = ui.add(
                        egui::TextEdit::multiline(&mut draft.source)
                            .id(egui::Id::new("continuous-dialogue-source"))
                            .desired_width(f32::INFINITY)
                            .desired_rows(5)
                            .hint_text(qt("source_example")),
                    );
                    if std::mem::take(&mut draft.focus) {
                        source.request_focus();
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                !draft.source.trim().is_empty(),
                                egui::Button::new(qt("split")),
                            )
                            .on_hover_text(qt("split_hint"))
                            .clicked()
                        {
                            draft.rows = parse_dialogue(&draft.source, actors);
                            draft.parsed_source = draft.source.clone();
                            draft.error = None;
                        }
                        if !draft.rows.is_empty() {
                            ui.small(qmessage(
                                "row_count",
                                &[("count", draft.rows.len().to_string())],
                            ));
                        }
                    });
                    if draft.source != draft.parsed_source && !draft.rows.is_empty() {
                        ui.colored_label(ui.visuals().warn_fg_color, qt("source_changed"));
                    }
                    let mut remove = None;
                    let mut apply_name = None;
                    let mut speaker_counts = BTreeMap::<String, usize>::new();
                    for row in &draft.rows {
                        *speaker_counts
                            .entry(row.source_speaker.clone())
                            .or_default() += 1;
                    }
                    for (index, row) in draft.rows.iter_mut().enumerate() {
                        ui.push_id(("dialogue-row", index), |ui| {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.strong(qmessage("line", &[("line", row.line.to_string())]));
                                if !row.source_speaker.is_empty() {
                                    ui.weak(&row.source_speaker);
                                }
                                if ui.small_button(qt("remove_row")).clicked() {
                                    remove = Some(index);
                                }
                            });
                            let selected = match &row.speaker {
                                Speaker::Narration => qt("narration"),
                                Speaker::Character(id) => actors
                                    .iter()
                                    .find(|actor| &actor.id == id)
                                    .map(|actor| format!("{} ({})", actor.label, actor.id))
                                    .unwrap_or_else(|| id.clone()),
                                Speaker::Unresolved => qt("unmatched"),
                            };
                            crate::forms::resource_combo(ui, "dialogue-speaker", &selected, |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut row.actor_search)
                                        .hint_text(qt("actor_search"))
                                        .desired_width(340.0),
                                );
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.selectable_value(
                                            &mut row.speaker,
                                            Speaker::Narration,
                                            qt("narration"),
                                        );
                                        let search = row.actor_search.trim().to_lowercase();
                                        for actor in actors.iter().filter(|actor| {
                                            search.is_empty()
                                                || format!(
                                                    "{} {} {}",
                                                    actor.label,
                                                    actor.id,
                                                    actor.aliases.join(" ")
                                                )
                                                .to_lowercase()
                                                .contains(&search)
                                        }) {
                                            ui.selectable_value(
                                                &mut row.speaker,
                                                Speaker::Character(actor.id.clone()),
                                                format!("{} ({})", actor.label, actor.id),
                                            );
                                        }
                                    });
                            });
                            let same = speaker_counts[&row.source_speaker];
                            if !row.source_speaker.is_empty()
                                && same > 1
                                && row.speaker != Speaker::Unresolved
                            {
                                if ui
                                    .small_button(qmessage(
                                        "apply_same_speaker",
                                        &[
                                            ("name", row.source_speaker.clone()),
                                            ("count", same.to_string()),
                                        ],
                                    ))
                                    .clicked()
                                {
                                    apply_name =
                                        Some((row.source_speaker.clone(), row.speaker.clone()));
                                }
                            }
                            if row.speaker == Speaker::Unresolved {
                                ui.colored_label(ui.visuals().warn_fg_color, qt("match_required"));
                            }
                            ui.add(
                                egui::TextEdit::multiline(&mut row.text)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(2),
                            );
                            if row.text.trim().is_empty() {
                                ui.colored_label(ui.visuals().warn_fg_color, qt("empty_text"));
                            }
                        });
                    }
                    if let Some((name, speaker)) = apply_name {
                        apply_same_speaker(&mut draft.rows, &name, &speaker);
                    }
                    if let Some(index) = remove {
                        draft.rows.remove(index);
                    }
                });
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.separator();
            ui.small(qt("undo_hint"));
            let ready = draft.source == draft.parsed_source && dialogue_ready(&draft.rows, actors);
            ui.horizontal(|ui| {
                submit = ui
                    .add_enabled(ready, egui::Button::new(qt("insert")))
                    .clicked();
                cancel |= ui.button(qt("cancel")).clicked();
            });
        });
    (cancel, submit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_dialogue_shortcut_opens_the_review_dialog_without_inserting_a_step() {
        let mut app = app();
        let before = snapshot(&app.project);
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(state(&ctx).dialogue.is_some());
        assert!(snapshot(&app.project) == before);
    }

    fn actors() -> Vec<DialogueActor> {
        vec![
            DialogueActor {
                id: "player".into(),
                label: "赵活".into(),
                aliases: vec!["赵活".into()],
            },
            DialogueActor {
                id: "guest1".into(),
                label: "客人1".into(),
                aliases: vec!["客人".into()],
            },
            DialogueActor {
                id: "guest2".into(),
                label: "客人2".into(),
                aliases: vec!["客人".into()],
            },
            DialogueActor {
                id: "user:demo.actor".into(),
                label: "自建角色".into(),
                aliases: vec!["自建角色".into()],
            },
        ]
    }

    fn app() -> App {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = App::new(&cc, None, None);
        app.recovery_candidates.clear();
        app.auto_save_project = false;
        app
    }

    #[test]
    fn dialogue_parse_preserves_unknowns_ambiguity_narration_and_user_ids() {
        let rows = parse_dialogue("赵活：我来了。\r\n\n旁白：天色渐暗。\n没有署名的叙述。\n客人：同名要选择。\n不存在：不会猜ID。\nuser:demo.actor: hello: world\n赵活：", &actors());
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0].speaker, Speaker::Character("player".into()));
        assert_eq!(rows[1].speaker, Speaker::Narration);
        assert_eq!(rows[2].speaker, Speaker::Narration);
        assert_eq!(rows[3].speaker, Speaker::Unresolved);
        assert_eq!(rows[4].speaker, Speaker::Unresolved);
        assert_eq!(
            rows[5].speaker,
            Speaker::Character("user:demo.actor".into())
        );
        assert_eq!(rows[5].text, "hello: world");
        assert_eq!(rows[6].text, "");
        assert!(!dialogue_ready(&rows, &actors()));
        let rows = parse_dialogue("user:demo.actor：保留完整ID", &actors());
        assert_eq!(
            rows[0].speaker,
            Speaker::Character("user:demo.actor".into())
        );
        let rows = parse_dialogue("赵活: 时间：到了。\n旁白: 他道：回来了。", &actors());
        assert_eq!(rows[0].speaker, Speaker::Character("player".into()));
        assert_eq!(rows[0].text, "时间：到了。");
        assert_eq!(rows[1].speaker, Speaker::Narration);
        assert_eq!(rows[1].text, "他道：回来了。");
    }

    #[test]
    fn matching_one_speaker_can_apply_to_the_batch_without_touching_other_names_or_text() {
        let actors = actors();
        let source = "客人：第一句。\n客人：第二句。\n陌生人：仍需匹配。\n旁白：保留叙述。\nuser:demo.actor：保留自建人物。";
        let mut rows = parse_dialogue(source, &actors);
        rows[0].speaker = Speaker::Character("guest2".into());
        let original = rows.clone();
        let mut draft = DialogueDraft {
            chapter: "main".into(),
            anchor: None,
            source: source.into(),
            parsed_source: source.into(),
            rows,
            focus: false,
            composing: false,
            error: None,
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |draft: &mut DialogueDraft, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    dialogue_ui(ctx, draft, &actors);
                },
            )
        };
        let output = draw(&mut draft, vec![]);
        let label = qmessage(
            "apply_same_speaker",
            &[("name", "客人".into()), ("count", "2".into())],
        );
        let target = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, node)| node.label() == Some(label.as_str()))
            .unwrap()
            .0;
        draw(
            &mut draft,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target,
                    data: None,
                },
            )],
        );
        assert_eq!(draft.rows[1].speaker, Speaker::Character("guest2".into()));
        for index in 2..draft.rows.len() {
            assert_eq!(draft.rows[index].speaker, original[index].speaker);
        }
        assert_eq!(
            draft.rows.iter().map(|row| &row.text).collect::<Vec<_>>(),
            original.iter().map(|row| &row.text).collect::<Vec<_>>()
        );
        assert!(!dialogue_ready(&draft.rows, &actors));
    }

    #[test]
    fn reviewed_dialogue_batch_keeps_graph_edges_and_is_one_undo() {
        let mut app = app();
        let mut project = Project::new();
        project.stories.insert("main".into(), json!({"id":"main","story_schema":2,"start":"choice1",
            "vanilla_import":{"schema":2},"nodes":[
                {"id":"choice1","type":"choice","options":[{"text":"继续","goto":"say1"},{"text":"另一边","goto":"other"}]},
                {"id":"say1","type":"say","text":"原后续","mode":"narrative","goto":"end1"},
                {"id":"other","type":"say","text":"其他分支","mode":"narrative","goto":"end1"},
                {"id":"end1","type":"end"}]}));
        app.install(project);
        let before = snapshot(&app.project);
        let rows = parse_dialogue(
            "player：新增对白。\n旁白：新增叙述。",
            &app.dialogue_actors(),
        );
        assert_eq!(
            app.insert_dialogue_rows("main", Some("choice1"), &rows)
                .unwrap(),
            2
        );
        let story = app.story();
        assert_eq!(story["nodes"][0]["options"][0]["goto"], "say2");
        assert_eq!(story["nodes"][0]["options"][1]["goto"], "other");
        assert_eq!(story["nodes"][1]["character"], "player");
        assert_eq!(story["nodes"][1]["inherit_dialog"], false);
        assert_eq!(story["nodes"][1]["goto"], "say3");
        assert_eq!(story["nodes"][2]["mode"], "narrative");
        assert!(story["nodes"][2].get("character").is_none());
        assert_eq!(story["nodes"][2]["goto"], "say1");
        assert_eq!(app.undo.len(), 1);
        app.undo();
        assert!(snapshot(&app.project) == before);
        app.redo();
        assert_eq!(app.story()["nodes"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn dialogue_batch_compiles_and_survives_project_reopen() {
        let mut app = app();
        let chapter = app.current.clone();
        let anchor = app.story()["nodes"][app.selected]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let before = snapshot(&app.project);
        let unknown = parse_dialogue("not.a.real.actor：需要匹配", &app.dialogue_actors());
        assert!(app
            .insert_dialogue_rows(&chapter, Some(&anchor), &unknown)
            .is_err());
        assert!(snapshot(&app.project) == before);
        let rows = parse_dialogue(
            "player：批量编译对白。\n旁白：批量编译旁白。",
            &app.dialogue_actors(),
        );
        app.insert_dialogue_rows(&chapter, Some(&anchor), &rows)
            .unwrap();
        let lua = compile_with_assets(&app.project, &app.story()).unwrap();
        assert!(lua.contains("setsaydialog(saydialogs.character)"));
        assert!(lua.contains("setsaydialog(saydialogs.narrative)"));
        let texts = lom_core::localization::iter_localizable_texts(&app.story());
        assert!(texts.values().any(|text| text == "批量编译对白。"));
        assert!(texts.values().any(|text| text == "批量编译旁白。"));
        let root = tempfile::tempdir().unwrap();
        app.project.save_to(root.path()).unwrap();
        let reopened = Project::open(root.path()).unwrap();
        assert_eq!(reopened.stories[&chapter], app.project.stories[&chapter]);
        assert_eq!(
            compile_with_assets(&reopened, &reopened.stories[&chapter]).unwrap(),
            lua
        );
    }

    #[test]
    fn command_palette_keyboard_search_executes_the_selected_chapter() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.project.stories.insert("keyboard.chapter".into(), json!({"id":"keyboard.chapter","title":"快捷键目标","start":"end1","nodes":[{"id":"end1","type":"end"}]}));
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let mut frame = |events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 850.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.quick_tools_shortcuts(ctx);
                    app.quick_tools(ctx);
                },
            )
        };
        let _ = frame(vec![key(egui::Key::K, egui::Modifiers::COMMAND)]);
        let _ = frame(vec![egui::Event::Text("keyboard.chapter".into())]);
        assert_eq!(state(&ctx).palette.unwrap().query, "keyboard.chapter");
        let _ = frame(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(app.current, "keyboard.chapter");
        assert_eq!(app.center, Center::Chapter);
        assert!(state(&ctx).palette.is_none());
    }

    #[test]
    fn command_search_uses_real_asset_paths_and_existing_navigation() {
        let mut app = app();
        app.project.stories.insert("next".into(), json!({"id":"next","title":"第二章","start":"end1","nodes":[{"id":"end1","type":"end"}]}));
        let path = "assets/user/image/my.picture/content.json";
        app.project.assets.insert(
            path.into(),
            serde_json::to_vec(&json!({"id":"my.picture","type":"image","name":"我的图片"}))
                .unwrap(),
        );
        let entries = app.command_entries();
        assert!(entries
            .iter()
            .any(|entry| command_matches(entry, "node say")));
        let asset = entries
            .iter()
            .find(|entry| command_matches(entry, "asset my.picture"))
            .unwrap()
            .action
            .clone();
        app.execute_quick_action(&egui::Context::default(), asset);
        assert_eq!(app.center, Center::Assets);
        assert_eq!(app.asset_selection, path);
        let chapter = entries
            .iter()
            .find(|entry| matches!(&entry.action, QuickAction::Chapter(id) if id == "next"))
            .unwrap()
            .action
            .clone();
        app.execute_quick_action(&egui::Context::default(), chapter);
        assert_eq!(app.current, "next");
        assert_eq!(app.center, Center::Chapter);
    }

    #[test]
    fn ime_return_does_not_execute_a_palette_result() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.open_command_palette(&ctx);
        let before = snapshot(&app.project);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::Ime(egui::ImeEvent::Preedit("对".into())),
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| app.quick_tools(ctx),
        );
        assert!(state(&ctx).palette.is_some());
        assert!(snapshot(&app.project) == before);
    }
}
