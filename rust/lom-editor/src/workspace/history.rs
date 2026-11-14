//! Document snapshots, dirty state and undo/redo.
use super::*;
use crate::json::same as same_json;
use std::sync::Arc;

// Share small node blocks between edits. Contiguous values keep large-chapter
// comparisons fast without copying whole chapters into each undo entry.
const SNAPSHOT_BLOCK: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StorySnapshot {
    fields: Value,
    nodes: Option<Arc<Vec<Arc<Vec<Value>>>>>,
}

impl StorySnapshot {
    pub(super) fn capture(story: &Value, previous: Option<&Self>) -> Self {
        let Some(nodes) = story.get("nodes").and_then(Value::as_array) else {
            return Self {
                fields: story.clone(),
                nodes: None,
            };
        };
        let fields = Value::Object(
            story
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if key == "nodes" {
                            Value::Null
                        } else {
                            value.clone()
                        },
                    )
                })
                .collect(),
        );
        let old = previous.and_then(|old| old.nodes.as_ref());
        let next: Vec<_> = nodes
            .chunks(SNAPSHOT_BLOCK)
            .enumerate()
            .map(|(index, block)| {
                old.and_then(|nodes| nodes.get(index))
                    .filter(|old| {
                        old.len() == block.len()
                            && old.iter().zip(block).all(|(a, b)| same_json(a, b))
                    })
                    .cloned()
                    .unwrap_or_else(|| Arc::new(block.to_vec()))
            })
            .collect();
        let nodes = if let Some(old) = old.filter(|old| {
            old.len() == next.len() && old.iter().zip(&next).all(|(a, b)| Arc::ptr_eq(a, b))
        }) {
            old.clone()
        } else {
            Arc::new(next)
        };
        Self {
            fields,
            nodes: Some(nodes),
        }
    }

    fn same(&self, story: &Value) -> bool {
        let Some(nodes) = &self.nodes else {
            return same_json(&self.fields, story);
        };
        let Some(current) = story.get("nodes").and_then(Value::as_array) else {
            return false;
        };
        let Some(fields) = story.as_object() else {
            return false;
        };
        fields.len() == self.fields.as_object().unwrap().len()
            && self.fields.as_object().unwrap().iter().all(|(key, value)| {
                key == "nodes"
                    || fields
                        .get(key)
                        .is_some_and(|current| same_json(value, current))
            })
            && nodes.len() == current.len().div_ceil(SNAPSHOT_BLOCK)
            && nodes
                .iter()
                .zip(current.chunks(SNAPSHOT_BLOCK))
                .all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_json(a, b)))
    }

    pub(super) fn to_value(&self) -> Value {
        let mut story = self.fields.clone();
        if let Some(nodes) = &self.nodes {
            story["nodes"] = Value::Array(
                nodes
                    .iter()
                    .flat_map(|block| block.iter().cloned())
                    .collect(),
            );
        }
        story
    }
}

impl crate::persistence::StoryBaseline for Arc<StorySnapshot> {
    fn saved_story(&self) -> std::borrow::Cow<'_, Value> {
        std::borrow::Cow::Owned(self.to_value())
    }
}

fn changed_stories(
    left: &BTreeMap<String, Arc<StorySnapshot>>,
    right: &BTreeMap<String, Value>,
) -> Option<BTreeMap<String, Arc<StorySnapshot>>> {
    let mut next = (left.len() != right.len()).then(|| left.clone());
    for (id, story) in right {
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
    pub(super) fn story(&self) -> Value {
        self.project
            .stories
            .get(&self.current)
            .cloned()
            .unwrap_or(json!({"nodes":[]}))
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
        let assets_changed =
            std::mem::take(&mut self.asset_dirty) && *self.last.assets != self.project.assets;
        let stories_changed = changed_stories(&self.last.stories, &self.project.stories);
        if self.last.manifest != self.project.manifest
            || self.last.scenario != self.project.scenario
            || stories_changed.is_some()
            || self.last.paths != self.project.paths
            || assets_changed
        {
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
