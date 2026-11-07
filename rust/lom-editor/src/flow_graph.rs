//! Layout and drawing for the editor's actual intra-chapter control-flow edges.
use super::{character_display_name, edges, text};
use crate::i18n::{key, message};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use lom_core::project::Project;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const CARD: Vec2 = Vec2::new(192.0, 72.0);
const COLUMN: f32 = 236.0;
const ROW: f32 = 126.0;

struct LayeredPositions {
    x: Vec<f32>,
    width: f32,
    channels: BTreeMap<(usize, usize), Vec<Pos2>>,
}

/// Sugiyama layering, as used by ELK: split long edges into virtual vertices,
/// sweep both ways to reduce crossings, then compact while preserving order.
/// Virtual vertices reserve narrow corridors through intermediate node rows.
/// Reference: https://eclipse.dev/elk/reference/algorithms/org-eclipse-elk-layered.html
fn layered_positions(ranks: &[usize], forward: &[Vec<usize>]) -> LayeredPositions {
    let count = ranks.len();
    let mut rows = vec![Vec::new(); ranks.iter().max().copied().unwrap_or(0) + 1];
    for (node, &rank) in ranks.iter().enumerate() {
        rows[rank].push(node);
    }
    let mut incoming = vec![Vec::new(); count];
    let mut outgoing = vec![Vec::new(); count];
    let mut channels = BTreeMap::new();
    for (from, targets) in forward.iter().enumerate() {
        for &to in targets {
            if ranks[to] <= ranks[from] || channels.contains_key(&(from, to)) {
                continue;
            }
            let mut previous = from;
            let mut via = Vec::new();
            for row in rows.iter_mut().take(ranks[to]).skip(ranks[from] + 1) {
                let dummy = incoming.len();
                incoming.push(vec![previous]);
                outgoing.push(Vec::new());
                outgoing[previous].push(dummy);
                row.push(dummy);
                via.push(dummy);
                previous = dummy;
            }
            outgoing[previous].push(to);
            incoming[to].push(previous);
            channels.insert((from, to), via);
        }
    }
    let mut order = vec![0.0; incoming.len()];
    let update_order = |rows: &[Vec<usize>], order: &mut [f32]| {
        for row in rows {
            for (position, &node) in row.iter().enumerate() {
                order[node] = position as f32;
            }
        }
    };
    let crossings = |rows: &[Vec<usize>], order: &[f32]| -> usize {
        rows.iter()
            .map(|row| {
                let edges: Vec<_> = row
                    .iter()
                    .flat_map(|&from| {
                        outgoing[from]
                            .iter()
                            .map(move |&to| (order[from], order[to]))
                    })
                    .collect();
                edges
                    .iter()
                    .enumerate()
                    .map(|(i, &(a, b))| {
                        edges[i + 1..]
                            .iter()
                            .filter(|&&(c, d)| (a - c) * (b - d) < 0.0)
                            .count()
                    })
                    .sum::<usize>()
            })
            .sum()
    };
    update_order(&rows, &mut order);
    let mut best = rows.clone();
    let mut best_crossings = crossings(&rows, &order);
    for _ in 0..4 {
        for down in [true, false] {
            let neighbours = if down { &incoming } else { &outgoing };
            let indices: Vec<_> = if down {
                (0..rows.len()).collect()
            } else {
                (0..rows.len()).rev().collect()
            };
            for row in indices {
                let barycenter = |node: usize| {
                    let adjacent = &neighbours[node];
                    if adjacent.is_empty() {
                        order[node]
                    } else {
                        adjacent.iter().map(|&other| order[other]).sum::<f32>()
                            / adjacent.len() as f32
                    }
                };
                rows[row].sort_by(|&a, &b| barycenter(a).total_cmp(&barycenter(b)));
                for (position, &node) in rows[row].iter().enumerate() {
                    order[node] = position as f32;
                }
            }
            let score = crossings(&rows, &order);
            if score < best_crossings {
                best_crossings = score;
                best = rows.clone();
            }
        }
    }
    rows = best;
    let node_width = |node| if node < count { CARD.x } else { 12.0 };
    let row_width = |row: &[usize]| {
        row.iter().map(|&n| node_width(n)).sum::<f32>()
            + row.len().saturating_sub(1) as f32 * (COLUMN - CARD.x)
    };
    let width = rows.iter().map(|row| row_width(row)).fold(CARD.x, f32::max);
    let mut x = vec![0.0; incoming.len()];
    for row in &rows {
        let mut cursor = (width - row_width(row)) / 2.0;
        for &node in row {
            x[node] = cursor + node_width(node) / 2.0;
            cursor += node_width(node) + COLUMN - CARD.x;
        }
    }
    // Least-squares compaction (isotonic blocks) aligns chains and centres forks
    // and joins without pushing neighbouring cards into each other.
    for _ in 0..4 {
        for down in [true, false] {
            let neighbours = if down { &incoming } else { &outgoing };
            let indices: Vec<_> = if down {
                (0..rows.len()).collect()
            } else {
                (0..rows.len()).rev().collect()
            };
            for rank in indices {
                let row = &rows[rank];
                let mut offsets = Vec::new();
                let mut offset = 0.0;
                let mut blocks: Vec<(usize, usize, f32)> = Vec::new();
                for (i, &node) in row.iter().enumerate() {
                    offset += node_width(node) / 2.0;
                    offsets.push(offset);
                    let adjacent = &neighbours[node];
                    let desired = if adjacent.is_empty() {
                        x[node]
                    } else {
                        adjacent.iter().map(|&other| x[other]).sum::<f32>() / adjacent.len() as f32
                    };
                    blocks.push((i, 1, desired - offset));
                    while blocks.len() > 1 {
                        let n = blocks.len();
                        let (a, ac, av) = blocks[n - 2];
                        let (_, bc, bv) = blocks[n - 1];
                        if av <= bv {
                            break;
                        }
                        blocks.truncate(n - 2);
                        blocks.push((
                            a,
                            ac + bc,
                            (av * ac as f32 + bv * bc as f32) / (ac + bc) as f32,
                        ));
                    }
                    offset += node_width(node) / 2.0 + COLUMN - CARD.x;
                }
                let slack = (width - row_width(row)).max(0.0);
                for (start, length, value) in blocks {
                    for i in start..start + length {
                        x[row[i]] = offsets[i] + value.clamp(0.0, slack);
                    }
                }
            }
        }
    }
    let channels = channels
        .into_iter()
        .map(|(edge, via)| {
            let from_rank = ranks[edge.0];
            (
                edge,
                via.into_iter()
                    .enumerate()
                    .map(|(i, node)| {
                        Pos2::new(
                            x[node],
                            32.0 + (from_rank + i + 1) as f32 * ROW + CARD.y / 2.0,
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    x.truncate(count);
    LayeredPositions { x, width, channels }
}

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
    outgoing: Vec<usize>,
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
    layout_graph(nodes, text(story, "start"), edges(story))
}

fn layout_graph(nodes: &[Value], start: &str, raw: Vec<(String, String, String)>) -> Layout {
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (text(node, "id"), index))
        .collect();
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
    if let Some(&start) = indices.get(start) {
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
    let positions = layered_positions(&ranks, &forward);
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
    let cards: Vec<_> = positions
        .x
        .iter()
        .enumerate()
        .map(|(index, &x)| {
            Rect::from_min_size(
                Pos2::new(left + x - CARD.x / 2.0, 32.0 + ranks[index] as f32 * ROW),
                CARD,
            )
        })
        .collect();
    let right = left + positions.width;
    let mut links = Vec::new();
    let mut outgoing = vec![0; nodes.len()];
    let mut back_lane = 0;
    for (from, target, label) in raw {
        let Some(&a) = indices.get(from.as_str()) else {
            continue;
        };
        outgoing[a] += 1;
        let to = indices.get(target.as_str()).copied();
        let mut points = Vec::new();
        let backward = to.is_some_and(|b| ranks[b] <= ranks[a]);
        let label_at;
        let label_width;
        if let Some(b) = to {
            if backward {
                let lane = if a == b {
                    cards[a].left() - 18.0
                } else {
                    left - 30.0 - back_lane as f32 * 16.0
                };
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
                    let source_corridor = cards[a].top() - 18.0;
                    let target_corridor = cards[b].top() - 18.0;
                    points.extend([
                        cards[a].left_center(),
                        Pos2::new(cards[a].left() - 12.0, cards[a].center().y),
                        Pos2::new(cards[a].left() - 12.0, source_corridor),
                        Pos2::new(lane, source_corridor),
                        Pos2::new(lane, target_corridor),
                        Pos2::new(cards[b].left() - 12.0, target_corridor),
                        Pos2::new(cards[b].left() - 12.0, cards[b].center().y),
                        cards[b].left_center(),
                    ]);
                }
                label_width = 112.0;
                label_at = Pos2::new(lane - label_width - 5.0, cards[a].top() - 22.0);
            } else {
                let mut exits: Vec<_> = forward[a]
                    .iter()
                    .copied()
                    .filter(|&child| ranks[child] > ranks[a])
                    .collect();
                let mut entries: Vec<_> = reverse[b]
                    .iter()
                    .copied()
                    .filter(|&parent| ranks[parent] < ranks[b])
                    .collect();
                let exit_x = |child| {
                    positions.channels[&(a, child)]
                        .first()
                        .map_or(cards[child].center().x, |point| left + point.x)
                };
                let entry_x = |parent| {
                    positions.channels[&(parent, b)]
                        .last()
                        .map_or(cards[parent].center().x, |point| left + point.x)
                };
                exits.sort_by(|&a, &b| exit_x(a).total_cmp(&exit_x(b)));
                entries.sort_by(|&a, &b| entry_x(a).total_cmp(&entry_x(b)));
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
                    // Long edges follow reserved virtual-node corridors instead
                    // of detouring around the full width of the chapter.
                    let via = &positions.channels[&(a, b)];
                    let mut previous = start;
                    points.push(start);
                    for next in via
                        .iter()
                        .map(|point| *point + Vec2::new(left, 0.0))
                        .chain([end])
                    {
                        let middle = if next == end {
                            end.y - (ROW - CARD.y) / 2.0
                        } else {
                            next.y - CARD.y / 2.0 - (ROW - CARD.y) / 2.0
                        };
                        points.extend([
                            Pos2::new(previous.x, middle),
                            Pos2::new(next.x, middle),
                            next,
                        ]);
                        previous = next;
                    }
                    label_width = 156.0;
                    label_at = Pos2::new(start.x + 6.0, start.y + 16.0);
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
        .get(start)
        .copied()
        .or_else(|| (!nodes.is_empty()).then_some(0));
    let mut pending: VecDeque<_> = start.into_iter().collect();
    while let Some(index) = pending.pop_front() {
        if reachable.insert(index) {
            pending.extend(forward[index].iter().copied());
        }
    }
    let extra = if links.iter().any(|link| link.to.is_none()) {
        190.0
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
        outgoing,
    }
}

#[derive(Clone)]
struct ViewState {
    zoom: f32,
    full: bool,
    selected: Option<String>,
    layout: Option<std::sync::Arc<CachedLayout>>,
}
struct CachedLayout {
    story: Value,
    selected: Option<String>,
    full: bool,
    members: Vec<Vec<usize>>,
    layout: Layout,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            full: false,
            selected: None,
            layout: None,
        }
    }
}

fn native_decision(node: &Value) -> bool {
    node["operations"].as_array().is_some_and(|operations| {
        operations.iter().any(|operation| {
            let expression = if operation["kind"] == "assign" {
                &operation["value"]
            } else {
                &operation["expression"]
            };
            let function = &expression["function"];
            expression["kind"] == "call"
                && (function["kind"] == "name" && function["name"] == "choose"
                    || function["kind"] == "member"
                        && function["object"]["name"] == "checkpointmanager"
                        && matches!(text(function, "field"), "Switch" | "Condition" | "Dice"))
        })
    })
}

fn display_layout(story: &Value, selected: usize, full: bool) -> CachedLayout {
    let nodes = story["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let selected_id = nodes.get(selected).map(|node| text(node, "id").to_owned());
    if full {
        return CachedLayout {
            story: story.clone(),
            selected: selected_id,
            full,
            members: (0..nodes.len()).map(|index| vec![index]).collect(),
            layout: layout(story),
        };
    }
    let raw = edges(story);
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (text(node, "id"), index))
        .collect();
    let mut forward = vec![Vec::new(); nodes.len()];
    let mut reverse = vec![Vec::new(); nodes.len()];
    let mut missing = vec![false; nodes.len()];
    for (from, target, _) in &raw {
        if let Some(&from) = indices.get(from.as_str()) {
            if let Some(&target) = indices.get(target.as_str()) {
                forward[from].push(target);
                reverse[target].push(from);
            } else {
                missing[from] = true;
            }
        }
    }
    let checks = &lom_core::validate::node_schema()["check_types"];
    let mut boundary: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let kind = text(node, "type");
            index == selected
                || native_decision(node)
                || text(node, "id") == text(story, "start")
                || missing[index]
                || reverse[index].len() > 1
                || forward[index].len() != 1
                || matches!(
                    kind,
                    "choice"
                        | "branch"
                        | "native_branch"
                        | "dice"
                        | "end"
                        | "goto_scene"
                        | "death"
                        | "combat"
                        | "battle"
                        | "battle_result"
                        | "raw"
                )
                || checks
                    .as_array()
                    .is_some_and(|types| types.iter().any(|value| value == kind))
                || node["operations"].as_array().is_some_and(|operations| {
                    operations
                        .iter()
                        .any(|operation| operation["kind"] == "return")
                })
        })
        .collect();
    let mut colors = vec![0; nodes.len()];
    let mut back_edges = BTreeSet::new();
    for index in 0..nodes.len() {
        if colors[index] == 0 {
            find_back_edges(index, &forward, &mut colors, &mut back_edges);
        }
    }
    for (from, target) in back_edges {
        boundary[from] = true;
        boundary[target] = true;
    }
    let mut owners = vec![usize::MAX; nodes.len()];
    let mut members = Vec::new();
    for index in 0..nodes.len() {
        if owners[index] != usize::MAX {
            continue;
        }
        let mut first = index;
        if !boundary[first] {
            while reverse[first].len() == 1 {
                let previous = reverse[first][0];
                if boundary[previous] || owners[previous] != usize::MAX {
                    break;
                }
                first = previous;
            }
        }
        let group = members.len();
        let mut chain = Vec::new();
        let mut current = first;
        loop {
            owners[current] = group;
            chain.push(current);
            if boundary[current] || forward[current].len() != 1 {
                break;
            }
            let next = forward[current][0];
            if boundary[next] || owners[next] != usize::MAX {
                break;
            }
            current = next;
        }
        members.push(chain);
    }
    let representatives: Vec<_> = members
        .iter()
        .map(|members| nodes[members[0]].clone())
        .collect();
    let mut projected: BTreeMap<(usize, String), Vec<String>> = BTreeMap::new();
    for (from, target, label) in raw {
        let original_from = indices[from.as_str()];
        let from = owners[original_from];
        let target = if let Some(&target) = indices.get(target.as_str()) {
            let group = owners[target];
            if from == group && original_from != target {
                continue;
            }
            text(&representatives[group], "id").to_owned()
        } else {
            target
        };
        let labels = projected.entry((from, target)).or_default();
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let links = projected
        .into_iter()
        .map(|((from, target), labels)| {
            (
                text(&representatives[from], "id").to_owned(),
                target,
                labels.join(" / "),
            )
        })
        .collect();
    let start = indices
        .get(text(story, "start"))
        .map(|&index| text(&representatives[owners[index]], "id"))
        .unwrap_or("");
    let layout = layout_graph(&representatives, start, links);
    CachedLayout {
        story: story.clone(),
        selected: selected_id,
        full,
        members,
        layout,
    }
}

fn summary(project: &Project, node: &Value) -> String {
    if matches!(text(node, "type"), "native_action" | "native_branch") {
        return node["_editor"]["note"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| crate::native_forms::summary(node));
    }
    if text(node, "type") == "end" && !text(node, "next_script").is_empty() {
        return message(
            "flow_graph.next_chapter",
            &[("chapter", text(node, "next_script").into())],
        );
    }
    if text(node, "type") == "raw" {
        return key("flow_graph.raw_unknown");
    }
    if !text(node, "text").is_empty() {
        return text(node, "text").replace(['\n', '\r'], " ");
    }
    if !text(node, "character").is_empty() {
        return character_display_name(project, text(node, "character"));
    }
    let resource = match text(node, "type") {
        "music" => Some(("music", "name")),
        "sound" => Some((
            if text(node, "kind") == "env" {
                "env_sounds"
            } else {
                "sounds"
            },
            "name",
        )),
        "scene" => Some(("views", "view")),
        _ => None,
    };
    if let Some((category, key)) = resource {
        let id = text(node, key);
        let name = lom_core::validate::editor_data()[category]
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| entry["id"] == id)
            .and_then(|entry| entry["name"].as_str())
            .unwrap_or(id);
        return crate::i18n::term(category, id, name);
    }
    for key in ["image", "view", "name", "flag", "key", "scene"] {
        if !text(node, key).is_empty() {
            return text(node, key).into();
        }
    }
    for key in ["options", "cases", "bands"] {
        if let Some(rows) = node[key].as_array() {
            return message(
                "flow_graph.branch_count",
                &[("count", rows.len().to_string())],
            );
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

fn card_summary(project: &Project, nodes: &[Value], members: &[usize]) -> String {
    members
        .iter()
        .map(|&index| &nodes[index])
        .find(|node| !text(node, "text").is_empty())
        .map(|node| summary(project, node))
        .unwrap_or_else(|| summary(project, &nodes[members[0]]))
}

fn short_edge_label(label: &str, backward: bool, full: bool) -> String {
    if backward && !full {
        return key("flow_graph.loop");
    }
    let mut short: String = label.chars().take(if full { 18 } else { 10 }).collect();
    if short.len() < label.len() {
        short.push('…');
    }
    short
}

fn edge_label(label: &str, source: &Value, compact: bool) -> String {
    // Match complete source labels before translating generated prefixes or suffixes.
    // Authored text can itself contain the shared-edge separator or outcome words.
    if matches!(text(source, "type"), "choice" | "dice") {
        let mut candidates = Vec::new();
        for (field, raw, name) in [
            ("goto", "跳转", "flow_graph.jump"),
            ("win", "胜利", "flow_graph.win"),
            ("lose", "失败", "flow_graph.failure"),
            ("success", "成功", "flow_graph.success"),
            ("failure", "失败", "flow_graph.failure"),
        ] {
            if source[field].as_str().is_some() {
                candidates.push((raw.to_owned(), key(name)));
            }
        }
        for field in ["options", "bands"] {
            for (index, row) in source[field].as_array().into_iter().flatten().enumerate() {
                let authored = row["text"].as_str().filter(|text| !text.is_empty());
                let raw = authored.map(str::to_owned).unwrap_or_else(|| {
                    format!(
                        "{} {}",
                        if field == "bands" { "区间" } else { "选项" },
                        index + 1
                    )
                });
                let translated = authored.map(str::to_owned).unwrap_or_else(|| {
                    message(
                        if field == "bands" {
                            "editor.flow.band"
                        } else {
                            "editor.flow.option"
                        },
                        &[("number", (index + 1).to_string())],
                    )
                });
                for (jump, outcome) in [
                    ("goto", None),
                    ("goto_大成功", Some("editor.flow.critical_success")),
                    ("goto_成功", Some("flow_graph.success")),
                    ("goto_失败", Some("flow_graph.failure")),
                ] {
                    if row[jump].as_str().is_none() {
                        continue;
                    }
                    candidates.push(if let Some(name) = outcome {
                        (
                            format!("{raw} · {}", jump.trim_start_matches("goto_")),
                            format!("{translated} · {}", key(name)),
                        )
                    } else {
                        (raw.clone(), translated.clone())
                    });
                }
            }
        }
        candidates.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
        let mut remaining = label;
        let mut translated = Vec::new();
        while !remaining.is_empty() {
            let Some((raw, display)) = candidates.iter().find(|(raw, _)| {
                remaining == raw
                    || remaining
                        .strip_prefix(raw.as_str())
                        .is_some_and(|tail| tail.starts_with(" / "))
            }) else {
                return label.into();
            };
            // An identical label with different provenance cannot be safely localized.
            translated.push(
                if candidates
                    .iter()
                    .any(|(other, value)| other == raw && value != display)
                {
                    raw.clone()
                } else {
                    display.clone()
                },
            );
            remaining = remaining
                .strip_prefix(raw.as_str())
                .unwrap()
                .strip_prefix(" / ")
                .unwrap_or("");
        }
        return translated.join(" / ");
    }
    label
        .split(" / ")
        .map(|part| {
            let name = match part {
                "下一步" => "flow_graph.next",
                "跳转" => "flow_graph.jump",
                "胜利" => "flow_graph.win",
                "失败" => "flow_graph.failure",
                "成功" => "flow_graph.success",
                "条件成立" => {
                    if compact {
                        "flow_graph.true_short"
                    } else {
                        "flow_graph.condition_true"
                    }
                }
                "条件不成立" => {
                    if compact {
                        "flow_graph.false_short"
                    } else {
                        "flow_graph.condition_false"
                    }
                }
                _ => "",
            };
            if !name.is_empty() {
                return key(name);
            }
            for (prefix, key_name) in [
                ("条件成立（", "flow_graph.condition_true"),
                ("条件不成立（", "flow_graph.condition_false"),
            ] {
                if let Some(value) = part
                    .strip_prefix(prefix)
                    .and_then(|value| value.strip_suffix('）'))
                {
                    return message(
                        "flow_graph.condition_result",
                        &[("condition", key(key_name)), ("value", value.into())],
                    );
                }
            }
            if let Some(comparison) = part.strip_prefix("条件 ") {
                return message(
                    "flow_graph.condition_compare",
                    &[("comparison", comparison.into())],
                );
            }
            part.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

fn label_position(
    preferred: Pos2,
    size: Vec2,
    zoom: f32,
    cards: &[Rect],
    placed: &[Rect],
) -> Option<(Pos2, Rect)> {
    [0.0, -18.0, 18.0, -36.0, 36.0]
        .into_iter()
        .find_map(|offset| {
            let at = preferred + Vec2::new(0.0, offset * zoom);
            let bounds = Rect::from_min_size(at, size).expand(2.0);
            (!placed.iter().any(|prior| prior.intersects(bounds))
                && !cards.iter().any(|card| card.intersects(bounds)))
            .then_some((at, bounds))
        })
}

pub(super) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    story: &Value,
    selected: usize,
) -> Option<usize> {
    let nodes = story["nodes"].as_array()?;
    let state_id = ui.id().with(("flow-view", text(story, "id")));
    let mut state = ui
        .data_mut(|data| data.get_temp::<ViewState>(state_id))
        .unwrap_or_default();
    let selected_id = nodes.get(selected).map(|node| text(node, "id").to_owned());
    let mut locate = selected_id != state.selected;
    let mut fit = state.layout.is_none();
    ui.horizontal_wrapped(|ui| {
        if ui.button("−").on_hover_text(key("flow.zoom_out")).clicked() {
            state.zoom = (state.zoom - 0.1).max(0.35);
            locate = true;
        }
        ui.label(format!("{:.0}%", state.zoom * 100.0));
        if ui.button("＋").on_hover_text(key("flow.zoom_in")).clicked() {
            state.zoom = (state.zoom + 0.1).min(1.75);
            locate = true;
        }
        if ui.button(key("flow_graph.fit_width")).clicked() {
            fit = true;
            locate = true;
        }
        if ui.button(key("flow_graph.locate")).clicked() {
            locate = true;
        }
        if ui
            .checkbox(&mut state.full, key("flow_graph.full"))
            .on_hover_text(key("flow_graph.mode_tip"))
            .changed()
        {
            fit = true;
            locate = true;
        }
    });
    if state.layout.as_ref().is_none_or(|cached| {
        cached.story != *story
            || cached.full != state.full
            || !state.full && cached.selected != selected_id
    }) {
        state.layout = Some(std::sync::Arc::new(display_layout(
            story, selected, state.full,
        )));
    }
    let cached = state.layout.clone().unwrap();
    let layout = &cached.layout;
    let selected_card = cached
        .members
        .iter()
        .position(|members| members.contains(&selected));
    if fit {
        state.zoom = (ui.max_rect().width() / layout.size.x).clamp(0.35, 1.2);
    }
    ui.weak(message(
        "flow_graph.stats",
        &[
            (
                "mode",
                key(if state.full {
                    "flow_graph.full"
                } else {
                    "flow_graph.overview"
                }),
            ),
            ("steps", nodes.len().to_string()),
            ("cards", cached.members.len().to_string()),
            ("links", layout.links.len().to_string()),
        ],
    ));
    if layout.links.iter().any(|link| link.to.is_none()) {
        let missing: BTreeSet<_> = layout
            .links
            .iter()
            .filter(|link| link.to.is_none())
            .map(|link| link.target.as_str())
            .collect();
        ui.colored_label(
            Color32::DARK_RED,
            message(
                "flow_graph.missing_targets",
                &[(
                    "targets",
                    missing.into_iter().collect::<Vec<_>>().join(", "),
                )],
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
            let clip = ui.clip_rect().expand(20.0);
            let mut labels = Vec::new();
            let mut links: Vec<_> = layout.links.iter().collect();
            links.sort_by_key(|link| {
                Some(link.from) == selected_card
                    || link.to == selected_card && selected_card.is_some()
            });
            for link in links {
                let source = &nodes[cached.members[link.from][0]];
                let detail = edge_label(&link.label, source, false);
                let bounds = Rect::from_points(&link.points);
                let bounds = Rect::from_min_max(transform(bounds.min), transform(bounds.max));
                let label_bounds = Rect::from_min_size(
                    transform(link.label_at) - Vec2::new(0.0, 20.0 * zoom),
                    Vec2::new(link.label_width, 24.0) * zoom,
                );
                if !bounds.union(label_bounds).intersects(clip) {
                    continue;
                }
                let active = Some(link.from) == selected_card
                    || link.to == selected_card && selected_card.is_some();
                let color = if link.to.is_none() {
                    Color32::DARK_RED
                } else if active {
                    crate::shell::ACCENT
                } else if link.backward {
                    Color32::from_rgb(172, 119, 41)
                } else {
                    Color32::from_rgb(116, 130, 143)
                };
                let stroke = Stroke::new(if active { 1.8 } else { 1.2 } * zoom.max(0.6), color);
                let points: Vec<_> = link.points.iter().copied().map(transform).collect();
                crate::graph_edges::paint(
                    &painter,
                    &points,
                    stroke,
                    zoom,
                    ui.visuals().panel_fill,
                    false,
                );
                for (segment_index, segment) in link.points.windows(2).enumerate() {
                    let (a, b) = (transform(segment[0]), transform(segment[1]));
                    ui.interact(
                        Rect::from_two_pos(a, b).expand(4.0),
                        ui.id()
                            .with(("flow-edge", link.from, &link.target, segment_index)),
                        egui::Sense::hover(),
                    )
                    .on_hover_text(&detail);
                }
                let plain_forward = link.label == "跳转"
                    && !link.backward
                    && link.to.is_some()
                    && layout.outgoing[link.from] == 1;
                if (link.label != "下一步" && !plain_forward) || link.backward || link.to.is_none()
                {
                    let label = if link.to.is_none() {
                        message(
                            "flow_graph.missing_target",
                            &[("target", link.target.clone())],
                        )
                    } else {
                        short_edge_label(
                            &edge_label(&link.label, source, !state.full),
                            link.backward,
                            state.full,
                        )
                    };
                    labels.push((link, label, detail, color));
                }
            }
            for (index, members) in cached.members.iter().enumerate() {
                let original = members[0];
                let node = &nodes[original];
                let card = rect(index);
                if !card.intersects(clip) {
                    continue;
                }
                let response = ui.interact(
                    card,
                    ui.id().with(("flow-node", index)),
                    egui::Sense::click(),
                );
                let reached = layout.reachable.contains(&index);
                let fill = if Some(index) == selected_card {
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
                        if Some(index) == selected_card {
                            2.0_f32
                        } else {
                            1.0_f32
                        },
                        if Some(index) == selected_card {
                            crate::shell::ACCENT
                        } else {
                            Color32::from_gray(177)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                let label_key = format!("node.{}", text(node, "type"));
                let translated = crate::i18n::key(&label_key);
                let kind = if translated == label_key {
                    let kind = text(node, "type");
                    if lom_core::validate::node_schema()["nodes"]
                        .get(kind)
                        .is_some()
                    {
                        kind.to_owned()
                    } else {
                        message("flow_graph.unsupported", &[("kind", kind.into())])
                    }
                } else {
                    translated
                };
                let title = format!(
                    "{:02} {}{}",
                    original + 1,
                    if members.len() > 1 {
                        message(
                            "flow_graph.segment",
                            &[("count", members.len().to_string())],
                        )
                    } else {
                        kind
                    },
                    if reached {
                        String::new()
                    } else {
                        format!(" · {}", key("flow.unreachable"))
                    }
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
                    card_summary(project, nodes, members),
                    (12.0 * zoom).max(8.0),
                    available,
                    color,
                );
                paint_text(
                    &painter,
                    card.min + Vec2::new(padding, 51.0 * zoom),
                    if members.len() > 1 {
                        message(
                            "flow_graph.first_step",
                            &[("number", format!("{:02}", original + 1))],
                        )
                    } else {
                        text(node, "id").into()
                    },
                    (10.0 * zoom).max(7.0),
                    available,
                    Color32::GRAY,
                );
                if response
                    .on_hover_text(format!(
                        "{}\n{}\n{}",
                        text(node, "id"),
                        text(node, "type"),
                        card_summary(project, nodes, members)
                    ))
                    .clicked()
                {
                    choose = Some(original);
                }
            }
            // All lines are already drawn. Label backgrounds use the actual
            // galley size so later edges cannot cut through text at any zoom.
            let mut placed = Vec::<Rect>::new();
            let cards: Vec<_> = (0..layout.cards.len()).map(rect).collect();
            for (link, label, detail, color) in labels {
                let mut job = egui::text::LayoutJob::simple(
                    label.clone(),
                    FontId::proportional((11.0 * zoom).max(8.0)),
                    color,
                    link.label_width * zoom,
                );
                job.wrap.max_rows = 1;
                job.wrap.overflow_character = Some('…');
                let galley = painter.layout_job(job);
                let preferred = transform(link.label_at) - Vec2::new(0.0, galley.size().y);
                let Some((at, bounds)) =
                    label_position(preferred, galley.size(), zoom, &cards, &placed)
                else {
                    continue;
                };
                placed.push(bounds);
                painter.rect_filled(bounds, 2.0, Color32::from_rgb(249, 249, 247));
                painter.galley(at, galley, color);
                ui.interact(
                    bounds,
                    ui.id().with(("flow-label", link.from, &link.target)),
                    egui::Sense::hover(),
                )
                .on_hover_text(detail);
            }
            if locate {
                if let Some(selected) = selected_card {
                    ui.scroll_to_rect(rect(selected), Some(egui::Align::Center));
                }
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

    fn long_story(count: usize) -> Value {
        let nodes: Vec<_> = (0..count).map(|index| json!({"id":format!("n{index}"),"type":if index + 1 == count {"end"} else {"say"},"text":format!("第{index}句")})).collect();
        json!({"id":"main","start":"n0","nodes":nodes})
    }

    #[test]
    fn overview_collapses_linear_runs_and_keeps_the_externally_selected_step() {
        let story = long_story(200);
        let before = story.clone();
        let overview = display_layout(&story, 100, false);
        assert_eq!(overview.members.len(), 5);
        assert!(overview.members.contains(&vec![100]));
        assert!(overview.members.contains(&(1..100).collect::<Vec<_>>()));
        assert!(overview.members.contains(&(101..199).collect::<Vec<_>>()));
        assert_eq!(overview.layout.links.len(), 4);
        let complete = display_layout(&story, 100, true);
        assert_eq!(complete.members.len(), 200);
        assert_eq!(complete.layout.links.len(), 199);
        assert!(overview.layout.size.y < complete.layout.size.y / 20.0);
        assert_eq!(story, before);
    }

    #[test]
    fn overview_retains_choice_native_conditions_and_real_merge_points() {
        for native in [false, true] {
            let mut story = json!({"id":"main","start":"start","nodes":[
                {"id":"start","type":"message"},
                {"id":"a","type":"say"},
                {"id":"b","type":"say"},
                {"id":"choice","type":"choice","options":[{"text":"走左边","goto":"l1"},{"text":"走右边","goto":"r1"}]},
                {"id":"l1","type":"say","text":"左侧首句","goto":"l2"},
                {"id":"l2","type":"say","goto":"merge"},
                {"id":"r1","type":"say","text":"右侧首句","goto":"r2"},
                {"id":"r2","type":"say","goto":"merge"},
                {"id":"merge","type":"message","goto":"tail"},
                {"id":"tail","type":"say"},
                {"id":"end","type":"end"}
            ]});
            if native {
                story["nodes"][3] = json!({"id":"choice","type":"native_branch","condition":{"kind":"literal","value":true},"success":"l1","failure":"r1"});
            }
            let model = display_layout(&story, 0, false);
            assert!(model.members.contains(&vec![3]));
            assert!(model.members.contains(&vec![8]));
            assert!(model.members.contains(&vec![4, 5]));
            assert!(model.members.contains(&vec![6, 7]));
            assert_eq!(model.layout.links.len(), 8);
            let merge = model
                .members
                .iter()
                .position(|members| members == &[8])
                .unwrap();
            assert_eq!(
                model
                    .layout
                    .links
                    .iter()
                    .filter(|link| link.to == Some(merge))
                    .count(),
                2
            );
            assert!(model.layout.links.iter().all(|link| link.to.is_some()));
            if native {
                assert!(model
                    .layout
                    .links
                    .iter()
                    .any(|link| link.label == "条件成立"));
                assert!(model
                    .layout
                    .links
                    .iter()
                    .any(|link| link.label == "条件不成立"));
            }
        }
    }

    #[test]
    fn overview_preserves_loop_endpoints_and_self_loops() {
        let story = json!({"start":"start","nodes":[
            {"id":"start","type":"message"},
            {"id":"again","type":"message"},
            {"id":"check","type":"native_branch","success":"a","failure":"end"},
            {"id":"a","type":"say"},{"id":"b","type":"say"},{"id":"c","type":"say"},
            {"id":"back","type":"message","goto":"again"},
            {"id":"end","type":"end"}
        ]});
        let model = display_layout(&story, 0, false);
        assert!(model.members.contains(&vec![3, 4, 5]));
        let backward = model
            .layout
            .links
            .iter()
            .find(|link| link.backward)
            .unwrap();
        assert_eq!(model.members[backward.from], vec![6]);
        assert_eq!(model.members[backward.to.unwrap()], vec![1]);
        let own_loop = display_layout(
            &json!({"start":"a","nodes":[{"id":"a","type":"message","goto":"a"}]}),
            0,
            false,
        );
        assert_eq!(own_loop.layout.links.len(), 1);
        assert!(own_loop.layout.links[0].backward);
        assert_eq!(own_loop.layout.links[0].to, Some(0));
    }

    #[test]
    fn native_option_and_checkpoint_calls_remain_visible_decision_cards() {
        for function in [
            json!({"kind":"name","name":"choose"}),
            json!({"kind":"member","object":{"kind":"name","name":"checkpointmanager"},"field":"Switch"}),
            json!({"kind":"member","object":{"kind":"name","name":"checkpointmanager"},"field":"Dice"}),
        ] {
            let mut story = long_story(6);
            story["nodes"][2] = json!({"id":"n2","type":"native_action","operations":[
                {"kind":"assign","target":{"kind":"local","name":"result","symbol":"v1"},"value":{"kind":"call","function":function,"args":[]}}
            ]});
            let model = display_layout(&story, 0, false);
            assert!(model.members.contains(&vec![2]));
            assert_eq!(model.layout.links.len(), 4);
        }
    }

    #[test]
    fn branch_and_merge_ports_follow_card_order_instead_of_identifier_order() {
        let reversed = layout(&json!({"start":"choice","nodes":[
            {"id":"choice","type":"choice","options":[{"text":"左","goto":"zleft"},{"text":"右","goto":"aright"}]},
            {"id":"zleft","type":"say","goto":"end"},
            {"id":"aright","type":"say","goto":"end"},{"id":"end","type":"end"}
        ]}));
        let left = reversed
            .links
            .iter()
            .find(|link| link.from == 0 && link.to == Some(1))
            .unwrap();
        let right = reversed
            .links
            .iter()
            .find(|link| link.from == 0 && link.to == Some(2))
            .unwrap();
        assert!(left.points[0].x < right.points[0].x);
        let merged = layout(&json!({"start":"choice","nodes":[
            {"id":"choice","type":"choice","options":[{"text":"左","goto":"left"},{"text":"右","goto":"right"}]},
            {"id":"left","type":"say","goto":"second_left"},
            {"id":"right","type":"say","goto":"second_right"},
            {"id":"second_right","type":"say","goto":"end"},
            {"id":"second_left","type":"say","goto":"end"},{"id":"end","type":"end"}
        ]}));
        assert!(merged.cards[4].center().x < merged.cards[3].center().x);
        let left = merged.links.iter().find(|link| link.from == 4).unwrap();
        let right = merged.links.iter().find(|link| link.from == 3).unwrap();
        assert!(left.points.last().unwrap().x < right.points.last().unwrap().x);
    }

    #[test]
    fn edge_labels_avoid_cards_and_previously_placed_labels() {
        let cards = [Rect::from_min_size(Pos2::ZERO, CARD)];
        let size = Vec2::new(80.0, 12.0);
        let (_, first) = label_position(Pos2::new(0.0, 80.0), size, 1.0, &cards, &[]).unwrap();
        let (_, second) =
            label_position(Pos2::new(18.0, 80.0), size, 1.0, &cards, &[first]).unwrap();
        assert!(!first.intersects(second));
        assert!(cards
            .iter()
            .all(|card| !card.intersects(first) && !card.intersects(second)));
    }

    #[test]
    fn system_edge_labels_use_locale_keys_and_authored_option_text_stays_literal() {
        let condition = json!({"type":"native_branch"});
        assert_eq!(
            edge_label("条件成立", &condition, true),
            key("flow_graph.true_short")
        );
        assert_eq!(
            edge_label("条件不成立", &condition, false),
            key("flow_graph.condition_false")
        );
        assert_eq!(
            edge_label("条件成立（1）", &json!({"type":"branch"}), false),
            message(
                "flow_graph.condition_result",
                &[
                    ("condition", key("flow_graph.condition_true")),
                    ("value", "1".into())
                ]
            )
        );
        assert_eq!(
            edge_label("条件成立", &json!({"type":"choice"}), true),
            "条件成立"
        );
        assert_eq!(
            edge_label("成功 / 失败", &json!({"type":"dice"}), false),
            "成功 / 失败"
        );
    }

    #[test]
    fn generated_option_and_dice_labels_preserve_complete_authored_text() {
        let choice =
            json!({"type":"choice", "options":[{"goto":"a"}, {"text":"成功 / 失败", "goto":"a"}]});
        assert_eq!(
            edge_label("选项 1 / 成功 / 失败", &choice, false),
            format!(
                "{} / 成功 / 失败",
                message("editor.flow.option", &[("number", "1".into())])
            )
        );
        let dice = json!({"type":"dice", "bands":[{"goto":"a"}], "options":[{"text":"条件成立 / 失败", "goto_大成功":"a", "goto_失败":"a"}]});
        assert_eq!(
            edge_label(
                "条件成立 / 失败 · 大成功 / 条件成立 / 失败 · 失败 / 区间 1",
                &dice,
                false
            ),
            format!(
                "条件成立 / 失败 · {} / 条件成立 / 失败 · {} / {}",
                key("editor.flow.critical_success"),
                key("flow_graph.failure"),
                message("editor.flow.band", &[("number", "1".into())])
            )
        );
    }
    #[test]
    fn overview_segment_clicks_and_external_selection_return_real_story_indices() {
        let story = long_story(100);
        let before = story.clone();
        let project = Project::new();
        let context = egui::Context::default();
        context.enable_accesskit();
        let frame = |selected, events| {
            let mut chosen = None;
            let mut full = false;
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(680.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        chosen = show(ui, &project, &story, selected);
                        full = ui
                            .data_mut(|data| {
                                data.get_temp::<ViewState>(ui.id().with(("flow-view", "main")))
                            })
                            .unwrap()
                            .full;
                    });
                },
            );
            (output, chosen, full)
        };
        frame(0, vec![]);
        frame(50, vec![]);
        let (output, _, _) = frame(50, vec![]);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let selected = nodes
            .iter()
            .find(|(_, node)| node.label().is_some_and(|label| label.ends_with(" · n50")))
            .unwrap();
        let bounds = selected.1.bounds().unwrap();
        assert!(bounds.y0 > 40.0 && bounds.y1 < 900.0);
        let segment = nodes
            .iter()
            .find(|(_, node)| {
                node.label().is_some_and(|label| {
                    label.contains(&message("flow_graph.segment", &[("count", "48".into())]))
                        && label.ends_with(" · n51")
                })
            })
            .unwrap()
            .0;
        let mode = nodes
            .iter()
            .find(|(_, node)| node.label() == Some(key("flow_graph.full").as_str()))
            .unwrap()
            .0;
        let (_, chosen, _) = frame(
            50,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: segment,
                    data: None,
                },
            )],
        );
        assert_eq!(chosen, Some(51));
        let (_, _, full) = frame(
            50,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: mode,
                    data: None,
                },
            )],
        );
        assert!(full);
        assert_eq!(story, before);
    }

    #[test]
    fn music_cards_show_the_same_simplified_title_as_the_resource_picker() {
        let project = Project::new();
        let node = json!({"id":"music","type":"music","name":"嚴肅_001"});
        let options = crate::forms::Catalog::new().options("music", &node, &[], &[]);
        let title = &options.iter().find(|(id, _)| id == "嚴肅_001").unwrap().1;
        assert_eq!(summary(&project, &node), *title);
        assert_ne!(*title, "嚴肅_001");
    }

    #[test]
    fn long_edges_use_reserved_corridors_without_crossing_intermediate_cards() {
        let graph = layout(&json!({"start":"choice","nodes":[
            {"id":"choice","type":"choice","options":[{"text":"跳过","goto":"end"},{"text":"继续","goto":"a"}]},
            {"id":"a","type":"say","goto":"b"},
            {"id":"b","type":"say","goto":"end"},
            {"id":"end","type":"end"}
        ]}));
        let skip = graph
            .links
            .iter()
            .find(|link| link.from == 0 && link.to == Some(3))
            .unwrap();
        for segment in skip.points.windows(2) {
            assert!(segment[0].x == segment[1].x || segment[0].y == segment[1].y);
            for card in &graph.cards[1..3] {
                let crosses = if segment[0].x == segment[1].x {
                    segment[0].x > card.left()
                        && segment[0].x < card.right()
                        && segment[0].y.max(segment[1].y) > card.top()
                        && segment[0].y.min(segment[1].y) < card.bottom()
                } else {
                    segment[0].y > card.top()
                        && segment[0].y < card.bottom()
                        && segment[0].x.max(segment[1].x) > card.left()
                        && segment[0].x.min(segment[1].x) < card.right()
                };
                assert!(!crosses, "skip edge crosses an intermediate card");
            }
        }
        let right = graph.cards.iter().map(Rect::right).fold(0.0_f32, f32::max);
        assert!(skip.points.iter().all(|point| point.x <= right));
    }

    #[test]
    fn layered_order_removes_crossings_and_keeps_separate_chains_straight() {
        let positions = layered_positions(&[0, 0, 1, 1], &[vec![3], vec![2], vec![], vec![]]);
        assert!((positions.x[0] - positions.x[1]) * (positions.x[3] - positions.x[2]) > 0.0);
        assert_eq!(positions.x[0], positions.x[3]);
        assert_eq!(positions.x[1], positions.x[2]);
    }

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
            message("flow_graph.next_chapter", &[("chapter", "chapter2".into())])
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
            .find(|(_, node)| node.label() == Some(key("flow_graph.locate").as_str()))
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
