//! Export the existing stage painter in one hidden graphics session per batch.
use anyhow::{ensure, Context, Result};
use eframe::egui::{self, Color32};
use lom_core::project::Project;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

struct Shot {
    script: String,
    node: String,
    index: usize,
    time: Option<f64>,
    output: PathBuf,
}
struct Renderer {
    project: Project,
    preview: crate::preview::Preview,
    shots: Vec<Shot>,
    at: usize,
    story_revision: u64,
    settle: usize,
    waiting: bool,
    ticks: usize,
    width: u32,
    height: u32,
    frames: Vec<Value>,
    report: Value,
    batch: Option<PathBuf>,
    result: Arc<Mutex<Option<std::result::Result<Value, String>>>>,
}

pub(crate) fn render(command: &str, request: &Value, base: &Path) -> Result<Value> {
    let path = |key: &str| -> Result<PathBuf> {
        Ok(base.join(
            request[key]
                .as_str()
                .with_context(|| format!("{key} 必须是路径字符串"))?,
        ))
    };
    let project = Project::open(&path("project")?)?;
    let size = |key: &str, default| -> Result<u32> {
        let value = request
            .get(key)
            .map_or(Some(default), Value::as_u64)
            .with_context(|| format!("{key} 必须是正整数"))?;
        ensure!(value <= 4096, "舞台导出尺寸不能超过 4096 像素");
        Ok(value as u32)
    };
    let width = size("width", 1280)?;
    let height = size("height", 720)?;
    ensure!(
        width >= 320 && height >= 180 && width * 9 == height * 16,
        "舞台出图须为 16:9，最小 320×180"
    );
    let output = path("output")?;
    let batch = (command == "render-previews").then(|| output.clone());
    let inputs = if batch.is_some() {
        request["frames"]
            .as_array()
            .context("frames 必须是数组")?
            .clone()
    } else {
        vec![request.clone()]
    };
    ensure!(
        !inputs.is_empty() && inputs.len() <= 200,
        "一次舞台导出需要 1～200 个关键帧"
    );
    let mut shots = Vec::new();
    for (i, item) in inputs.iter().enumerate() {
        if batch.is_some() {
            for key in item.as_object().context("关键帧必须是对象")?.keys() {
                ensure!(
                    ["script", "node", "time"].contains(&key.as_str()),
                    "关键帧不支持字段 {key}"
                );
            }
        }
        let script = item["script"]
            .as_str()
            .context("关键帧缺少 script")?
            .to_owned();
        let node = item["node"].as_str().context("关键帧缺少 node")?.to_owned();
        let story = project
            .stories
            .get(&script)
            .with_context(|| format!("章节不存在：{script}"))?;
        let index = story["nodes"]
            .as_array()
            .context("章节缺少 nodes")?
            .iter()
            .position(|n| n["id"] == node)
            .with_context(|| format!("步骤不存在：{script}/{node}"))?;
        ensure!(
            crate::preview::simulate(story, &node)["reached"] == true,
            "步骤不可达：{script}/{node}"
        );
        let time = item
            .get("time")
            .map(|t| t.as_f64().context("time 必须是秒数"))
            .transpose()?;
        ensure!(
            time.is_none_or(|t| t.is_finite() && t >= 0.0),
            "time 必须是非负有限秒数"
        );
        let file = if batch.is_some() {
            output.join(format!("{:04}.png", i + 1))
        } else {
            output.clone()
        };
        ensure!(
            file.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png")),
            "舞台图片需要 .png 扩展名"
        );
        ensure!(!file.exists(), "舞台图片已存在：{}", file.display());
        shots.push(Shot {
            script,
            node,
            index,
            time,
            output: file,
        });
    }
    if batch.is_some() {
        ensure!(
            !output.exists() || output.is_dir() && std::fs::read_dir(&output)?.next().is_none(),
            "批量舞台输出目录必须为空"
        );
    }
    let mut preview = crate::preview::Preview::new(Path::new(""));
    if request.get("library").is_some() {
        preview.set_export_library(&path("library")?)?;
    } else {
        let library = crate::asset_query::resolve_library(request, base)?;
        if library.join("preview_map.json").is_file() {
            preview.set_export_library(&library)?;
        }
    }
    preview.stage_only = true;
    preview.resolution = [width, height];
    let report = crate::preview_report::assessment(&project, request, base)?;
    if let Some(directory) = &batch {
        std::fs::create_dir_all(directory)?;
    } else if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let result = Arc::new(Mutex::new(None));
    let capture = result.clone();
    eframe::run_native(
        "舞台图片导出",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([width as f32, height as f32])
                .with_visible(false)
                .with_active(false)
                .with_decorations(false)
                .with_resizable(false)
                .with_clamp_size_to_monitor_size(false),
            persist_window: false,
            vsync: false,
            ..Default::default()
        },
        Box::new(move |cc| {
            crate::shell::style(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.0);
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    width as f32,
                    height as f32,
                )));
            Ok(Box::new(Renderer {
                project,
                preview,
                shots,
                at: 0,
                story_revision: 0,
                settle: 0,
                waiting: false,
                ticks: 0,
                width,
                height,
                frames: Vec::new(),
                report,
                batch,
                result: capture,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("舞台图形渲染失败：{e}"))?;
    let value = result
        .lock()
        .unwrap()
        .take()
        .context("舞台渲染在返回图片前关闭")?
        .map_err(anyhow::Error::msg)?;
    Ok(value)
}

impl Renderer {
    fn finish(&self, ctx: &egui::Context, result: Result<Value>) {
        *self.result.lock().unwrap() = Some(result.map_err(|e| format!("{e:#}")));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    fn save(&mut self, image: &egui::ColorImage) -> Result<()> {
        ensure!(
            image.size == [self.width as usize, self.height as usize],
            "舞台实际像素 {:?} 与请求 {}×{} 不一致",
            image.size,
            self.width,
            self.height
        );
        let pixels = image
            .pixels
            .iter()
            .flat_map(|p| p.to_array())
            .collect::<Vec<_>>();
        let shot = &self.shots[self.at];
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(self.width, self.height, pixels)
                .context("舞台像素数量不正确")?,
        )
        .write_to(&mut png, image::ImageFormat::Png)?;
        lom_core::project::atomic_write(&shot.output, &png.into_inner())?;
        let missing_effects = self.preview.stage_snapshot["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| {
                matches!(
                    item["cache_status"].as_str(),
                    Some("missing" | "missing_tail" | "truncated" | "tail_truncated")
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let has_effects = self.preview.stage_snapshot["effects"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
        let effect_error = self.preview.stage_effect_error().filter(|_| has_effects);
        self.frames.push(json!({"script":shot.script,"node":shot.node,"time":shot.time,"output":shot.output,
            "width":self.width,"height":self.height,"rendered":true,"visual_review_performed":false,
            "complete":self.preview.missing_images.is_empty() && missing_effects.is_empty() && effect_error.is_none(),"missing_images":self.preview.missing_images,
            "missing_effect_previews":missing_effects,"effect_error":effect_error,
            "state":self.preview.stage_snapshot}));
        Ok(())
    }
    fn complete(&self) -> Result<Value> {
        let images_complete = self.frames.iter().all(|frame| frame["complete"] == true);
        let mut assessment = self.report.clone();
        assessment["rendering_performed"] = json!(true);
        assessment["rendered_images"] = json!(self.frames.len());
        assessment["image_assets_complete"] = json!(images_complete);
        let mut result = json!({"ok":true,"renderer":"editor_stage","rendering_performed":true,
            "output":self.batch.as_deref().unwrap_or(&self.shots[0].output),"image_assets_complete":images_complete,
            "rendered_images":self.frames.len(),"visual_review_performed":false,"game_tested":false,
            "frames":self.frames,"assessment":assessment});
        if let Some(directory) = &self.batch {
            let columns = self.frames.len().min(3) as u32;
            let rows = (self.frames.len() as u32).div_ceil(columns);
            let mut sheet = image::RgbaImage::from_pixel(
                columns * 384,
                rows * 216,
                image::Rgba([20, 24, 30, 255]),
            );
            for (i, shot) in self.shots.iter().enumerate() {
                let thumb = image::open(&shot.output)?.thumbnail(384, 216).into_rgba8();
                image::imageops::overlay(
                    &mut sheet,
                    &thumb,
                    (i as u32 % columns * 384) as i64,
                    (i as u32 / columns * 216) as i64,
                );
            }
            let path = directory.join("contact-sheet.png");
            sheet.save(&path)?;
            result["contact_sheet"] = json!(path);
            result["contact_sheet_order"] = json!("row_major_request_order");
            let manifest = directory.join("frames.json");
            lom_core::project::atomic_write(&manifest, &serde_json::to_vec_pretty(&result)?)?;
            result["manifest"] = json!(manifest);
        }
        Ok(result)
    }
}
impl eframe::App for Renderer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.result.lock().unwrap().is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.ticks += 1;
        if self.ticks > self.shots.len() * 20 + 60 {
            self.finish(ctx, Err(anyhow::anyhow!("图形后端未返回舞台图片")));
            return;
        }
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot.filter(|_| self.waiting) {
            if let Err(e) = self.save(&image) {
                self.finish(ctx, Err(e));
                return;
            }
            let previous = self.at;
            self.at += 1;
            if self.at < self.shots.len()
                && self.shots[self.at].script != self.shots[previous].script
            {
                self.story_revision += 1;
            }
            self.settle = 0;
            self.waiting = false;
            if self.at == self.shots.len() {
                self.finish(ctx, self.complete());
                return;
            }
        }
        self.preview.missing_images.clear();
        let shot = &self.shots[self.at];
        self.preview.stage_time = shot.time;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.preview.show_versioned(
                    ui,
                    &self.project,
                    &self.project.stories[&shot.script],
                    shot.index,
                    shot.time,
                    self.story_revision,
                );
            });
        self.settle += 1;
        if self.settle >= 3 && !self.waiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.waiting = true;
        }
        ctx.request_repaint();
    }
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::BLACK.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_batch_fails_before_creating_outputs() {
        let tmp = tempfile::tempdir().unwrap();
        Project::new().save_to(&tmp.path().join("p")).unwrap();
        let error = render(
            "render-previews",
            &json!({"project":"p","frames":[{"script":"main","node":"missing"}],"output":"images"}),
            tmp.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("步骤不存在"));
        assert!(!tmp.path().join("images").exists());
    }
}
