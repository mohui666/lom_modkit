//! Document snapshots, dirty state and undo/redo.
use super::*;
use crate::json::same as same_json;
use std::sync::Arc;

// Share small node blocks between edits. Contiguous values keep large-chapter
// comparisons fast without copying whole chapters into each undo entry.
const SNAPSHOT_BLOCK: usize = 32;
pub(super) struct ChapterView {
    chapter: String,
    sections: Value,
    pub(super) ids: Vec<String>,
    pub(super) rows: Vec<authoring::Row>,
    pub(super) sections_error: Option<String>,
}
pub(super) struct StepSearch {
    query: String,
    revision: u64,
    results: Arc<Vec<(String, usize)>>,
}
type SnapshotBlocks = Arc<Vec<Arc<Vec<Value>>>>;

#[derive(Clone, Debug)]
pub(super) struct StorySnapshot {
    fields: Arc<Value>,
    nodes: Option<SnapshotBlocks>,
    origins: Option<SnapshotBlocks>,
}

fn same_block(old: &[Value], current: &[Value]) -> bool {
    old.len() == current.len() && old.iter().zip(current).all(|(a, b)| same_json(a, b))
}

fn capture_blocks(nodes: &[Value], old: &[Arc<Vec<Value>>]) -> Vec<Arc<Vec<Value>>> {
    let mut start = 0;
    if old.iter().map(|block| block.len()).sum::<usize>() == nodes.len() {
        return old
            .iter()
            .map(|block| {
                let current = &nodes[start..start + block.len()];
                start += block.len();
                if same_block(block, current) {
                    block.clone()
                } else {
                    Arc::new(current.to_vec())
                }
            })
            .collect();
    }

    // An insertion shifts the remaining indices. Match the untouched suffix
    // from the end so history still shares its original blocks.
    let prefix = old
        .iter()
        .take_while(|block| {
            let next = start + block.len();
            let matched = nodes
                .get(start..next)
                .is_some_and(|current| same_block(block, current));
            if matched {
                start = next;
            }
            matched
        })
        .count();
    let mut end = nodes.len();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .take_while(|block| {
            let Some(next) = end.checked_sub(block.len()).filter(|&next| next >= start) else {
                return false;
            };
            if same_block(block, &nodes[next..end]) {
                end = next;
                true
            } else {
                false
            }
        })
        .count();
    old[..prefix]
        .iter()
        .cloned()
        .chain(
            nodes[start..end]
                .chunks(SNAPSHOT_BLOCK)
                .map(|chunk| Arc::new(chunk.to_vec())),
        )
        .chain(old[old.len() - suffix..].iter().cloned())
        .collect()
}

impl PartialEq for StorySnapshot {
    fn eq(&self, other: &Self) -> bool {
        if !Arc::ptr_eq(&self.fields, &other.fields) && !same_json(&self.fields, &other.fields) {
            return false;
        }
        same_blocks(self.nodes.as_ref(), other.nodes.as_ref())
            && same_blocks(self.origins.as_ref(), other.origins.as_ref())
    }
}

fn same_blocks(a: Option<&SnapshotBlocks>, b: Option<&SnapshotBlocks>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) if Arc::ptr_eq(a, b) => true,
        (Some(a), Some(b)) => {
            if a.iter().map(|block| block.len()).sum::<usize>()
                != b.iter().map(|block| block.len()).sum::<usize>()
            {
                return false;
            }
            if a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a.len() == b.len()) {
                return a
                    .iter()
                    .zip(b.iter())
                    .all(|(a, b)| Arc::ptr_eq(a, b) || same_block(a, b));
            }
            a.iter()
                .flat_map(|block| block.iter())
                .eq(b.iter().flat_map(|block| block.iter()))
        }
        (None, None) => true,
        _ => false,
    }
}

fn share_blocks(values: &[Value], old: Option<&SnapshotBlocks>) -> SnapshotBlocks {
    let next = capture_blocks(values, old.map_or(&[], |blocks| blocks.as_slice()));
    if let Some(old) = old.filter(|old| {
        old.len() == next.len() && old.iter().zip(&next).all(|(a, b)| Arc::ptr_eq(a, b))
    }) {
        return old.clone();
    }
    Arc::new(next)
}

fn same_array(blocks: &SnapshotBlocks, values: &[Value]) -> bool {
    blocks.iter().map(|block| block.len()).sum::<usize>() == values.len()
        && blocks
            .iter()
            .flat_map(|block| block.iter())
            .zip(values)
            .all(|(a, b)| same_json(a, b))
}

fn materialize_blocks(blocks: &SnapshotBlocks) -> Value {
    Value::Array(
        blocks
            .iter()
            .flat_map(|block| block.iter().cloned())
            .collect(),
    )
}

fn equal_fields_except(a: &Value, b: &Value, excluded: &str) -> bool {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return false;
    };
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            key == excluded || b.get(key).is_some_and(|other| same_json(value, other))
        })
}

impl Eq for StorySnapshot {}

impl StorySnapshot {
    pub(super) fn capture(story: &Value, previous: Option<&Self>) -> Self {
        let Some(nodes) = story.get("nodes").and_then(Value::as_array) else {
            return Self {
                fields: Arc::new(story.clone()),
                nodes: None,
                origins: None,
            };
        };
        let origins = story["vanilla_import"]["origins"].as_array();
        let fields = previous
            .filter(|old| {
                old.nodes.is_some()
                    && old.origins.is_some() == origins.is_some()
                    && old.same_fields(story)
            })
            .map(|old| old.fields.clone())
            .unwrap_or_else(|| {
                Arc::new(Value::Object(
                    story
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(key, value)| {
                            (
                                key.clone(),
                                match key.as_str() {
                                    "nodes" => Value::Null,
                                    "vanilla_import" if origins.is_some() => Value::Object(
                                        value
                                            .as_object()
                                            .unwrap()
                                            .iter()
                                            .map(|(key, value)| {
                                                (
                                                    key.clone(),
                                                    if key == "origins" {
                                                        Value::Null
                                                    } else {
                                                        value.clone()
                                                    },
                                                )
                                            })
                                            .collect(),
                                    ),
                                    _ => value.clone(),
                                },
                            )
                        })
                        .collect(),
                ))
            });
        Self {
            fields,
            nodes: Some(share_blocks(
                nodes,
                previous.and_then(|old| old.nodes.as_ref()),
            )),
            origins: origins.map(|origins| {
                share_blocks(origins, previous.and_then(|old| old.origins.as_ref()))
            }),
        }
    }

    fn same_fields(&self, story: &Value) -> bool {
        let (Some(fields), Some(current)) = (self.fields.as_object(), story.as_object()) else {
            return false;
        };
        fields.len() == current.len()
            && fields.iter().all(|(key, value)| {
                key == "nodes"
                    || current.get(key).is_some_and(|current| {
                        if key == "vanilla_import" && self.origins.is_some() {
                            equal_fields_except(value, current, "origins")
                        } else {
                            same_json(value, current)
                        }
                    })
            })
    }

    fn same(&self, story: &Value) -> bool {
        let Some(nodes) = &self.nodes else {
            return same_json(&self.fields, story);
        };
        let Some(current) = story.get("nodes").and_then(Value::as_array) else {
            return false;
        };
        self.same_fields(story)
            && same_array(nodes, current)
            && match (&self.origins, story["vanilla_import"]["origins"].as_array()) {
                (Some(blocks), Some(current)) => same_array(blocks, current),
                (None, None) => true,
                _ => false,
            }
    }

    pub(super) fn to_value(&self) -> Value {
        let mut story = (*self.fields).clone();
        if let Some(nodes) = &self.nodes {
            story["nodes"] = materialize_blocks(nodes);
        }
        if let Some(origins) = &self.origins {
            story["vanilla_import"]["origins"] = materialize_blocks(origins);
        }
        story
    }
}

impl crate::persistence::StoryBaseline for Arc<StorySnapshot> {
    fn saved_story(&self) -> std::borrow::Cow<'_, Value> {
        std::borrow::Cow::Owned(self.to_value())
    }
}

#[test]
fn insert_snapshots_share_source_and_unchanged_blocks_across_shifted_indices() {
    let mut story = json!({
        "id":"main","start":"n0","vanilla_import":{"schema":2,"target":"chapter","origins":
            (0..96).map(|n| json!({"code":format!("original source {n}"),"node":{"goto":format!("n{}",n+1)}})).collect::<Vec<_>>()
        },
        "nodes": (0..96).map(|n| json!({"id":format!("n{n}"), "type":"say"})).collect::<Vec<_>>()
    });
    let original = StorySnapshot::capture(&story, None);
    story["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(35, json!({"id":"inserted","type":"say"}));
    story["vanilla_import"]["origins"][34]["node"]["goto"] = json!("inserted");
    let inserted = StorySnapshot::capture(&story, Some(&original));
    assert!(Arc::ptr_eq(&original.fields, &inserted.fields));
    assert!(Arc::ptr_eq(
        original.nodes.as_ref().unwrap().last().unwrap(),
        inserted.nodes.as_ref().unwrap().last().unwrap(),
    ));
    assert!(Arc::ptr_eq(
        original.origins.as_ref().unwrap().last().unwrap(),
        inserted.origins.as_ref().unwrap().last().unwrap(),
    ));
    assert_eq!(inserted.to_value(), story);
    story["nodes"].as_array_mut().unwrap().remove(35);
    story["vanilla_import"]["origins"][34]["node"]["goto"] = json!("n35");
    assert_eq!(StorySnapshot::capture(&story, Some(&inserted)), original);
}

fn changed_stories(
    left: &BTreeMap<String, Arc<StorySnapshot>>,
    right: &mut lom_core::project::Stories,
) -> Option<BTreeMap<String, Arc<StorySnapshot>>> {
    let changes = right.take_changes();
    let mut next = (left.len() != right.len()).then(|| left.clone());
    let candidates: Box<dyn Iterator<Item = &String> + '_> = match &changes {
        Some(changes) => Box::new(changes.iter()),
        None => Box::new(right.keys()),
    };
    for id in candidates {
        let Some(story) = right.get(id) else {
            if left.contains_key(id) {
                next.get_or_insert_with(|| left.clone()).remove(id);
            }
            continue;
        };
        if left.get(id).is_none_or(|old| !old.same(story)) {
            next.get_or_insert_with(|| left.clone()).insert(
                id.clone(),
                Arc::new(StorySnapshot::capture(story, left.get(id).map(Arc::as_ref))),
            );
        }
    }
    if let Some(stories) = &mut next {
        stories.retain(|id, _| right.contains_key(id));
    }
    next
}

impl App {
    pub(super) fn step_search(&mut self, query: &str) -> Arc<Vec<(String, usize)>> {
        if let Some(search) = &self.step_search {
            if search.query == query && search.revision == self.document_revision {
                return search.results.clone();
            }
        }
        let mut results = Vec::new();
        for (id, story) in &self.project.stories {
            let Some(nodes) = story["nodes"].as_array() else {
                continue;
            };
            let flag_hits: BTreeSet<_> =
                crate::game_flags::find_steps(&self.catalog.data, nodes, query)
                    .into_iter()
                    .map(|(index, _, _, _)| index)
                    .collect();
            results.extend(nodes.iter().enumerate().filter_map(|(index, node)| {
                (flag_hits.contains(&index) || node.to_string().to_lowercase().contains(query))
                    .then(|| (id.clone(), index))
            }));
        }
        let results = Arc::new(results);
        self.step_search = Some(StepSearch {
            query: query.to_owned(),
            revision: self.document_revision,
            results: results.clone(),
        });
        results
    }
    pub(super) fn chapter_view(&mut self) -> Arc<ChapterView> {
        if let Some((revision, view)) = &mut self.chapter_view {
            if view.chapter == self.current {
                if *revision == self.document_revision {
                    return view.clone();
                }
                let story = self
                    .project
                    .stories
                    .get(&self.current)
                    .unwrap_or(&Value::Null);
                if view.sections == story["_editor"]["sections"]
                    && story["nodes"].as_array().is_some_and(|nodes| {
                        nodes.len() == view.ids.len()
                            && nodes
                                .iter()
                                .zip(&view.ids)
                                .all(|(node, id)| node["id"].as_str() == Some(id.as_str()))
                    })
                {
                    // A prose/action edit does not change the sidebar's row
                    // structure. Rebuilding grouped rows scans chapter bounds
                    // repeatedly and used to block every typed character.
                    *revision = self.document_revision;
                    return view.clone();
                }
            }
        }
        let story = self.story();
        let view = Arc::new(ChapterView {
            chapter: self.current.clone(),
            sections: story["_editor"]["sections"].clone(),
            ids: story["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|n| n["id"].as_str().map(str::to_owned))
                .collect(),
            rows: authoring::section_rows(story),
            sections_error: authoring::validate_sections(story)
                .err()
                .map(crate::error_ui::capture),
        });
        self.chapter_view = Some((self.document_revision, view.clone()));
        view
    }
    pub(super) fn dirty(&self) -> bool {
        self.document_dirty
            || self.asset_dirty
            || self.content_panel.has_pending()
            || self.advanced.has_pending()
    }
    pub(super) fn mark_saved(&mut self) {
        self.saved = self.last.clone();
        self.document_dirty = false;
    }
    pub(super) fn is_vanilla_patch(&self) -> bool {
        self.project
            .stories
            .get(&self.current)
            .is_some_and(|story| story.get("vanilla_patch").is_some())
    }
    pub(super) fn story(&self) -> &Value {
        static EMPTY: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({"nodes":[]}));
        self.project.stories.get(&self.current).unwrap_or(&EMPTY)
    }
    pub(super) fn flush(&mut self) {
        if let Some(s) = self.pending_history.take() {
            self.undo.push(s);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
    }
    pub(super) fn track(&mut self) {
        // Asset editors report actual mutations. Comparing every image/audio byte
        // on every UI frame makes even an unchanged large project stall.
        let assets_changed = (std::mem::take(&mut self.asset_dirty)
            || self.last.assets.len() != self.project.assets.len())
            && *self.last.assets != self.project.assets;
        let stories_changed = changed_stories(&self.last.stories, &mut self.project.stories);
        if self.last.manifest != self.project.manifest
            || self.last.scenario != self.project.scenario
            || stories_changed.is_some()
            || self.last.paths != self.project.paths
            || assets_changed
        {
            self.document_revision = self.document_revision.wrapping_add(1);
            let current = Snapshot {
                manifest: self.project.manifest.clone(),
                scenario: self.project.scenario.clone(),
                stories: stories_changed.unwrap_or_else(|| self.last.stories.clone()),
                paths: self.project.paths.clone(),
                assets: if assets_changed {
                    std::sync::Arc::new(self.project.assets.clone())
                } else {
                    self.last.assets.clone()
                },
            };
            self.auto = false;
            self.preview_elapsed = None;
            if current.assets != self.last.assets {
                self.catalog.sync_assets(&self.project.assets);
            }
            self.catalog.sync_story_titles(&self.project.stories);
            if self.pending_history.is_none() {
                self.pending_history = Some(self.last.clone());
                self.redo.clear();
            }
            self.last = current;
            self.document_dirty = self.last != self.saved;
            self.last_edit = Instant::now();
            self.needs_compile = true;
        }
        if self.last_edit.elapsed() > Duration::from_millis(650) {
            self.flush();
        }
    }
    pub(super) fn restore(&mut self, s: Snapshot) {
        self.document_revision = self.document_revision.wrapping_add(1);
        self.preview.cancel_layout_drag();
        self.content_panel.reset();
        self.advanced.reset();
        self.project
            .stories
            .retain(|id, _| s.stories.contains_key(id));
        for (id, story) in &s.stories {
            if self
                .last
                .stories
                .get(id)
                .is_none_or(|previous| !std::sync::Arc::ptr_eq(previous, story))
            {
                self.project.stories.insert(id.clone(), story.to_value());
            }
        }
        self.project.manifest = s.manifest.clone();
        self.project.scenario = s.scenario.clone();
        self.project.paths = s.paths.clone();
        if !std::sync::Arc::ptr_eq(&self.last.assets, &s.assets) {
            self.project.assets = (*s.assets).clone();
            self.catalog.sync_assets(&self.project.assets);
        }
        self.catalog.sync_story_titles(&self.project.stories);
        self.asset_dirty = false;
        if !self.project.stories.contains_key(&self.current) {
            self.current = self
                .project
                .stories
                .keys()
                .next()
                .cloned()
                .unwrap_or_default();
        }
        self.selected = self.selected.min(
            self.project
                .stories
                .get(&self.current)
                .and_then(|story| story["nodes"].as_array())
                .map(Vec::len)
                .unwrap_or(1)
                .saturating_sub(1),
        );
        self.last = s;
        self.document_dirty = self.last != self.saved;
        self.pending_history = None;
        self.needs_compile = true;
        self.multiselect.clear();
    }
    pub(super) fn undo(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.track();
        self.flush();
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.last.clone());
            self.restore(s);
            self.status = "已撤销".into();
        }
    }
    pub(super) fn redo(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.track();
        self.flush();
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.last.clone());
            self.restore(s);
            self.status = "已重做".into();
        }
    }
}
