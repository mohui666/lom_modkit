//! Offline help: one article at a time, with the same field names as the editor.
use crate::i18n::{self, tr};
use eframe::egui::{self, Color32, RichText};
use serde_json::Value;
use std::sync::OnceLock;

const BLUE: Color32 = Color32::from_rgb(41, 101, 158);
const INK: Color32 = Color32::from_rgb(45, 51, 59);

#[derive(Debug)]
enum Block {
    Paragraph(String),
    Heading(String),
    List { numbered: bool, items: Vec<String> },
    Note(String),
    Example(String),
}
#[derive(Debug)]
struct Article {
    id: String,
    title: String,
    blocks: Vec<Block>,
    search_text: String,
}

// These are our own small, embedded HTML documents, not arbitrary web pages.
// Keep the source readable in a browser as well as in the native reader.
fn plain(html: &str) -> String {
    static TAGS: OnceLock<regex::Regex> = OnceLock::new();
    TAGS.get_or_init(|| regex::Regex::new(r"<[^>]+>").unwrap())
        .replace_all(html, "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .trim()
        .to_owned()
}
fn parse(html: &str) -> Vec<Article> {
    let headings = regex::Regex::new(r#"(?s)<h2 id="([^"]+)">(.*?)</h2>"#).unwrap();
    let blocks =
        regex::Regex::new(r"(?s)<(p|h3|ol|ul|aside|pre)>(.*?)</(?:p|h3|ol|ul|aside|pre)>").unwrap();
    let items = regex::Regex::new(r"(?s)<li>(.*?)</li>").unwrap();
    let sections: Vec<_> = headings.captures_iter(html).collect();
    sections
        .iter()
        .enumerate()
        .map(|(index, heading)| {
            let start = heading.get(0).unwrap().end();
            let end = sections
                .get(index + 1)
                .map_or(html.len(), |h| h.get(0).unwrap().start());
            let source = &html[start..end];
            Article {
                id: heading[1].into(),
                title: plain(&heading[2]),
                search_text: plain(&format!("{} {source}", &heading[2])).to_lowercase(),
                blocks: blocks
                    .captures_iter(source)
                    .map(|c| match &c[1] {
                        "h3" => Block::Heading(plain(&c[2])),
                        "ol" | "ul" => Block::List {
                            numbered: &c[1] == "ol",
                            items: items.captures_iter(&c[2]).map(|i| plain(&i[1])).collect(),
                        },
                        "aside" => Block::Note(plain(&c[2])),
                        "pre" => Block::Example(plain(&c[2])),
                        _ => Block::Paragraph(plain(&c[2])),
                    })
                    .collect(),
            }
        })
        .collect()
}
fn articles() -> &'static [Article] {
    static GUIDES: OnceLock<[Vec<Article>; 4]> = OnceLock::new();
    let guides = GUIDES.get_or_init(|| i18n::LOCALES.map(|locale| parse(i18n::help_html(locale))));
    &guides[i18n::LOCALES
        .iter()
        .position(|l| *l == i18n::locale())
        .unwrap_or(0)]
}
fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(include_str!("../data/authoring.json")).unwrap())
}
fn reference() -> &'static Value {
    static REFERENCE: OnceLock<Value> = OnceLock::new();
    REFERENCE.get_or_init(|| serde_json::from_str(include_str!("../data/reference.json")).unwrap())
}
fn field_label(kind: &str, field: &Value) -> String {
    let specific = format!("field.{kind}.{}", field[0].as_str().unwrap_or(""));
    let generic = format!("field.{}", field[0].as_str().unwrap_or(""));
    // Match the labels used by Catalog::field, including existing Chinese overrides.
    if i18n::locale() == "chs" {
        return field[1].as_str().unwrap_or("").into();
    }
    for key in [specific, generic] {
        let translated = i18n::key(&key);
        if translated != key {
            return translated;
        }
    }
    tr(field[1].as_str().unwrap_or(""))
}
fn field_hint(kind: &str, field: &Value) -> String {
    for key in [
        format!("field_help.{kind}.{}", field[0].as_str().unwrap_or("")),
        format!("field_help.{}", field[2].as_str().unwrap_or("")),
    ] {
        let text = i18n::key(&key);
        if text != key {
            return text;
        }
    }
    tr(match field[2].as_str().unwrap_or("") {
        "float" | "int" => "填写数值；时长的单位是秒。",
        "bool" => "选择启用或关闭；默认会沿用预设。",
        "text" => "填写要显示的文字。",
        "node_ref" => "选择当前章节中的后续步骤。",
        "story_ref" | "script" => "选择作品中的章节。",
        "actions" => "添加两个或更多演出动作，分别填写人物与时长。",
        "string" => "按字段名称填写；有预设时可以保留原值。",
        _ => "在表单中选择或填写对应内容。",
    })
}
fn node_matches(kind: &str, query: &str) -> bool {
    query.is_empty()
        || format!(
            "{kind} {} {} {}",
            i18n::key(&format!("node.{kind}")),
            i18n::key(&format!("help.{kind}")),
            schema()["NODE_SCHEMAS"][kind]["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|f| format!("{} {}", field_label(kind, f), field_hint(kind, f)))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase()
        .contains(query)
}
fn related(kind: &str) -> Option<&'static str> {
    match kind {
        "parallel" => Some("parallel"),
        "transition" => Some("transition"),
        "say" | "show" | "portrait" | "mood" | "move" | "hide" | "face" | "offset" => {
            Some("characters")
        }
        "scene" | "background" | "custom_cg" | "overlay" => Some("assets"),
        "choice" | "branch" | "end" | "flag" | "goto_scene" => Some("flow"),
        _ => None,
    }
}

#[derive(Default)]
pub struct HelpDocument {
    search: String,
    page: usize,
    guide: String,
    node: String,
    reset_scroll: bool,
}
impl HelpDocument {
    pub fn open_guide(&mut self) {
        self.search.clear();
        self.page = 0;
        self.guide = "start".into();
        if self.node.is_empty() {
            self.node = "say".into();
        }
        self.reset_scroll = true;
    }
    pub fn open_node(&mut self, kind: &str) {
        self.search.clear();
        self.page = 1;
        self.node = kind.into();
        self.reset_scroll = true;
    }
    /// Returns true when the reader closes. This component never modifies a project.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("help-document"))
            .frame(crate::shell::dialog_frame())
            .show(ctx, |ui| {
                let size = ctx.content_rect().size();
                ui.set_width((size.x - 64.0).clamp(280.0, 1010.0));
                close |= crate::shell::dialog_header(ui, &i18n::key("app.help_title"));
                ui.label(
                    RichText::new(tr(
                        "从第一段对白开始，按你想做的事查找。点步骤旁的“说明”可直达对应页面。",
                    ))
                    .size(13.0)
                    .weak(),
                );
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_value(&mut self.page, 0, i18n::key("help.guide_tab"))
                        .changed()
                        | ui.selectable_value(&mut self.page, 1, i18n::key("help.reference_tab"))
                            .changed()
                    {
                        self.reset_scroll = true;
                    }
                });
                ui.horizontal(|ui| {
                    let search_width = (ui.available_width() - 92.0).max(80.0);
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text(tr("搜索帮助，例如：同时、黑幕、结束剧情"))
                                .desired_width(search_width),
                        )
                        .changed()
                    {
                        self.reset_scroll = true;
                    }
                    if ui
                        .add_enabled(!self.search.is_empty(), egui::Button::new(tr("清空搜索")))
                        .clicked()
                    {
                        self.search.clear();
                        self.reset_scroll = true;
                    }
                });
                ui.add_space(4.0);
                let query = self.search.trim().to_lowercase();
                let guide_matches: Vec<_> = articles()
                    .iter()
                    .filter(|a| query.is_empty() || a.search_text.contains(&query))
                    .collect();
                let node_matches: Vec<_> = schema()["NODE_SCHEMAS"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|k| node_matches(k, &query))
                    .collect();
                if self.page == 0 && !guide_matches.iter().any(|a| a.id == self.guide) {
                    self.guide = guide_matches
                        .first()
                        .map(|a| a.id.clone())
                        .unwrap_or_default();
                }
                if self.page == 1 && !node_matches.iter().any(|k| **k == self.node) {
                    self.node = node_matches
                        .first()
                        .map(|k| (*k).clone())
                        .unwrap_or_default();
                }
                let height = (size.y - 220.0).clamp(220.0, 580.0);
                let reset_scroll = std::mem::take(&mut self.reset_scroll);
                let width = ui.available_width();
                let nav_width = (width * 0.23).clamp(160.0, 218.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(nav_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(nav_width);
                            let count = if self.page == 0 {
                                guide_matches.len()
                            } else {
                                node_matches.len()
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{} · {count}",
                                    tr(if query.is_empty() {
                                        "目录"
                                    } else {
                                        "搜索结果"
                                    })
                                ))
                                .small()
                                .weak(),
                            );
                            egui::ScrollArea::vertical()
                                .id_salt("help-directory")
                                .max_height(height - 30.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(nav_width - 14.0);
                                    ui.spacing_mut().item_spacing.y = 4.0;
                                    if self.page == 0 {
                                        for article in &guide_matches {
                                            if nav_button(
                                                ui,
                                                self.guide == article.id,
                                                &article.title,
                                                reset_scroll,
                                            ) {
                                                self.guide.clone_from(&article.id);
                                                self.reset_scroll = true;
                                            }
                                        }
                                    } else {
                                        for group in schema()["NODE_GROUPS_SRC"].as_array().unwrap()
                                        {
                                            let kinds: Vec<_> = group[1]
                                                .as_array()
                                                .unwrap()
                                                .iter()
                                                .filter_map(Value::as_str)
                                                .filter(|kind| {
                                                    node_matches.iter().any(|k| k.as_str() == *kind)
                                                })
                                                .collect();
                                            if kinds.is_empty() {
                                                continue;
                                            }
                                            ui.add_space(5.0);
                                            ui.label(
                                                RichText::new(i18n::key(
                                                    group[0].as_str().unwrap(),
                                                ))
                                                .small()
                                                .weak(),
                                            );
                                            for kind in kinds {
                                                if nav_button(
                                                    ui,
                                                    self.node == kind,
                                                    &i18n::key(&format!("node.{kind}")),
                                                    reset_scroll,
                                                ) {
                                                    self.node = kind.into();
                                                    self.reset_scroll = true;
                                                }
                                            }
                                        }
                                    }
                                    if count == 0 {
                                        ui.label(tr("没有找到相关内容。"));
                                    }
                                });
                        },
                    );
                    let article_width = (width - nav_width - 12.0).max(100.0);
                    egui::Frame::new()
                        .fill(Color32::WHITE)
                        .corner_radius(8.0)
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(227, 231, 234)))
                        .inner_margin(20.0)
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width((article_width - 40.0).max(80.0));
                                ui.style_mut().text_styles.insert(
                                    egui::TextStyle::Body,
                                    egui::FontId::proportional(15.0),
                                );
                                ui.visuals_mut().override_text_color = Some(INK);
                                ui.spacing_mut().item_spacing.y = 9.0;
                                let mut scroll = egui::ScrollArea::vertical()
                                    .id_salt((
                                        "help-article",
                                        self.page,
                                        &self.guide,
                                        &self.node,
                                        i18n::locale(),
                                    ))
                                    .max_height(height - 40.0)
                                    .auto_shrink([false, false]);
                                if reset_scroll || self.reset_scroll {
                                    scroll = scroll.vertical_scroll_offset(0.0);
                                }
                                scroll.show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if self.page == 0 {
                                        if let Some(article) =
                                            articles().iter().find(|a| a.id == self.guide)
                                        {
                                            article_header(
                                                ui,
                                                &i18n::key("help.guide_tab"),
                                                &article.title,
                                            );
                                            render_blocks(ui, &article.blocks);
                                        } else {
                                            empty(ui);
                                        }
                                    } else if !self.node.is_empty() {
                                        self.show_node(ui);
                                    } else {
                                        empty(ui);
                                    }
                                });
                            });
                        });
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr("F1 打开帮助 · Esc 关闭 · 文档可离线阅读"))
                            .small()
                            .weak(),
                    );
                    crate::shell::dialog_buttons(ui, |ui| {
                        if ui.button(i18n::key("help.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        close || response.should_close()
    }
    fn show_node(&mut self, ui: &mut egui::Ui) {
        let kind = self.node.clone();
        article_header(
            ui,
            &i18n::key("help.reference_tab"),
            &i18n::key(&format!("node.{kind}")),
        );
        ui.label(i18n::key(&format!("help.{kind}")));
        if kind == "end" {
            note(
                ui,
                &tr("结束后回到哪里？“下一章节”留空会回到自由模式；选了章节就会接着播放该章。"),
            );
        } else if kind == "parallel" {
            note(
                ui,
                &tr(
                    "示例：加入两个人物退场动作，各选一个人物，时长都填 0.5 秒；两个人会一起淡出。",
                ),
            );
        } else if kind == "transition" {
            note(ui, &tr("这里的淡入是让黑幕出现；淡出是揭开黑幕。要让换景后的人物已经就位，先盖黑幕，再换背景和放人物，最后揭幕。"));
        }
        subheading(ui, &tr("表单怎么填"));
        let fields = schema()["NODE_SCHEMAS"][&kind]["fields"]
            .as_array()
            .unwrap();
        if fields.is_empty() {
            ui.label(tr("这个步骤无需填写额外内容，添加后即可使用。"));
        }
        egui::Grid::new(("help-fields", &kind))
            .num_columns(2)
            .spacing(egui::vec2(18.0, 12.0))
            .striped(true)
            .show(ui, |ui| {
                for field in fields {
                    ui.add_sized(
                        [125.0, 0.0],
                        egui::Label::new(RichText::new(field_label(&kind, field)).strong()).wrap(),
                    );
                    ui.add_sized(
                        [(ui.available_width() - 8.0).max(80.0), 0.0],
                        egui::Label::new(field_hint(&kind, field)).wrap(),
                    );
                    ui.end_row();
                }
            });
        if let Some(article) = related(&kind).and_then(|id| articles().iter().find(|a| a.id == id))
        {
            subheading(ui, &tr("跟着示例做"));
            if ui.link(&article.title).clicked() {
                self.page = 0;
                self.guide.clone_from(&article.id);
                self.search.clear();
                self.reset_scroll = true;
                ui.ctx().request_repaint();
            }
        }
        ui.add_space(14.0);
        ui.separator();
        ui.collapsing(tr("技术参考（编写脚本时使用）"), |ui| {
            ui.label(tr("表单创作可以跳过这一节。"));
            ui.monospace(&kind);
            ui.monospace(reference()["RUNTIME_API"][&kind].as_str().unwrap_or(""));
            ui.add(
                egui::Label::new(
                    RichText::new(
                        serde_json::to_string_pretty(&schema()["NODE_SCHEMAS"][&kind]).unwrap(),
                    )
                    .monospace(),
                )
                .wrap(),
            );
        });
    }
}
fn nav_button(ui: &mut egui::Ui, selected: bool, title: &str, scroll_to_selected: bool) -> bool {
    let response = ui.add(
        egui::Button::new(RichText::new(title).size(14.0))
            .selected(selected)
            .frame(selected)
            .wrap()
            .min_size(egui::vec2(ui.available_width(), 29.0)),
    );
    if selected && scroll_to_selected {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    response.clicked()
}
fn article_header(ui: &mut egui::Ui, section: &str, title: &str) {
    ui.label(RichText::new(section).size(12.0).color(BLUE));
    ui.label(RichText::new(title).size(25.0).strong());
    ui.add_space(5.0);
}
fn subheading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(9.0);
    ui.label(RichText::new(title).size(17.0).strong());
}
fn note(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(237, 244, 249))
        .corner_radius(5.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width((ui.available_width()).max(50.0));
            ui.label(text);
        });
}
fn empty(ui: &mut egui::Ui) {
    subheading(ui, &tr("没有找到相关内容。"));
    ui.label(tr(
        "试试更短的词，或切换“使用指南”和“步骤查询”。清空搜索可查看全部目录。",
    ));
}
fn render_blocks(ui: &mut egui::Ui, blocks: &[Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(text) => {
                ui.label(text);
            }
            Block::Heading(text) => subheading(ui, text),
            Block::Note(text) => note(ui, text),
            Block::Example(text) => {
                egui::Frame::new()
                    .fill(Color32::from_rgb(247, 248, 249))
                    .corner_radius(5.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width().max(50.0));
                        ui.label(text);
                    });
            }
            Block::List { numbered, items } => {
                for (index, text) in items.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        ui.add_sized(
                            [22.0, 0.0],
                            egui::Label::new(
                                RichText::new(if *numbered {
                                    format!("{:02}", index + 1)
                                } else {
                                    "·".into()
                                })
                                .size(14.0)
                                .color(BLUE)
                                .strong(),
                            ),
                        );
                        ui.add(egui::Label::new(text).wrap());
                    });
                    ui.add_space(3.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_stacks_vertically_and_close_stays_visible_at_supported_sizes() {
        for size in [egui::vec2(900.0, 560.0), egui::vec2(1280.0, 760.0)] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut doc = HelpDocument::default();
            doc.open_guide();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input.clone(), |ctx| {
                doc.show(ctx);
            });
            let output = ctx.run(input, |ctx| {
                doc.show(ctx);
            });
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            let bounds = |prefix: &str| {
                nodes
                    .iter()
                    .find_map(|(_, n)| {
                        n.label()
                            .or_else(|| n.value())
                            .filter(|s| s.starts_with(prefix))
                            .and_then(|_| n.bounds())
                    })
                    .unwrap_or_else(|| panic!("Missing {prefix}: {nodes:?}"))
            };
            let intro = bounds("先写一段能从头播到尾");
            let heading = bounds("从一句对白开始");
            let step_one = bounds("在“文件”中选择");
            let step_two = bounds("选中左侧“对白");
            assert!(intro.width() >= 300.0, "Unreadable column: {intro:?}");
            assert!(heading.y0 >= intro.y1 && step_one.y0 >= heading.y1 && step_two.y0 >= step_one.y1,
                "Article must flow top to bottom: {intro:?}, {heading:?}, {step_one:?}, {step_two:?}");
            let close = bounds(&i18n::key("help.close"));
            assert!(
                close.x0 >= 0.0
                    && close.y0 >= 0.0
                    && close.x1 <= size.x as f64
                    && close.y1 <= size.y as f64,
                "Close is outside {size:?}: {close:?}"
            );
        }
    }

    #[test]
    fn all_locales_have_matching_navigable_articles_and_no_lost_lists() {
        let ids: Vec<_> = parse(i18n::help_html("chs"))
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(ids.len(), 10);
        for locale in i18n::LOCALES {
            let guide = parse(i18n::help_html(locale));
            assert_eq!(
                guide.iter().map(|a| &a.id).collect::<Vec<_>>(),
                ids.iter().collect::<Vec<_>>(),
                "{locale}"
            );
            for article in guide {
                assert!(!article.blocks.is_empty(), "Empty {locale}/{}", article.id);
                assert!(!article.search_text.contains('<'));
                assert!(article.blocks.iter().all(|b| match b {
                    Block::List { items, .. } =>
                        !items.is_empty() && items.iter().all(|i| !i.is_empty()),
                    _ => true,
                }));
            }
        }
        let groups = schema()["NODE_GROUPS_SRC"].as_array().unwrap();
        let listed: std::collections::BTreeSet<_> = groups
            .iter()
            .flat_map(|g| g[1].as_array().unwrap())
            .filter_map(Value::as_str)
            .collect();
        let defined: std::collections::BTreeSet<_> = schema()["NODE_SCHEMAS"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            listed, defined,
            "Every node reference must be reachable from the directory"
        );
    }

    #[test]
    fn search_reaches_relevant_help_and_recovers_from_no_results() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut doc = HelpDocument::default();
        doc.open_guide();
        let draw = |doc: &mut HelpDocument| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 560.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    doc.show(ctx);
                },
            );
            output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .into_iter()
                .filter_map(|(_, n)| n.label().or_else(|| n.value()).map(str::to_owned))
                .collect::<Vec<_>>()
                .join("\n")
        };
        doc.search = "黑幕".into();
        let text = draw(&mut doc);
        assert!(text.contains("换景后人物已经就位"), "{text}");
        doc.search = "no-match-7632".into();
        assert!(draw(&mut doc).contains("没有找到相关内容"));
        doc.search.clear();
        draw(&mut doc);
        assert_eq!(doc.guide, "start");
        doc.open_node("end");
        let text = draw(&mut doc);
        assert!(text.contains("结束后回到哪里"), "{text}");
        assert!(
            !text.contains("RUNTIME_API"),
            "Technical data should stay collapsed"
        );
        assert!(node_matches("end", "下一章节"));
    }
}
