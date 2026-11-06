//! Read-only, game-derived chapter routes and a deterministic station layout.
use crate::story_bookmarks::{Bookmark, Timeline, TimelineEntry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptLink {
    pub target: String,
    pub condition: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionChange {
    pub series: String,
    pub key: String,
    pub condition: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRoute {
    pub script: String,
    pub links: Vec<ScriptLink>,
    pub missions: Vec<MissionChange>,
    pub unknown_jumps: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRoutes {
    pub scripts: Vec<ScriptRoute>,
}

#[derive(Clone, Debug)]
pub struct Station {
    pub entry: TimelineEntry,
    pub scripts: Vec<String>,
    pub column: usize,
    pub lane: i32,
    pub spine: bool,
}
#[derive(Clone, Debug)]
pub struct RouteEdge {
    pub from: usize,
    pub to: usize,
    pub labels: Vec<String>,
    /// Only a calendar guide, never a claim that a story branch reaches here.
    pub by_date: bool,
}
#[derive(Clone, Debug, Default)]
pub struct RouteGraph {
    pub stations: Vec<Station>,
    pub edges: Vec<RouteEdge>,
    pub owners: BTreeMap<String, Vec<usize>>,
    pub unknown_jumps: usize,
    pub main_bottom: i32,
}

impl RouteGraph {
    pub const MARKED_SERIES: &'static str = "已标记章节";
    pub fn with_bookmarks(timeline: &Timeline, routes: &ScriptRoutes, marks: &[Bookmark]) -> Self {
        let graph = Self::build(timeline, routes);
        let mut known: BTreeSet<_> = graph.owners.keys().cloned().collect();
        let scripts: BTreeSet<_> = routes
            .scripts
            .iter()
            .map(|script| script.script.to_lowercase())
            .collect();
        let mut extended = timeline.clone();
        for mark in marks
            .iter()
            .filter(|mark| mark.game_dir == timeline.game_dir)
        {
            let name = mark.script.to_lowercase();
            if !scripts.contains(&name) || !known.insert(name) {
                continue;
            }
            let date = (mark.year > 0 && mark.month > 0 && mark.stage > 0)
                .then(|| (mark.year - 1) * 36 + (mark.month - 1) * 3 + mark.stage - 1);
            extended.missions.push(TimelineEntry {
                script: mark.script.clone(),
                title: if mark.text.is_empty() {
                    mark.script.clone()
                } else {
                    mark.text.chars().take(42).collect()
                },
                series: Self::MARKED_SERIES.into(),
                key: mark.script.clone(),
                main: false,
                next_key: String::new(),
                condition: String::new(),
                place: "标记处".into(),
                when_label: mark.caption(),
                from_round: date,
                to_round: date,
                active: true,
            });
        }
        if extended.missions.len() == timeline.missions.len() {
            graph
        } else {
            Self::build(&extended, routes)
        }
    }
    pub fn build(timeline: &Timeline, routes: &ScriptRoutes) -> Self {
        let mut graph = Self {
            stations: timeline
                .missions
                .iter()
                .map(|entry| Station {
                    entry: entry.clone(),
                    scripts: vec![],
                    column: 0,
                    lane: 0,
                    spine: false,
                })
                .collect(),
            unknown_jumps: routes
                .scripts
                .iter()
                .map(|script| script.unknown_jumps)
                .sum(),
            ..Default::default()
        };
        let scripts: BTreeMap<_, _> = routes
            .scripts
            .iter()
            .map(|script| (script.script.to_lowercase(), script))
            .collect();
        let keys: BTreeMap<_, _> = graph
            .stations
            .iter()
            .enumerate()
            .map(|(i, node)| {
                (
                    (
                        mission_series(&node.entry.series).to_owned(),
                        node.entry.key.clone(),
                    ),
                    i,
                )
            })
            .collect();
        let mut roots = BTreeMap::<String, Vec<usize>>::new();
        for (i, node) in graph.stations.iter().enumerate() {
            roots
                .entry(node.entry.script.to_lowercase())
                .or_default()
                .push(i);
        }
        for from in 0..graph.stations.len() {
            let mut queue =
                VecDeque::from([(graph.stations[from].entry.script.clone(), String::new())]);
            let mut visited = BTreeSet::new();
            let mut overrides_default = false;
            while let Some((script, path)) = queue.pop_front() {
                let name = script.to_lowercase();
                if !visited.insert(name.clone()) {
                    continue;
                }
                graph.owners.entry(name.clone()).or_default().push(from);
                graph.stations[from].scripts.push(script.clone());
                let Some(route) = scripts.get(&name) else {
                    continue;
                };
                for change in &route.missions {
                    if let Some(&to) = keys.get(&(
                        mission_series(&change.series).to_owned(),
                        change.key.clone(),
                    )) {
                        let condition = join(&path, &change.condition);
                        if mission_series(&change.series)
                            == mission_series(&graph.stations[from].entry.series)
                            && condition.is_empty()
                        {
                            overrides_default = true;
                        }
                        graph.add_edge(
                            from,
                            to,
                            format!(
                                "{script} · {}",
                                if condition.is_empty() {
                                    "剧情推进"
                                } else {
                                    &condition
                                }
                            ),
                            false,
                        );
                    }
                }
                for link in &route.links {
                    let condition = join(&path, &link.condition);
                    if let Some(targets) = roots.get(&link.target.to_lowercase()) {
                        for &to in targets {
                            if to != from {
                                graph.add_edge(
                                    from,
                                    to,
                                    format!(
                                        "{script} · {}",
                                        if condition.is_empty() {
                                            "接入章节"
                                        } else {
                                            &condition
                                        }
                                    ),
                                    false,
                                );
                            }
                        }
                        if targets.iter().any(|&to| to != from) {
                            continue;
                        }
                    }
                    queue.push_back((link.target.clone(), condition));
                }
            }
            let entry = &graph.stations[from].entry;
            if !overrides_default {
                if let Some(&to) = keys.get(&(
                    mission_series(&entry.series).to_owned(),
                    entry.next_key.clone(),
                )) {
                    if to != from {
                        graph.add_edge(from, to, "入口默认推进（章节内仍可改写）".into(), false);
                    }
                }
            }
        }
        graph.layout();
        graph
    }

    fn add_edge(&mut self, from: usize, to: usize, label: String, by_date: bool) {
        if let Some(edge) = self
            .edges
            .iter_mut()
            .find(|edge| edge.from == from && edge.to == to && edge.by_date == by_date)
        {
            if !label.is_empty() && !edge.labels.contains(&label) {
                edge.labels.push(label);
            }
        } else {
            self.edges.push(RouteEdge {
                from,
                to,
                labels: vec![label],
                by_date,
            });
        }
    }

    fn layout(&mut self) {
        let main: BTreeSet<_> = self
            .stations
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.entry.main.then_some(i))
            .collect();
        let dates: BTreeSet<_> = main
            .iter()
            .filter_map(|&i| self.stations[i].entry.from_round)
            .collect();
        let dates: BTreeMap<_, _> = dates
            .into_iter()
            .enumerate()
            .map(|(i, date)| (date, i))
            .collect();
        // A dashed calendar spine also keeps disconnected mission entries findable.
        let mut dated = BTreeMap::<i32, usize>::new();
        for &i in &main {
            if let Some(date) = self.stations[i].entry.from_round {
                dated.entry(date).or_insert(i);
            }
        }
        let date_nodes: Vec<_> = dated.values().copied().collect();
        for pair in date_nodes.windows(2) {
            if !self
                .edges
                .iter()
                .any(|e| e.from == pair[0] && e.to == pair[1])
            {
                self.add_edge(pair[0], pair[1], "时间顺序，不表示必然接续".into(), true);
            }
        }
        let mut pending = main.clone();
        let mut done = BTreeSet::new();
        let mut score = BTreeMap::<usize, usize>::new();
        let mut previous = BTreeMap::new();
        while !pending.is_empty() {
            let i = pending
                .iter()
                .filter(|&&i| {
                    !self
                        .edges
                        .iter()
                        .any(|e| e.to == i && e.from != i && pending.contains(&e.from))
                })
                .min_by_key(|&&i| (self.stations[i].entry.from_round.unwrap_or(i32::MAX), i))
                .or_else(|| {
                    pending.iter().min_by_key(|&&i| {
                        (self.stations[i].entry.from_round.unwrap_or(i32::MAX), i)
                    })
                })
                .copied()
                .unwrap();
            let predecessors: Vec<_> = self
                .edges
                .iter()
                .filter(|e| e.to == i && done.contains(&e.from))
                .map(|e| e.from)
                .collect();
            self.stations[i].column = predecessors
                .iter()
                .map(|&p| self.stations[p].column + 1)
                .max()
                .unwrap_or(0)
                .max(
                    self.stations[i]
                        .entry
                        .from_round
                        .and_then(|date| dates.get(&date).copied())
                        .unwrap_or(0),
                );
            let best = predecessors
                .iter()
                .max_by_key(|p| score.get(p).copied().unwrap_or(0))
                .copied();
            score.insert(i, best.map_or(1, |p| score[&p] + 1));
            if let Some(p) = best {
                previous.insert(i, p);
            }
            pending.remove(&i);
            done.insert(i);
        }
        if let Some((&end, _)) = score.iter().max_by_key(|(_, score)| *score) {
            let mut at = Some(end);
            while let Some(i) = at {
                self.stations[i].spine = true;
                at = previous.get(&i).copied();
            }
        }
        let mut remaining: BTreeSet<_> = main
            .iter()
            .copied()
            .filter(|&i| !self.stations[i].spine)
            .collect();
        let mut spans = BTreeMap::<i32, Vec<(usize, usize)>>::new();
        let mut occupied = BTreeSet::new();
        for &i in &main {
            if self.stations[i].spine {
                occupied.insert((self.stations[i].column, 0));
            }
        }
        while let Some(&start) = remaining.first() {
            let mut queue = VecDeque::from([start]);
            let mut group = Vec::new();
            remaining.remove(&start);
            while let Some(i) = queue.pop_front() {
                group.push(i);
                for edge in self.edges.iter().filter(|e| !e.by_date) {
                    let neighbour = if edge.from == i {
                        Some(edge.to)
                    } else if edge.to == i {
                        Some(edge.from)
                    } else {
                        None
                    };
                    if let Some(n) = neighbour {
                        if remaining.remove(&n) {
                            queue.push_back(n);
                        }
                    }
                }
            }
            let low = group
                .iter()
                .map(|&i| self.stations[i].column)
                .min()
                .unwrap();
            let high = group
                .iter()
                .map(|&i| self.stations[i].column)
                .max()
                .unwrap();
            let mut lane = 1;
            while spans
                .get(&lane)
                .is_some_and(|spans| spans.iter().any(|&(a, b)| low <= b && a <= high))
            {
                lane = if lane > 0 { -lane } else { -lane + 1 };
            }
            spans.entry(lane).or_default().push((low, high));
            for i in group {
                let mut row = lane;
                while occupied.contains(&(self.stations[i].column, row)) {
                    row += if row >= 0 { 1 } else { -1 };
                }
                self.stations[i].lane = row;
                occupied.insert((self.stations[i].column, row));
            }
        }
        let min = self
            .stations
            .iter()
            .filter(|n| n.entry.main)
            .map(|n| n.lane)
            .min()
            .unwrap_or(0);
        for node in &mut self.stations {
            if node.entry.main {
                node.lane -= min;
            }
        }
        self.main_bottom = self
            .stations
            .iter()
            .filter(|n| n.entry.main)
            .map(|n| n.lane)
            .max()
            .unwrap_or(0);
        let mut side = BTreeMap::<String, Vec<usize>>::new();
        for (i, node) in self.stations.iter().enumerate() {
            if !node.entry.main && node.entry.series != Self::MARKED_SERIES {
                side.entry(node.entry.series.clone()).or_default().push(i);
            }
        }
        let mut row = self.main_bottom + 2;
        for nodes in side.values() {
            let mut occupied = BTreeSet::new();
            for &i in nodes {
                let date = self.stations[i].entry.from_round;
                let column = date
                    .and_then(|date| {
                        dated
                            .range(..=date)
                            .next_back()
                            .map(|(_, &main)| self.stations[main].column)
                    })
                    .unwrap_or(0);
                let mut col = column;
                for edge in self
                    .edges
                    .iter()
                    .filter(|e| e.to == i && nodes.contains(&e.from) && e.from < i)
                {
                    col = col.max(self.stations[edge.from].column + 1);
                }
                while !occupied.insert(col) {
                    col += 1;
                }
                self.stations[i].column = col;
                self.stations[i].lane = row;
            }
            row += 1;
        }
        let marked: Vec<_> = self
            .stations
            .iter()
            .enumerate()
            .filter_map(|(i, node)| (node.entry.series == Self::MARKED_SERIES).then_some(i))
            .collect();
        if !marked.is_empty() {
            for node in &mut self.stations {
                node.lane += 1;
                node.column += 1;
            }
            self.main_bottom += 1;
            let mut occupied = BTreeSet::new();
            for i in marked {
                let mut col = self
                    .edges
                    .iter()
                    .filter(|edge| edge.from == i && edge.to != i)
                    .map(|edge| self.stations[edge.to].column.saturating_sub(1))
                    .min()
                    .unwrap_or(0);
                while !occupied.insert(col) {
                    col += 1;
                }
                self.stations[i].column = col;
                self.stations[i].lane = 0;
            }
        }
    }
}

fn mission_series(name: &str) -> &str {
    // MissionManagerData.SetKey ignores case only for the reserved Main name.
    // SubMissionsData.Get compares the remaining mission names exactly.
    if name.eq_ignore_ascii_case("main") {
        "main"
    } else {
        name
    }
}

fn join(path: &str, condition: &str) -> String {
    if path.is_empty() {
        condition.into()
    } else if condition.is_empty() || condition == path {
        path.into()
    } else {
        format!("{path} / {condition}")
    }
}
