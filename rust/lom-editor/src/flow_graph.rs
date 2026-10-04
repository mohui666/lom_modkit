//! Layout and drawing for the editor's actual intra-chapter control-flow edges.
use super::{character_display_name, edges, text};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use lom_core::project::Project;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const CARD: Vec2 = Vec2::new(192.0, 72.0);
const COLUMN: f32 = 236.0;
const ROW: f32 = 126.0;

struct Link {
    from: usize,
    to: Option<usize>,
    target: String,
    label: String,
    points: Vec<Pos2>,
    label_at: Pos2,
    label_width: f32,
    backward: bool,
}

struct Layout {
    cards: Vec<Rect>,
    #[cfg(test)]
    ranks: Vec<usize>,
    links: Vec<Link>,
    reachable: BTreeSet<usize>,
    size: Vec2,
}

fn find_back_edges(
    index: usize,
    graph: &[Vec<usize>],
    colors: &mut [u8],
    back_edges: &mut BTreeSet<(usize, usize)>,
) {
    colors[index] = 1;
    for &next in &graph[index] {
        match colors[next] {
            0 => find_back_edges(next, graph, colors, back_edges),
            1 => {
                back_edges.insert((index, next));
            }
            _ => {}
        }
    }
    colors[index] = 2;
}

fn layout(story: &Value) -> Layout {
    let nodes = story["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (text(node, "id"), index))
        .collect();
    let raw = edges(story);
    let mut forward = vec![Vec::new(); nodes.len()];
    let mut reverse = vec![Vec::new(); nodes.len()];
    for (from, to, _) in &raw {
        if let (Some(&a), Some(&b)) = (indices.get(from.as_str()), indices.get(to.as_str())) {
            forward[a].push(b);
            reverse[b].push(a);
        }
    }
    let mut back_edges = BTreeSet::new();
    let mut colors = vec![0; nodes.len()];
    if let Some(&start) = indices.get(text(story, "start")) {
        find_back_edges(start, &forward, &mut colors, &mut back_edges);
    }
    for index in 0..nodes.len() {
        if colors[index] == 0 {
            find_back_edges(index, &forward, &mut colors, &mut back_edges);
        }
    }
    // Exclude only DFS return edges from ranking. Collapsing whole cycles would
    // erase the parallel branches and merges inside a loop. All edges are drawn.
    let mut indegree = vec![0; nodes.len()];
    for (index, children) in forward.iter().enumerate() {
        for &child in children {
            if !back_edges.contains(&(index, child)) {
                indegree[child] += 1;
            }
        }
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .enumerate()
        .filter(|(_, degree)| **degree == 0)
        .map(|(index, _)| index)
        .collect();
    let mut ranks = vec![0; nodes.len()];
    while let Some(index) = ready.pop_first() {
        for &child in &forward[index] {
            if back_edges.contains(&(index, child)) {
                continue;
            }
            ranks[child] = ranks[child].max(ranks[index] + 1);
            indegree[child] -= 1;
            if indegree[child] == 0 {
                ready.insert(child);
            }
        }
    }
    let mut rows = BTreeMap::<usize, Vec<usize>>::new();
    for (index, &rank) in ranks.iter().enumerate() {
        rows.entry(rank).or_default().push(index);
    }
    let max_columns = rows.values().map(Vec::len).max().unwrap_or(1);
    let mut x = vec![0.0_f32; nodes.len()];
    for members in rows.values_mut() {
        let center = |index: usize| {
            let parents: Vec<_> = reverse[index]
                .iter()
                .filter(|&&parent| ranks[parent] < ranks[index])
                .collect();
            if parents.is_empty() {
                index as f32 * COLUMN
            } else {
                parents.iter().map(|&&parent| x[parent]).sum::<f32>() / parents.len() as f32
            }
        };
        members.sort_by(|&a, &b| center(a).total_cmp(&center(b)).then(a.cmp(&b)));
        let margin = (max_columns - members.len()) as f32 * COLUMN / 2.0;
        for (column, &index) in members.iter().enumerate() {
            x[index] = margin + column as f32 * COLUMN;
        }
    }
    let back_count = raw
        .iter()
        .filter(|(from, to, _)| {
            indices
                .get(from.as_str())
                .zip(indices.get(to.as_str()))
                .is_some_and(|(&a, &b)| ranks[b] <= ranks[a])
        })
        .count();
    let left = if back_count > 0 {
        144.0 + back_count as f32 * 16.0
    } else {
        20.0
    };
    let cards: Vec<_> = x
        .iter()
        .enumerate()
        .map(|(index, &x)| {
            Rect::from_min_size(Pos2::new(left + x, 32.0 + ranks[index] as f32 * ROW), CARD)
        })
        .collect();
    let right = left + (max_columns - 1) as f32 * COLUMN + CARD.x;
    let mut links = Vec::new();
    let mut back_lane = 0;
    let mut long_lane = 0;
    for (from, target, label) in raw {
        let Some(&a) = indices.get(from.as_str()) else {
            continue;
        };
        let to = indices.get(target.as_str()).copied();
        let mut points = Vec::new();
        let backward = to.is_some_and(|b| ranks[b] <= ranks[a]);
        let label_at;
        let label_width;
        if let Some(b) = to {
            if backward {
                let lane = left - 30.0 - back_lane as f32 * 16.0;
                back_lane += 1;
                if a == b {
                    points.extend([
                        cards[a].left_center(),
                        Pos2::new(lane, cards[a].center().y),
                        Pos2::new(lane, cards[a].top() - 20.0),
                        Pos2::new(cards[a].center().x, cards[a].top() - 20.0),
                        cards[a].center_top(),
                    ]);
                } else {
                    points.extend([
                        cards[a].left_center(),
                        Pos2::new(lane, cards[a].center().y),
                        Pos2::new(lane, cards[b].center().y),
                        cards[b].left_center(),
                    ]);
                }
                label_width = 112.0;
                label_at = Pos2::new(lane - label_width - 5.0, cards[a].center().y - 4.0);
            } else {
                let exits: Vec<_> = forward[a]
                    .iter()
                    .copied()
                    .filter(|&child| ranks[child] > ranks[a])
                    .collect();
                let entries: Vec<_> = reverse[b]
                    .iter()
                    .copied()
                    .filter(|&parent| ranks[parent] < ranks[b])
                    .collect();
                let exit = exits.iter().position(|&index| index == b).unwrap_or(0);
                let entry = entries.iter().position(|&index| index == a).unwrap_or(0);
                let start = Pos2::new(
                    cards[a].left() + CARD.x * (exit + 1) as f32 / (exits.len() + 1) as f32,
                    cards[a].bottom(),
                );
                let end = Pos2::new(
                    cards[b].left() + CARD.x * (entry + 1) as f32 / (entries.len() + 1) as f32,
                    cards[b].top(),
                );
                if ranks[b] == ranks[a] + 1 {
                    let middle = (start.y + end.y) / 2.0;
                    points.extend([
                        start,
                        Pos2::new(start.x, middle),
                        Pos2::new(end.x, middle),
                        end,
                    ]);
                    let span = (start.x - end.x).abs();
                    let branch_label = label != "下一步" && label != "跳转";
                    label_width = if branch_label {
                        CARD.x - 12.0
                    } else if span < 20.0 {
                        112.0
                    } else {
                        (span - 12.0).clamp(36.0, 156.0)
                    };
                    label_at = Pos2::new(
                        if branch_label {
                            cards[b].center().x - label_width / 2.0
                        } else if span < 20.0 {
                            start.x + 6.0
                        } else {
                            (start.x + end.x - label_width) / 2.0
                        },
                        middle - 4.0,
                    );
                } else {
                    let lane = right + 30.0 + long_lane as f32 * 18.0;
                    long_lane += 1;
                    points.extend([
                        start,
                        Pos2::new(start.x, start.y + 18.0),
                        Pos2::new(lane, start.y + 18.0),
                        Pos2::new(lane, end.y - 18.0),
                        Pos2::new(end.x, end.y - 18.0),
                        end,
                    ]);
                    label_width = 156.0;
                    label_at = Pos2::new(lane + 5.0, start.y + 14.0);
                }
            }
        } else {
            let start = cards[a].right_center();
            points.extend([start, Pos2::new(right + 30.0, start.y)]);
            label_width = 156.0;
            label_at = Pos2::new(right + 34.0, start.y - 4.0);
        }
        links.push(Link {
            from: a,
            to,
            target,
            label,
            points,
            label_at,
            label_width,
            backward,
        });
    }
    let mut reachable = BTreeSet::new();
    let start = indices
        .get(text(story, "start"))
        .copied()
        .or_else(|| (!nodes.is_empty()).then_some(0));
    let mut pending: VecDeque<_> = start.into_iter().collect();
    while let Some(index) = pending.pop_front() {
        if reachable.insert(index) {
            pending.extend(forward[index].iter().copied());
        }
    }
    let extra = if links.iter().any(|link| link.to.is_none()) || long_lane > 0 {
        190.0 + long_lane as f32 * 18.0
    } else {
        20.0
    };
    let height = cards.iter().map(Rect::bottom).fold(32.0_f32, f32::max) + 30.0;
    Layout {
        cards,
        #[cfg(test)]
        ranks,
        links,
        reachable,
        size: Vec2::new(right + extra, height),
    }
}

#[derive(Clone)]
struct ViewState {
    zoom: f32,
    selected: Option<String>,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            selected: None,
        }
    }
}

fn summary(project: &Project, node: &Value) -> String {
    if text(node, "type") == "end" && !text(node, "next_script").is_empty() {
        return format!("→ 章节 {}", text(node, "next_script"));
    }
    if text(node, "type") == "raw" {
        return "Lua 逻辑 · 结果未知".into();
    }
    if !text(node, "text").is_empty() {
        return text(node, "text").replace(['\n', '\r'], " ");
    }
    if !text(node, "character").is_empty() {
        return character_display_name(project, text(node, "character"));
    }
    for key in ["image", "view", "name", "flag", "key", "scene"] {
        if !text(node, key).is_empty() {
            return text(node, key).into();
        }
    }
    for key in ["options", "cases", "bands"] {
        if let Some(rows) = node[key].as_array() {
            return format!("{} 条分支", rows.len());
        }
    }
    String::new()
}

fn paint_text(
    painter: &egui::Painter,
    at: Pos2,
    text: String,
    size: f32,
    width: f32,
    color: Color32,
) {
    let mut job = egui::text::LayoutJob::simple(text, FontId::proportional(size), color, width);
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    painter.galley(at, painter.layout_job(job), color);
}

pub(super) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    story: &Value,
    selected: usize,
) -> Option<usize> {
    let nodes = story["nodes"].as_array()?;
    let layout = layout(story);
    let state_id = ui.id().with(("flow-view", text(story, "id")));
    let mut state = ui
        .data_mut(|data| data.get_temp::<ViewState>(state_id))
        .unwrap_or_default();
    let selected_id = nodes.get(selected).map(|node| text(node, "id").to_owned());
    let mut locate = selected_id != state.selected;
    ui.horizontal_wrapped(|ui| {
        if ui.button("−").on_hover_text("缩小流程图").clicked() {
            state.zoom = (state.zoom - 0.1).max(0.35);
            locate = true;
        }
        ui.label(format!("{:.0}%", state.zoom * 100.0));
        if ui.button("＋").on_hover_text("放大流程图").clicked() {
            state.zoom = (state.zoom + 0.1).min(1.75);
            locate = true;
        }
        if ui.button("适应宽度").clicked() {
            state.zoom = (ui.max_rect().width() / layout.size.x).clamp(0.35, 1.2);
            locate = true;
        }
        if ui.button("定位选中").clicked() {
            locate = true;
        }
        ui.weak(format!("{} 步 · {} 连接", nodes.len(), layout.links.len()));
    });
    if layout.links.iter().any(|link| link.to.is_none()) {
        let missing: BTreeSet<_> = layout
            .links
            .iter()
            .filter(|link| link.to.is_none())
            .map(|link| link.target.as_str())
            .collect();
        ui.colored_label(
            Color32::DARK_RED,
            format!(
                "缺失目标：{}",
                missing.into_iter().collect::<Vec<_>>().join("、")
            ),
        );
    }
    ui.separator();
    let mut choose = None;
    egui::ScrollArea::both()
        .id_salt(("flow-canvas", text(story, "id")))
        .auto_shrink([false, false])
        .animated(false)
        .show(ui, |ui| {
            let zoom = state.zoom;
            let (canvas, _) = ui.allocate_exact_size(layout.size * zoom, egui::Sense::hover());
            let transform = |p: Pos2| canvas.min + p.to_vec2() * zoom;
            let rect = |index: usize| {
                Rect::from_min_max(
                    transform(layout.cards[index].min),
                    transform(layout.cards[index].max),
                )
            };
            let painter = ui.painter().clone();
            let mut labels = Vec::new();
            for link in &layout.links {
                let active = link.from == selected || link.to == Some(selected);
                let color = if link.to.is_none() {
                    Color32::DARK_RED
                } else if active {
                    crate::shell::ACCENT
                } else if link.backward {
                    Color32::from_rgb(172, 119, 41)
                } else {
                    Color32::from_rgb(116, 130, 143)
                };
                let stroke = Stroke::new(if active { 2.0_f32 } else { 1.3_f32 }, color);
                for segment in link.points.windows(2) {
                    let (a, b) = (transform(segment[0]), transform(segment[1]));
                    painter.line_segment([a, b], stroke);
                }
                if let Some(segment) = link
                    .points
                    .windows(2)
                    .rfind(|segment| segment[0] != segment[1])
                {
                    let (a, b) = (transform(segment[0]), transform(segment[1]));
                    let tip = (b - a).normalized() * (10.0 * zoom).min(a.distance(b));
                    painter.arrow(b - tip, tip, stroke);
                }
                let plain_forward = link.label == "跳转"
                    && !link.backward
                    && link.to.is_some()
                    && layout
                        .links
                        .iter()
                        .filter(|edge| edge.from == link.from)
                        .count()
                        == 1;
                if (link.label != "下一步" && !plain_forward) || link.backward || link.to.is_none()
                {
                    let label = if link.to.is_none() {
                        format!("缺失：{}", link.target)
                    } else {
                        link.label.clone()
                    };
                    labels.push((link, label, color));
                }
            }
            for (index, node) in nodes.iter().enumerate() {
                let card = rect(index);
                let response = ui.interact(
                    card,
                    ui.id().with(("flow-node", index)),
                    egui::Sense::click(),
                );
                let reached = layout.reachable.contains(&index);
                let fill = if index == selected {
                    Color32::from_rgb(224, 239, 250)
                } else if response.hovered() {
                    Color32::from_rgb(238, 244, 247)
                } else {
                    Color32::from_rgb(252, 252, 251)
                };
                painter.rect_filled(card, 6.0, fill);
                painter.rect_stroke(
                    card,
                    6.0,
                    Stroke::new(
                        if index == selected { 2.0_f32 } else { 1.0_f32 },
                        if index == selected {
                            crate::shell::ACCENT
                        } else {
                            Color32::from_gray(177)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                let key = format!("node.{}", text(node, "type"));
                let translated = crate::i18n::key(&key);
                let kind = if translated == key {
                    let kind = text(node, "type");
                    if lom_core::validate::node_schema()["nodes"]
                        .get(kind)
                        .is_some()
                    {
                        kind.to_owned()
                    } else {
                        format!("{kind} · 未支持")
                    }
                } else {
                    translated
                };
                let title = format!(
                    "{:02} {}{}",
                    index + 1,
                    kind,
                    if reached { "" } else { " · 不可达" }
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("{title} · {}", text(node, "id")),
                    )
                });
                let color = if reached {
                    Color32::from_rgb(44, 56, 66)
                } else {
                    Color32::from_rgb(100, 107, 114)
                };
                let padding = 10.0 * zoom;
                let available = card.width() - padding * 2.0;
                paint_text(
                    &painter,
                    card.min + Vec2::splat(padding),
                    title,
                    (13.0 * zoom).max(8.0),
                    available,
                    color,
                );
                paint_text(
                    &painter,
                    card.min + Vec2::new(padding, 30.0 * zoom),
                    summary(project, node),
                    (12.0 * zoom).max(8.0),
                    available,
                    color,
                );
                paint_text(
                    &painter,
                    card.min + Vec2::new(padding, 51.0 * zoom),
                    text(node, "id").into(),
                    (10.0 * zoom).max(7.0),
                    available,
                    Color32::GRAY,
                );
                if response
                    .on_hover_text(format!(
                        "{}\n{}\n{}",
                        text(node, "id"),
                        text(node, "type"),
                        summary(project, node)
                    ))
                    .clicked()
                {
                    choose = Some(index);
                }
            }
            // All lines are already drawn. Label backgrounds use the actual
            // galley size so later edges cannot cut through text at any zoom.
            for (index, (link, label, color)) in labels.into_iter().enumerate() {
                let mut job = egui::text::LayoutJob::simple(
                    label.clone(),
                    FontId::proportional((11.0 * zoom).max(8.0)),
                    color,
                    link.label_width * zoom,
                );
                job.wrap.max_rows = 1;
                job.wrap.overflow_character = Some('…');
                let galley = painter.layout_job(job);
                let at = transform(link.label_at) - Vec2::new(0.0, galley.size().y);
                let bounds = Rect::from_min_size(at, galley.size()).expand(2.0);
                painter.rect_filled(bounds, 2.0, Color32::from_rgb(249, 249, 247));
                painter.galley(at, galley, color);
                ui.interact(
                    bounds,
                    ui.id().with(("flow-label", index)),
                    egui::Sense::hover(),
                )
                .on_hover_text(label);
            }
            if locate && selected < layout.cards.len() {
                ui.scroll_to_rect(rect(selected), Some(egui::Align::Center));
            }
        });
    state.selected = selected_id;
    ui.data_mut(|data| data.insert_temp(state_id, state));
    choose
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn linear_flow_has_compact_aligned_cards_and_direct_edges() {
        let graph = layout(
            &json!({"start":"a", "nodes":[{"id":"a", "type":"say"},{"id":"b", "type":"say"},{"id":"c", "type":"end"}]}),
        );
        assert_eq!(graph.ranks, vec![0, 1, 2]);
        assert!(graph
            .cards
            .iter()
            .all(|card| card.size() == CARD && card.center().x == graph.cards[0].center().x));
        assert!(graph
            .links
            .iter()
            .all(|link| link.points.iter().all(|point| point.x == link.points[0].x)));
        assert_eq!(graph.links.len(), 2);
    }

    #[test]
    fn diamond_spreads_branches_then_merges_with_both_incoming_edges() {
        let graph = layout(
            &json!({"start":"a", "nodes":[{"id":"a", "type":"choice", "options":[{"text":"甲", "goto":"b"},{"text":"乙", "goto":"c"}]},{"id":"b", "type":"say", "goto":"d"},{"id":"c", "type":"say", "goto":"d"},{"id":"d", "type":"end"}]}),
        );
        assert_eq!(graph.ranks, vec![0, 1, 1, 2]);
        assert_ne!(graph.cards[1].center().x, graph.cards[2].center().x);
        assert_eq!(graph.cards[0].center().x, graph.cards[3].center().x);
        assert_eq!(
            graph.links.iter().filter(|link| link.to == Some(3)).count(),
            2
        );
        assert_eq!(
            graph
                .links
                .iter()
                .filter(|link| link.from == 0)
                .map(|link| link.label.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["甲", "乙"])
        );
    }

    #[test]
    fn cycles_keep_all_edges_and_use_an_outer_return_lane() {
        let graph = layout(
            &json!({"start":"a", "nodes":[{"id":"a", "type":"say", "goto":"b"},{"id":"b", "type":"choice", "options":[{"text":"重来", "goto":"a"},{"text":"继续", "goto":"c"}]},{"id":"c", "type":"end"}]}),
        );
        assert_eq!(graph.links.len(), 3);
        let back = graph.links.iter().find(|link| link.backward).unwrap();
        assert_eq!((back.from, back.to), (1, Some(0)));
        assert!(back.points.iter().any(|point| point.x
            < graph
                .cards
                .iter()
                .map(Rect::left)
                .fold(f32::INFINITY, f32::min)));
        assert!(graph.cards[2].top() > graph.cards[1].bottom());
        let onward = graph.links.iter().find(|link| link.to == Some(2)).unwrap();
        assert!(onward
            .points
            .iter()
            .all(|point| point.x == onward.points[0].x));
    }

    #[test]
    fn loop_preserves_its_inner_diamond_and_conditional_exit() {
        // The Windows UI fixture: both choice paths merge before the conditional
        // loop, so they are in one cycle but must still appear side by side.
        let story = json!({"start":"start_message", "nodes":[
            {"id":"start_message","type":"message","goto":"choose_route"},
            {"id":"choose_route","type":"choice","options":[
                {"text":"走左边：查看青石小径","goto":"say_left"},
                {"text":"走右边：查看竹林岔道","goto":"say_right"}]},
            {"id":"say_left","type":"say","goto":"merge_message"},
            {"id":"say_right","type":"say","goto":"merge_message"},
            {"id":"merge_message","type":"message","goto":"branch_loop_seen"},
            {"id":"branch_loop_seen","type":"branch","source":"mod","flag":"GRAPH_UI_LOOP_SEEN","cases":[
                {"value":1,"goto":"end_to_second"},{"value":2,"goto":"loop_message"}]},
            {"id":"loop_message","type":"message","goto":"mark_loop_seen"},
            {"id":"mark_loop_seen","type":"flag","goto":"returntochoice"},
            {"id":"returntochoice","type":"message","goto":"choose_route"},
            {"id":"end_to_second","type":"end","next_script":"second"}
        ]});
        let graph = layout(&story);
        assert_eq!(graph.ranks, vec![0, 1, 2, 2, 3, 4, 5, 6, 7, 5]);
        assert_eq!(graph.cards[2].top(), graph.cards[3].top());
        assert!(graph.cards[2].right() < graph.cards[3].left());
        let first = graph.links.iter().find(|link| link.from == 0).unwrap();
        assert!(first
            .points
            .iter()
            .all(|point| point.x == first.points[0].x));
        assert_eq!(graph.links.len(), 11);
        let back: Vec<_> = graph.links.iter().filter(|link| link.backward).collect();
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].from, back[0].to), (8, Some(1)));
        assert_eq!(back[0].label, "跳转");
        assert_eq!(
            graph.links.iter().filter(|link| link.to == Some(4)).count(),
            2
        );
        for (target, label) in [(9, "条件成立（1）"), (6, "条件不成立（2）")] {
            assert_eq!(
                graph
                    .links
                    .iter()
                    .find(|link| link.from == 5 && link.to == Some(target))
                    .unwrap()
                    .label,
                label
            );
        }
        assert!(graph.links.iter().all(|link| link.to.is_some()));
    }

    #[test]
    fn missing_local_targets_are_errors_but_next_chapter_is_not() {
        let graph = layout(
            &json!({"start":"a", "nodes":[{"id":"a", "type":"choice", "options":[{"text":"错误", "goto":"missing"},{"text":"结束", "goto":"end"}]},{"id":"end", "type":"end", "next_script":"chapter2"}]}),
        );
        assert_eq!(
            graph
                .links
                .iter()
                .filter(|link| link.to.is_none())
                .map(|link| link.target.as_str())
                .collect::<Vec<_>>(),
            vec!["missing"]
        );
        assert_eq!(graph.links.len(), 2);
        assert_eq!(
            summary(
                &Project::new(),
                &json!({"type":"end", "next_script":"chapter2"})
            ),
            "→ 章节 chapter2"
        );
    }

    #[test]
    fn parallel_outcomes_keep_every_label_on_the_shared_edge() {
        let graph = layout(
            &json!({"start":"a", "nodes":[{"id":"a", "type":"combat", "win":"end", "lose":"end"},{"id":"end", "type":"end"}]}),
        );
        assert_eq!(graph.links.len(), 1);
        assert_eq!(graph.links[0].label, "胜利 / 失败");
    }

    #[test]
    fn long_graph_locates_selected_node_and_keeps_controls_visible() {
        let nodes: Vec<_> = (0..57).map(|index| json!({"id":format!("n{index}"), "type":if index == 56 {"end"} else {"say"}, "text":"台词"})).collect();
        let story = json!({"id":"main", "start":"n0", "nodes":nodes});
        let project = Project::new();
        let context = egui::Context::default();
        context.enable_accesskit();
        let frame = |events| {
            let mut chosen = None;
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(560.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        chosen = show(ui, &project, &story, 56);
                    });
                },
            );
            (output, chosen)
        };
        let _ = frame(vec![]);
        let (output, _) = frame(vec![]);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let (selected_id, selected) = nodes
            .iter()
            .find(|(_, node)| node.label().is_some_and(|label| label.ends_with(" · n56")))
            .unwrap();
        let bounds = selected.bounds().unwrap();
        assert!(
            bounds.y0 >= 40.0 && bounds.y1 <= 600.0,
            "selected node remains outside viewport: {bounds:?}"
        );
        let controls = nodes
            .iter()
            .find(|(_, node)| node.label() == Some("定位选中"))
            .unwrap()
            .1
            .bounds()
            .unwrap();
        assert!(controls.y1 < 60.0, "graph controls scrolled away");
        let (_, chosen) = frame(vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target: *selected_id,
                data: None,
            },
        )]);
        assert_eq!(chosen, Some(56));
    }
}
