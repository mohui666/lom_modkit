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
    "mood",
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
    "render_priority",
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
        let properties: &[&str] = match kind {
            "move" | "shock" => &["位置"],
            "offset" => &["位置", "大小"],
            "rotate" => &["角度"],
            "face" => &["朝向"],
            "dim" => &["明暗"],
            "focus" => &["前后顺序"],
            "show" | "hide" | "portrait" => &["位置", "角度", "朝向", "大小", "明暗", "立绘"],
            _ => &[],
        };
        result.extend(
            properties
                .iter()
                .map(|property| format!("人物 {character} 的{property}")),
        );
        if matches!(kind, "mood" | "show" | "hide") {
            result.push(format!("人物 {character} 的气泡"));
        }
    }
    match kind {
        "render_priority" => {
            let (target, key, tag) = crate::render_priority::identity(node);
            result.push(format!("渲染顺序 {target}/{key}/{tag}"));
        }
        "scene" => result.extend(["背景".into(), "剧情界面".into()]),
        "background" => result.push("背景".into()),
        "transition" | "mask" => result.push("剧情界面".into()),
        "custom_cg" => result.push("全屏插图".into()),
        "overlay" => result.push(format!("前景位置 {}", node["slot"].as_str().unwrap_or(""))),
        "camera" => result.push(format!("镜头滤镜 {}", node["name"].as_str().unwrap_or(""))),
        "effect" => {
            let playing = node["play"]
                .as_bool()
                .unwrap_or_else(|| node.get("d").is_none_or(|v| v.as_f64() == Some(1.0)));
            if playing && node["c"].as_f64().unwrap_or(1.0) != 1.0 {
                return result;
            }
            let tag = node["tag"].as_str().unwrap_or("");
            let effective_tag = if tag.is_empty() {
                node["name"].as_str().unwrap_or("")
            } else {
                tag
            };
            result.push(format!("特效实例 {effective_tag}"));
        }
        "dayenv" => result.push("日夜环境".into()),
        "mood" if character.is_empty() => result.push("全场气泡".into()),
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
    if actions.iter().any(|action| {
        action["type"] == "mood" && action["character"].as_str().unwrap_or("").is_empty()
    }) {
        ensure!(
            actions
                .iter()
                .filter(|action| action["type"] == "mood")
                .count()
                == 1,
            "全场气泡开关和单个人物气泡请拆成先后步骤，避免同时清除或覆盖设置"
        );
    }
    // A stop without a tag addresses every tracked instance of that name,
    // including tagged instances. Distinct tagged plays remain independent.
    for (index, action) in actions.iter().enumerate() {
        let playing = action["play"].as_bool().unwrap_or_else(|| {
            action
                .get("d")
                .is_none_or(|value| value.as_f64() == Some(1.0))
        });
        if action["type"] == "effect" && !playing {
            for (other_index, other) in actions.iter().enumerate() {
                let other_playing = other["play"].as_bool().unwrap_or_else(|| {
                    other
                        .get("d")
                        .is_none_or(|value| value.as_f64() == Some(1.0))
                });
                let tag = action["tag"].as_str().unwrap_or("");
                let other_tag = other["tag"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| other["name"].as_str())
                    .unwrap_or("");
                let matches = if tag.is_empty() {
                    other["name"] == action["name"]
                } else {
                    other_tag == tag
                };
                if other_index != index && other["type"] == "effect" && matches && other_playing {
                    anyhow::bail!(
                        "第 {} 和第 {} 个同时动作会播放和停止同一特效实例；请拆成先后步骤",
                        index + 1,
                        other_index + 1
                    );
                }
            }
        }
    }
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
                    "第 {previous} 和第 {number} 个同时动作都会控制{channel}（{} / {}），请拆成先后步骤",
                    action_label(actions[previous-1]["type"].as_str().unwrap_or("")),action_label(kind)
                );
            }
        }
    }
    Ok(())
}

fn action_label(kind: &str) -> &str {
    match kind {
        "show" => "人物登场",
        "hide" => "人物退场",
        "portrait" => "人物换表情",
        "move" => "人物位移",
        "offset" => "人物偏移／大小",
        "rotate" => "人物旋转",
        "face" => "人物转向",
        "dim" => "人物明暗",
        "focus" => "人物前后顺序",
        "mood" => "气泡表情",
        _ => kind,
    }
}

/// Only newly supported combinations use the preserving transform bridge.
/// Released groups with one transform retain their exact historical Lua.
pub fn independent_transforms(group: &Value, character: &str) -> bool {
    !character.is_empty()
        && actions(group)
            .iter()
            .filter(|action| {
                action["character"] == character
                    && matches!(
                        action["type"].as_str(),
                        Some("move" | "offset" | "rotate" | "face")
                    )
            })
            .count()
            > 1
}
