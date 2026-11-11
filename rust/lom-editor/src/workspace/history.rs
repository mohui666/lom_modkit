//! Document snapshots, dirty state and undo/redo.
use super::*;

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
        let assets_changed = *self.last.assets != self.project.assets;
        if self.last.manifest != self.project.manifest
            || self.last.scenario != self.project.scenario
            || self.last.stories != self.project.stories
            || self.last.paths != self.project.paths
            || assets_changed
        {
            let current = Snapshot {
                manifest: self.project.manifest.clone(),
                scenario: self.project.scenario.clone(),
                stories: self.project.stories.clone(),
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
        self.project.stories = s.stories;
        self.project.manifest = s.manifest;
        self.project.scenario = s.scenario;
        self.project.paths = s.paths;
        self.project.assets = (*s.assets).clone();
        self.catalog.sync_assets(&self.project.assets);
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
            self.story()["nodes"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(1)
                .saturating_sub(1),
        );
        self.last = snapshot(&self.project);
        self.document_dirty = self.last != self.saved;
        self.pending_history = None;
        self.needs_compile = true;
        self.multiselect.clear();
    }
    pub(super) fn undo(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        if let Some(s) = self.undo.pop() {
            self.redo.push(snapshot(&self.project));
            self.restore(s);
            self.status = "已撤销".into();
        }
    }
    pub(super) fn redo(&mut self) {
        if !self.apply_pending_drafts() {
            return;
        }
        self.flush();
        if let Some(s) = self.redo.pop() {
            self.undo.push(snapshot(&self.project));
            self.restore(s);
            self.status = "已重做".into();
        }
    }
}
