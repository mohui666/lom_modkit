//! Keep audio on the visual preview clock, including preparation and pause.
use super::*;

impl App {
    pub(super) fn stop_preview(&mut self) {
        self.auto = false;
        self.preview_waiting_branch = false;
        self.preview_elapsed = None;
        self.preview_audio.stop();
        self.preview_buffering = None;
    }

    pub(super) fn tick_preview_audio(&mut self) -> bool {
        if !self.preview_sound
            || !matches!(self.center, Center::Node | Center::Chapter)
            || self.view != View::Stage
            || self.is_vanilla_patch()
        {
            self.preview_audio.stop();
            self.preview_buffering = None;
            return false;
        }
        if self.auto {
            let story = self.story();
            let id = story["nodes"][self.selected]["id"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            if self.preview_audio.location().is_none() {
                self.audio.stop();
                self.preview_audio
                    .begin(&self.current, &story, self.selected);
            } else if self.preview_audio.location() != Some(&(self.current.clone(), id)) {
                self.preview_audio
                    .enter(&self.current, &story["nodes"][self.selected]);
            }
            let successors = lom_core::analysis::successors(&story, self.selected);
            if successors.len() == 1 {
                if let Some(next) = story["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|node| node["id"] == successors[0])
                {
                    self.preview_audio.prefetch(next);
                }
            }
        }
        self.preview_audio
            .update(&self.project, &self.tools_panel.game_directory(), self.auto);
        if self.auto && self.preview_audio.buffering() {
            let elapsed = *self
                .preview_buffering
                .get_or_insert_with(|| self.last_step.elapsed().as_secs_f64());
            self.last_step = Instant::now() - Duration::from_secs_f64(elapsed);
            self.preview_elapsed = Some(elapsed);
            true
        } else {
            if let Some(elapsed) = self.preview_buffering.take() {
                if self.auto {
                    self.last_step = Instant::now() - Duration::from_secs_f64(elapsed);
                }
            }
            false
        }
    }
}
