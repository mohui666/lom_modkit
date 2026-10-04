//! The editor's local and cross-chapter preflight terminal rules.
use super::{nodes, text, PreflightIssue, Stories};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
type Graph = BTreeMap<String, BTreeSet<String>>;
fn terminal(node: &Value) -> bool {
    [
        "end",
        "goto_scene",
        "death",
        "combat",
        "battle",
        "battle_result",
        "stat_check",
        "affinity_check",
        "item_check",
        "talent_check",
        "flag_check",
        "activity",
        "quest_check",
        "persistent_check",
    ]
    .contains(&text(node, "type"))
}
fn walk(graph: &Graph, roots: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let mut todo: Vec<_> = roots.into_iter().collect();
    let mut seen = BTreeSet::new();
    while let Some(n) = todo.pop() {
        if let Some(edges) = graph.get(&n) {
            if seen.insert(n) {
                todo.extend(edges.iter().cloned());
            }
        }
    }
    seen
}
fn reverse(graph: &Graph) -> Graph {
    let mut result: Graph = graph
        .keys()
        .map(|key| (key.clone(), BTreeSet::new()))
        .collect();
    for (from, targets) in graph {
        for to in targets {
            if let Some(edges) = result.get_mut(to) {
                edges.insert(from.clone());
            }
        }
    }
    result
}
pub(super) fn flow_issues(stories: &Stories) -> Vec<PreflightIssue> {
    let mut issues = Vec::new();
    let mut chapter_edges: Graph = stories
        .keys()
        .map(|s| (s.clone(), BTreeSet::new()))
        .collect();
    let mut edge_node = BTreeMap::new();
    let mut finals = BTreeSet::new();
    let mut uncertain = BTreeSet::new();
    for (sid, story) in stories {
        let known: BTreeSet<_> = nodes(story)
            .iter()
            .map(|n| text(n, "id").to_owned())
            .filter(|id| !id.is_empty())
            .collect();
        let graph: Graph = nodes(story)
            .iter()
            .enumerate()
            .filter(|(_, n)| !text(n, "id").is_empty())
            .map(|(i, n)| {
                (
                    text(n, "id").to_owned(),
                    crate::analysis::successors(story, i)
                        .into_iter()
                        .filter(|target| known.contains(target))
                        .collect(),
                )
            })
            .collect();
        let reached = walk(&graph, [text(story, "start").to_owned()]);
        let can_finish = walk(
            &reverse(&graph),
            nodes(story)
                .iter()
                .filter(|n| terminal(n))
                .map(|n| text(n, "id").to_owned()),
        );
        for node in nodes(story) {
            let nid = text(node, "id");
            if nid.is_empty() {
                continue;
            }
            if !reached.contains(nid) {
                issues.push(PreflightIssue::new(
                    "warning",
                    "unreachable_node",
                    sid,
                    nid,
                    "节点不可达",
                ));
            } else {
                if !terminal(node) && graph.get(nid).is_some_and(BTreeSet::is_empty) {
                    issues.push(PreflightIssue::new(
                        "warning",
                        "broken_flow",
                        sid,
                        nid,
                        "节点缺少后继",
                    ));
                }
                if !can_finish.contains(nid)
                    && walk(&graph, graph.get(nid).into_iter().flatten().cloned()).contains(nid)
                {
                    issues.push(PreflightIssue::new(
                        "error",
                        "no_exit_scc",
                        sid,
                        nid,
                        "循环没有可达出口",
                    ));
                }
                if node["type"] == "raw" {
                    uncertain.insert(sid.clone());
                }
                if terminal(node) {
                    let next = text(node, "next_script");
                    if node["type"] == "end" && stories.contains_key(next) {
                        chapter_edges.get_mut(sid).unwrap().insert(next.into());
                        edge_node.insert((sid.clone(), next.to_owned()), nid.to_owned());
                    } else if !(node["type"] == "end" && !next.is_empty()) {
                        finals.insert(sid.clone());
                    }
                }
            }
            if node["type"] == "end"
                && !text(node, "next_script").is_empty()
                && !stories.contains_key(text(node, "next_script"))
            {
                issues.push(PreflightIssue::new(
                    "error",
                    "invalid_cross_story_goto",
                    sid,
                    nid,
                    format!("下一章节不存在: {}", text(node, "next_script")),
                ));
            }
        }
    }
    let can_finish = walk(&reverse(&chapter_edges), finals);
    let reach_by_story: BTreeMap<_, _> = chapter_edges
        .iter()
        .map(|(sid, next)| (sid.clone(), walk(&chapter_edges, next.iter().cloned())))
        .collect();
    for (sid, reached) in &reach_by_story {
        if can_finish.contains(sid) || !reached.contains(sid) {
            continue;
        }
        let component: BTreeSet<_> = reached
            .iter()
            .filter(|other| {
                reach_by_story
                    .get(*other)
                    .is_some_and(|set| set.contains(sid))
            })
            .cloned()
            .collect();
        if !component.is_disjoint(&uncertain) {
            continue;
        }
        let nid = chapter_edges[sid]
            .iter()
            .find(|target| component.contains(*target))
            .and_then(|target| edge_node.get(&(sid.clone(), target.clone())))
            .map(String::as_str)
            .unwrap_or("");
        issues.push(PreflightIssue::new(
            "error",
            "no_exit_scc",
            sid,
            nid,
            "跨章节循环没有可达出口",
        ));
    }
    issues
}
