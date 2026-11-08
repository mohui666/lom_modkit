//! Static route analysis. Reads Lua tokens without executing game scripts.
use crate::{
    story_routes::{MissionChange, ScriptLink, ScriptRoute, ScriptRoutes},
    vanilla_patch::{tokens, Token},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
type Strings = BTreeMap<String, String>;
type State = BTreeMap<String, BTreeSet<String>>;
type Bindings = BTreeMap<String, (String, String)>;
const SLOTS: [&str; 5] = ["1", "2", "travel", "var1", "var2"];
fn empty() -> State {
    SLOTS
        .into_iter()
        .map(|s| (s.into(), BTreeSet::new()))
        .collect()
}
fn initial() -> State {
    SLOTS
        .into_iter()
        .map(|s| (s.into(), BTreeSet::from([format!("@{s}")])))
        .collect()
}
#[derive(Clone)]
struct Frame {
    kind: String,
    label: String,
    entry: State,
    dynamic: Bindings,
    arms: Vec<(State, Bindings)>,
    has_else: bool,
}
#[derive(Clone)]
struct Return {
    slot: String,
    condition: String,
    temps: State,
    origin: Option<(String, String)>,
}
struct Route {
    route: ScriptRoute,
    returns: Vec<Return>,
    link_temps: Vec<State>,
    writes: State,
}

fn route_tokens(source: &str) -> Vec<Token> {
    // Protect strings/comments before discarding presentation-only call lines.
    let mut protected = String::new();
    let mut end = 0;
    for token in tokens(source) {
        protected.extend(
            source[end..token.start]
                .chars()
                .filter(|c| c.is_whitespace()),
        );
        if token.string && source.as_bytes()[token.start] == b'[' {
            protected.push_str(&serde_json::to_string(&token.value).unwrap());
        } else {
            protected.push_str(&source[token.start..token.end]);
        }
        end = token.end;
    }
    let call = regex::Regex::new(r"^\s*[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*\s*[({]")
        .unwrap();
    let route=regex::Regex::new(r"(?:luamanager\.(?:SetNextScript|SetCurrentStoryScript|SetTempScript|SetCurrentTravelScript)|statmodifymanager\.Mission)\s*\(").unwrap();
    tokens(
        &protected
            .lines()
            .filter(|line| !call.is_match(line) || route.is_match(line))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}
fn arguments(items: &[Token], opening: usize) -> Vec<Vec<Token>> {
    let mut args = vec![];
    let mut current = vec![];
    let mut depth = 0i32;
    for item in items.iter().skip(opening + 1) {
        let value = if item.string { "" } else { &item.value };
        if value == ")" && depth == 0 {
            args.push(current);
            return args;
        }
        if value == "," && depth == 0 {
            args.push(std::mem::take(&mut current));
        } else {
            current.push(item.clone());
            if ["(", "{", "["].contains(&value) {
                depth += 1;
            } else if [")", "}", "]"].contains(&value) {
                depth -= 1;
            }
        }
    }
    vec![]
}
fn literal(arg: &[Token], variables: &Strings) -> Option<String> {
    if arg.len() != 1 {
        None
    } else if arg[0].string {
        Some(arg[0].value.clone())
    } else {
        variables.get(&arg[0].value).cloned()
    }
}
fn expression(
    parts: &[Token],
    bindings: &Bindings,
    choices: &BTreeMap<String, Strings>,
    texts: &Strings,
) -> String {
    if parts.len() == 3
        && parts[1].value == "=="
        && parts[2].value.bytes().all(|b| b.is_ascii_digit())
    {
        if let Some((kind, target)) = bindings.get(&parts[0].value) {
            if kind == "choice" {
                if let Some(key) = choices.get(target).and_then(|c| c.get(&parts[2].value)) {
                    return format!("选项：{}", texts.get(key).unwrap_or(key));
                }
            }
            if kind == "check" {
                return format!("检定 {target}：结果 {}", parts[2].value);
            }
        }
    }
    parts
        .iter()
        .map(|p| {
            if p.string {
                format!("\"{}\"", p.value)
            } else {
                p.value.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn parse(
    name: &str,
    source: &str,
    texts: &Strings,
    positions: &BTreeMap<String, Vec<String>>,
) -> Route {
    let items = route_tokens(source);
    let values: Vec<_> = items.iter().map(|t| t.value.as_str()).collect();
    let mut variables = Strings::new();
    let mut choices = BTreeMap::<String, Strings>::new();
    let mut bindings = Bindings::new();
    let mut stack = Vec::<Frame>::new();
    let mut assigned = BTreeSet::new();
    let mut dynamic = Bindings::new();
    let mut temps = initial();
    let mut returns = vec![];
    let mut link_temps = vec![];
    let mut writes = empty();
    let mut route = ScriptRoute {
        script: name.into(),
        links: vec![],
        missions: vec![],
        unknown_jumps: 0,
    };
    let mut at = 0;
    let follows =
        |i: usize, wanted: &[&str]| -> bool { values.get(i..i + wanted.len()) == Some(wanted) };
    while at < items.len() {
        if items[at].string {
            at += 1;
            continue;
        }
        let value = values[at];
        if value == "if" || value == "elseif" {
            let end = (at + 1..items.len())
                .find(|&i| !items[i].string && values[i] == "then")
                .unwrap_or(items.len());
            let label = expression(&items[at + 1..end], &bindings, &choices, texts);
            if value == "if" {
                stack.push(Frame {
                    kind: "if".into(),
                    label,
                    entry: temps.clone(),
                    dynamic: dynamic.clone(),
                    arms: vec![],
                    has_else: false,
                });
            } else if let Some(frame) = stack.last_mut().filter(|f| f.kind == "if") {
                frame.arms.push((temps.clone(), dynamic.clone()));
                temps = frame.entry.clone();
                dynamic = frame.dynamic.clone();
                frame.label = format!("否则若 {label}");
            }
            at = end;
        } else if value == "else" && stack.last().is_some_and(|f| f.kind == "if") {
            let frame = stack.last_mut().unwrap();
            frame.arms.push((temps.clone(), dynamic.clone()));
            temps = frame.entry.clone();
            dynamic = frame.dynamic.clone();
            frame.label = format!("其余分支（{}）", frame.label);
            frame.has_else = true;
        } else if ["function", "do", "repeat"].contains(&value) {
            stack.push(Frame {
                kind: value.into(),
                label: if value == "function" {
                    "函数体内"
                } else {
                    "循环或代码块内"
                }
                .into(),
                entry: temps.clone(),
                dynamic: dynamic.clone(),
                arms: vec![],
                has_else: false,
            });
        } else if (value == "end" || value == "until") && !stack.is_empty() {
            let mut frame = stack.pop().unwrap();
            frame.arms.push((temps.clone(), dynamic.clone()));
            if !frame.has_else {
                frame.arms.push((frame.entry, frame.dynamic));
            }
            temps = empty();
            for (state, _) in &frame.arms {
                for (slot, targets) in state {
                    temps.get_mut(slot).unwrap().extend(targets.iter().cloned());
                }
            }
            dynamic = frame.arms[0]
                .1
                .iter()
                .filter(|(key, v)| frame.arms.iter().all(|(_, b)| b.get(*key) == Some(*v)))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }
        if at + 2 < items.len() && values[at + 1] == "=" {
            let continues = values.get(at + 3).is_some_and(|v| {
                [
                    "..", "+", "-", "*", "/", "%", "^", "and", "or", "==", "~=", "<", ">", "<=",
                    ">=", "[", ".", "(",
                ]
                .contains(v)
            });
            if items[at + 2].string && !continues && stack.is_empty() && !assigned.contains(value) {
                variables.insert(value.into(), values[at + 2].into());
            } else {
                variables.remove(value);
            }
            assigned.insert(value.to_owned());
            dynamic.remove(value);
            if follows(at + 2, &["checkpointmanager", ".", "Position", "("]) {
                let args = arguments(&items, at + 5);
                if let Some(target) = args
                    .first()
                    .and_then(|a| literal(a, &variables))
                    .filter(|s| !s.is_empty())
                {
                    dynamic.insert(value.into(), ("position".into(), target));
                }
            } else if follows(at + 2, &["luamanager", ".", "GetTempScript", "("]) {
                let args = arguments(&items, at + 5);
                if args.len() == 1
                    && args[0].len() == 1
                    && ["1", "2"].contains(&args[0][0].value.as_str())
                {
                    dynamic.insert(value.into(), ("temp".into(), args[0][0].value.clone()));
                }
            } else if follows(
                at + 2,
                &["luamanager", ".", "GetCurrentTravelScript", "(", ")"],
            ) {
                dynamic.insert(value.into(), ("temp".into(), "travel".into()));
            }
            if value == "var1" || value == "var2" {
                let binding = dynamic.get(value);
                let targets = if items[at + 2].string && !continues {
                    BTreeSet::from([values[at + 2].to_owned()])
                } else if let Some((kind, key)) = binding {
                    if kind == "position" {
                        positions
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| vec![String::new()])
                            .into_iter()
                            .collect()
                    } else {
                        temps[key].clone()
                    }
                } else {
                    BTreeSet::from([variables.get(value).cloned().unwrap_or_default()])
                };
                writes.get_mut(value).unwrap().extend(
                    targets
                        .iter()
                        .filter(|s| !s.is_empty() && !s.starts_with('@'))
                        .cloned(),
                );
                temps.insert(value.into(), targets);
            }
            if follows(at + 2, &["choose", "("]) && at + 4 < items.len() {
                bindings.insert(value.into(), ("choice".into(), values[at + 4].into()));
            } else if at + 6 < items.len()
                && values[at + 2] == "checkpointmanager"
                && ["Condition", "Switch"].contains(&values[at + 4])
            {
                let args = arguments(&items, at + 5);
                if let Some(target) = args
                    .first()
                    .and_then(|a| literal(a, &variables))
                    .filter(|s| !s.is_empty())
                {
                    bindings.insert(value.into(), ("check".into(), target));
                }
            }
        }
        if at + 5 < items.len() && values[at + 1] == "[" && follows(at + 3, &["]", "="]) {
            let row = if items[at + 5].string {
                Some(&items[at + 5])
            } else if at + 7 < items.len() && values[at + 6] == "and" && items[at + 7].string {
                Some(&items[at + 7])
            } else {
                None
            };
            if let Some(row) = row {
                choices
                    .entry(value.into())
                    .or_default()
                    .insert(values[at + 2].into(), row.value.clone());
            }
        }
        if at + 3 < items.len()
            && values[at + 1] == "."
            && values[at + 3] == "("
            && (value == "luamanager"
                && [
                    "SetNextScript",
                    "SetCurrentStoryScript",
                    "SetTempScript",
                    "SetCurrentTravelScript",
                ]
                .contains(&values[at + 2])
                || value == "statmodifymanager" && values[at + 2] == "Mission")
        {
            let method = values[at + 2];
            let args = arguments(&items, at + 3);
            let condition = stack
                .iter()
                .map(|f| f.label.as_str())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" / ");
            if value == "luamanager" && ["SetNextScript", "SetCurrentStoryScript"].contains(&method)
            {
                if let Some(target) = args
                    .first()
                    .and_then(|a| literal(a, &variables))
                    .filter(|s| crate::vanilla_patch::valid_target(s))
                {
                    route.links.push(ScriptLink { target, condition });
                    link_temps.push(temps.clone());
                } else {
                    let variable = if args.len() == 1 && args[0].len() == 1 {
                        Some(args[0][0].value.as_str())
                    } else {
                        None
                    };
                    let binding = variable.and_then(|v| dynamic.get(v));
                    if let Some((kind, key)) = binding
                        .filter(|(kind, key)| kind == "position" && positions.contains_key(key))
                    {
                        let _ = kind;
                        for candidate in &positions[key] {
                            let detail = format!("场所检定 {key}：配置候选");
                            route.links.push(ScriptLink {
                                target: candidate.clone(),
                                condition: join(&condition, &detail),
                            });
                            link_temps.push(temps.clone());
                        }
                    } else if variable.is_some_and(|v| v == "var1" || v == "var2") {
                        returns.push(Return {
                            slot: variable.unwrap().into(),
                            condition,
                            temps: temps.clone(),
                            origin: binding.cloned(),
                        });
                    } else if let Some((kind, slot)) = binding.filter(|(kind, _)| kind == "temp") {
                        let _ = kind;
                        returns.push(Return {
                            slot: slot.clone(),
                            condition,
                            temps: temps.clone(),
                            origin: binding.cloned(),
                        });
                    } else {
                        route.unknown_jumps += 1;
                    }
                }
            } else if value == "luamanager" && method == "SetTempScript" && args.len() == 2 {
                if args[0].len() == 1 && ["1", "2"].contains(&args[0][0].value.as_str()) {
                    let slot = &args[0][0].value;
                    let target = literal(&args[1], &variables).unwrap_or_default();
                    temps.insert(slot.clone(), BTreeSet::from([target.clone()]));
                    if !target.is_empty() {
                        writes.get_mut(slot).unwrap().insert(target);
                    }
                }
            } else if value == "luamanager" && method == "SetCurrentTravelScript" && args.len() == 1
            {
                let target = literal(&args[0], &variables).unwrap_or_default();
                temps.insert("travel".into(), BTreeSet::from([target.clone()]));
                if !target.is_empty() {
                    writes.get_mut("travel").unwrap().insert(target);
                }
            } else if value == "statmodifymanager" && method == "Mission" && args.len() >= 2 {
                match (literal(&args[0], &variables), literal(&args[1], &variables)) {
                    (Some(series), Some(key)) if !series.is_empty() && !key.is_empty() => {
                        route.missions.push(MissionChange {
                            series,
                            key,
                            condition,
                        })
                    }
                    _ => route.unknown_jumps += 1,
                }
            }
        }
        at += 1;
    }
    Route {
        route,
        returns,
        link_temps,
        writes,
    }
}
fn join(first: &str, second: &str) -> String {
    if first.is_empty() {
        second.into()
    } else {
        format!("{first} / {second}")
    }
}
fn state_for(incoming: &State, changes: &State) -> State {
    let mut state = empty();
    for (slot, targets) in changes {
        for target in targets {
            if let Some(reference) = target.strip_prefix('@') {
                state
                    .get_mut(slot)
                    .unwrap()
                    .extend(incoming[reference].iter().cloned());
            } else {
                state.get_mut(slot).unwrap().insert(target.clone());
            }
        }
    }
    state
}
/// Analyze a set of installed scripts, resolving only known return-slot contexts.
pub fn analyze(
    sources: &Strings,
    texts: &Strings,
    positions: &BTreeMap<String, Vec<String>>,
) -> ScriptRoutes {
    let mut routes: Vec<_> = sources
        .iter()
        .map(|(name, source)| parse(name, source, texts, positions))
        .collect();
    let names: BTreeMap<_, _> = routes
        .iter()
        .enumerate()
        .map(|(i, r)| (r.route.script.to_lowercase(), i))
        .collect();
    let mut incoming = vec![empty(); routes.len()];
    let mut configured = empty();
    for route in &routes {
        for (slot, targets) in &route.writes {
            configured.get_mut(slot).unwrap().extend(
                targets
                    .iter()
                    .filter(|t| names.contains_key(&t.to_lowercase()))
                    .cloned(),
            );
        }
    }
    let mut pending: VecDeque<_> = (0..routes.len()).collect();
    let mut queued: BTreeSet<_> = (0..routes.len()).collect();
    while let Some(i) = pending.pop_front() {
        queued.remove(&i);
        let route = &routes[i];
        let mut outgoing = vec![];
        for (link, changes) in route.route.links.iter().zip(&route.link_temps) {
            outgoing.push((link.target.clone(), state_for(&incoming[i], changes)));
        }
        for item in &route.returns {
            let state = state_for(&incoming[i], &item.temps);
            for target in &state[&item.slot] {
                if !target.is_empty() {
                    outgoing.push((target.clone(), state.clone()));
                }
            }
        }
        for (target, state) in outgoing {
            let Some(&target) = names.get(&target.to_lowercase()) else {
                continue;
            };
            let mut changed = false;
            for slot in SLOTS {
                let new: Vec<_> = state[slot]
                    .difference(&incoming[target][slot])
                    .cloned()
                    .collect();
                changed |= !new.is_empty();
                incoming[target].get_mut(slot).unwrap().extend(new);
            }
            if changed && queued.insert(target) {
                pending.push_back(target);
            }
        }
    }
    for (i, route) in routes.iter_mut().enumerate() {
        for item in &route.returns {
            let state = state_for(&incoming[i], &item.temps);
            let values = &state[&item.slot];
            let unbounded = values.contains("");
            let mut targets: BTreeSet<_> =
                values.iter().filter(|v| !v.is_empty()).cloned().collect();
            let mut detail = "调用链候选";
            if targets.is_empty() && !unbounded {
                for term in &item.temps[&item.slot] {
                    if let Some(reference) = term.strip_prefix('@') {
                        targets.extend(configured[reference].iter().cloned());
                    } else if !term.is_empty() {
                        targets.insert(term.clone());
                    }
                }
                detail = "全局配置候选（调用上下文未记录）";
            }
            if targets.is_empty() || unbounded {
                route.route.unknown_jumps += 1;
            }
            for target in targets {
                let slot = item
                    .origin
                    .as_ref()
                    .filter(|(kind, _)| kind == "temp")
                    .map(|(_, s)| s.as_str())
                    .unwrap_or(&item.slot);
                let slot_label = if ["1", "2"].contains(&slot) {
                    format!("临时章节槽 {slot}")
                } else if slot == "travel" {
                    "当前游历章节".into()
                } else {
                    format!("跨章节变量 {slot}")
                };
                let condition = join(&item.condition, &format!("返回{slot_label}：{detail}"));
                if !route
                    .route
                    .links
                    .iter()
                    .any(|l| l.target == target && l.condition == condition)
                {
                    route.route.links.push(ScriptLink { target, condition });
                }
            }
        }
    }
    ScriptRoutes {
        scripts: routes.into_iter().map(|r| r.route).collect(),
    }
}
