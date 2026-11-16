//! Local, read-only context for an identified flag reference. Calendar proximity
//! and similarly named checkpoints are never interpreted as executable links.
use super::*;
use lom_core::story_bookmarks::TimelineEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UseKind {
    Set,
    Add,
    Read,
    Related,
}

struct Hit {
    line: usize,
    kind: UseKind,
    nearby: Vec<usize>,
}

pub(super) struct SourceContext {
    hits: Vec<Hit>,
}

struct Token<'a> {
    start: usize,
    value: &'a str,
    string: bool,
}

fn long_string(source: &str, at: usize) -> Option<(usize, usize, usize)> {
    let rest = source.get(at..)?;
    let middle = rest.strip_prefix('[')?;
    let equals = middle.bytes().take_while(|&b| b == b'=').count();
    if middle.as_bytes().get(equals) != Some(&b'[') {
        return None;
    }
    let start = at + equals + 2;
    let close = format!("]{}]", "=".repeat(equals));
    let end = source[start..]
        .find(&close)
        .map_or(source.len(), |n| start + n);
    Some((start, end, (end + close.len()).min(source.len())))
}

// Keep locations while skipping comments and treating quoted code as one token.
// Flag IDs use plain ASCII literals; escaped/dynamic keys remain unclassified.
fn tokens(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut at = 0;
    let mut result = Vec::new();
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"--") {
            at = long_string(source, at + 2).map_or_else(
                || source[at..].find('\n').map_or(bytes.len(), |n| at + n + 1),
                |(_, _, end)| end,
            );
            continue;
        }
        let start = at;
        let (value, string) = if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            let inner = at;
            while at < bytes.len() && bytes[at] != quote {
                at = if bytes[at] == b'\\' {
                    (at + 2).min(bytes.len())
                } else {
                    at + 1
                };
            }
            let value = &source[inner..at];
            at = (at + 1).min(bytes.len());
            (value, true)
        } else if let Some((inner, content_end, end)) = long_string(source, at) {
            at = end;
            (&source[inner..content_end], true)
        } else {
            at += source[at..].chars().next().unwrap().len_utf8();
            if bytes[start].is_ascii_alphanumeric() || bytes[start] == b'_' {
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
            }
            (&source[start..at], false)
        };
        result.push(Token {
            start,
            value,
            string,
        });
    }
    result
}

impl SourceContext {
    fn new(source: &Source, flag: &str) -> Self {
        let tokens = tokens(&source.text);
        let locations: Vec<_> = source
            .sites
            .iter()
            .enumerate()
            .filter_map(|(index, site)| {
                source
                    .text
                    .match_indices(&site.find)
                    .nth(site.occurrence - 1)
                    .map(|(at, _)| (at, index))
            })
            .collect();
        let hits = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| token.string && token.value == flag)
            .map(|(i, token)| {
                let call = i.checked_sub(4).and_then(|start| tokens.get(start..i));
                let kind = match call {
                    Some(call)
                        if !call.iter().any(|token| token.string)
                            && call[1].value == "."
                            && call[3].value == "("
                            && (i == 4
                                || !matches!(tokens[i - 5].value, "." | ":" | "function")) =>
                    {
                        match (
                            call[0].value,
                            call[2].value,
                            tokens.get(i + 1).map(|t| t.value),
                        ) {
                            ("statmodifymanager", "SetFlag", Some(",")) => UseKind::Set,
                            ("statmodifymanager", "AddFlag", Some(",")) => UseKind::Add,
                            ("luamanager", "GetFlagData", Some(")")) => UseKind::Read,
                            _ => UseKind::Related,
                        }
                    }
                    _ => UseKind::Related,
                };
                let split = locations.partition_point(|&(at, _)| at < token.start);
                let nearby = (split.saturating_sub(2)..(split + 2).min(locations.len()))
                    .map(|i| locations[i].1)
                    .collect();
                let start = if kind == UseKind::Related {
                    token.start
                } else {
                    tokens[i - 4].start
                };
                Hit {
                    line: source.text[..start].bytes().filter(|&b| b == b'\n').count(),
                    kind,
                    nearby,
                }
            })
            .collect();
        Self { hits }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn navigation() -> (Timeline, ScriptRoutes) {
        let entry = |script: &str, title: &str, series: &str, day: i32, active: bool| {
            json!({
                "script":script,"title":title,"series":series,"key":script,"main":series=="main",
                "next_key":"","condition":if series=="main" {""} else {"好感度 >= 20"},"place":"無",
                "when":format!("第2年10月{}",if day==63 {"上旬"} else {"下旬"}),
                "from_round":day,"to_round":day,"active":active
            })
        };
        let timeline = serde_json::from_value(json!({"schema":2,"game_dir":"unused_cached_game","game_version":"test",
            "missions":[entry("ch3_7_1","總攻擊","main",63,true),entry("S2321_01","崆峒探險隊","S2321",65,false)]})).unwrap();
        let route = |script: &str, links: Value| json!({"script":script,"links":links,"missions":[],"unknown_jumps":0});
        let routes = serde_json::from_value(json!({"scripts":[
            route("ch3_7_1",json!([{"target":"ch3_10_2","condition":"主线分支"}])),
            route("S2321_01",json!([{"target":"ch3_10_2","condition":"选项：祝你好运"}])),
            route("ch3_10_2",json!([{"target":"ch3_10_3","condition":""}])),
            route("ch3_10_3",json!([{"target":"ch3_11_1","condition":"检定 Battle_Result_000：结果 1"},
                {"target":"ch3_10_4","condition":"否则若 检定 Battle_Result_000：结果 2"}]))
        ]})).unwrap();
        (timeline, routes)
    }

    #[test]
    fn m30077_context_keeps_separate_entry_dates_and_conditional_direct_neighbors() {
        // A small route fixture exercises multiple owners; no game Lua is kept.
        let (mut timeline, routes) = navigation();
        let mut calendar_only = timeline.missions[0].clone();
        calendar_only.script = "unrelated_calendar_entry".into();
        calendar_only.key = "unrelated".into();
        calendar_only.from_round = Some(64);
        calendar_only.to_round = Some(64);
        calendar_only.when_label = "第2年10月中旬".into();
        timeline.missions.push(calendar_only);
        let graph = RouteGraph::build(&timeline, &routes);
        let entries = events(Some(&timeline), Some(&graph), "CH3_10_3");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].from_round, Some(63));
        assert_eq!(entries[1].from_round, Some(65));
        assert!(entries[0].main && !entries[1].main && !entries[1].active);
        assert_eq!(entries[1].condition, "好感度 >= 20");
        assert_eq!(neighbors(&routes, "ch3_10_3", true), vec![("ch3_10_2", "")]);
        assert_eq!(
            neighbors(&routes, "ch3_10_3", false),
            vec![
                ("ch3_10_4", "否则若 检定 Battle_Result_000：结果 2"),
                ("ch3_11_1", "检定 Battle_Result_000：结果 1")
            ]
        );
        assert!(graph.edges.iter().any(|edge| edge.by_date));
        assert!(neighbors(&routes, "ch3_7_1", false)
            .iter()
            .all(|(target, _)| *target != "S2321_01"));
    }

    #[test]
    fn undated_and_unknown_context_does_not_invent_dates_or_numbered_neighbors() {
        let (mut timeline, _) = navigation();
        timeline.missions.truncate(1);
        let entry = &mut timeline.missions[0];
        entry.script = "ch3_10_3".into();
        entry.from_round = None;
        entry.to_round = None;
        entry.when_label.clear();
        let routes = ScriptRoutes { scripts: vec![] };
        let graph = RouteGraph::build(&timeline, &routes);
        let entries = events(Some(&timeline), Some(&graph), "ch3_10_3");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].from_round.is_none() && entries[0].when_label.is_empty());
        assert!(events(Some(&timeline), Some(&graph), "ch3_10_4").is_empty());
        assert!(neighbors(&routes, "ch3_10_3", true).is_empty());
        assert!(neighbors(&routes, "ch3_10_3", false).is_empty());
    }

    #[test]
    fn direct_flag_uses_skip_comments_quoted_code_and_same_named_checkpoints() {
        let lua = r#"-- statmodifymanager.SetFlag("M30077", 99)
--[=[ luamanager.GetFlagData("M30077") ]=]
say("statmodifymanager.SetFlag('M30077', 99)")
say("前一句")
statmodifymanager . SetFlag(
    'M30077', 1)
statmodifymanager.AddFlag("M30077", -1)
local state = luamanager.GetFlagData("M30077")
checkpointmanager.Switch("M30077_01")
say("后一句")
other.statmodifymanager.SetFlag("M30077", 99)
"#;
        let panel = Panel::with_source("chapter", lua);
        let source = &panel.sources["chapter"];
        let context = SourceContext::new(source, "M30077");
        assert_eq!(
            context.hits.iter().map(|hit| hit.kind).collect::<Vec<_>>(),
            vec![UseKind::Set, UseKind::Add, UseKind::Read, UseKind::Related]
        );
        assert_eq!(context.hits[0].line, 4);
        let adjacent: Vec<_> = context.hits[0]
            .nearby
            .iter()
            .map(|&i| source.display(&source.sites[i]))
            .collect();
        assert!(adjacent.contains(&"前一句".to_owned()) && adjacent.contains(&"后一句".to_owned()));
        let checkpoint =
            Panel::with_source("checkpoint", "checkpointmanager.Switch(\"M30077_01\")");
        assert!(
            SourceContext::new(&checkpoint.sources["checkpoint"], "M30077")
                .hits
                .is_empty()
        );
    }

    #[test]
    fn context_button_opens_events_neighbors_and_nearby_dialogue_without_editing_the_project() {
        crate::i18n::set_locale("chs");
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let (timeline, routes) = navigation();
        let graph = RouteGraph::build(&timeline, &routes);
        let mut reference = FlagReference {
            panel: Panel {
                navigation_loaded: true,
                routes_attempted: true,
                timeline: Some(timeline),
                routes: Some(routes),
                graph: Some(graph),
                ..Panel::with_source(
                    "ch3_10_3",
                    "say(\"前一句\")\nstatmodifymanager.SetFlag(\"M30077\", 1)\nsay(\"后一句\")",
                )
            },
            ..Default::default()
        };
        let mut project = lom_core::project::Project::new();
        project.stories.get_mut("main").unwrap()["nodes"][0] = json!({"id":"check","type":"flag_check",
            "source":"game_flag","key":"M30077","op":"eq","value":0,"success":"end","failure":"end"});
        let before = (
            project.manifest.clone(),
            project.stories.clone(),
            project.scenario.clone(),
            project.assets.clone(),
        );
        let data = json!({"game_flags":[{"id":"M30077","name":"小竹參戰江陵大戰","default":0,"scripts":["ch3_10_3"]}]});
        let mut draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1250.0, 1600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!crate::game_flags::field(
                            ui,
                            &data,
                            &mut project.stories.get_mut("main").unwrap()["nodes"][0],
                            "key",
                            "原版旗标",
                            false
                        ));
                    });
                    reference.show(ctx, PathBuf::from("unused_cached_game"));
                },
            )
        };
        let output = draw(vec![]);
        let target = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(id, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && node.label() == Some("查看相关事件与前后文"))
                .then_some(*id)
            })
            .expect("visible context button");
        let output = draw(vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target,
                data: None,
            },
        )]);
        let text = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .flat_map(|(_, node)| [node.label(), node.value()].into_iter().flatten())
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "总攻击",
            "崆峒探险队",
            "第2年10月上旬",
            "第2年10月下旬",
            "原版配置停用",
            "好感度 >= 20",
            "ch3_10_2",
            "Battle_Result_000",
            "前一句",
            "后一句",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        drop(draw);
        assert!(reference.open && reference.panel.job.is_none());
        assert_eq!(
            (
                project.manifest,
                project.stories,
                project.scenario,
                project.assets
            ),
            before
        );
    }
}

fn events<'a>(
    timeline: Option<&'a Timeline>,
    graph: Option<&'a RouteGraph>,
    script: &str,
) -> Vec<&'a TimelineEntry> {
    if let Some(graph) = graph {
        return graph
            .owners
            .get(&script.to_ascii_lowercase())
            .into_iter()
            .flatten()
            .map(|&at| &graph.stations[at].entry)
            .filter(|entry| entry.series != RouteGraph::MARKED_SERIES)
            .collect();
    }
    timeline
        .into_iter()
        .flat_map(|timeline| &timeline.missions)
        .filter(|entry| entry.script.eq_ignore_ascii_case(script))
        .collect()
}

fn neighbors<'a>(routes: &'a ScriptRoutes, script: &str, before: bool) -> Vec<(&'a str, &'a str)> {
    let mut result = BTreeSet::new();
    for route in &routes.scripts {
        for link in &route.links {
            if before && link.target.eq_ignore_ascii_case(script) {
                result.insert((route.script.as_str(), link.condition.as_str()));
            } else if !before && route.script.eq_ignore_ascii_case(script) {
                result.insert((link.target.as_str(), link.condition.as_str()));
            }
        }
    }
    result.into_iter().collect()
}

fn same_directory(a: &std::path::Path, b: &std::path::Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}

impl FlagReference {
    fn load_context_navigation(&mut self) {
        self.panel.load_navigation();
        if self
            .panel
            .timeline
            .as_ref()
            .is_some_and(|timeline| !same_directory(&timeline.game_dir, &self.panel.game))
        {
            self.panel.timeline = None;
            self.panel.graph = None;
            self.panel.routes = None;
            self.panel.routes_attempted = false;
            self.panel.navigation_status = Some(key("game_flags.context_other_game"));
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, configured_game: PathBuf) {
        if let Some(request) = crate::game_flags::take_reference(ctx) {
            self.panel.activate_target(&request.script);
            self.request = Some(request);
            self.context = None;
            self.filter.clear();
            self.open = true;
        }
        if !self.open || self.request.is_none() {
            return;
        }
        if self.panel.game.as_os_str().is_empty() {
            self.panel.game = configured_game;
        }
        if !self.panel.navigation_loaded {
            self.load_context_navigation();
        }
        self.panel.poll();
        let request = self.request.as_ref().unwrap();
        if !self.panel.game.as_os_str().is_empty() && self.panel.job.is_none() {
            if !self
                .panel
                .attempted
                .contains(&request.script.to_ascii_lowercase())
            {
                self.panel.read(ctx, Some(&request.script));
            } else if !self.panel.routes_attempted && self.panel.error.is_none() {
                self.panel.routes_attempted = true;
                self.panel.read_request(
                    ctx,
                    self.panel.game.clone(),
                    vec!["--story-routes".into()],
                    None,
                );
            }
        }
        let mut open = self.open;
        egui::Window::new(key("game_flags.reference_title"))
            .id(egui::Id::new("flag-reference-window"))
            .default_size(egui::vec2(850.0, 720.0))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("flag-context-window")
                    .show(ui, |ui| self.show_context(ui));
            });
        self.open = open;
    }

    fn show_context(&mut self, ui: &mut egui::Ui) {
        let request = self.request.as_ref().unwrap().clone();
        ui.strong(format!("{} · {}", request.flag, request.script));
        ui.small(key("game_flags.reference_hint"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.panel.job.is_none(),
                    egui::Button::new(key("vanilla.choose_directory")),
                )
                .clicked()
            {
                if let Some(game) = crate::file_dialog::at(Some(&self.panel.game)).pick_folder() {
                    self.panel = Panel {
                        game,
                        ..Default::default()
                    };
                    self.panel.activate_target(&request.script);
                    self.context = None;
                }
            }
            if ui
                .add_enabled(
                    self.panel.job.is_none(),
                    egui::Button::new(key("game_flags.context_refresh")),
                )
                .clicked()
            {
                self.load_context_navigation();
                self.panel.routes_attempted = false;
                self.panel
                    .attempted
                    .remove(&request.script.to_ascii_lowercase());
                self.panel
                    .sources
                    .remove(&request.script.to_ascii_lowercase());
                self.context = None;
            }
            if self.panel.job.is_some() {
                ui.spinner();
                ui.label(key("game_flags.reference_loading"));
            }
        });
        let mut selected = None;
        if request.scripts.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.weak(key("game_flags.context_identified"));
                for script in &request.scripts {
                    if ui
                        .selectable_label(script.eq_ignore_ascii_case(&request.script), script)
                        .clicked()
                    {
                        selected = Some(script.clone());
                    }
                }
            });
        }
        if let Some(error) = &self.panel.error {
            crate::error_ui::show_with_actions(ui, error);
        }
        if let Some(error) = &self.panel.navigation_error {
            crate::error_ui::show_with_actions(ui, error);
        }
        if let Some(status) = &self.panel.navigation_status {
            ui.small(status);
        }
        ui.separator();
        ui.strong(key("game_flags.context_events"));
        ui.small(key("game_flags.context_events_hint"));
        let entries = events(
            self.panel.timeline.as_ref(),
            self.panel.graph.as_ref(),
            &request.script,
        );
        if entries.is_empty() {
            ui.small(key(
                if self.panel.routes.is_none() && self.panel.job.is_some() {
                    "game_flags.reference_loading"
                } else if self.panel.timeline.is_none() {
                    "game_flags.context_need_index"
                } else {
                    "game_flags.context_no_event"
                },
            ));
        }
        for entry in entries {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(crate::game_flags::prose(&entry.title));
                    ui.weak(key(if entry.main {
                        "game_flags.context_main"
                    } else {
                        "game_flags.context_side"
                    }));
                    if !entry.active {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            key("game_flags.context_inactive"),
                        );
                    }
                });
                ui.small(format!(
                    "{} · {} / {}",
                    entry.script, entry.series, entry.key
                ));
                let date = if entry.when_label.is_empty() {
                    key("game_flags.context_no_date")
                } else {
                    crate::game_flags::prose(&entry.when_label)
                };
                ui.label(message("game_flags.context_date", &[("date", date)]));
                if !entry.place.is_empty() {
                    ui.small(message(
                        "game_flags.context_place",
                        &[("place", crate::game_flags::prose(&entry.place))],
                    ));
                }
                if !entry.condition.is_empty() {
                    ui.small(message(
                        "game_flags.context_condition",
                        &[("condition", crate::game_flags::prose(&entry.condition))],
                    ));
                }
            });
        }
        ui.separator();
        ui.strong(key("game_flags.context_neighbors"));
        ui.small(key("game_flags.context_neighbors_hint"));
        if let Some(routes) = &self.panel.routes {
            for before in [true, false] {
                ui.label(key(if before {
                    "game_flags.context_before"
                } else {
                    "game_flags.context_after"
                }));
                let adjacent = neighbors(routes, &request.script, before);
                if adjacent.is_empty() {
                    ui.weak(key("game_flags.context_no_neighbor"));
                }
                for (script, condition) in adjacent {
                    ui.horizontal_wrapped(|ui| {
                        if ui.link(script).clicked() {
                            selected = Some(script.into());
                        }
                        ui.small(if condition.is_empty() {
                            key("game_flags.context_no_condition")
                        } else {
                            crate::game_flags::prose(condition)
                        });
                    });
                }
            }
            if routes.scripts.iter().any(|route| {
                route.script.eq_ignore_ascii_case(&request.script) && route.unknown_jumps > 0
            }) {
                ui.small(key("game_flags.context_dynamic"));
            }
        } else {
            ui.small(key("game_flags.context_need_routes"));
        }
        ui.separator();
        let target = request.script.to_ascii_lowercase();
        if let Some(source) = self.panel.sources.get(&target) {
            let context = self
                .context
                .get_or_insert_with(|| SourceContext::new(source, &request.flag));
            ui.strong(key("game_flags.context_nearby"));
            if context.hits.is_empty() {
                ui.small(key("game_flags.context_no_call"));
            }
            let lines: Vec<_> = source.text.lines().collect();
            for hit in &context.hits {
                ui.label(message(
                    "game_flags.context_use",
                    &[
                        ("line", (hit.line + 1).to_string()),
                        (
                            "kind",
                            key(match hit.kind {
                                UseKind::Set => "game_flags.context_set",
                                UseKind::Add => "game_flags.context_add",
                                UseKind::Read => "game_flags.context_read",
                                UseKind::Related => "game_flags.context_related",
                            }),
                        ),
                    ],
                ));
                ui.code(lines[hit.line]);
                for &at in &hit.nearby {
                    ui.label(crate::game_flags::prose(&source.display(&source.sites[at])));
                }
            }
            egui::CollapsingHeader::new(key("game_flags.reference_calls")).show(ui, |ui| {
                let visible: BTreeSet<_> = context
                    .hits
                    .iter()
                    .flat_map(|hit| hit.line.saturating_sub(2)..(hit.line + 4).min(lines.len()))
                    .collect();
                for at in visible {
                    ui.monospace(format!("{}  {}", at + 1, lines[at]));
                }
            });
            egui::CollapsingHeader::new(key("game_flags.reference_dialogue")).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(key("game_flags.reference_search")),
                );
                let query = crate::i18n::simplified_label(&self.filter).to_lowercase();
                for site in &source.sites {
                    let text = crate::game_flags::prose(&source.display(site));
                    if crate::i18n::simplified_label(&text)
                        .to_lowercase()
                        .contains(&query)
                    {
                        ui.label(text);
                    }
                }
            });
        }
        if let Some(script) = selected {
            self.panel.activate_target(&script);
            self.request.as_mut().unwrap().script = script;
            self.context = None;
            self.filter.clear();
        }
    }
}
