//! Render review examples with the real error component and native egui renderer.
//! Usage: cargo run -p lom-editor --example error_dialogs -- audio out/audio.png
#![allow(dead_code)]
#[path = "../src/error_location.rs"]
mod error_location;
#[path = "../src/error_ui.rs"]
mod error_ui;
#[path = "../src/i18n.rs"]
mod i18n;
#[path = "../src/macos_window.rs"]
mod macos_window;
#[path = "../src/shell.rs"]
mod shell;

use eframe::egui;
use lom_core::{project::Project, release::PreflightIssue};
use std::path::PathBuf;

struct Preview {
    title: String,
    message: String,
    project: Project,
    issues: Vec<PreflightIssue>,
    output: PathBuf,
    frames: usize,
}

impl Preview {
    fn new(kind: &str, output: PathBuf) -> Self {
        let mut project = Project::new();
        let mut issues = Vec::new();
        let (title, message) = match kind {
            "audio" => ("缺少试听组件", "无法试听音频“子彈咻咻_001”：原版音频试听组件缺失：C:/LoM Modkit/audio-decoder/vgmstream-cli.exe。请下载新版完整 Windows 包并完整解压，保留其中的 asset-extractor 和 audio-decoder 文件夹。内置更新不会补装这两个组件。".into()),
            "dynamic" => ("多段音效", "准备音频试听失败（exit code: 1）：此音频事件包含多段或动态混音，目前不能作为单段试听；请在游戏中确认".into()),
            "save" => ("保存失败", error_ui::capture(anyhow::anyhow!("Permission denied (os error 5)").context("C:/我的作品/story/chapter1.json").context("保存失败；修改仍保留在编辑器中。"))),
            "open" => ("包内脚本与章节不一致", "打开失败\nC:/我的作品/测试模组.lommod\n包内 lua/chapter1.lua 不是由对应 story/chapter1.json 编译生成，拒绝导入".into()),
            "validation" => {
                for (id, title, node) in [
                    ("chapter1", "第一章 · 山道初遇", serde_json::json!({"id":"effect1","type":"effect","name":"","_editor":{"note":"山道烟雾"}})),
                    ("chapter2", "第二章 · 唐门夜话", serde_json::json!({"id":"wait1","type":"wait","seconds":"输入错误","_editor":{"note":"对白后等待"}})),
                ] {
                    let story = serde_json::json!({"story_schema":2,"id":id,"title":title,"start":node["id"],"nodes":[node,{"id":"end","type":"end"}]});
                    issues.extend(lom_core::release::compiler_issues(id, &story));
                    project.stories.insert(id.into(), story);
                }
                ("章节校验未通过", format!("项目存在 {} 项检查错误，请先修正以下步骤。", issues.len()))
            }
            _ => panic!("Choose audio, save, open, or validation"),
        };
        Self {
            title: title.into(),
            message,
            project,
            issues,
            output,
            frames: 0,
        }
    }
}

impl eframe::App for Preview {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.frames += 1;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(239, 241, 243)))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("错误窗口示例 · 当前组件实际渲染").size(17.0));
                ui.small(format!(
                    "示例：{}（用于外观评审，不代表当前项目发生故障）",
                    self.title
                ));
            });
        egui::Modal::new(egui::Id::new("operation-error"))
            .frame(shell::dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(460.0);
                shell::dialog_header(ui, &error_ui::title(&self.message));
                egui::ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.5)
                    .show(ui, |ui| {
                        error_ui::show(ui, &self.message);
                        for issue in &self.issues {
                            ui.add_space(10.0);
                            ui.group(|ui| {
                                ui.strong(error_location::issue_label(issue, &self.project));
                                error_ui::show(ui, &issue.message);
                                let target =
                                    error_location::issue_destination(issue, &self.project);
                                let _ = ui.button(target.label());
                            });
                        }
                        error_ui::copy_button(ui, || self.message.clone());
                    });
                ui.add_space(16.0);
                shell::dialog_buttons(ui, |ui| {
                    ui.add_sized([80.0, 30.0], egui::Button::new("关闭"));
                });
            });
        if self.frames == 12 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = screenshot {
            let bytes: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::save_buffer(
                &self.output,
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("Rendered {}", self.output.display());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let kind = args.first().expect("audio/save/open/validation").clone();
    let output = PathBuf::from(args.get(1).expect("output PNG"));
    eframe::run_native(
        "错误窗口示例",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([880.0, 680.0])
                .with_resizable(false),
            ..Default::default()
        },
        Box::new(move |cc| {
            shell::style(&cc.egui_ctx);
            Ok(Box::new(Preview::new(&kind, output)))
        }),
    )
}
