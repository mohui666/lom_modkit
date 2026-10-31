//! Shared authoring rules for a joined group of concurrent presentation actions.
use anyhow::{ensure, Result};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MAX_ACTIONS: usize = 32;
pub const ACTION_TYPES: &[&str] = &[
    "show",
    "portrait",
    "hide",
    "move",
    "face",
    "focus",
    "offset",
    "rotate",
    "dim",
    "shock",
    "scene",
    "background",
    "custom_cg",
    "overlay",
    "effect",
    "camera",
    "transition",
    "mask",
    "dayenv",
    "music",
    "sound",
    "wait",
];

pub fn actions(node: &Value) -> &[Value] {
    if node["type"] == "parallel" {
        node["actions"].as_array().map(Vec::as_slice).unwrap_or(&[])
    } else {
        std::slice::from_ref(node)
    }
}

/// Resource and character scanners must include actions inside a group. The
/// parent remains the only flow-graph vertex and the diagnostic navigation target.
pub fn story_actions(story: &Value) -> impl Iterator<Item = (&Value, &Value)> {
    story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|parent| actions(parent).iter().map(move |action| (parent, action)))
}

fn channels(node: &Value) -> Vec<String> {
    let kind = node["type"].as_str().unwrap_or("");
    let character = node["character"].as_str().unwrap_or("");
    let mut result = Vec::new();
    if !character.is_empty() {
        result.push(format!("人物 {character}"));
    }
    match kind {
        "scene" => result.extend(["背景".into(), "剧情界面".into()]),
        "background" => result.push("背景".into()),
        "transition" | "mask" => result.push("剧情界面".into()),
        "custom_cg" => result.push("全屏插图".into()),
        "overlay" => result.push(format!("前景位置 {}", node["slot"].as_str().unwrap_or(""))),
        "camera" => result.push(format!("镜头滤镜 {}", node["name"].as_str().unwrap_or(""))),
        "effect" => result.push(format!("特效 {}", node["name"].as_str().unwrap_or(""))),
        "dayenv" => result.push("日夜环境".into()),
        // Native music changes can also stop the environment-sound channel.
        "music" => result.extend(["背景音乐".into(), "环境音".into()]),
        "sound" if node["kind"] == "env" || node["op"] == "fadeout" => result.push("环境音".into()),
        // The official shock flowchart has one shared Character/Duration pair.
        "shock" if !character.starts_with("user:") => result.push("原版人物震动流图".into()),
        _ => {}
    }
    result
}

pub fn validate_group(node: &Value) -> Result<()> {
    let actions = node["actions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("同时执行组的 actions 必须是数组"))?;
    ensure!(
        (2..=MAX_ACTIONS).contains(&actions.len()),
        "同时执行组需要 2~{MAX_ACTIONS} 个动作"
    );
    let mut owners = BTreeMap::new();
    for (index, action) in actions.iter().enumerate() {
        let number = index + 1;
        ensure!(action.is_object(), "第 {number} 个同时动作必须是对象");
        let kind = action["type"].as_str().unwrap_or("");
        ensure!(ACTION_TYPES.contains(&kind), "第 {number} 个同时动作不支持类型 {kind:?}；对白、选项、跳转、原生 Lua 和嵌套组请放在组外");
        ensure!(
            action.get("id").is_none() && action.get("goto").is_none(),
            "第 {number} 个同时动作不允许 id/goto；整组完成后才统一继续"
        );
        ensure!(
            action.get("appearance").is_none(),
            "第 {number} 个同时动作：赵活外观切换请放在同时执行组之前"
        );
        for field in [
            "duration",
            "seconds",
            "fade",
            "fade_out",
            "fade_in",
            "fadeDuration",
            "moveDuration",
        ] {
            if let Some(value) = action.get(field) {
                ensure!(
                    value.as_f64().is_some_and(|v| v.is_finite() && v >= 0.0),
                    "第 {number} 个同时动作的 {field} 必须是非负有限数值"
                );
            }
        }
        for channel in channels(action) {
            if let Some(previous) = owners.insert(channel.clone(), number) {
                anyhow::bail!(
                    "第 {previous} 和第 {number} 个同时动作都会控制{channel}，请拆成先后步骤"
                );
            }
        }
    }
    Ok(())
}
