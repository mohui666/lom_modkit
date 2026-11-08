use crate::{
    images,
    unity::{self, Reader},
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use unity_rs_core::{
    scene::{read_animator, read_game_object},
    studio::{Studio, StudioObject},
};

pub fn extract(studio: &Studio, output: &Path) -> Result<Value> {
    let panels = studio
        .objects()
        .filter(|o| {
            o.class_id() == 114 && unity::basename(o.source_path()) == "sharedassets2.assets"
        })
        .filter_map(|o| match unity::script_type(studio, o) {
            Ok((_, name)) if name == "CharacterMoodPanel" => Some(Ok(o)),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(panels.len() == 1, "无法唯一定位原版 CharacterMoodPanel");
    let panel = panels[0];
    let bytes = unity::raw(panel)?;
    let mut r = Reader::at(&bytes, 28);
    r.string()?;
    let mut cache = BTreeMap::new();
    let mut moods = serde_json::Map::new();
    for _ in 0..r.count(1000)? {
        let right = r.ptr()?;
        let left = r.ptr()?;
        let count = r.count(1000)?;
        ensure!(
            right.file_id == 0 && left.file_id == 0,
            "气泡预制体使用了未知外部引用"
        );
        let mut entries = serde_json::Map::new();
        for (name, ptr) in [("left", left), ("right", right)] {
            if !cache.contains_key(&ptr.path_id) {
                cache.insert(
                    ptr.path_id,
                    animation(studio, unity::resolve(studio, panel, ptr)?, output)?,
                );
            }
            entries.insert(name.into(), cache[&ptr.path_id].clone());
        }
        for _ in 0..count {
            let ptr = r.ptr()?;
            ensure!(ptr.file_id == 0, "气泡映射使用了未知外部引用");
            let key = unity::mapping(studio, panel, ptr)?;
            moods.insert(key, Value::Object(entries.clone()));
        }
    }
    Ok(Value::Object(moods))
}
fn animation(studio: &Studio, object: StudioObject<'_>, output: &Path) -> Result<Value> {
    let file = &studio.collection().serialized_files()[object.file_index()].file;
    let game_object = read_game_object(file, object.object_index(), Default::default())?;
    let components = game_object
        .components
        .iter()
        .map(|x| unity::resolve(studio, object, x.component))
        .collect::<Result<Vec<_>>>()?;
    let rect = *components
        .iter()
        .find(|o| o.class_id() == 224)
        .context("气泡缺少 RectTransform")?;
    let animator = *components
        .iter()
        .find(|o| o.class_id() == 95)
        .context("气泡缺少 Animator")?;
    let file = &studio.collection().serialized_files()[animator.file_index()].file;
    let animator_data = read_animator(file, animator.object_index(), Default::default())?;
    let controller_obj = unity::resolve(studio, animator, animator_data.controller)?;
    let controller = controller_obj.read_animator_controller(Default::default())?;
    let machine = controller
        .controller
        .state_machines
        .first()
        .context("气泡 Animator 没有状态机")?;
    let mut state_index = machine.default_state;
    let mut visited = BTreeSet::new();
    let mut clips = vec![];
    while visited.insert(state_index) {
        let state = machine
            .states
            .get(state_index as usize)
            .context("气泡动画状态无效")?;
        ensure!(
            state.blend_trees.len() == 1
                && state.blend_trees[0].nodes.len() == 1
                && state.speed == 1.0,
            "气泡 Animator 使用了未知混合或速度"
        );
        let clip_id = state.blend_trees[0].nodes[0].clip_id as usize;
        clips.push(unity::resolve(
            studio,
            controller_obj,
            *controller
                .animation_clips
                .get(clip_id)
                .context("气泡动画引用无效")?,
        )?);
        if state.transitions.is_empty() {
            break;
        }
        ensure!(
            state.transitions.len() == 1,
            "气泡 Animator 使用了未知切换规则"
        );
        let transition = &state.transitions[0];
        ensure!(
            transition.conditions.is_empty()
                && transition.flags.has_exit_time()
                && transition.exit_time == 1.0
                && transition.transition_duration == 0.0,
            "气泡 Animator 使用了未知切换规则"
        );
        state_index = transition.destination_state;
    }
    let mut frames = vec![];
    let mut duration = 0.0f64;
    let mut loop_start = None;
    for clip_obj in clips {
        let clip = clip_obj.read_animation_clip(Default::default())?;
        let muscle = clip.muscle_clip.context("气泡动画没有精灵曲线")?;
        if muscle.flags.loop_time() {
            loop_start = Some(duration);
        }
        let stream = muscle.clip.streamed;
        ensure!(stream.curve_count == 1, "气泡动画曲线结构已改变");
        let words = stream.data.read_values(1_000_000)?;
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let mut r = Reader::new(&bytes);
        let mut clip_frames = 0;
        while r.at < bytes.len() {
            let time = r.f32()?;
            let count = r.count(100000)?;
            for _ in 0..count {
                let curve = r.i32()?;
                r.skip(12)?;
                let value = r.f32()?;
                ensure!(
                    curve == 0 && value.is_finite() && value >= 0.0 && value.fract() == 0.0,
                    "气泡动画包含未知精灵帧"
                );
                let ptr = *clip
                    .binding_constant
                    .pptr_curve_mapping
                    .get(value as usize)
                    .context("气泡动画精灵帧越界")?;
                let sprite = unity::resolve(studio, clip_obj, ptr)?;
                let relative = format!("assets/moods/{}/{}.png", object.path_id(), ptr.path_id);
                let path = output.join(&relative);
                if !path.exists() {
                    images::save(&images::decode(studio, sprite, true)?, &path, None)?;
                }
                frames.push(json!({"time":duration+time.max(0.0) as f64,"file":relative}));
                clip_frames += 1;
            }
        }
        ensure!(clip_frames > 0, "气泡动画没有精灵帧");
        duration += muscle.stop_time as f64;
    }
    // RectTransform extends Transform after the child/father references.
    let bytes = unity::raw(rect)?;
    let mut r = Reader::at(&bytes, 12 + 16 + 12);
    let scale = [r.f32()?, r.f32()?];
    r.skip(4)?;
    let count = r.count(100000)?;
    r.skip(count * 12 + 12 + 16)?;
    let offset = [r.f32()?, r.f32()?];
    let size = [r.f32()?, r.f32()?];
    let pivot = [r.f32()?, r.f32()?];
    Ok(
        json!({"frames":frames,"duration":duration,"loop_start":loop_start,"size":size,"offset":offset,"pivot":pivot,"scale":scale}),
    )
}
