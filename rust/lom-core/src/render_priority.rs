//! Per-scene render rules. A later matching rule wins, including an explicit reset.
use anyhow::{ensure, Result};
use serde_json::{json, Value};

pub fn identity(node: &Value) -> (&str, &str, &str) {
    let target = node["target"].as_str().unwrap_or("");
    let key = match target {
        "character" => node["character"].as_str().unwrap_or(""),
        "overlay" => node["slot"].as_str().unwrap_or(""),
        "effect" if node["tag"].as_str().unwrap_or("").is_empty() => {
            node["name"].as_str().unwrap_or("")
        }
        _ => "",
    };
    (
        target,
        key,
        if target == "effect" {
            node["tag"].as_str().unwrap_or("")
        } else {
            ""
        },
    )
}

pub fn validate(node: &Value) -> Result<()> {
    let (target, key, tag) = identity(node);
    ensure!(
        ["character", "background", "custom_cg", "overlay", "effect"].contains(&target),
        "排序对象必须是人物、背景、CG、前景或特效"
    );
    ensure!(
        match target {
            "character" | "overlay" => !key.trim().is_empty(),
            "effect" => !key.trim().is_empty() || !tag.trim().is_empty(),
            _ => true,
        },
        "请填写排序目标人物、图片槽位或特效名称／标记"
    );
    let order = node.get("order").map_or(Some(0.0), Value::as_f64);
    ensure!(
        order.is_some_and(|n| n.is_finite()
            && n.fract() == 0.0
            && (-10000.0..=10000.0).contains(&n)),
        "渲染优先级必须是 -10000～10000 的整数"
    );
    Ok(())
}

pub fn apply(rules: &mut Value, node: &Value) {
    if !rules.is_array() {
        *rules = json!([]);
    }
    let items = rules.as_array_mut().unwrap();
    let id = identity(node);
    items.retain(|old| identity(old) != id);
    items.push(node.clone());
}

pub fn priority(rules: &Value, kind: &str, key: &str, tag: &str) -> i64 {
    rules
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .find(|node| {
            let (target, name, mark) = identity(node);
            target == kind
                && if kind == "effect" {
                    if mark.is_empty() {
                        name == key
                    } else {
                        mark == tag
                    }
                } else {
                    name == key
                }
        })
        .filter(|node| node["enabled"] != false)
        .and_then(|node| node["order"].as_f64())
        .unwrap_or(0.0) as i64
}

pub fn active(rules: &Value) -> bool {
    rules
        .as_array()
        .into_iter()
        .flatten()
        .any(|node| node["enabled"] != false)
}
