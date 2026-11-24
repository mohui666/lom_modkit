use eframe::egui;
use lom_core::gif_image;
use std::{sync::Arc, time::Duration};

pub(crate) struct Player {
    frames: Vec<(f64, Arc<egui::ColorImage>)>,
    loops: u32,
    started: Option<f64>,
    displayed: usize,
}

impl Player {
    pub(crate) fn new(image: gif_image::Animation) -> Self {
        let mut elapsed = 0.0;
        let frames = image
            .frames
            .into_iter()
            .map(|frame| {
                elapsed += f64::from(gif_image::frame_seconds(&frame));
                let pixels = frame.buffer();
                let size = [pixels.width() as usize, pixels.height() as usize];
                (
                    elapsed,
                    Arc::new(egui::ColorImage::from_rgba_unmultiplied(
                        size,
                        pixels.as_raw(),
                    )),
                )
            })
            .collect();
        Self {
            frames,
            loops: image.loops,
            started: None,
            displayed: 0,
        }
    }

    pub(crate) fn first(&self) -> Arc<egui::ColorImage> {
        self.frames[0].1.clone()
    }

    pub(crate) fn bytes(&self) -> usize {
        self.frames.len() * self.frames[0].1.pixels.len() * 4
    }

    pub(crate) fn update(&mut self, ctx: &egui::Context) -> Option<Arc<egui::ColorImage>> {
        if self.frames.len() == 1 {
            return None;
        }
        let now = ctx.input(|input| input.time);
        let elapsed = (now - *self.started.get_or_insert(now)).max(0.0);
        let last = self.frames.len() - 1;
        let duration = self.frames[last].0;
        let index = if self.loops != 0 && elapsed >= duration * f64::from(self.loops) {
            last
        } else {
            let position = elapsed % duration;
            let index = self.frames.partition_point(|(end, _)| *end <= position);
            ctx.request_repaint_after(Duration::from_secs_f64(self.frames[index].0 - position));
            index
        };
        if index == self.displayed {
            None
        } else {
            self.displayed = index;
            Some(self.frames[index].1.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_preview_updates_at_the_decoded_frame_boundaries_and_loops() {
        let bytes = include_bytes!(
            "../../../samples/showcase3/assets/user/image/showcase.gif_lantern/lantern.gif"
        );
        let mut player = Player::new(gif_image::decode(bytes, None).unwrap());
        let first = player.first();
        let ctx = egui::Context::default();
        let mut updated = None;
        let mut paint_at = |time| {
            let _ = ctx.run(egui::RawInput { time: Some(time), ..Default::default() }, |ctx| {
                updated = player.update(ctx);
            });
            updated.take()
        };
        assert!(paint_at(1.0).is_none());
        assert!(paint_at(1.1).is_none());
        assert_ne!(paint_at(1.13).unwrap().pixels, first.pixels);
        assert_eq!(paint_at(1.56).unwrap().pixels, first.pixels);
    }
}
