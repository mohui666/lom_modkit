use crate::i18n;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use lom_core::{
    story_bookmarks::{self, Bookmark, RuntimeRoutes},
    story_routes::{RouteEdge, RouteGraph},
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    time::{Duration, Instant},
};

pub enum Action {
    Open(String),
    OpenMark,
}
pub struct View {
    zoom: f32,
    selected: Option<usize>,
    focus_current: bool,
    year: i32,
    month: i32,
    show_all_links: bool,
    layout: Option<RouteLayout>,
    runtime: Option<RuntimeRoutes>,
    runtime_error: Option<String>,
    runtime_checked: Option<Instant>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 0.85,
            selected: None,
            focus_current: true,
            year: 0,
            month: 0,
            show_all_links: false,
            layout: None,
            runtime: None,
            runtime_error: None,
            runtime_checked: None,
        }
    }
}
impl View {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        graph: &RouteGraph,
        marks: &[Bookmark],
        query: &str,
    ) -> Option<Action> {
        let mut action = None;
        if self
            .runtime_checked
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(1))
        {
            match story_bookmarks::read_runtime_routes(&story_bookmarks::default_root()) {
                Ok(snapshot) => {
                    self.runtime =
                        snapshot.filter(|value| same_game_root(&value.game_dir, &graph.game_dir));
                    self.runtime_error = None;
                }
                Err(error) => {
                    self.runtime = None;
                    self.runtime_error = Some(crate::error_ui::capture(error));
                }
            }
            self.runtime_checked = Some(Instant::now());
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        let query = display_label(query).to_lowercase();
        let current: BTreeSet<_> = marks
            .first()
            .and_then(|m| graph.owners.get(&m.script.to_lowercase()))
            .into_iter()
            .flatten()
            .copied()
            .collect();
        if self.selected.is_none_or(|i| i >= graph.stations.len()) {
            self.selected = current
                .first()
                .copied()
                .or((!graph.stations.is_empty()).then_some(0));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(i18n::key("routes.zoom"));
            ui.add(egui::Slider::new(&mut self.zoom, 0.45..=1.4).show_value(false));
            if ui.button(i18n::key("routes.return_to_mark")).clicked() {
                self.selected = current.first().copied().or(self.selected);
                self.focus_current = true;
            }
            if ui
                .add_enabled(
                    !marks.is_empty(),
                    egui::Button::new(i18n::key("routes.edit_marked_dialogue")),
                )
                .clicked()
            {
                action = Some(Action::OpenMark);
            }
            ui.label(i18n::key("routes.year"));
            ui.add(egui::DragValue::new(&mut self.year).range(0..=9));
            ui.label(i18n::key("routes.month"));
            ui.add(egui::DragValue::new(&mut self.month).range(0..=12));
            ui.small(i18n::key("routes.all_dates"));
            ui.checkbox(&mut self.show_all_links, i18n::key("routes.show_all_links"));
        });
        ui.small(i18n::key("routes.click_guide"));
        ui.small(i18n::key("routes.mark_guide"));
        if let Some(error) = &self.runtime_error {
            crate::error_ui::show_with_actions(ui, error);
        }
        if let Some(snapshot) = &self.runtime {
            if snapshot.has_return_slot() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(26, 132, 88),
                        i18n::message(
                            "routes.runtime.snapshot",
                            &[
                                ("script", snapshot.script.clone()),
                                (
                                    "target",
                                    snapshot
                                        .return_target()
                                        .map(str::to_owned)
                                        .unwrap_or_else(|| i18n::key("routes.runtime.empty")),
                                ),
                            ],
                        ),
                    );
                    if ui.button(i18n::key("routes.runtime.focus")).clicked() {
                        self.selected = graph
                            .owners
                            .get(&snapshot.script.to_lowercase())
                            .and_then(|nodes| nodes.first())
                            .copied()
                            .or(self.selected);
                        self.focus_current = true;
                    }
                });
            }
        }
        let focus_this_frame = std::mem::take(&mut self.focus_current);
        let height = ui.available_height().min(720.0).max(300.0);
        let detail_width = (ui.available_width() * 0.27).clamp(210.0, 290.0);
        let canvas_width = (ui.available_width() - detail_width - 12.0).max(200.0);
        let next: BTreeSet<_> = graph
            .edges
            .iter()
            .filter(|e| !e.by_date && self.selected == Some(e.from))
            .map(|e| e.to)
            .collect();
        let previous: BTreeSet<_> = graph
            .edges
            .iter()
            .filter(|edge| !edge.by_date && self.selected == Some(edge.to))
            .map(|edge| edge.from)
            .collect();
        let shown_edges = visible_edges(&graph.edges, self.selected, self.show_all_links);
        if self
            .layout
            .as_ref()
            .is_none_or(|layout| !layout.matches(graph))
        {
            self.layout = Some(RouteLayout::build(graph));
        }
        let layout = self.layout.as_ref().unwrap();
        let failed = layout.paths.iter().filter(|path| path.is_empty()).count();
        if failed > 0 {
            ui.colored_label(
                Color32::RED,
                i18n::message("routes.routing_failed", &[("count", failed.to_string())]),
            );
        }
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(canvas_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::both()
                        .id_salt("original-route-canvas")
                        .auto_shrink([false, false])
                        .max_height(height)
                        .show(ui, |ui| {
                            let size = layout.size * self.zoom;
                            let (canvas, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                            let painter = ui.painter_at(canvas);
                            painter.rect_filled(canvas, 8.0, ui.visuals().faint_bg_color);
                            let rects: Vec<_> = layout
                                .rects
                                .iter()
                                .map(|rect| {
                                    Rect::from_min_max(
                                        canvas.min + rect.min.to_vec2() * self.zoom,
                                        canvas.min + rect.max.to_vec2() * self.zoom,
                                    )
                                })
                                .collect();
                            let clip = ui.clip_rect();
                            let mut edge_order = shown_edges.clone();
                            edge_order.sort_by_key(|&index| {
                                let edge = &graph.edges[index];
                                (
                                    !edge.by_date,
                                    self.selected == Some(edge.from)
                                        || self.selected == Some(edge.to),
                                    self.runtime.as_ref().is_some_and(|snapshot| {
                                        runtime_edge_matches(snapshot, graph, edge)
                                    }),
                                )
                            });
                            for edge_index in edge_order {
                                let edge = &graph.edges[edge_index];
                                let focused = self.selected == Some(edge.from)
                                    || self.selected == Some(edge.to);
                                let outgoing = self.selected == Some(edge.from);
                                let runtime_return =
                                    self.runtime.as_ref().is_some_and(|snapshot| {
                                        runtime_edge_matches(snapshot, graph, edge)
                                    });
                                let colour = if edge.by_date {
                                    Color32::from_gray(195)
                                } else if runtime_return {
                                    Color32::from_rgb(26, 132, 88)
                                } else if !focused {
                                    Color32::from_gray(205)
                                } else if outgoing {
                                    Color32::from_rgb(184, 126, 34)
                                } else {
                                    Color32::from_rgb(59, 112, 169)
                                };
                                let points: Vec<_> = layout.paths[edge_index]
                                    .iter()
                                    .map(|point| canvas.min + point.to_vec2() * self.zoom)
                                    .collect();
                                if points.len() < 2 {
                                    continue;
                                }
                                let bounds = points.iter().fold(Rect::NOTHING, |bounds, point| {
                                    bounds.union(Rect::from_min_max(*point, *point))
                                });
                                if !bounds.expand(30.0 * self.zoom).intersects(clip) {
                                    continue;
                                }
                                let width = if edge.by_date {
                                    1.0
                                } else if runtime_return {
                                    2.4
                                } else if focused {
                                    1.8
                                } else {
                                    1.1
                                } * self.zoom.max(0.6);
                                crate::graph_edges::paint(
                                    &painter,
                                    &points,
                                    Stroke::new(width, colour),
                                    self.zoom,
                                    ui.visuals().faint_bg_color,
                                    edge.by_date,
                                );
                                if !edge.by_date {
                                    let b = *points.last().unwrap();
                                    if focused {
                                        // Keep the selected station's centre free of labels. Each
                                        // badge sits toward its other endpoint; full text is in details.
                                        let (port, outside) = if outgoing {
                                            (b, points[points.len() - 2])
                                        } else {
                                            (points[0], points[1])
                                        };
                                        let centre =
                                            port + (outside - port).normalized() * 28.0 * self.zoom;
                                        let badge = edge_badge(edge, edge_index);
                                        let font = FontId::proportional(11.0 * self.zoom);
                                        let galley = painter.layout_no_wrap(
                                            badge.clone(),
                                            font.clone(),
                                            colour,
                                        );
                                        let label_rect = Rect::from_center_size(
                                            centre,
                                            galley.size() + Vec2::new(8.0, 4.0) * self.zoom,
                                        );
                                        painter.rect_filled(
                                            label_rect,
                                            4.0 * self.zoom,
                                            Color32::from_rgb(249, 249, 247),
                                        );
                                        painter.text(
                                            label_rect.center(),
                                            Align2::CENTER_CENTER,
                                            badge,
                                            font,
                                            colour,
                                        );
                                        ui.interact(
                                            label_rect,
                                            ui.id().with(("route-edge", edge_index)),
                                            egui::Sense::hover(),
                                        )
                                        .on_hover_text(
                                            edge.labels
                                                .iter()
                                                .map(|label| localize_route_label(label))
                                                .collect::<Vec<_>>()
                                                .join(&i18n::key("routes.or_separator")),
                                        );
                                    }
                                }
                            }
                            let mut series = BTreeSet::new();
                            for (i, node) in graph.stations.iter().enumerate() {
                                let rect = rects[i];
                                if focus_this_frame && self.selected == Some(i) {
                                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                                }
                                if !rect.intersects(clip.expand(120.0)) {
                                    continue;
                                }
                                if !node.entry.main
                                    && series.insert((&node.entry.series, node.lane))
                                {
                                    painter.text(
                                        rect.left_top() - Vec2::new(0.0, 9.0 * self.zoom),
                                        Align2::LEFT_BOTTOM,
                                        if node.entry.series == RouteGraph::MARKED_SERIES {
                                            i18n::key("routes.marked_station")
                                        } else {
                                            i18n::message(
                                                "routes.side_series",
                                                &[("series", display_label(&node.entry.series))],
                                            )
                                        },
                                        FontId::proportional(12.0 * self.zoom),
                                        Color32::from_gray(120),
                                    );
                                }
                                let matches = (query.is_empty()
                                    || display_label(&format!(
                                        "{} {} {} {} {} {}",
                                        node.entry.title,
                                        node.entry.place,
                                        node.entry.script,
                                        node.entry.series,
                                        node.entry.condition,
                                        node.scripts.join(" ")
                                    ))
                                    .to_lowercase()
                                    .contains(query.as_str())
                                    || graph.edges.iter().any(|edge| {
                                        (edge.from == i || edge.to == i)
                                            && edge.labels.iter().any(|label| {
                                                localize_route_label(label)
                                                    .to_lowercase()
                                                    .contains(query.as_str())
                                            })
                                    }))
                                    && node.entry.matches_date(self.year, self.month, false);
                                let related = self.selected == Some(i)
                                    || previous.contains(&i)
                                    || next.contains(&i);
                                let fill = if self.selected == Some(i) {
                                    Color32::from_rgb(244, 229, 193)
                                } else if next.contains(&i) {
                                    Color32::from_rgb(248, 242, 225)
                                } else if previous.contains(&i) {
                                    Color32::from_rgb(229, 239, 248)
                                } else {
                                    Color32::from_rgb(243, 245, 247)
                                };
                                let border = if self.selected == Some(i) {
                                    Color32::from_rgb(151, 103, 24)
                                } else if current.contains(&i) {
                                    Color32::from_rgb(185, 57, 52)
                                } else if next.contains(&i) {
                                    Color32::from_rgb(195, 145, 38)
                                } else if previous.contains(&i) {
                                    Color32::from_rgb(59, 112, 169)
                                } else {
                                    Color32::from_gray(205)
                                };
                                let title = if node.entry.title.is_empty() {
                                    &node.entry.script
                                } else {
                                    &node.entry.title
                                };
                                let label = i18n::message(
                                    "routes.station_caption",
                                    &[
                                        ("when", display_label(&node.entry.when_label)),
                                        ("title", short(title, 30)),
                                        ("place", display_label(&node.entry.place)),
                                        ("script", node.entry.script.clone()),
                                    ],
                                );
                                let response = ui.put(
                                    rect,
                                    egui::Button::new(
                                        RichText::new(label).size(12.5 * self.zoom).color(
                                            if matches && related {
                                                Color32::from_gray(45)
                                            } else {
                                                Color32::from_gray(150)
                                            },
                                        ),
                                    )
                                    .fill(if matches {
                                        fill
                                    } else {
                                        ui.visuals().faint_bg_color
                                    })
                                    .stroke(Stroke::new(
                                        if related || current.contains(&i) {
                                            2.2_f32
                                        } else {
                                            1.0_f32
                                        },
                                        border,
                                    ))
                                    .corner_radius(9.0 * self.zoom),
                                );
                                let response = response.on_hover_text(i18n::message(
                                    "routes.station_hover",
                                    &[
                                        ("title", display_label(&node.entry.title)),
                                        ("key", node.entry.key.clone()),
                                        ("condition", localize_condition(&node.entry.condition)),
                                    ],
                                ));
                                if response.clicked() {
                                    self.selected = Some(i);
                                }
                                if response.double_clicked() {
                                    action = Some(Action::Open(node.entry.script.clone()));
                                }
                                if current.contains(&i) {
                                    let centre = rect.left_top() + Vec2::new(4.0, -2.0) * self.zoom;
                                    painter.circle_filled(
                                        centre,
                                        12.0 * self.zoom,
                                        Color32::from_rgb(185, 57, 52),
                                    );
                                    painter.text(
                                        centre,
                                        Align2::CENTER_CENTER,
                                        i18n::key("routes.current_badge"),
                                        FontId::proportional(13.0 * self.zoom),
                                        Color32::WHITE,
                                    );
                                } else if node.scripts.iter().any(|script| {
                                    marks
                                        .iter()
                                        .any(|mark| script.eq_ignore_ascii_case(&mark.script))
                                }) {
                                    painter.text(
                                        rect.right_top() + Vec2::new(-5.0, 5.0),
                                        Align2::RIGHT_TOP,
                                        "✓",
                                        FontId::proportional(14.0 * self.zoom),
                                        Color32::from_rgb(52, 123, 99),
                                    );
                                }
                            }
                        });
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                Vec2::new(detail_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("route-station-details")
                        .max_height(height)
                        .show(ui, |ui| {
                            if let Some(i) = self.selected {
                                let node = &graph.stations[i];
                                ui.heading(i18n::key(if node.entry.main {
                                    "routes.main_station"
                                } else if node.entry.series == RouteGraph::MARKED_SERIES {
                                    "routes.marked_station"
                                } else {
                                    "routes.side_station"
                                }));
                                ui.strong(display_label(&node.entry.title));
                                ui.label(display_label(&node.entry.when_label));
                                ui.label(i18n::message(
                                    "routes.place",
                                    &[("place", display_label(&node.entry.place))],
                                ));
                                ui.small(i18n::message(
                                    "routes.identity",
                                    &[
                                        ("series", display_label(&node.entry.series)),
                                        ("key", node.entry.key.clone()),
                                    ],
                                ));
                                if !node.entry.active {
                                    ui.label(i18n::key("routes.inactive"));
                                }
                                if !node.entry.condition.is_empty() {
                                    ui.separator();
                                    ui.strong(i18n::key("routes.entry_condition"));
                                    ui.label(localize_condition(&node.entry.condition));
                                }
                                if ui.button(i18n::key("routes.edit_chapter")).clicked() {
                                    action = Some(Action::Open(node.entry.script.clone()));
                                }
                                ui.separator();
                                ui.strong(i18n::key("routes.runtime.heading"));
                                if let Some(snapshot) = &self.runtime {
                                    ui.small(i18n::message(
                                        "routes.runtime.capture",
                                        &[
                                            ("time", snapshot.captured_utc.clone()),
                                            ("session", snapshot.session.clone()),
                                        ],
                                    ));
                                    if snapshot.has_return_slot()
                                        && node.scripts.iter().any(|script| {
                                            script.eq_ignore_ascii_case(&snapshot.script)
                                        })
                                    {
                                        ui.label(i18n::key("routes.runtime.slot_guide"));
                                        if let Some(target) = snapshot.return_target() {
                                            if ui
                                                .button(i18n::message(
                                                    "routes.runtime.destination",
                                                    &[
                                                        ("script", snapshot.script.clone()),
                                                        ("target", target.to_owned()),
                                                    ],
                                                ))
                                                .clicked()
                                            {
                                                action = Some(Action::Open(target.to_owned()));
                                            }
                                        } else {
                                            ui.label(i18n::key("routes.runtime.empty_guide"));
                                        }
                                    }
                                    let mut found = false;
                                    for transition in
                                        snapshot.transitions.iter().rev().filter(|transition| {
                                            node.scripts.iter().any(|script| {
                                                script.eq_ignore_ascii_case(&transition.from)
                                            })
                                        })
                                    {
                                        found = true;
                                        ui.label(i18n::message(
                                            "routes.runtime.observed",
                                            &[
                                                ("script", transition.from.clone()),
                                                ("target", transition.target.clone()),
                                                ("time", transition.captured_utc.clone()),
                                            ],
                                        ));
                                    }
                                    if !found {
                                        ui.small(i18n::key("routes.runtime.no_transition"));
                                    }
                                } else {
                                    ui.small(i18n::key("routes.runtime.missing"));
                                }
                                ui.separator();
                                ui.strong(i18n::key("routes.incoming"));
                                for (edge_index, edge) in graph
                                    .edges
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, edge)| edge.to == i && !edge.by_date)
                                {
                                    let source = &graph.stations[edge.from];
                                    if ui
                                        .button(i18n::message(
                                            "routes.edge_destination",
                                            &[
                                                ("number", edge_badge(edge, edge_index)),
                                                ("title", display_label(&source.entry.title)),
                                            ],
                                        ))
                                        .clicked()
                                    {
                                        self.selected = Some(edge.from);
                                        self.focus_current = true;
                                    }
                                    for label in &edge.labels {
                                        ui.small(localize_route_label(label));
                                    }
                                }
                                ui.separator();
                                ui.strong(i18n::key("routes.outgoing"));
                                for (edge_index, edge) in graph
                                    .edges
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, e)| e.from == i && !e.by_date)
                                {
                                    let target = &graph.stations[edge.to];
                                    if ui
                                        .button(i18n::message(
                                            "routes.edge_destination",
                                            &[
                                                ("number", edge_badge(edge, edge_index)),
                                                ("title", display_label(&target.entry.title)),
                                            ],
                                        ))
                                        .clicked()
                                    {
                                        self.selected = Some(edge.to);
                                        self.focus_current = true;
                                    }
                                    for label in &edge.labels {
                                        ui.small(localize_route_label(label));
                                    }
                                }
                                ui.separator();
                                ui.strong(i18n::key("routes.related_chapters"));
                                for script in &node.scripts {
                                    if ui.button(script).clicked() {
                                        action = Some(Action::Open(script.clone()));
                                    }
                                }
                            } else {
                                ui.label(i18n::key("routes.select_station"));
                            }
                            ui.separator();
                            ui.small(i18n::message(
                                "routes.summary",
                                &[
                                    ("stations", graph.stations.len().to_string()),
                                    (
                                        "routes",
                                        graph
                                            .edges
                                            .iter()
                                            .filter(|edge| !edge.by_date)
                                            .count()
                                            .to_string(),
                                    ),
                                ],
                            ));
                            if graph.unknown_jumps > 0 {
                                ui.small(i18n::message(
                                    "routes.unresolved",
                                    &[("count", graph.unknown_jumps.to_string())],
                                ));
                            }
                            ui.small(i18n::key("routes.dynamic_notice"));
                            ui.small(i18n::key("routes.mark_notice"));
                        });
                },
            );
        });
        action
    }
}

fn runtime_edge_matches(snapshot: &RuntimeRoutes, graph: &RouteGraph, edge: &RouteEdge) -> bool {
    !edge.by_date
        && snapshot.return_target().is_some_and(|target| {
            graph.stations[edge.from]
                .scripts
                .iter()
                .any(|script| script.eq_ignore_ascii_case(&snapshot.script))
                && graph.stations[edge.to]
                    .entry
                    .script
                    .eq_ignore_ascii_case(target)
        })
}

fn same_game_root(left: &std::path::Path, right: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        fn normalized(path: &std::path::Path) -> String {
            let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
            let text = if let Some(unc) = text.strip_prefix("//?/unc/") {
                format!("//{unc}")
            } else {
                text.strip_prefix("//?/").unwrap_or(&text).to_owned()
            };
            text.trim_end_matches('/').to_owned()
        }
        normalized(left) == normalized(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn visible_edges(edges: &[RouteEdge], selected: Option<usize>, all: bool) -> Vec<usize> {
    let mut indices: Vec<_> = edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            all || !edge.by_date && (selected == Some(edge.from) || selected == Some(edge.to))
        })
        .map(|(index, _)| index)
        .collect();
    indices.sort_by_key(|&index| {
        !edges[index].by_date
            && (selected == Some(edges[index].from) || selected == Some(edges[index].to))
    });
    indices
}

fn edge_badge(edge: &RouteEdge, index: usize) -> String {
    let candidate = edge.labels.iter().any(|label| {
        ["配置候选", "调用链候选", "全局配置候选"]
            .iter()
            .any(|term| label.contains(term))
    });
    i18n::message(
        if candidate {
            "routes.candidate_badge"
        } else {
            "routes.line_badge"
        },
        &[("number", (index + 1).to_string())],
    )
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn port(self, rect: Rect, fraction: f32) -> Pos2 {
        match self {
            Self::Left => Pos2::new(rect.left(), rect.top() + rect.height() * fraction),
            Self::Right => Pos2::new(rect.right(), rect.top() + rect.height() * fraction),
            Self::Top => Pos2::new(rect.left() + rect.width() * fraction, rect.top()),
            Self::Bottom => Pos2::new(rect.left() + rect.width() * fraction, rect.bottom()),
        }
    }

    fn outward(self) -> Vec2 {
        match self {
            Self::Left => Vec2::new(-1.0, 0.0),
            Self::Right => Vec2::new(1.0, 0.0),
            Self::Top => Vec2::new(0.0, -1.0),
            Self::Bottom => Vec2::new(0.0, 1.0),
        }
    }

    fn corner(self, rect: Rect, other: Pos2) -> Pos2 {
        match self {
            Self::Left if other.y <= rect.center().y => rect.left_top(),
            Self::Left => rect.left_bottom(),
            Self::Right if other.y <= rect.center().y => rect.right_top(),
            Self::Right => rect.right_bottom(),
            Self::Top if other.x <= rect.center().x => rect.left_top(),
            Self::Top => rect.right_top(),
            Self::Bottom if other.x <= rect.center().x => rect.left_bottom(),
            Self::Bottom => rect.right_bottom(),
        }
    }
}

struct RouteLayout {
    stations: Vec<(usize, i32)>,
    edges: Vec<(usize, usize)>,
    rects: Vec<Rect>,
    paths: Vec<Vec<Pos2>>,
    size: Vec2,
}

impl RouteLayout {
    fn matches(&self, graph: &RouteGraph) -> bool {
        self.stations
            .iter()
            .copied()
            .eq(graph.stations.iter().map(|node| (node.column, node.lane)))
            && self
                .edges
                .iter()
                .copied()
                .eq(graph.edges.iter().map(|edge| (edge.from, edge.to)))
    }

    fn build(graph: &RouteGraph) -> Self {
        const CLEARANCE: f32 = 12.0;
        let rects: Vec<_> = graph
            .stations
            .iter()
            .map(|node| {
                Rect::from_min_size(
                    Pos2::new(
                        45.0 + node.column as f32 * 250.0,
                        120.0 + node.lane as f32 * 155.0,
                    ),
                    Vec2::new(194.0, 100.0),
                )
            })
            .collect();
        let bounds = rects.iter().copied().fold(Rect::NOTHING, Rect::union);
        if rects.is_empty() {
            return Self {
                stations: vec![],
                edges: vec![],
                rects,
                paths: vec![],
                size: Vec2::new(300.0, 250.0),
            };
        }
        let sides: Vec<_> = graph
            .edges
            .iter()
            .map(|edge| {
                let from = rects[edge.from].center();
                let to = rects[edge.to].center();
                if edge.from == edge.to {
                    (Side::Right, Side::Top)
                } else if to.x > from.x {
                    (Side::Right, Side::Left)
                } else if to.x == from.x && to.y > from.y {
                    (Side::Bottom, Side::Top)
                } else {
                    (Side::Top, Side::Top)
                }
            })
            .collect();
        let mut groups: BTreeMap<(usize, Side), Vec<(usize, bool, usize)>> = BTreeMap::new();
        for (index, edge) in graph.edges.iter().enumerate() {
            groups
                .entry((edge.from, sides[index].0))
                .or_default()
                .push((index, true, edge.to));
            groups
                .entry((edge.to, sides[index].1))
                .or_default()
                .push((index, false, edge.from));
        }
        let mut ports = vec![(Pos2::ZERO, Pos2::ZERO); graph.edges.len()];
        for ((node, side), endpoints) in &mut groups {
            endpoints.sort_by(|left, right| {
                let a = rects[left.2].center();
                let b = rects[right.2].center();
                let (first, second) = if matches!(side, Side::Left | Side::Right) {
                    ((a.y, b.y), (a.x, b.x))
                } else {
                    ((a.x, b.x), (a.y, b.y))
                };
                first
                    .0
                    .total_cmp(&first.1)
                    .then(second.0.total_cmp(&second.1))
                    .then(left.0.cmp(&right.0))
            });
            for (rank, &(edge, source, _)) in endpoints.iter().enumerate() {
                let fraction = (rank + 1) as f32 / (endpoints.len() + 1) as f32;
                let port = side.port(rects[*node], fraction);
                if source {
                    ports[edge].0 = port;
                } else {
                    ports[edge].1 = port;
                }
            }
        }
        let obstacles: Vec<_> = rects.iter().map(|rect| rect.expand(CLEARANCE)).collect();
        let grid = RouteGrid::new(&obstacles, bounds);
        let paths: Vec<_> = graph
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                let (source, target) = ports[index];
                let (source_side, target_side) = sides[index];
                let source_stub = source + source_side.outward() * CLEARANCE;
                let target_stub = target + target_side.outward() * CLEARANCE;
                if let Some(mut direct) = direct_connection(
                    source_stub,
                    target_stub,
                    source_side,
                    target_side,
                    &obstacles,
                ) {
                    direct.insert(0, source);
                    direct.push(target);
                    return simplify_path(direct);
                }
                let start = source_side.corner(obstacles[edge.from], target);
                let end = target_side.corner(obstacles[edge.to], source);
                let mut path = vec![source, source_stub];
                // Let obstacle routing find a local corridor, including return
                // edges. A self-loop uses its own top-right corner, rather than
                // making a round trip around the entire chronology map.
                let Some(part) = grid.path(start, end) else {
                    return vec![];
                };
                path.extend(part);
                path.extend([target_stub, target]);
                simplify_path(path)
            })
            .collect();
        let maximum = paths
            .iter()
            .flatten()
            .fold(bounds.max.to_vec2(), |maximum, point| {
                maximum.max(point.to_vec2())
            });
        Self {
            stations: graph
                .stations
                .iter()
                .map(|node| (node.column, node.lane))
                .collect(),
            edges: graph
                .edges
                .iter()
                .map(|edge| (edge.from, edge.to))
                .collect(),
            rects,
            paths,
            size: maximum + Vec2::splat(35.0),
        }
    }
}

fn direct_connection(
    start: Pos2,
    end: Pos2,
    source: Side,
    target: Side,
    obstacles: &[Rect],
) -> Option<Vec<Pos2>> {
    let path = match (source, target) {
        (Side::Right, Side::Left) if start.x <= end.x => {
            let middle = (start.x + end.x) / 2.0;
            vec![
                start,
                Pos2::new(middle, start.y),
                Pos2::new(middle, end.y),
                end,
            ]
        }
        (Side::Bottom, Side::Top) if start.y <= end.y => {
            let middle = (start.y + end.y) / 2.0;
            vec![
                start,
                Pos2::new(start.x, middle),
                Pos2::new(end.x, middle),
                end,
            ]
        }
        _ => return None,
    };
    let clear = path.windows(2).all(|pair| {
        obstacles.iter().all(|rect| {
            if pair[0].x == pair[1].x {
                pair[0].x <= rect.left()
                    || pair[0].x >= rect.right()
                    || pair[0].y.max(pair[1].y) <= rect.top()
                    || pair[0].y.min(pair[1].y) >= rect.bottom()
            } else {
                pair[0].y <= rect.top()
                    || pair[0].y >= rect.bottom()
                    || pair[0].x.max(pair[1].x) <= rect.left()
                    || pair[0].x.min(pair[1].x) >= rect.right()
            }
        })
    });
    clear.then_some(path)
}

/// Orthogonal visibility grid in model coordinates. Card margins are blocked
/// once, and A* minimizes distance plus bends along the remaining corridors.
struct RouteGrid {
    xs: Vec<f32>,
    ys: Vec<f32>,
    horizontal: Vec<bool>,
    vertical: Vec<bool>,
}

impl RouteGrid {
    fn new(obstacles: &[Rect], bounds: Rect) -> Self {
        let mut xs: Vec<_> = obstacles
            .iter()
            .flat_map(|rect| [rect.left(), rect.right()])
            .collect();
        let mut ys: Vec<_> = obstacles
            .iter()
            .flat_map(|rect| [rect.top(), rect.bottom()])
            .collect();
        xs.push(bounds.left() - 24.0);
        for lane in 0..6 {
            xs.push(bounds.right() + 36.0 + lane as f32 * 12.0);
            ys.push(24.0 + lane as f32 * 12.0);
        }
        ys.push(bounds.bottom() + 24.0);
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        let mut grid = Self {
            horizontal: vec![true; (xs.len() - 1) * ys.len()],
            vertical: vec![true; xs.len() * (ys.len() - 1)],
            xs,
            ys,
        };
        let width = grid.xs.len();
        for rect in obstacles {
            let left = grid
                .xs
                .binary_search_by(|x| x.total_cmp(&rect.left()))
                .unwrap();
            let right = grid
                .xs
                .binary_search_by(|x| x.total_cmp(&rect.right()))
                .unwrap();
            let top = grid
                .ys
                .binary_search_by(|y| y.total_cmp(&rect.top()))
                .unwrap();
            let bottom = grid
                .ys
                .binary_search_by(|y| y.total_cmp(&rect.bottom()))
                .unwrap();
            for y in top + 1..bottom {
                for x in left..right {
                    grid.horizontal[y * (width - 1) + x] = false;
                }
            }
            for x in left + 1..right {
                for y in top..bottom {
                    grid.vertical[y * width + x] = false;
                }
            }
        }
        grid
    }

    fn path(&self, start: Pos2, end: Pos2) -> Option<Vec<Pos2>> {
        let width = self.xs.len();
        let height = self.ys.len();
        let index = |point: Pos2| {
            let x = self.xs.binary_search_by(|x| x.total_cmp(&point.x)).ok()?;
            let y = self.ys.binary_search_by(|y| y.total_cmp(&point.y)).ok()?;
            Some(y * width + x)
        };
        let start = index(start)?;
        let target = index(end)?;
        let point = |index: usize| Pos2::new(self.xs[index % width], self.ys[index / width]);
        let heuristic = |index: usize| {
            ((point(index).x - end.x).abs() * 10.0 + (point(index).y - end.y).abs() * 10.0).round()
                as u32
        };
        let mut costs = vec![u32::MAX; width * height * 3];
        let mut previous = vec![usize::MAX; costs.len()];
        let mut pending = BinaryHeap::new();
        costs[start * 3 + 2] = 0;
        pending.push(Reverse((heuristic(start), 0, start * 3 + 2)));
        while let Some(Reverse((_, cost, state))) = pending.pop() {
            if costs[state] != cost {
                continue;
            }
            let current = state / 3;
            if current == target {
                let mut result = vec![];
                let mut cursor = state;
                loop {
                    result.push(point(cursor / 3));
                    if previous[cursor] == usize::MAX {
                        break;
                    }
                    cursor = previous[cursor];
                }
                result.reverse();
                return Some(result);
            }
            let x = current % width;
            let y = current / width;
            for direction in 0..4 {
                let neighbor = match direction {
                    0 if x > 0 && self.horizontal[y * (width - 1) + x - 1] => {
                        Some((current - 1, 0))
                    }
                    1 if x + 1 < width && self.horizontal[y * (width - 1) + x] => {
                        Some((current + 1, 0))
                    }
                    2 if y > 0 && self.vertical[(y - 1) * width + x] => Some((current - width, 1)),
                    3 if y + 1 < height && self.vertical[y * width + x] => {
                        Some((current + width, 1))
                    }
                    _ => None,
                };
                let Some((neighbor, orientation)) = neighbor else {
                    continue;
                };
                let distance = point(current).distance(point(neighbor));
                let turn = if state % 3 != 2 && state % 3 != orientation {
                    100
                } else {
                    0
                };
                let next_cost = cost + (distance * 10.0).round() as u32 + turn;
                let next = neighbor * 3 + orientation;
                if next_cost < costs[next] {
                    costs[next] = next_cost;
                    previous[next] = state;
                    pending.push(Reverse((next_cost + heuristic(neighbor), next_cost, next)));
                }
            }
        }
        None
    }
}

fn simplify_path(points: Vec<Pos2>) -> Vec<Pos2> {
    let mut result: Vec<Pos2> = Vec::new();
    for point in points {
        if result.last() == Some(&point) {
            continue;
        }
        while result.len() >= 2 {
            let before = result[result.len() - 2];
            let last = result[result.len() - 1];
            let a = last - before;
            let b = point - last;
            if a.x * b.y != a.y * b.x || a.dot(b) < 0.0 {
                break;
            }
            result.pop();
        }
        result.push(point);
    }
    result
}

fn localize_route_label(value: &str) -> String {
    if let Some((script, condition)) = value.split_once(" · ") {
        if lom_core::vanilla_patch::valid_target(script) {
            return i18n::message(
                "routes.route_condition",
                &[
                    ("script", script.to_owned()),
                    ("condition", localize_condition(condition)),
                ],
            );
        }
    }
    localize_condition(value)
}

fn localize_condition(value: &str) -> String {
    localize_condition_using(value, &i18n::message)
}

fn localize_condition_using(
    value: &str,
    translate: &dyn Fn(&str, &[(&str, String)]) -> String,
) -> String {
    if let Some((parent, condition)) = value.rsplit_once(" / ") {
        if let Some(condition) = system_condition(condition, translate) {
            return translate(
                "routes.condition.path",
                &[
                    ("parent", localize_condition_using(parent, translate)),
                    ("condition", condition),
                ],
            );
        }
    }
    system_condition(value, translate).unwrap_or_else(|| display_label(value))
}

fn system_condition(
    value: &str,
    translate: &dyn Fn(&str, &[(&str, String)]) -> String,
) -> Option<String> {
    // Only the reader's anchored machine templates are interpreted. Option
    // text and unknown Lua expressions remain opaque data, including Chinese.
    if let Some(text) = value.strip_prefix("选项：") {
        return Some(translate(
            "routes.condition.option",
            &[("text", display_label(text))],
        ));
    }
    if let Some(condition) = value.strip_prefix("否则若 ") {
        return Some(translate(
            "routes.condition.else_if",
            &[("condition", localize_condition_using(condition, translate))],
        ));
    }
    if let Some(condition) = value
        .strip_prefix("其余分支（")
        .and_then(|rest| rest.strip_suffix('）'))
    {
        return Some(translate(
            "routes.condition.otherwise",
            &[("condition", localize_condition_using(condition, translate))],
        ));
    }
    if let Some(key) = value
        .strip_prefix("场所检定 ")
        .and_then(|rest| rest.strip_suffix("：配置候选"))
    {
        if lom_core::vanilla_patch::valid_target(key) {
            return Some(translate(
                "routes.condition.position",
                &[("key", key.to_owned())],
            ));
        }
    }
    if let Some((key, result)) = value
        .strip_prefix("检定 ")
        .and_then(|rest| rest.split_once("：结果 "))
    {
        if lom_core::vanilla_patch::valid_target(key)
            && !result.is_empty()
            && result.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Some(translate(
                "routes.condition.check",
                &[("key", key.to_owned()), ("result", result.to_owned())],
            ));
        }
    }
    if let Some((source, detail)) = value
        .strip_prefix("返回")
        .and_then(|rest| rest.split_once('：'))
    {
        let detail_key = match detail {
            "调用链候选" => "routes.condition.call_candidates",
            "全局配置候选（调用上下文未记录）" => {
                "routes.condition.global_candidates"
            }
            _ => return None,
        };
        let detail = translate(detail_key, &[]);
        if let Some(slot) = source
            .strip_prefix("临时章节槽 ")
            .filter(|slot| matches!(*slot, "1" | "2"))
        {
            return Some(translate(
                "routes.condition.return_temp",
                &[("slot", slot.to_owned()), ("detail", detail)],
            ));
        }
        if source == "当前游历章节" {
            return Some(translate(
                "routes.condition.return_travel",
                &[("detail", detail)],
            ));
        }
        if let Some(variable) = source
            .strip_prefix("跨章节变量 ")
            .filter(|name| lom_core::vanilla_patch::valid_target(name))
        {
            return Some(translate(
                "routes.condition.return_variable",
                &[("variable", variable.to_owned()), ("detail", detail)],
            ));
        }
        return None;
    }
    let key = match value {
        "函数体内" => "routes.condition.function",
        "循环或代码块内" => "routes.condition.loop",
        "剧情推进" => "routes.condition.progress",
        "接入章节" => "routes.condition.chapter_entry",
        "入口默认推进（章节内仍可改写）" => "routes.condition.default_progress",
        "时间顺序，不表示必然接续" => "routes.condition.date_order",
        _ => return None,
    };
    Some(translate(key, &[]))
}

fn display_label(value: &str) -> String {
    if crate::i18n::locale() == "chs" {
        crate::i18n::simplified_label(value)
    } else {
        value.to_owned()
    }
}

fn short(value: &str, count: usize) -> String {
    let value = display_label(value);
    let mut chars = value.chars();
    let value: String = chars.by_ref().take(count).collect();
    if chars.next().is_some() {
        format!("{value}…")
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lom_core::{story_bookmarks::TimelineEntry, story_routes::Station};

    fn fixture() -> RouteGraph {
        let stations = ["A", "B", "C", "D"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| Station {
                entry: TimelineEntry {
                    script: name.into(),
                    title: format!("Node{name}"),
                    series: "main".into(),
                    key: name.into(),
                    main: true,
                    next_key: String::new(),
                    condition: String::new(),
                    place: "Place".into(),
                    when_label: "Date".into(),
                    from_round: Some(0),
                    to_round: Some(0),
                    active: true,
                },
                scripts: vec![name.into()],
                column: index,
                lane: 0,
                spine: true,
            })
            .collect();
        let edges = [
            (0, 1, false),
            (1, 2, false),
            (2, 3, false),
            (1, 0, false),
            (1, 1, false),
            (0, 2, true),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (from, to, by_date))| RouteEdge {
            from,
            to,
            by_date,
            labels: vec![if index == 1 {
                "场所检定：配置候选".into()
            } else {
                "条件".into()
            }],
        })
        .collect();
        RouteGraph {
            stations,
            edges,
            owners: BTreeMap::from([("b".into(), vec![1])]),
            ..Default::default()
        }
    }

    fn crosses_card(a: Pos2, b: Pos2, rect: Rect) -> bool {
        let epsilon = 0.001;
        if (a.x - b.x).abs() < epsilon {
            a.x > rect.left() + epsilon
                && a.x < rect.right() - epsilon
                && a.y.max(b.y) > rect.top() + epsilon
                && a.y.min(b.y) < rect.bottom() - epsilon
        } else {
            a.y > rect.top() + epsilon
                && a.y < rect.bottom() - epsilon
                && a.x.max(b.x) > rect.left() + epsilon
                && a.x.min(b.x) < rect.right() - epsilon
        }
    }

    #[test]
    fn focused_routes_keep_direct_neighbors_and_candidate_labels() {
        let graph = fixture();
        assert_eq!(
            visible_edges(&graph.edges, Some(1), false),
            vec![0, 1, 3, 4]
        );
        assert_eq!(visible_edges(&graph.edges, Some(3), false), vec![2]);
        assert_eq!(
            visible_edges(&graph.edges, Some(1), true).len(),
            graph.edges.len()
        );
        assert_eq!(edge_badge(&graph.edges[1], 1), "候2");
        assert!(graph.edges[1].labels[0].contains("配置候选"));
    }

    #[test]
    fn runtime_highlight_uses_the_captured_slot_and_never_a_global_candidate() {
        let mut graph = fixture();
        graph.stations[0].scripts.push("S0019_01".into());
        let mut snapshot: RuntimeRoutes = serde_json::from_value(serde_json::json!({
            "schema":1,"game_dir":"game","session":"current-session","captured_utc":"time",
            "status":"story","script":"S0019_01","temp_script1":"B","travel_script":"C","transitions":[]
        })).unwrap();
        assert!(runtime_edge_matches(&snapshot, &graph, &graph.edges[0]));
        assert!(!runtime_edge_matches(&snapshot, &graph, &graph.edges[1]));
        snapshot.temp_script1.clear();
        assert!(!runtime_edge_matches(&snapshot, &graph, &graph.edges[0]));
        snapshot.temp_script1 = "B".into();
        snapshot.status = "inactive".into();
        assert!(!runtime_edge_matches(&snapshot, &graph, &graph.edges[0]));
    }

    #[cfg(windows)]
    #[test]
    fn runtime_installation_matching_accepts_windows_case_and_verbatim_paths() {
        use std::path::Path;
        let game = Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal");
        assert!(same_game_root(
            game,
            Path::new(r"c:\program files (x86)\steam\steamapps\common\legendofmortal")
        ));
        assert!(same_game_root(
            game,
            Path::new(r"\\?\C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal\")
        ));
        assert!(same_game_root(
            Path::new(r"\\SERVER\share\game"),
            Path::new(r"\\?\UNC\server\share\game\")
        ));
        assert!(!same_game_root(
            game,
            Path::new(r"D:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal")
        ));
        assert!(!same_game_root(
            game,
            Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal-copy")
        ));
    }

    #[test]
    fn self_loop_stays_beside_its_station_instead_of_wrapping_the_whole_map() {
        let graph = fixture();
        let layout = RouteLayout::build(&graph);
        let own = layout.rects[1].expand(12.0);
        assert!(layout.paths[4].iter().all(|point| own.contains(*point)));
    }

    #[test]
    fn adjacent_stations_connect_directly_but_intermediate_cards_still_block_shortcuts() {
        let mut graph = fixture();
        graph.edges.truncate(1);
        let layout = RouteLayout::build(&graph);
        assert_eq!(
            layout.paths[0],
            vec![
                layout.rects[0].right_center(),
                layout.rects[1].left_center()
            ]
        );

        graph.edges[0].to = 2;
        let layout = RouteLayout::build(&graph);
        assert!(layout.paths[0].len() > 2);
        for segment in layout.paths[0].windows(2) {
            assert!(!crosses_card(segment[0], segment[1], layout.rects[1]));
        }
    }

    #[test]
    fn orthogonal_routes_avoid_all_cards_and_assign_sorted_ports() {
        let graph = fixture();
        let layout = RouteLayout::build(&graph);
        assert!(layout.matches(&graph));
        for (index, path) in layout.paths.iter().enumerate() {
            assert!(path.len() >= 2, "missing route {index}");
            for segment in path.windows(2) {
                assert!(segment[0].x == segment[1].x || segment[0].y == segment[1].y);
                for (card, rect) in layout.rects.iter().enumerate() {
                    assert!(
                        !crosses_card(segment[0], segment[1], *rect),
                        "edge {index} crosses card {card}"
                    );
                }
            }
        }
        assert!(layout.paths[4][0].y < layout.paths[1][0].y);
        assert!(layout.paths[3]
            .iter()
            .any(|point| point.y <= layout.rects[0].top() - 12.0));
        let mut moved = graph.clone();
        moved.stations[2].column += 1;
        assert!(!layout.matches(&moved));
    }

    #[test]
    fn route_view_toggles_all_links_selects_unrelated_nodes_and_returns_to_mark() {
        let graph = fixture();
        let marks = [Bookmark {
            schema: 1,
            script: "B".into(),
            text_key: String::new(),
            text: String::new(),
            game_dir: "game".into(),
            year: 1,
            month: 1,
            stage: 1,
            captured_utc: String::new(),
        }];
        let context = egui::Context::default();
        context.enable_accesskit();
        let draw = |view: &mut View, events| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        view.show(ui, &graph, &marks, "");
                    });
                },
            )
        };
        let click = |output: &egui::FullOutput, role, text: &str| {
            let target = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == role && node.label().is_some_and(|label| label.contains(text))
                })
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
        let mut view = View::default();
        let initial = draw(&mut view, vec![]);
        assert_eq!(view.selected, Some(1));
        assert!(!view.show_all_links);
        let all = click(
            &initial,
            egui::accesskit::Role::CheckBox,
            &i18n::key("routes.show_all_links"),
        );
        let unrelated = click(&initial, egui::accesskit::Role::Button, "NodeD");
        let current = click(
            &initial,
            egui::accesskit::Role::Button,
            &i18n::key("routes.return_to_mark"),
        );
        draw(&mut view, all);
        assert!(view.show_all_links);
        draw(&mut view, unrelated);
        assert_eq!(view.selected, Some(3));
        draw(&mut view, current);
        assert_eq!(view.selected, Some(1));
    }

    #[test]
    fn system_conditions_localize_in_all_four_languages_without_rewriting_option_text() {
        let body = "保留选项：否则若、调用链候选和场所检定字样";
        for locale in ["chs", "cht", "ja", "ko"] {
            let translate = |key: &str, arguments: &[(&str, String)]| {
                i18n::message_for_locale(key, arguments, locale)
            };
            let option = translate("routes.condition.option", &[("text", body.into())]);
            assert_eq!(
                localize_condition_using(&format!("选项：{body}"), &translate),
                option
            );
            assert!(option.contains(body));
            let otherwise = translate("routes.condition.else_if", &[("condition", option)]);
            assert_eq!(
                localize_condition_using(&format!("否则若 选项：{body}"), &translate),
                otherwise
            );
            let remaining = translate("routes.condition.otherwise", &[("condition", otherwise)]);
            assert_eq!(
                localize_condition_using(&format!("其余分支（否则若 选项：{body}）"), &translate),
                remaining
            );
            let call = translate("routes.condition.call_candidates", &[]);
            let global = translate("routes.condition.global_candidates", &[]);
            let cases = [
                (
                    "检定 CHECK_001：结果 2",
                    "routes.condition.check",
                    vec![("key", "CHECK_001".into()), ("result", "2".into())],
                ),
                (
                    "场所检定 Travel_001：配置候选",
                    "routes.condition.position",
                    vec![("key", "Travel_001".into())],
                ),
                (
                    "返回临时章节槽 1：调用链候选",
                    "routes.condition.return_temp",
                    vec![("slot", "1".into()), ("detail", call.clone())],
                ),
                (
                    "返回临时章节槽 2：全局配置候选（调用上下文未记录）",
                    "routes.condition.return_temp",
                    vec![("slot", "2".into()), ("detail", global.clone())],
                ),
                (
                    "返回当前游历章节：调用链候选",
                    "routes.condition.return_travel",
                    vec![("detail", call)],
                ),
                (
                    "返回跨章节变量 var1：全局配置候选（调用上下文未记录）",
                    "routes.condition.return_variable",
                    vec![("variable", "var1".into()), ("detail", global)],
                ),
                ("函数体内", "routes.condition.function", vec![]),
                ("循环或代码块内", "routes.condition.loop", vec![]),
                ("剧情推进", "routes.condition.progress", vec![]),
                ("接入章节", "routes.condition.chapter_entry", vec![]),
                (
                    "入口默认推进（章节内仍可改写）",
                    "routes.condition.default_progress",
                    vec![],
                ),
                (
                    "时间顺序，不表示必然接续",
                    "routes.condition.date_order",
                    vec![],
                ),
            ];
            for (source, key, arguments) in cases {
                assert_eq!(
                    localize_condition_using(source, &translate),
                    translate(key, &arguments),
                    "{locale}/{source}"
                );
            }
            let raw = "checkpointmanager.Condition(\"否则若_调用链候选\")";
            assert_eq!(localize_condition_using(raw, &translate), raw);
            let path = "选项：原始对白 / 场所检定 Travel_001：配置候选";
            let expected = translate(
                "routes.condition.path",
                &[
                    (
                        "parent",
                        translate("routes.condition.option", &[("text", "原始对白".into())]),
                    ),
                    (
                        "condition",
                        translate("routes.condition.position", &[("key", "Travel_001".into())]),
                    ),
                ],
            );
            assert_eq!(localize_condition_using(path, &translate), expected);
        }
    }
}
