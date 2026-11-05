//! Native Unity-rendered frame playback. No synthetic particles or lifetimes.
use super::*;
use anyhow::{ensure, Context, Result};

pub const WORLD_WIDTH: f64 = 19.2;
pub const WORLD_HEIGHT: f64 = 10.8;

pub fn root() -> PathBuf {
    lom_core::content::default_repository_root()
        .parent()
        .unwrap()
        .join("effect-previews")
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub file: PathBuf,
    pub time: f64,
}
#[derive(Clone, Debug)]
struct Clip {
    frames: Vec<Frame>,
    duration: f64,
    preview_time: f64,
    looping: bool,
    loop_start: f64,
    complete: bool,
    tail_frames: Vec<Frame>,
    tail_duration: f64,
    tail_sample_time: Option<f64>,
    tail_complete: bool,
}

#[derive(Clone, Debug)]
pub struct Library {
    pub size: [usize; 2],
    pub world_size: Vec2,
    clips: BTreeMap<String, Clip>,
}

enum Sample<'a> {
    Play(&'a Frame),
    Tail(&'a Frame),
    Missing,
    MissingTail,
    Truncated,
    TailTruncated,
    Ended,
}
impl Sample<'_> {
    fn status(&self) -> &'static str {
        match self {
            Self::Play(_) => "play",
            Self::Tail(_) => "tail",
            Self::Missing => "missing",
            Self::MissingTail => "missing_tail",
            Self::Truncated => "truncated",
            Self::TailTruncated => "tail_truncated",
            Self::Ended => "ended",
        }
    }
}
fn seconds(value: &Value, label: &str) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .with_context(|| format!("原版特效缓存的{label}必须是非负有限秒数"))
}
fn frames(root: &Path, raw: &Value, duration: f64, label: &str) -> Result<Vec<Frame>> {
    let rows = raw
        .as_array()
        .with_context(|| format!("原版特效缓存缺少{label}"))?;
    let mut previous = 0.0;
    rows.iter()
        .map(|row| {
            let time = seconds(&row["time"], "帧时间")?;
            ensure!(
                time >= previous && time <= duration,
                "原版特效缓存帧时间超出范围或未排序"
            );
            previous = time;
            let file = row["file"].as_str().context("原版特效缓存帧缺少文件路径")?;
            lom_core::package::canonical_archive_name(file)?;
            let file = lom_core::package::resolve_confined_file(root, &root.join(file))?;
            ensure!(file.is_file(), "原版特效缓存帧不是文件");
            Ok(Frame { file, time })
        })
        .collect()
}
impl Library {
    pub fn current() -> Result<Option<Self>> {
        Self::current_at(&root())
    }
    pub(crate) fn current_at(root: &Path) -> Result<Option<Self>> {
        let pointer = root.join("current.json");
        if !pointer.try_exists()? {
            return Ok(None);
        }
        let current = lom_core::load_json(pointer)?;
        ensure!(current["format"] == 1, "原版特效缓存索引版本不支持");
        let requests = if let Some(values) = current.get("request_ids") {
            values
                .as_array()
                .context("原版特效缓存索引缺少请求编号列表")?
                .as_slice()
        } else {
            std::slice::from_ref(
                current
                    .get("request_id")
                    .context("原版特效缓存索引缺少请求编号")?,
            )
        };
        let mut library: Option<Self> = None;
        for request in requests {
            let request = request
                .as_str()
                .context("原版特效缓存请求编号必须是字符串")?;
            ensure!(
                request.len() == 32 && request.bytes().all(|b| b.is_ascii_hexdigit()),
                "原版特效缓存请求编号不合法"
            );
            let next = Self::load(&root.join(request))?;
            if let Some(library) = &mut library {
                library.merge(next)?;
            } else {
                library = Some(next);
            }
        }
        Ok(library)
    }
    pub fn merge(&mut self, other: Self) -> Result<()> {
        ensure!(
            self.size == other.size && self.world_size == other.world_size,
            "原版特效缓存的分辨率或舞台尺寸不一致，无法合并"
        );
        self.clips.extend(other.clips);
        Ok(())
    }
    pub fn contains(&self, name: &str) -> bool {
        self.clips.contains_key(name)
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.clips.keys().map(String::as_str)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let root = if path
            .file_name()
            .is_some_and(|name| name == "effect_map.json")
        {
            path.parent().context("原版特效缓存路径无效")?
        } else {
            path
        };
        let raw = lom_core::load_json(root.join("effect_map.json"))?;
        ensure!(
            raw["format"] == 1 && raw["source"] == "game-render",
            "这不是游戏实际渲染的原版特效缓存"
        );
        ensure!(
            raw["alpha_mode"] == "premultiplied",
            "原版特效缓存必须使用预乘透明度"
        );
        let dimension = |key: &str| {
            raw[key]
                .as_u64()
                .filter(|v| *v > 0)
                .and_then(|v| usize::try_from(v).ok())
                .with_context(|| format!("原版特效缓存{key}无效"))
        };
        let size = [dimension("width")?, dimension("height")?];
        let width = seconds(&raw["world_width"], "舞台宽度")?;
        let height = seconds(&raw["world_height"], "舞台高度")?;
        ensure!(
            width > 0.0
                && height > 0.0
                && (width as f32).is_finite()
                && (height as f32).is_finite(),
            "原版特效缓存舞台尺寸无效"
        );
        let mut clips = BTreeMap::new();
        for (name, entry) in raw["effects"]
            .as_object()
            .context("原版特效缓存缺少特效目录")?
        {
            let duration = seconds(&entry["duration"], "播放时长")?;
            let looping = entry["looping"]
                .as_bool()
                .context("原版特效缓存缺少循环信息")?;
            let complete = entry["complete"]
                .as_bool()
                .context("原版特效缓存缺少完成状态")?;
            let loop_start = if entry["loop_start"].is_null() {
                0.0
            } else {
                seconds(&entry["loop_start"], "循环起点")?
            };
            ensure!(
                !looping || loop_start < duration,
                "原版特效缓存循环范围无效"
            );
            let play_frames = frames(root, &entry["frames"], duration, "播放帧")?;
            ensure!(!play_frames.is_empty(), "原版特效缓存播放帧为空");
            let preview_time = seconds(&entry["preview_time"], "定位代表帧时刻")?;
            ensure!(
                preview_time < duration
                    && play_frames.iter().any(|frame| frame.time == preview_time),
                "原版特效缓存定位代表帧时刻必须对应播放时长内的实际帧"
            );
            let tail_duration = if entry["tail_duration"].is_null() {
                0.0
            } else {
                seconds(&entry["tail_duration"], "尾段时长")?
            };
            let tail_frames = if entry["tail_frames"].is_null() {
                Vec::new()
            } else {
                frames(root, &entry["tail_frames"], tail_duration, "停止尾段帧")?
            };
            let tail_sample_time = if entry["tail_sample_time"].is_null() {
                None
            } else {
                Some(seconds(&entry["tail_sample_time"], "尾段采样时刻")?)
            };
            clips.insert(
                name.clone(),
                Clip {
                    frames: play_frames,
                    duration,
                    preview_time,
                    looping,
                    loop_start,
                    complete,
                    tail_frames,
                    tail_duration,
                    tail_sample_time,
                    tail_complete: entry["tail_complete"].as_bool().unwrap_or(false),
                },
            );
        }
        ensure!(!clips.is_empty(), "原版特效缓存目录为空");
        Ok(Self {
            size,
            world_size: Vec2::new(width as f32, height as f32),
            clips,
        })
    }
    fn sample(&self, name: &str, age: f64, stopped_age: Option<f64>) -> Sample<'_> {
        let Some(clip) = self.clips.get(name) else {
            return Sample::Missing;
        };
        if let Some(stopped) = stopped_age {
            let time = (age - stopped).max(0.0);
            if clip.tail_sample_time.is_none() {
                return Sample::MissingTail;
            }
            if time >= clip.tail_duration {
                return if clip.tail_complete {
                    Sample::Ended
                } else {
                    Sample::TailTruncated
                };
            }
            return clip
                .tail_frames
                .iter()
                .rev()
                .find(|frame| frame.time <= time)
                .map_or(Sample::MissingTail, Sample::Tail);
        }
        if !clip.looping && age >= clip.duration && clip.complete {
            return Sample::Ended;
        }
        if !clip.looping && age >= clip.duration {
            return Sample::Truncated;
        }
        let time = if clip.looping && age >= clip.duration {
            clip.loop_start + (age - clip.loop_start) % (clip.duration - clip.loop_start)
        } else {
            age
        };
        clip.frames
            .iter()
            .rev()
            .find(|frame| frame.time <= time)
            .map_or(Sample::Missing, Sample::Play)
    }
}

pub fn plays(node: &Value) -> bool {
    node.get("play").map_or_else(
        || {
            node.get("d")
                .is_none_or(|value| value.as_f64() == Some(1.0))
        },
        |value| value.as_bool().unwrap_or(false),
    )
}
pub fn managed(story: &Value) -> bool {
    lom_core::parallel::story_actions(story).any(|(_, action)| {
        action["type"] == "effect"
            && lom_core::validate::EFFECT_CONTROL_FIELDS
                .iter()
                .any(|field| action.get(field).is_some())
    })
}
pub fn coordinates(node: &Value) -> (f64, f64) {
    let default = if node["space"] == "screen" { 50.0 } else { 0.0 };
    (number(node, "x", default), number(node, "y", default))
}
pub fn screen_position(node: &Value) -> (f64, f64) {
    let (x, y) = coordinates(node);
    if node["space"] == "screen" {
        (x, y)
    } else {
        (
            50.0 + x / WORLD_WIDTH * 100.0,
            50.0 - y / WORLD_HEIGHT * 100.0,
        )
    }
}

/// Only captured timings drive automatic replay; global playback never loops.
pub fn cycle(node: &Value, library: Option<&Library>) -> Option<f64> {
    let library = library?;
    let mut longest: f64 = 0.0;
    let mut found = false;
    for action in lom_core::parallel::actions(node)
        .iter()
        .filter(|action| action["type"] == "effect")
    {
        let Some(clip) = library.clips.get(text(action, "name")) else {
            continue;
        };
        let speed = number(action, "speed", 1.0).max(0.0);
        let tail = || {
            if action["clear"] == true {
                Some(0.0)
            } else if speed > 0.0 && clip.tail_complete {
                Some(clip.tail_duration / speed)
            } else {
                None
            }
        };
        let span = if !plays(action) {
            tail()?
        } else if let Some(stop) = action["stop_after"].as_f64() {
            stop + tail()?
        } else if !clip.looping && clip.complete && speed > 0.0 {
            clip.duration / speed
        } else {
            // Real loops must keep their captured intro/loop_start. A bounded
            // recording without natural completion is never a replay boundary.
            return None;
        };
        if !span.is_finite() {
            return None;
        }
        found = true;
        longest = longest.max(span + number(action, "wait_after", 0.0).max(0.0));
    }
    found.then_some(longest.max(0.001))
}

#[derive(Clone)]
struct Instance {
    source: LayoutTarget,
    spec: Value,
    name: String,
    key: String,
    unique: bool,
    age: f64,
    last_time: f64,
    stopped_age: Option<f64>,
    deadline: Option<f64>,
    cleared: bool,
}
impl Instance {
    fn new(spec: &Value, source: LayoutTarget, now: f64) -> Self {
        let name = text(spec, "name").to_owned();
        let key = spec["tag"]
            .as_str()
            .filter(|tag| !tag.is_empty())
            .unwrap_or(&name)
            .to_owned();
        Self {
            source,
            name,
            key,
            unique: number(spec, "c", 1.0) == 1.0,
            spec: spec.clone(),
            age: 0.0,
            last_time: now,
            stopped_age: None,
            deadline: spec["stop_after"].as_f64().map(|seconds| now + seconds),
            cleared: false,
        }
    }
    fn stop(&mut self, clear: bool) {
        self.stopped_age.get_or_insert(self.age);
        self.cleared |= clear;
        self.deadline = None;
    }
    fn advance(&mut self, now: f64) {
        let speed = number(&self.spec, "speed", 1.0).max(0.0);
        if let Some(deadline) = self.deadline.filter(|deadline| *deadline <= now) {
            self.age += (deadline - self.last_time).max(0.0) * speed;
            self.last_time = deadline.max(self.last_time);
            self.stop(self.spec["clear"] == true);
        }
        self.age += (now - self.last_time).max(0.0) * speed;
        self.last_time = now;
    }
    fn ended(&self, library: Option<&Library>) -> bool {
        self.cleared
            || library.is_some_and(|library| {
                matches!(
                    library.sample(&self.name, self.age, self.stopped_age),
                    Sample::Ended
                )
            })
    }
    fn value(&self, library: Option<&Library>) -> Value {
        let LayoutTarget::Effect { node, action } = &self.source else {
            unreachable!()
        };
        let mut value = self.spec.clone();
        value["source_node"] = json!(node);
        value["source_action"] = json!(action);
        value["age"] = json!(self.age);
        value["opacity"] = json!(1.0);
        value["stopped_age"] = json!(self.stopped_age);
        value["emitting"] = json!(self.stopped_age.is_none());
        let sample = library.map_or(Sample::Missing, |library| {
            library.sample(&self.name, self.age, self.stopped_age)
        });
        value["cache_status"] = json!(sample.status());
        if let Some(clip) = library.and_then(|library| library.clips.get(&self.name)) {
            value["tail_sample_time"] = json!(clip.tail_sample_time);
            value["capture_complete"] = json!(clip.complete);
            value["capture_duration"] = json!(clip.duration);
        }
        value
    }
}
fn event(
    instances: &mut Vec<Instance>,
    spec: &Value,
    source: LayoutTarget,
    now: f64,
    managed: bool,
    library: Option<&Library>,
) {
    for instance in instances.iter_mut() {
        instance.advance(now);
    }
    let name = text(spec, "name");
    let tag = text(spec, "tag");
    let key = if tag.is_empty() { name } else { tag };
    if !plays(spec) {
        if managed {
            for instance in instances.iter_mut().filter(|instance| {
                if tag.is_empty() {
                    instance.name == name
                } else {
                    instance.key == key
                }
            }) {
                instance.stop(spec["clear"] == true);
            }
        } else if number(spec, "c", 1.0) == 1.0 {
            if let Some(instance) = instances
                .iter_mut()
                .find(|instance| instance.unique && instance.name == name)
            {
                instance.stop(false);
            }
        }
        return;
    }
    if number(spec, "c", 1.0) == 1.0 {
        if let Some(index) = instances
            .iter()
            .position(|instance| instance.unique && instance.key == key)
        {
            if instances[index].name == name {
                let instance = &mut instances[index];
                if spec["restart"] == true || instance.ended(library) {
                    instance.age = 0.0;
                }
                instance.spec = spec.clone();
                instance.source = source;
                instance.stopped_age = None;
                instance.cleared = false;
                instance.deadline = spec["stop_after"].as_f64().map(|seconds| now + seconds);
                return;
            }
            instances.remove(index);
        }
    }
    instances.push(Instance::new(spec, source, now));
}
pub fn simulate(
    story: &Value,
    target: &str,
    elapsed: f64,
    include_target: bool,
    library: Option<&Library>,
) -> Value {
    let nodes = story["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let path = super::route(story, target).unwrap_or_else(|| vec![target.to_owned()]);
    let managed = managed(story);
    let mut instances = Vec::new();
    let mut now = 0.0;
    for id in path {
        if id == target && !include_target {
            break;
        }
        let Some(parent) = nodes.iter().find(|node| node["id"] == id) else {
            continue;
        };
        if matches!(
            text(parent, "type"),
            "end" | "goto_scene" | "death" | "endgame" | "combat" | "battle"
        ) {
            instances.clear();
        }
        for (index, action) in lom_core::parallel::actions(parent).iter().enumerate() {
            if action["type"] == "effect" {
                event(
                    &mut instances,
                    action,
                    LayoutTarget::Effect {
                        node: id.clone(),
                        action: (parent["type"] == "parallel").then_some(index),
                    },
                    now,
                    managed,
                    library,
                );
            }
        }
        if id == target {
            now += elapsed.max(0.0);
            break;
        }
        now += super::playback_duration(parent);
    }
    for instance in &mut instances {
        instance.advance(now);
    }
    Value::Array(
        instances
            .iter()
            .filter(|instance| !instance.ended(library))
            .map(|instance| instance.value(library))
            .collect(),
    )
}
pub fn target(item: &Value) -> Option<LayoutTarget> {
    Some(LayoutTarget::Effect {
        node: item["source_node"].as_str()?.to_owned(),
        action: item["source_action"].as_u64().map(|index| index as usize),
    })
}
pub fn label(item: &Value) -> String {
    let tag = text(item, "tag");
    let ready = matches!(text(item, "cache_status"), "play" | "tail");
    let mut label = format!(
        "{} · {}{}",
        crate::i18n::tr(if ready {
            "原版特效"
        } else {
            "特效定位框"
        }),
        text(item, "name"),
        if tag.is_empty() {
            String::new()
        } else {
            format!(" [{tag}]")
        }
    );
    if number(item, "a", 1.0) == 0.0 || number(item, "b", 1.0) == 0.0 {
        label.push_str(&format!(" · {}", crate::i18n::tr("缩放为零，已隐藏")));
    }
    label
}
pub fn item<'a>(state: &'a Value, wanted: &LayoutTarget) -> Option<&'a Value> {
    state["effects"]
        .as_array()?
        .iter()
        .find(|value| target(value).as_ref() == Some(wanted))
}
pub fn item_mut<'a>(state: &'a mut Value, wanted: &LayoutTarget) -> Option<&'a mut Value> {
    state
        .get_mut("effects")?
        .as_array_mut()?
        .iter_mut()
        .find(|value| target(value).as_ref() == Some(wanted))
}

pub(super) fn extraction_names(state: &Value, refresh: bool) -> Vec<String> {
    let mut names = Vec::new();
    for item in state["effects"].as_array().into_iter().flatten() {
        if refresh || matches!(text(item, "cache_status"), "missing" | "missing_tail") {
            let name = text(item, "name");
            if !name.is_empty() && !names.iter().any(|existing| existing == name) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

pub struct Clock {
    key: String,
    snapshot: Value,
    started: f64,
    held: f64,
    last: f64,
    paused: bool,
    manual: bool,
    replaying: bool,
    looping: bool,
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            key: String::new(),
            snapshot: Value::Null,
            started: 0.0,
            held: 0.0,
            last: 0.0,
            paused: false,
            manual: false,
            replaying: false,
            looping: true,
        }
    }
}
impl Clock {
    pub fn sync(&mut self, project: &Project, story: &Value, target: &str, now: f64) {
        let key = format!(
            "{:?}:{}:{}:{}:{target}",
            project.source, project.manifest["id"], project.manifest["campaign_id"], story["id"]
        );
        if self.key != key || self.snapshot != *story {
            self.key = key;
            self.snapshot = story.clone();
            self.started = now;
            self.held = 0.0;
            self.last = 0.0;
            self.paused = false;
            self.manual = false;
            self.replaying = false;
        }
    }
    pub fn time(
        &mut self,
        now: f64,
        external: Option<f64>,
        cycle: Option<f64>,
        layout: bool,
    ) -> f64 {
        self.last = if layout {
            0.0
        } else if self.paused {
            self.held
        } else if !self.manual && external.is_some() {
            external.unwrap().max(0.0)
        } else {
            (now - self.started).max(0.0)
        };
        if !layout
            && self.looping
            && (external.is_none() || self.replaying)
            && cycle.is_some_and(|seconds| seconds.is_finite() && seconds > 0.0)
        {
            self.last % cycle.unwrap()
        } else {
            self.last
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, selected_effect: bool, layout: bool) -> bool {
        use crate::i18n::tr;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(tr("原版特效预览")).strong());
            if ui
                .add_enabled(!layout, egui::Button::new(tr("重播预览")))
                .clicked()
            {
                self.started = ui.input(|input| input.time);
                self.held = 0.0;
                self.last = 0.0;
                self.paused = false;
                self.manual = true;
                self.replaying = true;
                changed = true;
            }
            if ui
                .add_enabled(
                    !layout,
                    egui::Button::new(tr(if self.paused {
                        "继续预览"
                    } else {
                        "暂停预览"
                    })),
                )
                .clicked()
            {
                self.paused = !self.paused;
                self.held = self.last;
                self.started = ui.input(|input| input.time) - self.held;
                self.manual = true;
                changed = true;
            }
            if selected_effect {
                changed |= ui.checkbox(&mut self.looping, tr("循环预览")).changed();
            }
        });
        ui.small(tr(
            "画面使用本机游戏实际渲染的原版帧；停止尾段为固定时刻采样，精确效果请用 F5 实机确认。",
        ));
        if !self.paused && !layout {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
        changed
    }
}

fn local_point(center: Pos2, units: Vec2, a: f32, b: f32, rotation: f32, point: Vec2) -> Pos2 {
    center
        + egui::emath::Rot2::from_angle(-rotation.to_radians())
            * Vec2::new(point.x * units.x * a, point.y * units.y * b)
}

pub(super) fn visible_bounds(image: &image::RgbaImage) -> Rect {
    let mut bounds = Rect::NOTHING;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0.iter().any(|channel| *channel != 0) {
            bounds.extend_with(Pos2::new(x as f32, y as f32));
            bounds.extend_with(Pos2::new(x as f32 + 1.0, y as f32 + 1.0));
        }
    }
    if !bounds.is_positive() {
        return Rect::ZERO;
    }
    let size = Vec2::new(image.width() as f32, image.height() as f32);
    Rect::from_min_max(
        (bounds.min.to_vec2() / size).to_pos2(),
        (bounds.max.to_vec2() / size).to_pos2(),
    )
}

pub(super) fn paint(
    painter: &egui::Painter,
    stage: Rect,
    items: &Value,
    layout: bool,
    library: Option<&Library>,
    mut texture: impl FnMut(&Frame, [usize; 2]) -> Option<(TextureHandle, Rect)>,
) -> Vec<LayoutObject> {
    let mut objects = Vec::new();
    for item in items.as_array().into_iter().flatten() {
        let (x, y) = screen_position(item);
        let center = stage.min
            + Vec2::new(
                stage.width() * x as f32 / 100.0,
                stage.height() * y as f32 / 100.0,
            );
        let a = number(item, "a", 1.0) as f32;
        let b = number(item, "b", 1.0) as f32;
        let rotation = number(item, "rotation", 0.0) as f32;
        if !center.is_finite() || !a.is_finite() || !b.is_finite() || !rotation.is_finite() {
            continue;
        }
        let units = Vec2::new(
            stage.width() / WORLD_WIDTH as f32,
            stage.height() / WORLD_HEIGHT as f32,
        );
        let extent = library.map_or(
            Vec2::new(WORLD_WIDTH as f32, WORLD_HEIGHT as f32),
            |library| library.world_size,
        ) / 2.0;
        let point = |p: Vec2| local_point(center, units, a, b, rotation, p);
        let corners = [
            Vec2::new(-extent.x, -extent.y),
            Vec2::new(extent.x, -extent.y),
            Vec2::new(extent.x, extent.y),
            Vec2::new(-extent.x, extent.y),
        ];
        if corners
            .iter()
            .any(|corner| !point(*corner * 2.0).is_finite())
        {
            continue;
        }
        let mut bounds = Rect::from_center_size(center, Vec2::splat(12.0));
        if a != 0.0 && b != 0.0 {
            if let Some(library) = library {
                let sample = library.sample(
                    text(item, "name"),
                    number(item, "age", 0.0),
                    item["stopped_age"].as_f64(),
                );
                let sample = if layout && matches!(&sample, Sample::Play(_)) {
                    library.sample(
                        text(item, "name"),
                        library.clips[text(item, "name")].preview_time,
                        None,
                    )
                } else {
                    sample
                };
                if let Sample::Play(frame) | Sample::Tail(frame) = sample {
                    if let Some((texture, opaque)) = texture(frame, library.size) {
                        if opaque.is_positive() && opaque.is_finite() {
                            let local = |uv: Pos2| {
                                local_point(
                                    center,
                                    units,
                                    a,
                                    b,
                                    0.0,
                                    (uv.to_vec2() * 2.0 - Vec2::splat(1.0)) * extent,
                                )
                            };
                            // Keep bounds unrotated; existing handles rotate them
                            // around the original effect origin exactly once.
                            bounds = Rect::from_two_pos(local(opaque.min), local(opaque.max));
                        }
                        let mut mesh = egui::Mesh::with_texture(texture.id());
                        let uv = [
                            Pos2::new(0.0, 0.0),
                            Pos2::new(1.0, 0.0),
                            Pos2::new(1.0, 1.0),
                            Pos2::new(0.0, 1.0),
                        ];
                        let color = Color32::WHITE
                            .gamma_multiply(number(item, "opacity", 1.0).clamp(0.0, 1.0) as f32);
                        for i in 0..4 {
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos: point(corners[i]),
                                uv: uv[i],
                                color,
                            });
                        }
                        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                        painter.add(egui::Shape::mesh(mesh));
                    }
                }
            }
        }
        if layout {
            if let Some(target) = target(item) {
                let (x, y) = coordinates(item);
                objects.push(LayoutObject {
                    target: target.clone(),
                    bounds,
                    pivot: center,
                    angle: -rotation,
                    value: LayoutChange {
                        target,
                        x,
                        y,
                        scale: 100.0,
                    },
                    units: if item["space"] == "screen" {
                        Vec2::new(stage.width() / 100.0, -stage.height() / 100.0)
                    } else {
                        units
                    },
                });
            }
        }
    }
    objects
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(looping: bool, complete: bool) -> Clip {
        Clip {
            frames: [0.0, 0.5, 1.0, 1.5]
                .into_iter()
                .map(|time| Frame {
                    file: PathBuf::from(format!("play-{time}.png")),
                    time,
                })
                .collect(),
            duration: 2.0,
            preview_time: 0.5,
            looping,
            loop_start: 1.0,
            complete,
            tail_frames: [0.0, 0.5]
                .into_iter()
                .map(|time| Frame {
                    file: PathBuf::from(format!("tail-{time}.png")),
                    time,
                })
                .collect(),
            tail_duration: 1.0,
            tail_sample_time: Some(1.25),
            tail_complete: true,
        }
    }
    fn library(clip: Clip) -> Library {
        Library {
            size: [640, 360],
            world_size: Vec2::new(19.2, 10.8),
            clips: ["Rain_001", "Glow_001"]
                .into_iter()
                .map(|name| (name.into(), clip.clone()))
                .collect(),
        }
    }
    fn sample_file(sample: Sample<'_>) -> PathBuf {
        match sample {
            Sample::Play(frame) | Sample::Tail(frame) => frame.file.clone(),
            _ => panic!("expected captured frame"),
        }
    }
    #[test]
    fn captured_metadata_controls_intro_loop_natural_end_and_truncation() {
        let looping = library(clip(true, false));
        assert_eq!(
            sample_file(looping.sample("Rain_001", 0.5, None)),
            PathBuf::from("play-0.5.png")
        );
        assert_eq!(
            sample_file(looping.sample("Rain_001", 2.25, None)),
            PathBuf::from("play-1.png")
        );
        assert_eq!(
            sample_file(looping.sample("Rain_001", 13.75, None)),
            PathBuf::from("play-1.5.png")
        );
        let complete = library(clip(false, true));
        assert!(matches!(
            complete.sample("Rain_001", 2.0, None),
            Sample::Ended
        ));
        let truncated = library(clip(false, false));
        assert!(matches!(
            truncated.sample("Rain_001", 2.0, None),
            Sample::Truncated
        ));
        assert!(matches!(
            truncated.sample("unknown", 0.0, None),
            Sample::Missing
        ));
    }
    #[test]
    fn stops_use_only_recorded_tail_and_do_not_invent_fade_or_natural_end() {
        let mut captured = clip(true, false);
        let complete = library(captured.clone());
        assert_eq!(
            sample_file(complete.sample("Rain_001", 10.6, Some(10.0))),
            PathBuf::from("tail-0.5.png")
        );
        assert!(matches!(
            complete.sample("Rain_001", 11.0, Some(10.0)),
            Sample::Ended
        ));
        captured.tail_complete = false;
        let truncated = library(captured.clone());
        assert!(matches!(
            truncated.sample("Rain_001", 11.0, Some(10.0)),
            Sample::TailTruncated
        ));
        captured.tail_sample_time = None;
        let missing = library(captured);
        assert!(matches!(
            missing.sample("Rain_001", 10.0, Some(10.0)),
            Sample::MissingTail
        ));
    }
    #[test]
    fn cache_load_requires_original_premultiplied_frames_and_confined_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("frame.png"), []).unwrap();
        let mut map = json!({"format":1,"source":"game-render","alpha_mode":"premultiplied",
            "width":640,"height":360,"world_width":19.2,"world_height":10.8,
            "effects":{"Rain_001":{"frames":[{"file":"frame.png","time":0}],
                "duration":1,"preview_time":0,"loop_start":0,"looping":false,"complete":false,
                "tail_frames":[],"tail_duration":0,"tail_sample_time":0.5,"tail_complete":true}}});
        let save = |map: &Value| {
            std::fs::write(
                temp.path().join("effect_map.json"),
                serde_json::to_vec(map).unwrap(),
            )
            .unwrap()
        };
        save(&map);
        assert_eq!(Library::load(temp.path()).unwrap().size, [640, 360]);
        map["alpha_mode"] = json!("straight");
        save(&map);
        assert!(Library::load(temp.path())
            .unwrap_err()
            .to_string()
            .contains("预乘"));
        map["alpha_mode"] = json!("premultiplied");
        map["effects"]["Rain_001"]["frames"][0]["file"] = json!("../outside.png");
        save(&map);
        assert!(Library::load(temp.path()).is_err());
    }
    #[test]
    fn current_cache_merges_batches_in_order_and_rejects_mismatched_geometry() {
        let temp = tempfile::tempdir().unwrap();
        let requests = [
            "11111111111111111111111111111111",
            "22222222222222222222222222222222",
        ];
        for (index, request) in requests.iter().enumerate() {
            let directory = temp.path().join(request);
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("frame.png"), []).unwrap();
            let entry = json!({"frames":[{"file":"frame.png","time":0}],"duration":1,"preview_time":0,
                "loop_start":0,"looping":false,"complete":true});
            let effects = if index == 0 {
                json!({"Rain_001":entry})
            } else {
                json!({"Rain_001":entry,"Glow_001":entry})
            };
            std::fs::write(directory.join("effect_map.json"),serde_json::to_vec(&json!({
                "format":1,"source":"game-render","alpha_mode":"premultiplied",
                "width":640,"height":360,"world_width":19.2,"world_height":10.8,"effects":effects
            })).unwrap()).unwrap();
        }
        std::fs::write(
            temp.path().join("current.json"),
            serde_json::to_vec(&json!({"format":1,"request_ids":requests})).unwrap(),
        )
        .unwrap();
        let merged = Library::current_at(temp.path()).unwrap().unwrap();
        assert_eq!(
            merged.names().collect::<Vec<_>>(),
            vec!["Glow_001", "Rain_001"]
        );
        assert!(merged.contains("Rain_001"));
        assert_eq!(
            sample_file(merged.sample("Rain_001", 0.0, None)),
            temp.path()
                .join(requests[1])
                .join("frame.png")
                .canonicalize()
                .unwrap()
        );
        let mut first = Library::load(&temp.path().join(requests[0])).unwrap();
        let mut second = Library::load(&temp.path().join(requests[1])).unwrap();
        second.size[0] = 320;
        assert!(first.merge(second).is_err());
        assert!(
            !first.contains("Glow_001"),
            "incompatible batch must not partially merge"
        );
    }
    #[test]
    fn extraction_requests_only_current_missing_names_unless_current_cache_needs_refresh() {
        let state = json!({"effects":[
            {"name":"Rain_001","cache_status":"play"},
            {"name":"Glow_001","cache_status":"missing"},
            {"name":"Glow_001","cache_status":"missing"},
            {"name":"EventBubble_001","cache_status":"missing_tail"},
            {"name":"Hit_001","cache_status":"truncated"}
        ]});
        assert_eq!(
            extraction_names(&state, false),
            vec!["Glow_001", "EventBubble_001"]
        );
        assert_eq!(
            extraction_names(&state, true),
            vec!["Rain_001", "Glow_001", "EventBubble_001", "Hit_001"]
        );
        assert!(extraction_names(&json!({"effects":[]}), true).is_empty());
    }
    #[test]
    fn unique_replay_resets_deadline_without_collecting_stacked_instances() {
        let captured = library(clip(true, false));
        let story = json!({"start":"first","nodes":[
            {"id":"first","type":"effect","name":"Rain_001","space":"world","c":1,"tag":"weather","stop_after":0.2,"clear":true,"wait_after":0.1},
            {"id":"stack","type":"effect","name":"Rain_001","c":0,"tag":"weather","wait_after":0.1},
            {"id":"replay","type":"effect","name":"Rain_001","c":1,"tag":"weather","stop_after":3.0},
            {"id":"stop","type":"effect","name":"unused","play":false,"tag":"weather","clear":true}
        ]});
        let active = simulate(&story, "replay", 0.4, true, Some(&captured));
        assert_eq!(active.as_array().unwrap().len(), 2);
        assert_eq!(active[0]["source_node"], "replay");
        assert_eq!(active[1]["source_node"], "stack");
        assert_eq!(active[0]["cache_status"], "play");
        assert!(simulate(&story, "stop", 0.0, true, Some(&captured))
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn same_tag_different_name_replaces_only_reuse_instance_and_stop_name_matches_tags() {
        let captured = library(clip(true, false));
        let story = json!({"start":"first","nodes":[
            {"id":"first","type":"effect","name":"Rain_001","c":1,"tag":"weather"},
            {"id":"stack","type":"effect","name":"Rain_001","c":0,"tag":"weather"},
            {"id":"replace","type":"effect","name":"Glow_001","c":1,"tag":"weather"},
            {"id":"stop","type":"effect","name":"Rain_001","play":false,"clear":true}
        ]});
        let active = simulate(&story, "replace", 0.0, true, Some(&captured));
        assert_eq!(active.as_array().unwrap().len(), 2);
        assert_eq!(active[0]["source_node"], "stack");
        assert_eq!(active[1]["name"], "Glow_001");
        let stopped = simulate(&story, "stop", 0.0, true, Some(&captured));
        assert_eq!(stopped.as_array().unwrap().len(), 1);
        assert_eq!(stopped[0]["name"], "Glow_001");
    }
    #[test]
    fn speed_scales_frame_time_but_stop_after_uses_unscaled_time() {
        let captured = library(clip(true, false));
        let mut story = json!({"start":"play","nodes":[
            {"id":"play","type":"effect","name":"Rain_001","speed":2.0,"stop_after":0.25}
        ]});
        let active = simulate(&story, "play", 0.5, true, Some(&captured));
        assert_eq!(active[0]["age"], 1.0);
        assert_eq!(active[0]["stopped_age"], 0.5);
        assert_eq!(active[0]["cache_status"], "tail");
        assert_eq!(active[0]["opacity"], 1.0);
        story["nodes"][0]["speed"] = json!(0.0);
        let frozen = simulate(&story, "play", 2.0, true, Some(&captured));
        assert_eq!(frozen[0]["age"], 0.0);
        assert_eq!(frozen[0]["emitting"], false);
        story["nodes"][0]["clear"] = json!(true);
        assert!(simulate(&story, "play", 0.3, true, Some(&captured))
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn legacy_nonunique_stop_does_not_stop_earlier_instance() {
        let captured = library(clip(true, false));
        let story = json!({"start":"play","nodes":[
            {"id":"play","type":"effect","name":"Rain_001","c":0,"d":1},
            {"id":"stop","type":"effect","name":"Rain_001","c":0,"d":0}
        ]});
        let active = simulate(&story, "stop", 0.0, true, Some(&captured));
        assert_eq!(active.as_array().unwrap().len(), 1);
        assert_eq!(active[0]["emitting"], true);
        assert!(!plays(&json!({"d":2})));
        assert!(plays(&json!({"play":true,"d":0})));
    }
    #[test]
    fn routed_parallel_sources_and_scene_cleanup_are_preserved() {
        let captured = library(clip(true, false));
        let story = json!({"start":"choose","nodes":[
            {"id":"choose","type":"choice","options":[{"goto":"a"},{"goto":"b"}]},
            {"id":"a","type":"parallel","actions":[{"type":"wait","seconds":1},{"type":"effect","name":"Rain_001"}],"goto":"say"},
            {"id":"b","type":"effect","name":"Glow_001","goto":"say"},
            {"id":"say","type":"say"},
            {"id":"done","type":"end"}
        ]});
        let before = story.clone();
        let active = simulate(&story, "say", 0.0, true, Some(&captured));
        assert_eq!(active.as_array().unwrap().len(), 1);
        assert_eq!(active[0]["source_node"], "a");
        assert_eq!(active[0]["source_action"], 1);
        assert!(simulate(&story, "done", 0.0, true, Some(&captured))
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(story, before);
    }
    #[test]
    fn replay_cycle_uses_capture_and_global_clock_does_not_resurrect_auto_stop() {
        let captured = library(clip(false, true));
        let node = json!({"type":"effect","name":"Rain_001","stop_after":0.2,"clear":true,"wait_after":3.0});
        assert_eq!(cycle(&node, None), None);
        assert_eq!(cycle(&node, Some(&captured)), Some(3.2));
        let mut clock = Clock::default();
        assert_eq!(clock.time(10.0, Some(6.45), Some(3.2), false), 6.45);
        assert!((clock.time(6.45, None, Some(3.2), false) - 0.05).abs() < 1e-9);
    }
    #[test]
    fn native_loops_and_truncated_recordings_are_not_replayed_from_the_intro() {
        let node = json!({"type":"effect","name":"Rain_001"});
        let looping = library(clip(true, false));
        assert_eq!(cycle(&node, Some(&looping)), None);
        let truncated = library(clip(false, false));
        assert_eq!(cycle(&node, Some(&truncated)), None);
        let complete = library(clip(false, true));
        assert_eq!(cycle(&node, Some(&complete)), Some(2.0));
        let mut clock = Clock::default();
        let elapsed = clock.time(2.25, None, cycle(&node, Some(&looping)), false);
        assert_eq!(
            sample_file(looping.sample("Rain_001", elapsed, None)),
            PathBuf::from("play-1.png")
        );
    }
    #[test]
    fn absent_cache_never_paints_a_synthetic_effect_but_allows_positioning_frame() {
        let context = egui::Context::default();
        let mut objects = Vec::new();
        let output = context.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            objects = paint(
                &painter,
                Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0)),
                &json!([{"name":"unknown","source_node":"fx","source_action":null}]),
                true,
                None,
                |_, _| panic!("no frame to load without cache"),
            );
        });
        assert!(output.shapes.is_empty());
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].bounds.size(), Vec2::splat(12.0));
        assert!(label(&json!({"name":"unknown","cache_status":"missing"}))
            .contains(&crate::i18n::tr("特效定位框")));
    }
    #[test]
    fn captured_frame_uses_signed_scale_extra_rotation_and_premultiplied_tint() {
        let context = egui::Context::default();
        let captured = library(clip(true, false));
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
        let output=context.run(egui::RawInput::default(),|ctx| {
            let texture=ctx.load_texture("real-frame",egui::ColorImage::new([1,1],vec![Color32::WHITE]),egui::TextureOptions::LINEAR);
            paint(&ctx.layer_painter(egui::LayerId::background()).with_clip_rect(stage),stage,
                &json!([{"name":"Rain_001","x":1,"y":2,"a":-2,"b":0.5,"rotation":90,"age":0,"opacity":0.5}]),
                false,Some(&captured),|_,_|Some((texture.clone(),uv())));
        });
        let mesh = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .unwrap();
        for (vertex, expected) in mesh.vertices.iter().zip([
            (790.0, -1580.0),
            (790.0, 2260.0),
            (1330.0, 2260.0),
            (1330.0, -1580.0),
        ]) {
            assert!((vertex.pos - Pos2::new(expected.0, expected.1)).length() < 0.001);
            assert_eq!(vertex.color, Color32::WHITE.gamma_multiply(0.5));
        }
        assert_eq!(output.shapes[0].clip_rect, stage);
    }
    #[test]
    fn finite_source_that_overflows_transformed_geometry_is_not_submitted_to_egui() {
        let context = egui::Context::default();
        let captured = library(clip(true, false));
        let output = context.run(egui::RawInput::default(), |ctx| {
            let objects = paint(
                &ctx.layer_painter(egui::LayerId::background()),
                Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0)),
                &json!([{"name":"Rain_001","a":3e38,"b":3e38,"source_node":"fx"}]),
                true,
                Some(&captured),
                |_, _| panic!("invalid transformed geometry must be skipped before loading"),
            );
            assert!(objects.is_empty());
        });
        assert!(output.shapes.is_empty());
    }
}
