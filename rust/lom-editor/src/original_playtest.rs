//! Reconstruct an imported chapter's entry state in the temporary F5 package.
use crate::{native_forms, preview};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Decision {
    selected: Option<Value>,
    excluded: Vec<Value>,
}

fn decisions(path: &[&Value]) -> BTreeMap<String, Decision> {
    let mut decisions = BTreeMap::<String, Decision>::new();
    for pair in path.windows(2) {
        let [branch, next] = pair else { continue };
        if branch["type"] != "native_branch" || branch["success"] == branch["failure"] {
            continue;
        }
        let success = branch["success"] == next["id"];
        let condition = &branch["condition"];
        let (binding, value, equal) = match condition["kind"].as_str() {
            Some("local") => (condition, json!(success), true),
            Some("unary") if condition["op"] == "not" => {
                (&condition["value"], json!(!success), true)
            }
            Some("binary") if matches!(condition["op"].as_str(), Some("==" | "~=")) => {
                let (left, right) = (&condition["left"], &condition["right"]);
                let (binding, literal) = if left["kind"] == "local" {
                    (left, right)
                } else {
                    (right, left)
                };
                if literal["kind"] != "literal" {
                    continue;
                }
                (
                    binding,
                    literal["value"].clone(),
                    success == (condition["op"] == "=="),
                )
            }
            _ => continue,
        };
        if binding["kind"] != "local" {
            continue;
        }
        let Some(symbol) = binding["symbol"].as_str() else {
            continue;
        };
        let entry = decisions.entry(symbol.to_owned()).or_default();
        if equal {
            entry.selected = Some(value);
        } else {
            entry.excluded.push(value);
        }
    }
    decisions
}

fn setup_value(value: &Value) -> bool {
    match value {
        Value::Array(values) => values.iter().all(setup_value),
        Value::Object(fields) => {
            (value["kind"] != "call"
                || matches!(
                    native_forms::call_name(value).as_str(),
                    "characters.Get"
                        | "characters.GetPortrait"
                        | "luamanager.GetStoryText"
                        | "luamanager.GetFlagData"
                        | "luamanager.GetStatData"
                        | "math.random"
                ))
                && fields.values().all(setup_value)
        }
        _ => true,
    }
}

fn setup_operations(path: &[&Value]) -> Vec<Value> {
    let decisions = decisions(path);
    let mut output = Vec::new();
    let mut local_tables = BTreeSet::new();
    for node in path.iter().take(path.len().saturating_sub(1)) {
        for operation in node["operations"].as_array().into_iter().flatten() {
            let value = &operation["value"];
            let target = &operation["target"];
            match operation["kind"].as_str() {
                Some("call") => {
                    let call = &operation["expression"];
                    if matches!(
                        native_forms::call_name(call).as_str(),
                        "setsaydialog" | "setmenudialog" | "setcharacter"
                    ) && setup_value(&call["args"])
                    {
                        output.push(operation.clone());
                    }
                }
                Some("assign") if target["kind"] == "local" => {
                    let symbol = target["symbol"].as_str().unwrap_or_default();
                    local_tables.remove(symbol);
                    let function = native_forms::call_name(value);
                    let default = match function.as_str() {
                        "choose" => Some(json!(1)),
                        "checkpointmanager.Condition" => Some(json!(false)),
                        "checkpointmanager.Switch" => Some(json!(0)),
                        _ if native_forms::raw_name(value) == "dicemenudialog.ResultSelection" => {
                            Some(json!(1))
                        }
                        _ => None,
                    };
                    if let Some(default) = default {
                        let selected = decisions.get(symbol).map_or(default.clone(), |decision| {
                            if let Some(value) = &decision.selected {
                                return value.clone();
                            }
                            if let Some(number) = default.as_i64() {
                                return (number..)
                                    .map(|n| json!(n))
                                    .find(|n| !decision.excluded.contains(n))
                                    .unwrap();
                            }
                            json!(decision.excluded.contains(&default))
                        });
                        let mut operation = operation.clone();
                        operation["value"] = json!({"kind":"literal","value":selected});
                        output.push(operation);
                    } else if setup_value(value) {
                        if value["kind"] == "table" {
                            local_tables.insert(symbol);
                        }
                        output.push(operation.clone());
                    }
                }
                Some("assign")
                    if matches!(target["kind"].as_str(), Some("member" | "index"))
                        && target["object"]["kind"] == "local"
                        && target["object"]["symbol"]
                            .as_str()
                            .is_some_and(|symbol| local_tables.contains(symbol))
                        && setup_value(target)
                        && setup_value(value) =>
                {
                    output.push(operation.clone());
                }
                Some("assign")
                    if matches!(
                        native_forms::raw_name(target).as_str(),
                        "sayoptions.waitforinput" | "sayoptions.fadewhendone"
                    ) && setup_value(value) =>
                {
                    output.push(operation.clone());
                }
                _ => {}
            }
        }
    }
    output
}

pub(super) fn start_at(story: &mut Value, target: &str) -> anyhow::Result<()> {
    if story["vanilla_import"]["schema"] == 1 {
        *story = lom_core::native_upgrade::upgrade(story).map_err(|error| {
            anyhow::anyhow!(
                "旧版整章的步骤 {target} 无法直接试玩：{error}。可按 Ctrl+F5 从章首试玩。"
            )
        })?;
        anyhow::ensure!(
            story["nodes"]
                .as_array()
                .is_some_and(|nodes| nodes.iter().any(|node| node["id"] == target)),
            "旧版原版步骤 {target} 无法单独定位，请先将整章升级为结构化步骤"
        );
    }
    let nodes = story["nodes"].as_array().expect("checked story nodes");
    let by_id: BTreeMap<_, _> = nodes
        .iter()
        .filter_map(|node| Some((node["id"].as_str()?, node)))
        .collect();
    let path = preview::route(story, target).unwrap_or_default();
    let path: Vec<_> = path
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .collect();
    let operations = setup_operations(&path);
    let mut prelude = preview::build_playtest_prelude(story, target);
    let used: BTreeSet<_> = by_id.keys().copied().collect();
    let entry = (0..)
        .map(|i| format!("zz_playtest_original_{i}"))
        .find(|id| !used.contains(id.as_str()))
        .unwrap();
    if let Some(last) = prelude.last_mut() {
        last["goto"] = json!(entry);
    }
    prelude.push(json!({"id":entry,"type":"native_action","operations":operations,"goto":target}));
    story["start"] = prelude[0]["id"].clone();
    story["nodes"].as_array_mut().unwrap().splice(..0, prelude);
    Ok(())
}
