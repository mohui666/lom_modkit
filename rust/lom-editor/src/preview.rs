use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, TextureHandle, Vec2};
use lom_core::project::Project;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
};

pub fn edges(story: &Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (index, node) in story["nodes"].as_array().into_iter().flatten().enumerate() {
        let id = text(node, "id");
        for target in lom_core::analysis::successors(story, index) {
            let mut label = "下一步".to_owned();
            for key in ["goto", "win", "lose", "success", "failure"] {
                if node[key].as_str() == Some(&target) {
                    label = key.to_owned();
                }
            }
            for key in ["options", "cases", "bands"] {
                for (i, row) in node[key].as_array().into_iter().flatten().enumerate() {
                    if ["goto", "goto_大成功", "goto_成功", "goto_失败"]
                        .iter()
                        .any(|key| row[*key].as_str() == Some(&target))
                    {
                        label = row["text"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{} {}", key, i + 1));
                    }
                }
            }
            out.push((id.to_owned(), target, label));
        }
    }
    out
}
pub fn route(story: &Value, target: &str) -> Option<Vec<String>> {
    let ns = story["nodes"].as_array()?;
    let start = story["start"]
        .as_str()
        .or_else(|| ns.first()?.get("id")?.as_str())?;
    let links = edges(story);
    let mut queue = VecDeque::from([start.to_owned()]);
    let mut prev: BTreeMap<String, Option<String>> = BTreeMap::from([(start.into(), None)]);
    while let Some(id) = queue.pop_front() {
        if id == target {
            let mut out = vec![id.clone()];
            let mut cur = id;
            while let Some(Some(p)) = prev.get(&cur) {
                out.push(p.clone());
                cur = p.clone();
            }
            out.reverse();
            return Some(out);
        }
        for (_, to, _) in links.iter().filter(|(from, _, _)| from == &id) {
            if !prev.contains_key(to) {
                prev.insert(to.clone(), Some(id.clone()));
                queue.push_back(to.clone());
            }
        }
    }
    None
}
pub fn simulate(story: &Value, target: &str) -> Value {
    simulate_until(story, target, true)
}
fn simulate_until(story: &Value, target: &str, include_target: bool) -> Value {
    let mut state = json!({"actors":{},"overlays":{},"background":null,"view":null,"custom_cg":null,"dialog":null,"choice":null,"reached":false,"steps":0});
    let Some(ns) = story["nodes"].as_array() else {
        return state;
    };
    let path = route(story, target);
    state["reached"] = path.is_some().into();
    let walk = path.unwrap_or_else(|| vec![target.into()]);
    for id in &walk {
        if !include_target && id == target {
            break;
        }
        if let Some(n) = ns.iter().find(|n| n["id"].as_str() == Some(id)) {
            apply(&mut state, n);
        }
    }
    state["steps"] = walk.len().into();
    state
}
pub fn build_playtest_prelude(story: &Value, target: &str) -> Vec<Value> {
    let state = simulate_until(story, target, false);
    let mut nodes = Vec::new();
    if let Some(view) = state["view"]
        .as_str()
        .filter(|v| !v.is_empty() && *v != "out")
    {
        nodes.push(json!({"type":"scene","view":view}));
    }
    if let Some(background) = state["background"].as_str().filter(|v| !v.is_empty()) {
        nodes.push(json!({"type":"background","action":"set","image":background}));
    }
    for (slot, overlay) in state["overlays"].as_object().into_iter().flatten() {
        if !text(overlay, "image").is_empty() {
            nodes.push(json!({"type":"overlay","action":"show","slot":slot,"image":overlay["image"],"position":overlay.get("position").cloned().unwrap_or(json!("center")),"scale":number(overlay,"scale",100.0),"opacity":number(overlay,"opacity",100.0),"layer":overlay.get("layer").cloned().unwrap_or(json!("front")),"fade":0}));
        }
    }
    let positions: Value = serde_json::from_str(include_str!("../../../data/stage_positions.json"))
        .expect("embedded stage positions");
    let mut actors: Vec<_> = state["actors"].as_object().into_iter().flatten().collect();
    actors.sort_by(|(a, x), (b, y)| {
        let pos = |v: &Value| number(&positions["positions"][text(v, "position")], "x", 0.5);
        pos(x).total_cmp(&pos(y)).then(a.cmp(b))
    });
    for (id, actor) in actors {
        let mut node = json!({"type":"show","character":id,"position":actor["position"],"portrait":actor["portrait"],"facing":actor["facing"]});
        if let Some(appearance) = actor["appearance"].as_str().filter(|s| !s.is_empty()) {
            node["appearance"] = appearance.into();
        }
        nodes.push(node);
    }
    let cg = &state["custom_cg"];
    if !text(cg, "image").is_empty() {
        nodes.push(json!({"type":"custom_cg","action":"show","image":cg["image"],"fade":0,"scale":number(cg,"scale",100.0),"x":number(cg,"x",0.0),"y":number(cg,"y",0.0)}));
    }
    let mut used: std::collections::BTreeSet<String> = story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect();
    let mut seq = 0;
    for node in &mut nodes {
        while used.contains(&format!("zz_playtest_{seq}")) {
            seq += 1;
        }
        let id = format!("zz_playtest_{seq}");
        used.insert(id.clone());
        node["id"] = id.into();
        seq += 1;
    }
    for i in 0..nodes.len() {
        let next = nodes
            .get(i + 1)
            .map(|v| v["id"].clone())
            .unwrap_or(json!(target));
        nodes[i]["goto"] = next;
    }
    nodes
}
fn apply(s: &mut Value, n: &Value) {
    s["dialog"] = Value::Null;
    s["choice"] = Value::Null;
    let cid = text(n, "character");
    let kind = text(n, "type");
    match kind {
        "scene" => {
            s["view"] = n["view"].clone();
            s["background"] = Value::Null;
        }
        "combat" => {
            s["view"] = n.get("background").cloned().unwrap_or(json!("center"));
            s["background"] = Value::Null;
        }
        "background" => {
            s["background"] = if matches!(text(n, "action"), "clear" | "fadeout") {
                Value::Null
            } else {
                n["image"].clone()
            };
        }
        "custom_cg" => {
            s["custom_cg"] = if text(n, "action") == "hide" {
                Value::Null
            } else {
                n.clone()
            };
        }
        "overlay" => {
            let slot = n["slot"].as_str().unwrap_or("main");
            if text(n, "action") == "hide" {
                if let Some(o) = s["overlays"].as_object_mut() {
                    o.remove(slot);
                }
            } else {
                s["overlays"][slot] = n.clone();
            }
        }
        "show" => {
            if !cid.is_empty() {
                s["actors"][cid] = json!({"position":n.get("position").cloned().unwrap_or(json!("M")),"portrait":n.get("portrait").cloned().unwrap_or(json!("normal")),"facing":n.get("facing").cloned().unwrap_or(json!("right")),"appearance":n["appearance"],"offset_x":0.0,"offset_y":0.0,"rotation":0.0,"dimmed":false});
            }
        }
        "hide" => {
            if let Some(o) = s["actors"].as_object_mut() {
                o.remove(cid);
            }
        }
        "move" | "face" | "rotate" | "dim" | "offset" => {
            if s["actors"].get(cid).is_some() {
                let a = &mut s["actors"][cid];
                match kind {
                    "move" => a["position"] = n["to"].clone(),
                    "face" => a["facing"] = n["facing"].clone(),
                    "rotate" => a["rotation"] = n["angle"].clone(),
                    "dim" => a["dimmed"] = n["dimmed"].clone(),
                    "offset" => {
                        a["offset_x"] = json!(number(a, "offset_x", 0.0) + number(n, "x", 0.0));
                        a["offset_y"] = json!(number(a, "offset_y", 0.0) + number(n, "y", 0.0));
                    }
                    _ => {}
                }
            }
        }
        "say" => {
            if s["actors"].get(cid).is_some() {
                s["actors"][cid]["portrait"] =
                    n.get("portrait").cloned().unwrap_or(json!("normal"));
            }
            s["dialog"] = n.clone();
        }
        "choice" => s["choice"] = n["options"].clone(),
        _ => {}
    }
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn number(v: &Value, key: &str, default: f64) -> f64 {
    v[key].as_f64().unwrap_or(default)
}

struct Cached {
    texture: TextureHandle,
    bytes: usize,
    touched: u64,
}
pub struct Preview {
    pub mapping: Value,
    pub library: PathBuf,
    positions: Value,
    cache: BTreeMap<String, Cached>,
    tick: u64,
}
impl Preview {
    pub fn new(_repo: &Path) -> Self {
        // Embedded mappings keep a standalone app independent of its build machine.
        // Never touch the compilation-time Documents checkout during app startup.
        let library = if let Some(root) = std::env::var_os("LOM_MODKIT_ROOT") {
            PathBuf::from(root).join("data")
        } else if let Some(appdata) = std::env::var_os("APPDATA") {
            PathBuf::from(appdata).join("lom_modkit/game-previews")
        } else {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            if cfg!(target_os = "macos") {
                home.join("Library/Application Support/lom_modkit/game-previews")
            } else {
                home.join(".local/share/lom_modkit/game-previews")
            }
        };
        let mapping: Value = serde_json::from_str(include_str!("../../../data/preview_map.json"))
            .expect("embedded preview mapping");
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json"))
                .unwrap_or(json!({}));
        Self {
            mapping,
            library,
            positions,
            cache: BTreeMap::new(),
            tick: 0,
        }
    }
    pub fn set_library(&mut self, path: &Path) -> anyhow::Result<()> {
        let root = if path.join("preview_map.json").is_file() {
            path.to_path_buf()
        } else {
            path.join("data")
        };
        let mapping = lom_core::load_json(root.join("preview_map.json"))?;
        anyhow::ensure!(
            mapping["characters"].is_object(),
            "素材库缺少 characters 映射"
        );
        self.mapping = mapping;
        self.library = root;
        self.cache.clear();
        Ok(())
    }
    fn asset(
        &self,
        p: &Project,
        name: &str,
        character: bool,
        portrait: &str,
    ) -> Option<(String, Vec<u8>, Value)> {
        if name.is_empty() {
            return None;
        }
        if let Some(id) = name.strip_prefix("user:") {
            for (key, bytes) in &p.assets {
                if key.ends_with("/content.json") {
                    if let Ok(meta) = serde_json::from_slice::<Value>(bytes) {
                        if meta["id"].as_str() == Some(id) {
                            let file = if character {
                                meta["portraits"][portrait]
                                    .as_str()
                                    .or_else(|| meta["portraits"]["normal"].as_str())
                                    .or_else(|| meta["files"]["main"].as_str())
                            } else {
                                meta["files"]["main"].as_str()
                            }?;
                            let path = Path::new(key)
                                .parent()?
                                .join(file)
                                .to_string_lossy()
                                .replace('\\', "/");
                            return p.assets.get(&path).map(|b| (path, b.clone(), meta));
                        }
                    }
                }
            }
            return None;
        }
        if let Some(bytes) = p.assets.get(name) {
            return Some((format!("project:{name}"), bytes.clone(), json!({})));
        }
        let rel = if character {
            self.mapping["characters"][name]["portraits"][portrait]
                .as_str()
                .or_else(|| self.mapping["characters"][name]["portraits"]["normal"].as_str())?
        } else {
            self.mapping["views"][name].as_str().unwrap_or(name)
        };
        let rel = Path::new(rel);
        if rel.is_absolute()
            || rel.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::Prefix(_)
                )
            })
        {
            return None;
        }
        let canon = self.library.join(rel).canonicalize().ok()?;
        let root = self.library.canonicalize().ok()?;
        if !canon.starts_with(root) {
            return None;
        }
        Some((
            canon.to_string_lossy().into(),
            std::fs::read(&canon).ok()?,
            json!({}),
        ))
    }
    fn texture(
        &mut self,
        ctx: &egui::Context,
        p: &Project,
        name: &str,
        character: bool,
        portrait: &str,
    ) -> Option<(TextureHandle, Value)> {
        let (key, bytes, meta) = self.asset(p, name, character, portrait)?;
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        let key = format!("{key}:{}", hash.finish());
        self.tick += 1;
        if let Some(c) = self.cache.get_mut(&key) {
            c.touched = self.tick;
            return Some((c.texture.clone(), meta));
        }
        if bytes.len() > 32 * 1024 * 1024 {
            return None;
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let image = reader.decode().ok()?.thumbnail(1600, 1600).into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let cost = size[0] * size[1] * 4;
        while !self.cache.is_empty()
            && (self.cache.len() >= 60
                || self.cache.values().map(|c| c.bytes).sum::<usize>() + cost > 256 * 1024 * 1024)
        {
            let oldest = self
                .cache
                .iter()
                .min_by_key(|(_, c)| c.touched)
                .map(|(k, _)| k.clone())
                .unwrap();
            self.cache.remove(&oldest);
        }
        let texture = ctx.load_texture(
            &key,
            egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        self.cache.insert(
            key,
            Cached {
                texture: texture.clone(),
                bytes: cost,
                touched: self.tick,
            },
        );
        Some((texture, meta))
    }
    fn anchor(&self, pos: &str) -> (f32, f32) {
        let key = if pos.eq_ignore_ascii_case("c") {
            String::from("M")
        } else {
            pos.to_uppercase()
        };
        let entry = &self.positions["positions"][&key];
        (
            number(entry, "x", 0.5) as f32,
            number(entry, "feet", 1.003) as f32,
        )
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        story: &Value,
        index: usize,
    ) -> Option<String> {
        let ctx = ui.ctx().clone();
        let node = &story["nodes"][index];
        let target = text(node, "id");
        let state = simulate(story, target);
        let size = Vec2::new(
            ui.available_width().max(160.0),
            ui.available_height().max(120.0),
        );
        let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_filled(outer, 8.0, Color32::from_black_alpha(4));
        let width = size.x.min(size.y * 16.0 / 9.0);
        let rect = Rect::from_center_size(outer.center(), Vec2::new(width, width * 9.0 / 16.0));
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 6.0, Color32::from_rgb(29, 35, 42));
        let bg = state["background"]
            .as_str()
            .or_else(|| state["view"].as_str())
            .unwrap_or("");
        if let Some((tex, _)) = self.texture(&ctx, p, bg, false, "") {
            painter.image(tex.id(), rect, uv(), Color32::WHITE);
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if bg.is_empty() { "" } else { bg },
                FontId::proportional(13.0),
                Color32::from_rgb(109, 118, 113),
            );
        }
        for layer in ["back", "actors", "front"] {
            if layer == "actors" {
                let mut actors: Vec<(&String, &Value)> = state["actors"]
                    .as_object()
                    .map(|o| o.iter().collect())
                    .unwrap_or_default();
                actors.sort_by(|a, b| {
                    self.anchor(text(a.1, "position"))
                        .0
                        .total_cmp(&self.anchor(text(b.1, "position")).0)
                });
                for (cid, a) in actors {
                    let lookup = if cid == "player" && a["appearance"] == "beautified" {
                        "player_beautified"
                    } else {
                        cid
                    };
                    let (x, _feet) = self.anchor(text(a, "position"));
                    let base = Pos2::new(
                        rect.left()
                            + rect.width() * x
                            + number(a, "offset_x", 0.0) as f32 * rect.width() / 1920.0,
                        rect.bottom()
                            - rect.height() * 0.02
                            - number(a, "offset_y", 0.0) as f32 * rect.height() / 1080.0,
                    );
                    if let Some((tex, meta)) =
                        self.texture(&ctx, p, lookup, true, text(a, "portrait"))
                    {
                        let h = rect.height() * 0.78 * number(&meta, "scale", 100.0) as f32 / 100.0;
                        let w = h * tex.size_vec2().x / tex.size_vec2().y;
                        let dest = Rect::from_min_size(
                            Pos2::new(base.x - w / 2.0, base.y - h),
                            Vec2::new(w, h),
                        );
                        let art_left = meta["art_facing"].as_str().unwrap_or("left") != "right";
                        let flipped = (text(a, "facing") == "left") != art_left;
                        let color = if a["dimmed"].as_bool().unwrap_or(false) {
                            Color32::from_white_alpha(130)
                        } else {
                            Color32::WHITE
                        };
                        sprite(
                            &painter,
                            tex.id(),
                            dest,
                            base,
                            number(a, "rotation", 0.0) as f32,
                            flipped,
                            color,
                        );
                    } else {
                        let h = rect.height() * 0.70;
                        let actor = Rect::from_min_size(
                            Pos2::new(base.x - h * 0.19, base.y - h),
                            Vec2::new(h * 0.38, h),
                        );
                        painter.rect_filled(actor, 20.0, Color32::from_rgb(78, 88, 83));
                        painter.text(
                            actor.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{}\n{}", cid, text(a, "portrait")),
                            FontId::proportional(12.0),
                            Color32::LIGHT_GRAY,
                        );
                    }
                }
            } else if let Some(overlays) = state["overlays"].as_object() {
                for item in overlays
                    .values()
                    .filter(|n| n["layer"].as_str().unwrap_or("front") == layer)
                {
                    self.overlay(&ctx, &painter, p, item, rect);
                }
            }
        }
        if state["custom_cg"].is_object() {
            self.overlay(&ctx, &painter, p, &state["custom_cg"], rect);
        }
        if let Some(dialog) = state["dialog"].as_object() {
            let txt = dialog.get("text").and_then(Value::as_str).unwrap_or("");
            let mode = dialog.get("mode").and_then(Value::as_str).unwrap_or("");
            let dr = if mode == "center" {
                rect.shrink2(Vec2::new(rect.width() * 0.08, rect.height() * 0.25))
            } else {
                Rect::from_min_max(
                    Pos2::new(rect.left() + 12.0, rect.bottom() - rect.height() * 0.28),
                    Pos2::new(rect.right() - 12.0, rect.bottom() - 9.0),
                )
            };
            painter.rect_filled(dr, 5.0, Color32::from_black_alpha(195));
            let name = dialog
                .get("character")
                .and_then(Value::as_str)
                .unwrap_or("");
            if mode == "character" || mode == "think" {
                painter.text(
                    dr.left_top() + Vec2::new(12.0, 9.0),
                    egui::Align2::LEFT_TOP,
                    name,
                    FontId::proportional(13.0),
                    Color32::from_rgb(221, 168, 110),
                );
            }
            let galley = painter.layout(
                txt.to_owned(),
                FontId::proportional((rect.width() / 38.0).clamp(12.0, 19.0)),
                Color32::from_rgb(246, 241, 221),
                dr.width() - 24.0,
            );
            painter.galley(
                dr.left_top()
                    + Vec2::new(
                        12.0,
                        if mode == "character" || mode == "think" {
                            30.0
                        } else {
                            12.0
                        },
                    ),
                galley,
                Color32::WHITE,
            );
        }
        if let Some(card) = card_data(p, node) {
            if card.kind == "ending" {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(9, 12, 12));
                let band = relative(rect, 0.0, 0.22, 1.0, 0.57);
                painter.rect_filled(band, 0.0, Color32::from_rgb(229, 213, 177));
                let book = relative(rect, 0.15, 0.10, 0.38, 0.78);
                painter.rect_filled(book, 0.0, Color32::from_rgb(200, 166, 107));
                let cover = book.shrink2(Vec2::new(book.width() * 0.08, book.height() * 0.04));
                painter.rect_filled(cover, 0.0, Color32::from_rgb(224, 204, 163));
                painter.rect_stroke(
                    book,
                    0.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(111, 77, 42)),
                    egui::StrokeKind::Inside,
                );
                let art = cover.shrink2(Vec2::new(cover.width() * 0.08, cover.height() * 0.10));
                if let Some((tex, _)) = self.texture(&ctx, p, &card.image, false, "") {
                    fit_image(&painter, &tex, art);
                } else {
                    paint_text(
                        &painter,
                        art,
                        "原版结局插图占位\n游戏内由原版面板显示",
                        rect.height() * 0.035,
                        Color32::from_rgb(80, 64, 45),
                    );
                }
                paint_text(
                    &painter,
                    relative(rect, 0.56, 0.31, 0.37, 0.14),
                    &card.title,
                    rect.height() * 0.075,
                    Color32::from_rgb(151, 48, 38),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.56, 0.48, 0.37, 0.24),
                    &card.body,
                    rect.height() * 0.036,
                    Color32::from_rgb(37, 31, 24),
                );
            } else if card.kind == "intro" {
                painter.rect_filled(rect, 0.0, Color32::from_black_alpha(125));
                let panel = relative(rect, 0.35, 0.33, 0.50, 0.49);
                painter.rect_filled(panel, 4.0, Color32::from_rgba_unmultiplied(25, 25, 29, 239));
                painter.rect_stroke(
                    panel,
                    4.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(164, 137, 91)),
                    egui::StrokeKind::Inside,
                );
                let scale = number(node, "image_scale", 100.0).clamp(40.0, 160.0) as f32 / 100.0;
                let x = number(node, "image_x", 0.0).clamp(-30.0, 30.0) as f32 / 100.0;
                let y = number(node, "image_y", 0.0).clamp(-30.0, 30.0) as f32 / 100.0;
                let avatar = Rect::from_center_size(
                    Pos2::new(
                        rect.left() + rect.width() * (0.31 + x),
                        rect.top() + rect.height() * (0.50 - y),
                    ),
                    Vec2::new(rect.width() * 0.30 * scale, rect.height() * 0.62 * scale),
                );
                if let Some((tex, _)) = self.texture(&ctx, p, &card.image, false, "") {
                    fit_image(&painter, &tex, avatar);
                } else {
                    painter.rect_filled(
                        avatar,
                        2.0,
                        Color32::from_rgba_unmultiplied(24, 27, 30, 170),
                    );
                    paint_text(
                        &painter,
                        avatar.shrink(12.0),
                        &card.placeholder,
                        rect.height() * 0.035,
                        Color32::from_rgb(220, 210, 187),
                    );
                }
                paint_text(
                    &painter,
                    relative(rect, 0.52, 0.40, 0.29, 0.075),
                    &card.title,
                    rect.height() * 0.033,
                    Color32::from_rgb(213, 184, 122),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.52, 0.48, 0.29, 0.10),
                    &card.name,
                    rect.height() * 0.070,
                    Color32::from_rgb(235, 78, 56),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.52, 0.61, 0.29, 0.17),
                    &card.body,
                    rect.height() * 0.031,
                    Color32::from_rgb(235, 229, 216),
                );
            } else {
                painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(15, 12, 14, 247));
                paint_text(
                    &painter,
                    relative(rect, 0.15, 0.22, 0.70, 0.18),
                    &card.title,
                    rect.height() * 0.10,
                    Color32::from_rgb(197, 78, 67),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.20, 0.48, 0.60, 0.28),
                    &card.body,
                    rect.height() * 0.045,
                    Color32::from_rgb(231, 221, 202),
                );
                paint_text(
                    &painter,
                    relative(rect, 0.28, 0.84, 0.6, 0.10),
                    &format!(
                        "死亡编号 {} · 完成后 {}",
                        text(node, "death_id"),
                        text(node, "next")
                    ),
                    rect.height() * 0.028,
                    Color32::GRAY,
                );
            }
        }
        let mut choice = None;
        if let Some(options) = state["choice"].as_array() {
            for (i, item) in options.iter().enumerate() {
                let button = Rect::from_min_size(
                    Pos2::new(
                        rect.left() + rect.width() * 0.25,
                        rect.top() + rect.height() * 0.30 + i as f32 * 35.0,
                    ),
                    Vec2::new(rect.width() * 0.5, 30.0),
                );
                if ui
                    .put(button, egui::Button::new(text(item, "text")))
                    .clicked()
                {
                    choice = Some(text(item, "goto").to_owned());
                }
            }
        }
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, Color32::from_rgb(173, 161, 134)),
            egui::StrokeKind::Inside,
        );
        if !state["reached"].as_bool().unwrap_or(false) {
            ui.painter().text(
                outer.left_bottom() + Vec2::new(8.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                "此步骤从入口不可达",
                FontId::proportional(11.0),
                Color32::LIGHT_RED,
            );
        }
        choice
    }
    pub fn show_portrait(&mut self, ui: &mut egui::Ui, project: &Project, node: &Value) {
        let cid = text(node, "character");
        if cid.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(crate::i18n::key("portrait.choose_character"));
            });
            return;
        }
        let portrait = node["portrait"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("normal");
        let lookup = if cid == "player" && node["appearance"] == "beautified" {
            "player_beautified"
        } else {
            cid
        };
        let title = lom_core::validate::editor_data()["characters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|v| v["id"] == cid)
            .and_then(|v| v["name"].as_str())
            .unwrap_or(cid);
        ui.label(egui::RichText::new(title).strong());
        ui.label(
            egui::RichText::new(format!("{cid} · {portrait}"))
                .small()
                .color(Color32::GRAY),
        );
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(18, 21, 31, 225));
        if let Some((texture, _)) = self.texture(ui.ctx(), project, lookup, true, portrait) {
            fit_image(&painter, &texture, rect.shrink(20.0));
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::key("portrait.no_assets"),
                FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
    }
    fn overlay(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        p: &Project,
        n: &Value,
        rect: Rect,
    ) {
        if let Some((tex, _)) = self.texture(ctx, p, text(n, "image"), false, "") {
            let scale = number(n, "scale", 100.0) as f32 / 100.0;
            let aspect = tex.size_vec2();
            let fit = (rect.width() / aspect.x).min(rect.height() / aspect.y) * scale;
            let size = aspect * fit;
            let mut center = rect.center()
                + Vec2::new(
                    number(n, "x", 0.0) as f32 / 100.0 * rect.width(),
                    -number(n, "y", 0.0) as f32 / 100.0 * rect.height(),
                );
            let position = text(n, "position");
            if position.contains("left") {
                center.x = rect.left() + size.x / 2.0;
            }
            if position.contains("right") {
                center.x = rect.right() - size.x / 2.0;
            }
            if position.contains("top") {
                center.y = rect.top() + size.y / 2.0;
            }
            if position.contains("bottom") {
                center.y = rect.bottom() - size.y / 2.0;
            }
            painter.image(
                tex.id(),
                Rect::from_center_size(center, size),
                uv(),
                Color32::from_white_alpha(
                    (number(n, "opacity", 100.0) * 2.55).clamp(0.0, 255.0) as u8
                ),
            );
        }
    }
}
fn uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}
fn sprite(
    p: &egui::Painter,
    id: egui::TextureId,
    rect: Rect,
    pivot: Pos2,
    degrees: f32,
    flip: bool,
    color: Color32,
) {
    let rotation = egui::emath::Rot2::from_angle(degrees.to_radians());
    let mut mesh = egui::Mesh::with_texture(id);
    let positions = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let uvs = if flip {
        [
            Pos2::new(1.0, 0.0),
            Pos2::new(0.0, 0.0),
            Pos2::new(0.0, 1.0),
            Pos2::new(1.0, 1.0),
        ]
    } else {
        [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ]
    };
    for i in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: pivot + rotation * (positions[i] - pivot),
            uv: uvs[i],
            color,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(egui::Shape::mesh(mesh));
}
#[derive(Debug)]
struct CardData {
    kind: &'static str,
    title: String,
    name: String,
    body: String,
    image: String,
    placeholder: String,
}
fn card_data(project: &Project, node: &Value) -> Option<CardData> {
    let kind = text(node, "type");
    let mut card = CardData {
        kind: "",
        title: String::new(),
        name: String::new(),
        body: String::new(),
        image: String::new(),
        placeholder: String::new(),
    };
    let official = lom_core::validate::editor_data();
    if kind == "goto_scene" && text(node, "scene") == "End" {
        card.kind = "ending";
        card.title = text(node, "title").into();
        if card.title.is_empty() {
            card.title = "自定义结局标题".into();
        }
        card.body = text(node, "desc").into();
        card.image = text(node, "image").into();
    } else if kind == "death" {
        card.kind = "death";
        card.title = text(node, "title").into();
        if card.title.is_empty() {
            card.title = official["death_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| v["id"] == node["death_id"])
                .and_then(|v| v["name"].as_str())
                .unwrap_or("命殒江湖")
                .into();
        }
        card.body = text(node, "text").into();
    } else if kind == "intro" {
        card.kind = "intro";
        match text(node, "intro_source") {
            "custom" => {
                card.title = text(node, "title").into();
                card.name = text(node, "name").into();
                card.body = text(node, "text").into();
                card.image = text(node, "image").into();
                card.placeholder = "自定义人物\n未选择人物图片".into();
            }
            "character" => {
                let id = text(node, "character").trim_start_matches("user:");
                card.name = text(node, "character").into();
                card.body = "还没有介绍卡。请在内容库填写角色介绍。".into();
                card.placeholder = "自定义角色\n未设置介绍图".into();
                for (path, bytes) in &project.assets {
                    if !path.ends_with("/content.json") {
                        continue;
                    }
                    let Ok(meta) = serde_json::from_slice::<Value>(bytes) else {
                        continue;
                    };
                    if meta["id"] != id {
                        continue;
                    }
                    let intro = &meta["intro"];
                    card.title = intro["title"]
                        .as_str()
                        .or_else(|| meta["title"].as_str())
                        .unwrap_or("")
                        .into();
                    card.name = intro["name"]
                        .as_str()
                        .or_else(|| meta["name"].as_str())
                        .unwrap_or(id)
                        .into();
                    if let Some(body) = intro["text"].as_str() {
                        card.body = body.into();
                    }
                    if let Some(image) = intro["image"].as_str() {
                        if let Some(parent) = Path::new(path).parent() {
                            card.image = parent.join(image).to_string_lossy().replace('\\', "/");
                        }
                    }
                    break;
                }
            }
            _ => {
                let id = text(node, "character");
                let entry = official["characters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|v| v["id"] == id);
                card.title = entry
                    .and_then(|v| v["title"].as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("原版人物资料")
                    .into();
                card.name = entry.and_then(|v| v["name"].as_str()).unwrap_or(id).into();
                card.body = entry
                    .and_then(|v| v["intro"].as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("未提取到该人物的原版介绍正文。")
                    .into();
                card.placeholder = "游戏内显示\n原版关系头像".into();
            }
        }
    } else {
        return None;
    }
    Some(card)
}
fn relative(rect: Rect, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        ),
        Vec2::new(rect.width() * w, rect.height() * h),
    )
}
fn paint_text(painter: &egui::Painter, rect: Rect, text: &str, size: f32, color: Color32) {
    let galley = painter.layout(
        text.into(),
        FontId::proportional(size.max(10.0)),
        color,
        rect.width(),
    );
    painter
        .with_clip_rect(rect)
        .galley(rect.left_top(), galley, color);
}
fn fit_image(painter: &egui::Painter, texture: &TextureHandle, rect: Rect) {
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    painter.image(
        texture.id(),
        Rect::from_center_size(rect.center(), size * scale),
        uv(),
        Color32::WHITE,
    );
}

pub fn graph(ui: &mut egui::Ui, story: &Value, selected: usize) -> Option<usize> {
    let Some(nodes) = story["nodes"].as_array() else {
        return None;
    };
    let links = edges(story);
    let mut choose = None;
    let mut positions = BTreeMap::new();
    let width = ui.available_width().max(260.0);
    let row_h = 62.0;
    let (canvas, _) = ui.allocate_exact_size(
        Vec2::new(width, nodes.len() as f32 * row_h + 20.0),
        egui::Sense::hover(),
    );
    for (i, n) in nodes.iter().enumerate() {
        let rect = Rect::from_min_size(
            canvas.min + Vec2::new(58.0, i as f32 * row_h + 10.0),
            Vec2::new(width - 80.0, 44.0),
        );
        positions.insert(text(n, "id").to_owned(), rect);
    }
    let painter = ui.painter();
    for (index, (from, to, _)) in links.iter().enumerate() {
        if let (Some(a), Some(b)) = (positions.get(from), positions.get(to)) {
            let x = canvas.left() + 14.0 + (index % 6) as f32 * 6.0;
            let color = Color32::from_rgb(153, 146, 126);
            painter.line_segment(
                [a.left_center(), Pos2::new(x, a.center().y)],
                Stroke::new(1.0_f32, color),
            );
            painter.line_segment(
                [Pos2::new(x, a.center().y), Pos2::new(x, b.center().y)],
                Stroke::new(1.0_f32, color),
            );
            painter.arrow(
                Pos2::new(x, b.center().y),
                b.left_center() - Pos2::new(x, b.center().y),
                Stroke::new(1.0_f32, color),
            );
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        let rect = positions[text(n, "id")];
        let reachable = route(story, text(n, "id")).is_some();
        let label = format!(
            "{}   {}{}",
            text(n, "id"),
            text(n, "type"),
            if reachable { "" } else { " · 不可达" }
        );
        if ui
            .put(rect, egui::Button::new(label).selected(i == selected))
            .clicked()
        {
            choose = Some(i);
        }
    }
    for (_, to, _) in &links {
        if !positions.contains_key(to) {
            ui.colored_label(Color32::RED, format!("缺失目标：{to}"));
        }
    }
    choose
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branch_preview_follows_reachable_path() {
        let story = json!({"start":"s","nodes":[{"id":"s","type":"show","character":"player","position":"LM1"},{"id":"c","type":"choice","options":[{"text":"甲","goto":"a"},{"text":"乙","goto":"b"}]},{"id":"a","type":"end"},{"id":"b","type":"say","character":"player","text":"分支"}]});
        let state = simulate(&story, "b");
        assert_eq!(state["actors"]["player"]["position"], "LM1");
        assert_eq!(state["dialog"]["text"], "分支");
        assert_eq!(state["reached"], true);
    }
    #[test]
    fn combat_has_no_implicit_fallthrough() {
        let story = json!({"nodes":[{"id":"a","type":"combat","win":"w","lose":"l"},{"id":"b","type":"say"}]});
        let edges = edges(&story);
        assert_eq!(edges.len(), 2);
        assert!(!edges.iter().any(|(_, to, _)| to == "b"));
    }
}

#[cfg(test)]
mod content_tests {
    use super::*;
    #[test]
    fn user_image_and_character_follow_files_main_contract() {
        let mut project = Project::new();
        let image_meta = include_bytes!(
            "../../../samples/showcase3/assets/user/image/showcase.courier_station/content.json"
        );
        project.assets.insert(
            "assets/user/image/showcase.courier_station/content.json".into(),
            image_meta.to_vec(),
        );
        project.assets.insert(
            "assets/user/image/showcase.courier_station/courier_station.png".into(),
            vec![1, 2, 3],
        );
        let character_meta = include_bytes!(
            "../../../samples/showcase3/assets/user/character/showcase.lin_deng/content.json"
        );
        project.assets.insert(
            "assets/user/character/showcase.lin_deng/content.json".into(),
            character_meta.to_vec(),
        );
        project.assets.insert(
            "assets/user/character/showcase.lin_deng/normal.png".into(),
            vec![4, 5],
        );
        project.assets.insert(
            "assets/user/character/showcase.lin_deng/happy.png".into(),
            vec![6, 7],
        );
        let preview = Preview::new(Path::new("/not-a-repository"));
        assert_eq!(
            preview
                .asset(&project, "user:showcase.courier_station", false, "")
                .unwrap()
                .1,
            vec![1, 2, 3]
        );
        assert_eq!(
            preview
                .asset(&project, "user:showcase.lin_deng", true, "happy")
                .unwrap()
                .1,
            vec![6, 7]
        );
        assert_eq!(
            preview
                .asset(&project, "user:showcase.lin_deng", true, "missing")
                .unwrap()
                .1,
            vec![4, 5]
        );
        assert!(preview.mapping["characters"].is_object());
    }
    #[test]
    fn legacy_dice_keeps_all_three_outcome_edges() {
        let story = json!({"nodes":[{"id":"d","type":"dice","options":[{"goto_大成功":"a","goto_成功":"b","goto_失败":"c"}]}]});
        let targets: std::collections::BTreeSet<_> =
            edges(&story).into_iter().map(|(_, to, _)| to).collect();
        assert_eq!(
            targets,
            std::collections::BTreeSet::from(["a".into(), "b".into(), "c".into()])
        );
    }
}

#[cfg(test)]
mod card_tests {
    use super::*;
    #[test]
    fn ending_uses_description_and_project_image() {
        let p = Project::new();
        let c=card_data(&p,&json!({"type":"goto_scene","scene":"End","title":"终章","desc":"真正结局正文","text":"错误字段","image":"assets/end.png"})).unwrap();
        assert_eq!(c.body, "真正结局正文");
        assert_eq!(c.image, "assets/end.png");
    }
    #[test]
    fn character_intro_uses_own_metadata_and_embedded_official_text() {
        let mut p = Project::new();
        p.assets.insert("assets/characters/demo.hero/content.json".into(),serde_json::to_vec(&json!({"id":"demo.hero","name":"角色名","title":"默认称号","intro":{"name":"介绍名","title":"介绍称号","text":"自定义正文","image":"intro.png"}})).unwrap());
        let c = card_data(
            &p,
            &json!({"type":"intro","intro_source":"character","character":"user:demo.hero"}),
        )
        .unwrap();
        assert_eq!(
            (c.title.as_str(), c.name.as_str(), c.body.as_str()),
            ("介绍称号", "介绍名", "自定义正文")
        );
        assert_eq!(c.image, "assets/characters/demo.hero/intro.png");
        let data = lom_core::validate::editor_data();
        if let Some(official) = data["characters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["intro"].as_str().is_some_and(|s| !s.is_empty()))
        {
            let c = card_data(
                &p,
                &json!({"type":"intro","intro_source":"official","character":official["id"]}),
            )
            .unwrap();
            assert_eq!(c.body, official["intro"].as_str().unwrap());
        }
    }
}

#[cfg(test)]
mod prelude_tests {
    use super::*;
    #[test]
    fn playtest_prelude_restores_only_visual_state_before_target_and_avoids_id_collisions() {
        let mut story = json!({"id":"main","start":"scene","nodes":[{"id":"scene","type":"scene","view":"black"},{"id":"zz_playtest_0","type":"show","character":"player","position":"L","portrait":"normal","facing":"right","appearance":"beautified"},{"id":"target","type":"hide","character":"player"},{"id":"end","type":"end"}]});
        let nodes = build_playtest_prelude(&story, "target");
        assert_eq!(nodes.len(), 2);
        assert_ne!(nodes[0]["id"], "zz_playtest_0");
        assert_eq!(nodes[1]["character"], "player");
        assert_eq!(nodes[1]["appearance"], "beautified");
        assert_eq!(nodes[1]["goto"], "target");
        story["start"] = nodes[0]["id"].clone();
        story["nodes"].as_array_mut().unwrap().extend(nodes);
        lom_core::validate::validate_story(&story).unwrap();
        assert!(build_playtest_prelude(&story, "scene").is_empty());
    }
}
