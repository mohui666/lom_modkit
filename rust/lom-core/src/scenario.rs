//! Event/task authoring is stored independently of presentation chapters.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const MIN_HOST: &str = "1.1.3";
pub fn performance_node(kind: &str) -> bool {
    ![
        "free_trigger",
        "mod_quest",
        "quest_check",
        "persistent_var",
        "persistent_check",
        "reward",
        "result_screen",
        "raw",
    ]
    .contains(&kind)
}
pub fn enabled(manifest: &Value) -> bool {
    manifest.get("play_mode").is_some()
}
pub fn default_config() -> Value {
    json!({"schema":1,"entry_event":"opening","tasks":[],"events":[{
        "id":"opening","name":"开场剧情","trigger":"start","script":"main",
        "once":true,"check":"stage","priority":0,"chance":100,"weight":1,
        "when":{"all":[]},"outcomes":{"completed":{"variables":{},"tasks":{}}}
    }]})
}
fn id(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}
fn fields(v: &Value, keys: &[&str]) -> Result<()> {
    for k in v.as_object().context("必须是对象")?.keys() {
        ensure!(keys.contains(&k.as_str()), "未知字段 {k}");
    }
    Ok(())
}
fn int(v: &Value, lo: i64, hi: i64) -> bool {
    v.as_i64().is_some_and(|n| (lo..=hi).contains(&n))
}
fn opt_int(v: &Value, key: &str, lo: i64, hi: i64) -> Result<()> {
    ensure!(
        v.get(key).is_none_or(|n| int(n, lo, hi)),
        "{key} 必须为 {lo}~{hi} 的整数"
    );
    Ok(())
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key]
        .as_array()
        .with_context(|| format!("{key} 必须为数组"))
}
pub fn validate(config: &Value, manifest: &Value, stories: &BTreeMap<String, Value>) -> Result<()> {
    validate_inner(config, manifest, stories).map_err(|e| anyhow::anyhow!("scenario.json: {e:#}"))
}
fn validate_inner(c: &Value, m: &Value, stories: &BTreeMap<String, Value>) -> Result<()> {
    fields(c, &["schema", "entry_event", "events", "tasks"])?;
    ensure!(c["schema"] == 1, "schema 必须是 1");
    ensure!(
        ["addon", "sandbox"].contains(&m["play_mode"].as_str().unwrap_or("")),
        "play_mode 必须为 addon 或 sandbox"
    );
    let mut tasks = BTreeMap::new();
    let task_list = list(c, "tasks")?;
    ensure!(task_list.len() <= 256, "最多 256 条任务");
    for t in task_list {
        fields(t, &["id", "name", "initial", "states", "transitions"])?;
        ensure!(id(&t["id"]), "任务 id 无效");
        let states = list(t, "states")?;
        ensure!(
            !states.is_empty() && states.len() <= 32 && states.iter().all(id),
            "任务 states 必须含 1~32 个有效状态 id"
        );
        ensure!(
            states
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<BTreeSet<_>>()
                .len()
                == states.len(),
            "任务状态重复"
        );
        ensure!(
            states.contains(&t["initial"]),
            "任务 initial 不在 states 中"
        );
        ensure!(
            tasks
                .insert(t["id"].as_str().unwrap().to_owned(), states.clone())
                .is_none(),
            "任务 id 重复"
        );
    }
    for t in task_list {
        if let Some(transitions) = t.get("transitions") {
            let transitions = transitions.as_array().context("transitions 必须是数组")?;
            ensure!(transitions.len() <= 128, "任务最多 128 条转换");
            for r in transitions {
                fields(r, &["from", "to", "when"])?;
                ensure!(
                    t["states"].as_array().unwrap().contains(&r["from"])
                        && t["states"].as_array().unwrap().contains(&r["to"]),
                    "转换引用了未定义状态"
                );
                ensure!(r["from"] != r["to"], "任务转换的起止状态相同");
                condition(&r["when"], &tasks, 0)?;
            }
        }
    }
    let events = list(c, "events")?;
    ensure!(
        !events.is_empty() && events.len() <= 512,
        "需要 1~512 条事件"
    );
    let mut ids = BTreeSet::new();
    let mut pools = BTreeMap::new();
    for e in events {
        fields(
            e,
            &[
                "id",
                "name",
                "enabled",
                "trigger",
                "position",
                "script",
                "once",
                "check",
                "priority",
                "chance",
                "pool",
                "weight",
                "weight_modifiers",
                "when",
                "outcomes",
            ],
        )?;
        ensure!(id(&e["id"]), "事件 id 无效");
        ensure!(ids.insert(e["id"].as_str().unwrap()), "事件 id 重复");
        ensure!(
            ["start", "auto", "position"].contains(&e["trigger"].as_str().unwrap_or("")),
            "事件 trigger 必须是 start/auto/position"
        );
        ensure!(
            id(&e["script"]) && stories.contains_key(e["script"].as_str().unwrap()),
            "事件 {} 引用了不存在的剧情章节",
            e["id"]
        );
        if e["trigger"] == "position" {
            ensure!(
                crate::validate::node_schema()["campaign_positions"]
                    .as_array()
                    .unwrap()
                    .contains(&e["position"]),
                "事件位置无效"
            );
        } else {
            ensure!(e.get("position").is_none(), "非地点事件不能填写 position");
        }
        for key in ["once", "enabled"] {
            ensure!(
                e.get(key).is_none_or(Value::is_boolean),
                "{key} 必须是布尔值"
            );
        }
        ensure!(
            e.get("check")
                .is_none_or(|v| ["month", "stage", "action"].contains(&v.as_str().unwrap_or(""))),
            "check 必须是 month/stage/action"
        );
        opt_int(e, "priority", -100000, 100000)?;
        opt_int(e, "weight", 0, 100000)?;
        ensure!(
            e.get("chance").is_none_or(|v| v
                .as_f64()
                .is_some_and(|n| n.is_finite() && (0.0..=100.0).contains(&n))),
            "chance 必须在 0~100 之间"
        );
        condition(&e["when"], &tasks, 0)?;
        if let Some(modifiers) = e.get("weight_modifiers") {
            let modifiers = modifiers
                .as_array()
                .context("weight_modifiers 必须是数组")?;
            ensure!(modifiers.len() <= 64, "最多 64 条权重修正");
            for r in modifiers {
                fields(r, &["when", "add"])?;
                ensure!(int(&r["add"], -100000, 100000), "权重修正 add 无效");
                condition(&r["when"], &tasks, 0)?;
            }
        }
        if let Some(pool) = e.get("pool") {
            ensure!(
                id(pool) && e["trigger"] != "start",
                "pool 必须是有效 id，开场事件不能加入事件池"
            );
            let signature = json!([
                e["trigger"],
                e["position"],
                e.get("priority").unwrap_or(&json!(0)),
                e.get("check").unwrap_or(&json!("stage"))
            ]);
            if let Some(previous) = pools.insert(pool.as_str().unwrap(), signature.clone()) {
                ensure!(
                    previous == signature,
                    "同一事件池的入口、地点、优先级、判定周期必须一致"
                );
            }
        }
        let outcomes = e["outcomes"].as_object().context("outcomes 必须是对象")?;
        ensure!(
            outcomes.contains_key("completed") && outcomes.len() <= 32,
            "outcomes 需要 completed，最多 32 个结果"
        );
        for (result, effects) in outcomes {
            ensure!(id(&json!(result)), "结果 id 无效");
            effects_valid(effects, &tasks)?;
        }
        let mut pending = vec![e["script"].as_str().unwrap().to_owned()];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let story = stories.get(&id).context("演出续接引用了不存在的章节")?;
            for node in story["nodes"].as_array().context("剧情 nodes 必须是数组")? {
                if node["type"] != "end" {
                    continue;
                }
                if let Some(next) = node["next_script"].as_str() {
                    pending.push(next.to_owned());
                } else {
                    let result = node["event_result"].as_str().unwrap_or("completed");
                    ensure!(
                        outcomes.contains_key(result),
                        "事件 {} 的演出 {} 回报了未配置结果 {result}",
                        e["id"],
                        id
                    );
                }
            }
        }
    }
    if m["play_mode"] == "sandbox" {
        let entry = events
            .iter()
            .find(|e| e["id"] == c["entry_event"])
            .context("entry_event 引用了不存在的事件")?;
        ensure!(
            entry["trigger"] == "start" && entry["script"] == m["entry"],
            "entry_event 必须是开场事件且脚本与 manifest.entry 相同"
        );
        ensure!(
            entry["enabled"] != false
                && entry
                    .get("chance")
                    .is_none_or(|v| v.as_f64() == Some(100.0))
                && entry["when"] == json!({"all":[]}),
            "开场事件必须启用、概率为 100 且不设条件"
        );
        ensure!(
            events.iter().filter(|e| e["trigger"] == "start").count() == 1,
            "自由剧本需要且只能有一个开场事件"
        );
    } else {
        ensure!(
            c.get("entry_event").is_none() && !events.iter().any(|e| e["trigger"] == "start"),
            "附加剧情不设置开场事件"
        );
    }
    for (sid, story) in stories {
        for n in story["nodes"].as_array().context("剧情 nodes 必须是数组")? {
            ensure!(performance_node(n["type"].as_str().unwrap_or("")),"剧情 {sid} 的 {} 节点属于事件/任务配置或结果动作，请移到 scenario.json；旧作品仍兼容",n["type"]);
        }
    }
    Ok(())
}
fn effects_valid(v: &Value, tasks: &BTreeMap<String, Vec<Value>>) -> Result<()> {
    fields(v, &["variables", "tasks", "actions"])?;
    if let Some(actions) = v.get("actions") {
        let actions = actions.as_array().context("actions 必须为数组")?;
        ensure!(actions.len() <= 64, "最多 64 个结果动作");
        for action in actions {
            let kind = action["type"].as_str().unwrap_or("");
            if kind == "time" {
                fields(action, &["type", "year", "month", "stage"])?;
                for (key, max) in [("year", 9), ("month", 12), ("stage", 3)] {
                    ensure!(int(&action[key], 1, max), "结果时间 {key} 无效");
                }
            } else {
                ensure!(
                    ["stat", "affinity", "flag", "game_flag", "item", "talent"].contains(&kind),
                    "未知结果动作 {kind}"
                );
                let keys: &[&str] = match kind {
                    "flag" => &["type", "key"],
                    "talent" => &["type", "key", "value"],
                    "item" => &["type", "key", "kind", "op", "value"],
                    _ => &["type", "key", "op", "value"],
                };
                fields(action, keys)?;
                ensure!(id(&action["key"]), "结果动作 key 无效");
                if kind != "flag" {
                    ensure!(
                        int(&action["value"], i32::MIN as i64, i32::MAX as i64),
                        "结果动作 value 必须是 Int32"
                    );
                }
                if kind == "item" {
                    ensure!(
                        ["book", "misc", "special"]
                            .contains(&action["kind"].as_str().unwrap_or(""))
                            && ["add", "remove"].contains(&action["op"].as_str().unwrap_or(""))
                            && int(&action["value"], 1, i32::MAX as i64),
                        "物品动作无效"
                    );
                } else if kind == "talent" {
                    ensure!(
                        action["value"] == 1 || action["value"] == -1,
                        "技能变动必须为 +1 或 -1"
                    );
                } else if kind != "flag" {
                    ensure!(
                        ["set", "add"].contains(&action["op"].as_str().unwrap_or("")),
                        "动作操作无效"
                    );
                }
            }
        }
    }
    for key in ["variables", "tasks"] {
        if let Some(values) = v.get(key) {
            let values = values
                .as_object()
                .with_context(|| format!("结果 {key} 必须为对象"))?;
            ensure!(values.len() <= 256, "结果项过多");
            for (name, value) in values {
                ensure!(id(&json!(name)), "结果键无效");
                if key == "tasks" {
                    ensure!(
                        tasks.get(name).is_some_and(|s| s.contains(value)),
                        "结果引用了未定义任务或状态 {name}"
                    );
                } else {
                    fields(value, &["op", "value"])?;
                    ensure!(
                        ["set", "add"].contains(&value["op"].as_str().unwrap_or(""))
                            && int(&value["value"], i32::MIN as i64, i32::MAX as i64),
                        "变量结果需要 set/add 和 Int32 value"
                    );
                }
            }
        }
    }
    Ok(())
}
fn condition(v: &Value, tasks: &BTreeMap<String, Vec<Value>>, depth: usize) -> Result<()> {
    ensure!(depth <= 12, "条件嵌套超过 12 层");
    let o = v
        .as_object()
        .context("when 必须是条件对象，空条件使用 {all:[]}")?;
    for key in ["all", "any", "not"] {
        if let Some(child) = o.get(key) {
            ensure!(o.len() == 1, "条件组合不能混用字段");
            if key == "not" {
                return condition(child, tasks, depth + 1);
            }
            let children = child.as_array().context("all/any 必须是数组")?;
            ensure!(children.len() <= 64, "条件项过多");
            for child in children {
                condition(child, tasks, depth + 1)?;
            }
            return Ok(());
        }
    }
    fields(v, &["source", "key", "op", "value"])?;
    let source = v["source"].as_str().unwrap_or("");
    ensure!(
        [
            "time",
            "stat",
            "affinity",
            "flag",
            "game_flag",
            "variable",
            "task"
        ]
        .contains(&source),
        "未知条件来源 {source}"
    );
    ensure!(
        ["eq", "ne", "lt", "le", "gt", "ge"].contains(&v["op"].as_str().unwrap_or("")),
        "条件 op 无效"
    );
    let key = v["key"].as_str().context("条件 key 必须是字符串")?;
    if source == "time" {
        ensure!(
            ["year", "month", "stage", "action"].contains(&key),
            "未知时间字段"
        );
    } else {
        ensure!(id(&v["key"]), "条件 key 无效");
    }
    if source == "task" {
        ensure!(
            ["eq", "ne"].contains(&v["op"].as_str().unwrap())
                && tasks.get(key).is_some_and(|s| s.contains(&v["value"])),
            "任务条件必须引用已有任务状态并使用 eq/ne"
        );
    } else {
        ensure!(
            int(&v["value"], i32::MIN as i64, i32::MAX as i64),
            "数值条件 value 必须是 Int32"
        );
    }
    Ok(())
}
