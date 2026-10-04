//! Conservative authoring analysis and deterministic offline story tests.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
type Graph = BTreeMap<String, BTreeSet<String>>;
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn nodes(story: &Value) -> &[Value] {
    story["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn checks(kind: &str) -> bool {
    [
        "stat_check",
        "affinity_check",
        "item_check",
        "talent_check",
        "flag_check",
        "activity",
        "quest_check",
        "persistent_check",
    ]
    .contains(&kind)
}
fn terminal(kind: &str) -> bool {
    [
        "end",
        "goto_scene",
        "death",
        "combat",
        "battle",
        "battle_result",
    ]
    .contains(&kind)
        || checks(kind)
}
fn final_ending(kind: &str) -> bool {
    matches!(kind, "end" | "goto_scene" | "death")
}
fn branch_fully_covered(node: &Value) -> bool {
    let source = node["source"].as_str().unwrap_or("mod");
    ["mod", "condition"].contains(&source)
        && node["cases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|case| case["value"].as_i64())
            .collect::<BTreeSet<_>>()
            == BTreeSet::from([1, 2])
}
pub fn successors(story: &Value, index: usize) -> Vec<String> {
    let ns = nodes(story);
    let Some(node) = ns.get(index) else {
        return vec![];
    };
    let kind = text(node, "type");
    let mut out = BTreeSet::new();
    let mut add = |v: &Value| {
        if let Some(target) = v.as_str().filter(|s| !s.is_empty()) {
            out.insert(target.to_string());
        }
    };
    add(&node["goto"]);
    match kind {
        "choice" => {
            for opt in node["options"].as_array().into_iter().flatten() {
                add(&opt["goto"]);
            }
        }
        "branch" => {
            for case in node["cases"].as_array().into_iter().flatten() {
                add(&case["goto"]);
            }
        }
        "dice" => {
            for band in node["bands"].as_array().into_iter().flatten() {
                add(&band["goto"]);
            }
            for opt in node["options"].as_array().into_iter().flatten() {
                for key in ["goto_大成功", "goto_成功", "goto_失败"] {
                    add(&opt[key]);
                }
            }
        }
        "combat" | "battle" | "battle_result" => {
            add(&node["win"]);
            add(&node["lose"]);
        }
        kind if checks(kind) => {
            add(&node["success"]);
            add(&node["failure"]);
        }
        _ => {}
    }
    if !terminal(kind)
        && !["choice", "dice"].contains(&kind)
        && !(kind == "branch" && branch_fully_covered(node))
        && (kind == "branch" || text(node, "goto").is_empty())
    {
        if let Some(next) = ns.get(index + 1) {
            add(&next["id"]);
        }
    }
    out.into_iter().collect()
}
fn reachable(graph: &Graph, roots: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let mut pending: Vec<_> = roots.into_iter().collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !graph.contains_key(&id) || !seen.insert(id.clone()) {
            continue;
        }
        pending.extend(graph[&id].iter().cloned());
    }
    seen
}
fn story_graph(story: &Value) -> Graph {
    nodes(story)
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                text(n, "id").into(),
                successors(story, i).into_iter().collect(),
            )
        })
        .collect()
}
fn node_key(story: &str, node: &str) -> String {
    format!("{story}/{node}")
}
fn project_graph(stories: &BTreeMap<String, Value>) -> Graph {
    let mut graph = Graph::new();
    for (sid, story) in stories {
        for (i, n) in nodes(story).iter().enumerate() {
            let mut successors: BTreeSet<_> = successors(story, i)
                .iter()
                .map(|id| node_key(sid, id))
                .collect();
            if text(n, "type") == "end" {
                if let Some(next) = stories.get(text(n, "next_script")) {
                    successors.insert(node_key(text(n, "next_script"), text(next, "start")));
                }
            }
            graph.insert(node_key(sid, text(n, "id")), successors);
        }
    }
    graph
}
fn write_dominates(story: &Value, target: &str, flag: &str) -> bool {
    let graph = story_graph(story);
    let start = text(story, "start");
    let reached = reachable(&graph, [start.into()]);
    if !reached.contains(target) {
        return false;
    }
    // Native raw Lua can mutate flags. A path through raw before this read
    // cannot prove a constant condition, even when a prior writer dominates.
    if nodes(story).iter().any(|node| {
        text(node, "type") == "raw"
            && reached.contains(text(node, "id"))
            && reachable(&graph, [text(node, "id").into()]).contains(target)
    }) {
        return false;
    }
    let blocked: BTreeSet<_> = nodes(story)
        .iter()
        .filter(|n| text(n, "type") == "flag" && text(n, "flag") == flag)
        .map(|n| text(n, "id").to_string())
        .collect();
    let graph: Graph = graph
        .into_iter()
        .filter(|(n, _)| !blocked.contains(n))
        .map(|(n, edges)| {
            (
                n,
                edges.into_iter().filter(|e| !blocked.contains(e)).collect(),
            )
        })
        .collect();
    !reachable(&graph, [start.into()]).contains(target)
}
pub fn analyze_project(stories: &BTreeMap<String, Value>, manifest: &Value) -> Value {
    let mut issues = Vec::new();
    let mut conditions = Vec::new();
    let mut uses: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    let mut count = 0;
    let mut dialogue = 0;
    let mut voiced = 0;
    let mut choice_count = 0;
    let mut characters = BTreeSet::new();
    for (sid, story) in stories {
        let graph = story_graph(story);
        let reached = reachable(&graph, [text(story, "start").into()]);
        let finals: BTreeSet<_> = nodes(story)
            .iter()
            .filter(|n| final_ending(text(n, "type")))
            .map(|n| text(n, "id").to_string())
            .collect();
        let mut reverse: Graph = graph
            .keys()
            .map(|id| (id.clone(), BTreeSet::new()))
            .collect();
        for (from, targets) in &graph {
            for target in targets {
                if let Some(incoming) = reverse.get_mut(target) {
                    incoming.insert(from.clone());
                }
            }
        }
        let can_finish = reachable(&reverse, finals.clone());
        for id in graph.keys().filter(|id| !reached.contains(*id)) {
            issues.push(json!({"severity":"warning","code":"unreachable","story":sid,"node":id,"detail":"节点不可达"}));
        }
        for (id, targets) in &graph {
            for target in targets {
                if !graph.contains_key(target) {
                    issues.push(json!({"severity":"error","code":"broken_target","story":sid,"node":id,"detail":format!("目标不存在: {target}")}));
                }
            }
            if reached.contains(id) && !can_finish.contains(id) {
                let cycle = targets
                    .iter()
                    .any(|next| reachable(&graph, [next.clone()]).contains(id));
                if cycle {
                    issues.push(json!({"severity":"error","code":"no_exit_scc","story":sid,"node":id,"detail":"循环没有可达出口"}));
                } else if targets.is_empty() {
                    issues.push(json!({"severity":"error","code":"dead_end","story":sid,"node":id,"detail":"节点缺少后继"}));
                }
            }
        }
        for n in nodes(story) {
            count += 1;
            let kind = text(n, "type");
            let nid = text(n, "id");
            if kind == "say" {
                dialogue += 1;
                if !text(n, "voice").is_empty() {
                    voiced += 1;
                }
            }
            if kind == "choice" {
                choice_count += 1;
            }
            if !text(n, "character").is_empty() {
                characters.insert(text(n, "character").to_string());
            }
            let symbol = match kind {
                "flag" => Some(("mod_flag", "write", text(n, "flag"))),
                "game_flag" => Some(("game_flag", "write", text(n, "flag"))),
                "branch" => {
                    let source = n["source"].as_str().unwrap_or("mod");
                    let flag = text(n, "flag");
                    let proven = source == "mod" && write_dominates(story, nid, flag);
                    conditions.push(json!({"story":sid,"node":nid,"source":source,"subject":if source=="stat"{text(n,"stat")}else{flag},"cases":n["cases"],"proof":if proven{"always_true"}else{"unknown"},"reason":if proven{"所有可达路径都先设置了该 Mod Flag"}else{"依赖未知运行时状态或存在未写入路径"}}));
                    if proven {
                        for case in n["cases"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|c| c["value"] == 2)
                        {
                            issues.push(json!({"severity":"warning","code":"dead_branch","story":sid,"node":nid,"detail":format!("Flag 已设置，此 false 分支不可达: {}",text(case,"goto"))}));
                        }
                    }
                    let family = match source {
                        "mod" => "mod_flag",
                        "flag_value" => "game_flag",
                        "game" => "checkpoint",
                        "condition" => "condition",
                        _ => "stat",
                    };
                    Some((
                        family,
                        "read",
                        if source == "stat" {
                            text(n, "stat")
                        } else {
                            flag
                        },
                    ))
                }
                _ => None,
            };
            if let Some((family, access, name)) = symbol {
                uses.entry((family.into(), name.into()))
                    .or_default()
                    .push(json!({"story":sid,"node":nid,"access":access}));
            }
        }
    }
    let graph = project_graph(stories);
    let entry = text(manifest, "entry");
    let mut roots = BTreeSet::new();
    if let Some(story) = stories.get(entry) {
        roots.insert(node_key(entry, text(story, "start")));
    } else {
        for (sid, story) in stories {
            roots.insert(node_key(sid, text(story, "start")));
        }
        if !entry.is_empty() {
            issues.push(json!({"severity":"error","code":"bad_entry","story":entry,"detail":"入口章节不存在"}));
        }
    }
    for trigger in manifest["campaign"]["triggers"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let target = text(trigger, "script");
        if let Some(story) = stories.get(target) {
            roots.insert(node_key(target, text(story, "start")));
        } else {
            issues.push(json!({"severity":"error","code":"bad_trigger","story":target,"detail":"触发器章节不存在"}));
        }
    }
    let mut symbols = Vec::new();
    for ((family, name), refs) in uses {
        let reads = refs.iter().filter(|u| u["access"] == "read").count();
        let writes = refs.len() - reads;
        let (unused, before) = if family == "mod_flag" {
            let blockers: BTreeSet<_> = refs
                .iter()
                .filter(|u| u["access"] == "write")
                .map(|u| node_key(text(u, "story"), text(u, "node")))
                .collect();
            let pruned: Graph = graph
                .iter()
                .filter(|(n, _)| !blockers.contains(*n))
                .map(|(n, es)| {
                    (
                        n.clone(),
                        es.iter()
                            .filter(|e| !blockers.contains(*e))
                            .cloned()
                            .collect(),
                    )
                })
                .collect();
            let reached = reachable(&pruned, roots.clone());
            let has_raw = stories
                .values()
                .any(|s| nodes(s).iter().any(|n| text(n, "type") == "raw"));
            (
                if has_raw {
                    Value::Null
                } else {
                    json!(reads == 0)
                },
                if has_raw {
                    Value::Null
                } else {
                    json!(refs
                        .iter()
                        .filter(|u| u["access"] == "read")
                        .any(|u| reached.contains(&node_key(text(u, "story"), text(u, "node")))))
                },
            )
        } else {
            (Value::Null, Value::Null)
        };
        symbols.push(json!({"kind":family,"name":name,"reads":reads,"writes":writes,"unused":unused,"possibly_read_before_write":before,"uses":refs}));
    }
    // Prove a final ending over cross-chapter edges; reachable raw Lua keeps the result unknown.
    let mut reverse: Graph = graph.keys().map(|k| (k.clone(), BTreeSet::new())).collect();
    for (from, tos) in &graph {
        for to in tos {
            if let Some(edges) = reverse.get_mut(to) {
                edges.insert(from.clone());
            }
        }
    }
    let mut finals = Vec::new();
    let mut unknown = Vec::new();
    for (sid, s) in stories {
        for n in nodes(s) {
            if text(n, "type") == "raw" {
                unknown.push(node_key(sid, text(n, "id")));
            }
            if final_ending(text(n, "type"))
                && !(text(n, "type") == "end" && !text(n, "next_script").is_empty())
            {
                finals.push(node_key(sid, text(n, "id")));
            }
            if text(n, "type") == "end"
                && !text(n, "next_script").is_empty()
                && !stories.contains_key(text(n, "next_script"))
            {
                issues.push(json!({"severity":"error","code":"bad_next_script","story":sid,"node":n["id"],"detail":"下一章节不存在"}));
            }
        }
    }
    let can_finish = reachable(&reverse, finals);
    let may_unknown = reachable(&reverse, unknown);
    for (sid, s) in stories {
        let root = node_key(sid, text(s, "start"));
        if !can_finish.contains(&root) && !may_unknown.contains(&root) {
            issues.push(json!({"severity":"error","code":"missing_ending","story":sid,"detail":"从章节入口无法到达最终结局"}));
        }
    }
    json!({"issues":issues,"conditions":conditions,"symbols":symbols,"stats":{"stories":stories.len(),"nodes":count,"dialogue":dialogue,"voiced":voiced,"unvoiced":dialogue-voiced,"voice_coverage":if dialogue==0{0.0}else{voiced as f64/dialogue as f64*100.0},"choice_nodes":choice_count,"characters":characters.len()}})
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
pub fn run_story_tests(stories: &BTreeMap<String, Value>, cases: &Value) -> Result<Value> {
    let cases = cases.as_array().context("测试定义必须为数组")?;
    let mut names = BTreeSet::new();
    let mut results = Vec::new();
    for case in cases {
        let name = text(case, "name").trim();
        ensure!(
            !name.is_empty() && names.insert(name),
            "测试名不能为空或重复"
        );
        ensure!(!text(case, "story").is_empty(), "测试缺少 story");
        for key in ["initial", "actions", "assert"] {
            ensure!(
                case.get(key).is_none_or(Value::is_object),
                "测试 {key} 必须是对象"
            );
        }
        for key in ["variables", "flags"] {
            ensure!(
                case["initial"].get(key).is_none_or(Value::is_object),
                "测试 initial.{key} 必须是对象"
            );
        }
        ensure!(
            case["actions"].get("choices").is_none_or(Value::is_array),
            "actions.choices 必须是数组"
        );
        let mut variables = case["initial"]
            .get("variables")
            .cloned()
            .unwrap_or(json!({}));
        let mut flags = json!({});
        for (key, v) in case["initial"]["flags"].as_object().into_iter().flatten() {
            flags[key] = json!(truth(v));
        }
        let mut actions: BTreeMap<String, VecDeque<usize>> = BTreeMap::new();
        for action in case["actions"]["choices"].as_array().into_iter().flatten() {
            ensure!(
                action["node"].is_string() && action["option"].is_u64(),
                "choice 动作需要 node 和非负整数 option"
            );
            actions
                .entry(text(action, "node").into())
                .or_default()
                .push_back(action["option"].as_u64().unwrap() as usize);
        }
        let mut visited = Vec::new();
        let mut sid = text(case, "story").to_string();
        let mut nid = stories
            .get(&sid)
            .map(|s| text(s, "start"))
            .unwrap_or("")
            .to_string();
        let mut ending = false;
        let execution = (|| -> Result<()> {
            for _ in 0..1000 {
                let story = stories.get(&sid).context("起始或下一章节不存在")?;
                let ns = nodes(story);
                let (index, n) = ns
                    .iter()
                    .enumerate()
                    .find(|(_, n)| text(n, "id") == nid)
                    .context("节点不存在")?;
                visited.push(json!([sid, nid]));
                let kind = text(n, "type");
                if [
                    "raw",
                    "dice",
                    "block",
                    "panel",
                    "enemy",
                    "battle_skill",
                    "mission",
                    "time",
                    "autosave",
                    "affinity",
                    "talent",
                    "item",
                    "combat",
                    "battle",
                    "reward",
                    "result_screen",
                    "custom_shop",
                    "mod_quest",
                    "persistent_var",
                    "battle_result",
                ]
                .contains(&kind)
                    || checks(kind)
                {
                    bail!("UNSUPPORTED: 节点 {sid}/{nid} 类型 {kind} 需要 Runtime");
                }
                let mut next = if !text(n, "goto").is_empty() {
                    text(n, "goto").to_string()
                } else {
                    ns.get(index + 1)
                        .map(|n| text(n, "id"))
                        .unwrap_or("")
                        .to_string()
                };
                match kind {
                    "flag" => flags[text(n, "flag")] = json!(true),
                    "stat_set" => variables[text(n, "key")] = n["value"].clone(),
                    "stat" | "game_flag" => {
                        let key = if kind == "stat" {
                            text(n, "key")
                        } else {
                            text(n, "flag")
                        };
                        let val = if kind == "stat" {
                            &n["delta"]
                        } else {
                            &n["value"]
                        };
                        if kind == "stat" || n["op"] == "add" {
                            let Some(old) = variables[key].as_f64() else {
                                bail!("UNSUPPORTED: {kind} {key} 没有初值");
                            };
                            variables[key] = json!(old + val.as_f64().context("变量增量不是数值")?);
                        } else {
                            variables[key] = val.clone();
                        }
                    }
                    "branch" => {
                        let source = n["source"].as_str().unwrap_or("mod");
                        let key = if source == "stat" {
                            text(n, "stat")
                        } else {
                            text(n, "flag")
                        };
                        let val = if source == "mod" {
                            if truth(&flags[key]) {
                                1.0
                            } else {
                                2.0
                            }
                        } else {
                            let Some(value) = variables.get(key) else {
                                bail!("UNSUPPORTED: {source} {key} 没有初值");
                            };
                            if source == "condition" {
                                if truth(value) {
                                    1.0
                                } else {
                                    2.0
                                }
                            } else {
                                value.as_f64().context("分支变量不是数值")?
                            }
                        };
                        for case in n["cases"].as_array().into_iter().flatten() {
                            let expect = case["value"].as_f64().context("case.value 不是数值")?;
                            let op = if ["stat", "flag_value"].contains(&source) {
                                case["op"].as_str().unwrap_or(">=")
                            } else {
                                "=="
                            };
                            let matched = match op {
                                ">=" => val >= expect,
                                ">" => val > expect,
                                "<=" => val <= expect,
                                "<" => val < expect,
                                "==" | "=" => val == expect,
                                "!=" => val != expect,
                                _ => false,
                            };
                            if matched {
                                next = text(case, "goto").into();
                                break;
                            }
                        }
                    }
                    "choice" => {
                        let option = actions
                            .get_mut(&nid)
                            .and_then(VecDeque::pop_front)
                            .with_context(|| {
                                format!("UNSUPPORTED: choice {nid} 没有 actions.choices")
                            })?;
                        next = n["options"].get(option).context("choice option 越界")?["goto"]
                            .as_str()
                            .context("choice 缺少 goto")?
                            .into();
                    }
                    "end" => {
                        if !text(n, "next_script").is_empty() {
                            sid = text(n, "next_script").into();
                            nid = stories
                                .get(&sid)
                                .map(|s| text(s, "start"))
                                .context("next_script 不存在")?
                                .into();
                            continue;
                        }
                        ending = true;
                        return Ok(());
                    }
                    "goto_scene" | "death" => {
                        ending = true;
                        return Ok(());
                    }
                    _ => {}
                }
                ensure!(!next.is_empty(), "节点没有后继: {sid}/{nid}");
                nid = next;
            }
            bail!("UNSUPPORTED: 超过 1000 步，可能存在循环")
        })();
        let (status, message) = match execution {
            Err(err) => {
                let message = format!("{err:#}");
                (
                    if message.starts_with("UNSUPPORTED:") {
                        "unsupported"
                    } else {
                        "fail"
                    },
                    message,
                )
            }
            Ok(()) => {
                let mut failures = Vec::new();
                let assertions = &case["assert"];
                let required = if assertions["reaches_node"].is_string() {
                    vec![assertions["reaches_node"].clone()]
                } else {
                    assertions["reaches_node"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                };
                for id in required {
                    if !visited.iter().any(|v| v[1] == id) {
                        failures.push(format!("未到达节点 {id}"));
                    }
                }
                if assertions["reaches_ending"] == true && !ending {
                    failures.push("未到达结局".into());
                }
                for field in ["variables", "flags"] {
                    let actual = if field == "variables" {
                        &variables
                    } else {
                        &flags
                    };
                    for (key, expected) in assertions[field].as_object().into_iter().flatten() {
                        let got = &actual[key];
                        let equal = if field == "flags" {
                            truth(got) == truth(expected)
                        } else {
                            got == expected
                                || got.is_number()
                                    && expected.is_number()
                                    && got.as_f64() == expected.as_f64()
                        };
                        if !equal {
                            failures.push(format!("{field}.{key}: 期望 {expected}，实际 {got}"));
                        }
                    }
                }
                (
                    if failures.is_empty() { "pass" } else { "fail" },
                    if failures.is_empty() {
                        "断言通过".into()
                    } else {
                        failures.join("；")
                    },
                )
            }
        };
        results.push(json!({"name":name,"status":status,"message":message,"visited":visited,"variables":variables,"flags":flags}));
    }
    Ok(json!(results))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branch_bypass_is_unknown_and_cross_chapter_cycles_have_no_ending() {
        let stories = BTreeMap::from([
            (
                "a".into(),
                json!({"start":"pick","nodes":[{"id":"pick","type":"choice","options":[{"goto":"set"},{"goto":"read"}]},{"id":"set","type":"flag","flag":"x"},{"id":"read","type":"branch","flag":"x","cases":[{"value":1,"goto":"end"},{"value":2,"goto":"end"}]},{"id":"end","type":"end","next_script":"b"}]}),
            ),
            (
                "b".into(),
                json!({"start":"end","nodes":[{"id":"end","type":"end","next_script":"a"}]}),
            ),
        ]);
        let report = analyze_project(&stories, &json!({"entry":"a"}));
        assert_eq!(report["conditions"][0]["proof"], "unknown");
        assert_eq!(report["symbols"][0]["possibly_read_before_write"], true);
        assert_eq!(
            report["issues"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v["code"] == "missing_ending")
                .count(),
            2
        );
    }
    #[test]
    fn tests_do_not_fake_runtime_actions() {
        let stories = BTreeMap::from([(
            "main".into(),
            json!({"start":"n","nodes":[{"id":"n","type":"reward","entries":[]},{"id":"end","type":"end"}]}),
        )]);
        assert_eq!(
            run_story_tests(&stories, &json!([{"name":"x","story":"main"}])).unwrap()[0]["status"],
            "unsupported"
        );
    }
    #[test]
    fn deterministic_choices_flags_and_assertions() {
        let stories = BTreeMap::from([(
            "main".into(),
            json!({"start":"n","nodes":[{"id":"n","type":"flag","flag":"x"},{"id":"c","type":"choice","options":[{"goto":"e"}]},{"id":"e","type":"end"}]}),
        )]);
        let result=run_story_tests(&stories,&json!([{"name":"walk","story":"main","actions":{"choices":[{"node":"c","option":0}]},"assert":{"reaches_node":"e","flags":{"x":true},"reaches_ending":true}}])).unwrap();
        assert_eq!(result[0]["status"], "pass");
    }
}
