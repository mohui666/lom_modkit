//! Conservative, non-executing import of straight-line Lua presentation scripts.
//! A script with unknown statements stays in one raw node so local scopes and
//! control flow are never split across generated node functions.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportReport {
    pub converted: bool,
    pub visual_nodes: usize,
    pub preview_stopped_at: Option<usize>,
    pub notes: Vec<String>,
}
impl ImportReport {
    pub fn summary(&self) -> String {
        if self.converted {
            format!(
                "已导入 {} 个可编辑演出步骤；原始 Lua 保存在 assets/imported/source.lua。",
                self.visual_nodes
            )
        } else {
            format!("原始 Lua 已完整保留为一个高级步骤（不拆分作用域）；仅静态预览已识别的前 {} 个演出步骤。{}", self.visual_nodes,
                self.notes.join(" "))
        }
    }
}
pub struct ImportResult {
    pub story: Value,
    pub report: ImportReport,
}

/// Never executes Lua. Original bytes are retained separately by Project.
pub fn import(source: &str, id: &str, title: &str) -> Result<ImportResult> {
    ensure!(!source.trim().is_empty(), "Lua 文件为空");
    let (mut nodes, mut report) = inspect(source);
    let visual = nodes.len();
    if !report.converted {
        nodes = vec![json!({"id":"lua_source","type":"raw","code":source})];
    } else {
        for (i, node) in nodes.iter_mut().enumerate() {
            node["id"] = json!(format!("lua_{}", i + 1));
        }
        nodes.push(json!({"id":"lua_end","type":"end"}));
    }
    report.visual_nodes = visual;
    let story = json!({"story_schema":2,"id":id,"title":title,"mood":true,
    "start":nodes[0]["id"],"nodes":nodes,"lua_import":{
        "source_asset":"assets/imported/source.lua","report":report,"notice":report.summary()
    }});
    crate::validate::validate_story(&story)?;
    Ok(ImportResult { story, report })
}

/// The known straight-line prefix is an illustration, never a Lua interpreter.
/// Stop before unknown calls, branches, loops or functions rather than guessing
/// which branch the installed game would take.
pub fn preview_nodes(source: &str) -> (Vec<Value>, ImportReport) {
    inspect(source)
}

#[derive(Clone, Debug)]
enum Literal {
    String(String),
    Number(f64),
    Bool(bool),
    Character(String),
    Portrait(String, String),
}
#[derive(Default)]
struct State {
    vars: BTreeMap<String, Literal>,
    actors: BTreeMap<String, Value>,
    portraits: BTreeMap<String, String>,
    speaker: Option<(String, String)>,
    dialog: String,
    view: Option<String>,
    needs_native_defaults: bool,
}
fn inspect(source: &str) -> (Vec<Value>, ImportReport) {
    let mut state = State {
        dialog: "character".into(),
        ..Default::default()
    };
    let mut report = ImportReport {
        converted: true,
        visual_nodes: 0,
        preview_stopped_at: None,
        notes: vec![],
    };
    let mut nodes = vec![];
    for statement in statements(source) {
        let before = nodes.len();
        if !recognize(&statement.code, &mut state, &mut nodes) {
            report.converted = false;
            report.preview_stopped_at = Some(statement.line);
            report.notes.push(format!(
                "第 {} 行起保留原生 Lua，静态预览在此停止。",
                statement.line
            ));
            break;
        }
        // Nodes generated here have literal, validated fields. A catalog mismatch
        // remains raw instead of inventing an official ID or dropping a command.
        for node in &nodes[before..] {
            let mut candidate = node.clone();
            candidate["id"] = json!("check");
            let probe =
                json!({"id":"check","start":"check","nodes":[candidate,{"id":"end","type":"end"}]});
            if crate::validate::validate_story(&probe).is_err() {
                nodes.truncate(before);
                report.converted = false;
                report.preview_stopped_at = Some(statement.line);
                report.notes.push(format!(
                    "第 {} 行的参数不符合当前节点契约，保留原生 Lua。",
                    statement.line
                ));
                report.visual_nodes = nodes.len();
                return (nodes, report);
            }
        }
    }
    if state.needs_native_defaults {
        report.converted = false;
        report
            .notes
            .push("含原版默认补间或异步移动，原生语句保留；预览仅示意动作完成时的画面。".into());
    }
    report.visual_nodes = nodes.len();
    (nodes, report)
}

fn recognize(code: &str, state: &mut State, nodes: &mut Vec<Value>) -> bool {
    let code = code.trim();
    if code.is_empty() {
        return true;
    }
    // Only literal aliases are safe to lift across independent editor steps.
    if let Some((lhs, rhs)) = assignment(code) {
        if let Some((call_name, args)) = call(lhs) {
            if call_name == "getvar"
                && args.len() == 2
                && args[0].trim() == "flowcharts.view"
                && string(&args[1], state).as_deref() == Some("ViewName")
            {
                if let Some(view) = string(rhs, state) {
                    state.view = Some(view);
                    return true;
                }
            }
        }
        if let Some(lhs) = lhs.strip_suffix(".value") {
            if let Some(("getvar", args)) = call(lhs) {
                if args.len() == 2
                    && args[0].trim() == "flowcharts.view"
                    && string(&args[1], state).as_deref() == Some("ViewName")
                {
                    if let Some(view) = string(rhs, state) {
                        state.view = Some(view);
                        return true;
                    }
                }
            }
        }
        if matches!(lhs, "sayoptions.waitforinput" | "sayoptions.fadewhendone") {
            return rhs.trim() == "true";
        }
        let name = lhs.strip_prefix("local ").unwrap_or(lhs).trim();
        if identifier(name) {
            if let Some(value) = literal(rhs, state) {
                state.vars.insert(name.into(), value);
                return true;
            }
        }
        return false;
    }
    let Some((name, args)) = call(code) else {
        return false;
    };
    match name {
        "runwait" if args.len() == 1 => {
            let Some((nested, a)) = call(&args[0]) else {
                return false;
            };
            match nested {
                "characters.LoadCharacterAsset" if a.len() == 1 => string(&a[0], state).is_some(),
                "flowcharts.LoadView" if a.len() == 1 => string(&a[0], state).is_some(),
                _ => false,
            }
        }
        "characters.LoadCharacterPortrait" if args.len() == 2 => {
            let (Some(cid), Some(portrait)) = (string(&args[0], state), string(&args[1], state))
            else {
                return false;
            };
            state.portraits.insert(cid, portrait);
            true
        }
        "setsaydialog" if args.len() == 1 => {
            let Some(mode) = args[0].trim().strip_prefix("saydialogs.") else {
                return false;
            };
            if !["character", "narrative", "center", "think"].contains(&mode) {
                return false;
            }
            state.dialog = mode.into();
            true
        }
        "setcharacter" if args.len() == 1 && args[0].trim() == "narrative" => {
            state.speaker = None;
            true
        }
        "setcharacter" if (1..=2).contains(&args.len()) => {
            let Some(cid) = character(&args[0], state) else {
                return false;
            };
            let portrait = if args.len() == 2 {
                let Some((owner, p)) = portrait(&args[1], state) else {
                    return false;
                };
                if owner != cid {
                    return false;
                };
                p
            } else {
                state
                    .portraits
                    .get(&cid)
                    .cloned()
                    .unwrap_or("normal".into())
            };
            state.speaker = Some((cid, portrait));
            true
        }
        "say" if args.len() == 1 => {
            let Some(text) = string(&args[0], state) else {
                return false;
            };
            let mode = state.dialog.as_str();
            let mut node = json!({"type":"say","mode":mode,"text":text});
            if ["character", "think"].contains(&mode) {
                let Some((cid, p)) = &state.speaker else {
                    return false;
                };
                node["character"] = json!(cid);
                node["portrait"] = json!(p);
            }
            nodes.push(node);
            true
        }
        "wait" if args.len() == 1 => {
            let Some(seconds) = number(&args[0], state) else {
                return false;
            };
            if seconds < 0.0 {
                return false;
            };
            nodes.push(json!({"type":"wait","seconds":seconds}));
            true
        }
        "runblock" if args.len() == 2 && args[0].trim() == "flowcharts.view" => {
            match string(&args[1], state).as_deref() {
                Some("out") => {
                    nodes.push(json!({"type":"scene","view":"out"}));
                    true
                }
                Some("view") => {
                    let Some(view) = &state.view else {
                        return false;
                    };
                    nodes.push(json!({"type":"scene","view":view}));
                    true
                }
                _ => false,
            }
        }
        "stage.show" if args.len() == 1 => show(&args[0], state, nodes),
        "stage.hide" if args.len() == 1 => {
            let Some(fields) = table(&args[0]) else {
                return false;
            };
            if fields
                .keys()
                .any(|k| !["character", "fadeDuration", "useDefaultSettings"].contains(&k.as_str()))
            {
                return false;
            }
            let Some(cid) = fields.get("character").and_then(|s| character(s, state)) else {
                return false;
            };
            let fade = match fields.get("fadeDuration") {
                Some(s) => {
                    let Some(n) = number(s, state) else {
                        return false;
                    };
                    n
                }
                None => 0.0,
            };
            if fields
                .get("useDefaultSettings")
                .is_none_or(|v| v.trim() != "false")
            {
                state.needs_native_defaults = true;
            }
            state.actors.remove(&cid);
            nodes.push(json!({"type":"hide","character":cid,"fadeDuration":fade}));
            true
        }
        "stage.showPortrait" if args.len() == 2 => {
            let Some(cid) = character(&args[0], state) else {
                return false;
            };
            let Some((owner, p)) = portrait(&args[1], state) else {
                return false;
            };
            if owner != cid {
                return false;
            }
            let Some(mut actor) = state.actors.get(&cid).cloned() else {
                return false;
            };
            actor["portrait"] = json!(p);
            actor["fadeDuration"] = json!(0);
            actor["moveDuration"] = json!(0);
            state.actors.insert(cid, actor.clone());
            nodes.push(actor);
            true
        }
        "characters.Focus" if args.len() == 1 => {
            let Some(cid) = string(&args[0], state) else {
                return false;
            };
            nodes.push(json!({"type":"focus","character":cid}));
            true
        }
        "mod_char_dim" if args.len() == 2 => {
            let Some(cid) = string(&args[0], state) else {
                return false;
            };
            let Some(Literal::Bool(dimmed)) = literal(&args[1], state) else {
                return false;
            };
            nodes.push(json!({"type":"dim","character":cid,"dimmed":dimmed}));
            true
        }
        "mod_hide_mood" if args.is_empty() => true,
        // No arbitrary object methods, loading aliases or user functions run here.
        _ => false,
    }
}

fn show(source: &str, state: &mut State, nodes: &mut Vec<Value>) -> bool {
    let Some(fields) = table(source) else {
        return false;
    };
    if fields.keys().any(|k| {
        ![
            "character",
            "portrait",
            "fromPosition",
            "toPosition",
            "facing",
            "fadeDuration",
            "moveDuration",
            "useDefaultSettings",
        ]
        .contains(&k.as_str())
    }) {
        return false;
    }
    let Some(cid) = fields.get("character").and_then(|s| character(s, state)) else {
        return false;
    };
    let old = state.actors.get(&cid);
    let fallback = old.and_then(|v| v["position"].as_str());
    let pos = fields
        .get("toPosition")
        .or_else(|| fields.get("fromPosition"))
        .and_then(|s| string(s, state))
        .or_else(|| fallback.map(str::to_owned));
    let Some(pos) = pos else { return false };
    let from = fields
        .get("fromPosition")
        .and_then(|s| string(s, state))
        .unwrap_or(pos.clone());
    // Keep native timing in raw, but illustrate its final position. Never turn
    // an asynchronous native movement into an editor blocking move implicitly.
    if from != pos
        || fields
            .get("useDefaultSettings")
            .is_none_or(|s| s.trim() != "false")
    {
        state.needs_native_defaults = true;
    }
    let p = if let Some(s) = fields.get("portrait") {
        let Some((owner, p)) = portrait(s, state) else {
            return false;
        };
        if owner != cid {
            return false;
        };
        p
    } else {
        old.and_then(|v| v["portrait"].as_str())
            .unwrap_or("normal")
            .into()
    };
    let facing = if let Some(s) = fields.get("facing") {
        let Some(v) = string(s, state) else {
            return false;
        };
        v
    } else {
        old.and_then(|v| v["facing"].as_str())
            .unwrap_or("right")
            .into()
    };
    let mut n = json!({"type":"show","character":cid,"position":pos,"portrait":p,"facing":facing});
    for key in ["fadeDuration", "moveDuration"] {
        if let Some(s) = fields.get(key) {
            let Some(v) = number(s, state) else {
                return false;
            };
            n[key] = json!(v);
        }
    }
    state.actors.insert(cid, n.clone());
    nodes.push(n);
    true
}

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}
fn string(s: &str, state: &State) -> Option<String> {
    match literal(s, state)? {
        Literal::String(s) => Some(s),
        _ => None,
    }
}
fn number(s: &str, state: &State) -> Option<f64> {
    match literal(s, state)? {
        Literal::Number(n) => Some(n),
        _ => None,
    }
}
fn character(s: &str, state: &State) -> Option<String> {
    match literal(s, state)? {
        Literal::Character(s) => Some(s),
        _ => None,
    }
}
fn portrait(s: &str, state: &State) -> Option<(String, String)> {
    match literal(s, state)? {
        Literal::Portrait(c, p) => Some((c, p)),
        _ => None,
    }
}
fn literal(s: &str, state: &State) -> Option<Literal> {
    let s = s.trim();
    if let Some(v) = state.vars.get(s) {
        return Some(v.clone());
    }
    if let Some(v) = lua_string(s) {
        return Some(Literal::String(v));
    }
    if let Ok(n) = s.parse::<f64>() {
        if n.is_finite() {
            return Some(Literal::Number(n));
        }
    }
    if s == "true" || s == "false" {
        return Some(Literal::Bool(s == "true"));
    }
    if let Some((name, a)) = call(s) {
        match (name, a.len()) {
            ("characters.Get", 1) => return Some(Literal::Character(string(&a[0], state)?)),
            ("characters.GetPortrait", 2) => {
                return Some(Literal::Portrait(
                    string(&a[0], state)?,
                    string(&a[1], state)?,
                ))
            }
            _ => {}
        }
    }
    None
}
fn assignment(source: &str) -> Option<(&str, &str)> {
    let index = top_level(source, b'=')?.first().copied()?;
    if source.as_bytes().get(index + 1) == Some(&b'=')
        || index > 0 && matches!(source.as_bytes()[index - 1], b'=' | b'<' | b'>' | b'~')
    {
        return None;
    }
    Some((source[..index].trim(), source[index + 1..].trim()))
}
fn table(source: &str) -> Option<BTreeMap<String, String>> {
    let s = source.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut result = BTreeMap::new();
    for part in split_top(s, b',')? {
        if part.trim().is_empty() {
            continue;
        }
        let (k, v) = assignment(part)?;
        if !identifier(k) || result.insert(k.into(), v.into()).is_some() {
            return None;
        }
    }
    Some(result)
}
fn call(source: &str) -> Option<(&str, Vec<String>)> {
    let s = source.trim();
    let index = s.find(['(', '{'])?;
    let name = s[..index].trim();
    if !name.split('.').all(identifier) {
        return None;
    }
    if s.as_bytes()[index] == b'{' {
        return Some((name, vec![s[index..].into()]));
    }
    let inside = s[index + 1..].strip_suffix(')')?;
    Some((
        name,
        split_top(inside, b',')?
            .into_iter()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().into())
            .collect(),
    ))
}
fn split_top(s: &str, separator: u8) -> Option<Vec<&str>> {
    let mut result = vec![];
    let mut start = 0;
    for i in top_level(s, separator)? {
        result.push(&s[start..i]);
        start = i + 1;
    }
    result.push(&s[start..]);
    Some(result)
}
/// Byte offsets are safe delimiters because all Lua punctuation is ASCII.
fn top_level(s: &str, separator: u8) -> Option<Vec<usize>> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut stack = vec![];
    let mut out = vec![];
    while i < b.len() {
        if matches!(b[i], b'\'' | b'"') {
            i = quoted_end(b, i)?;
            continue;
        }
        if let Some((_, end)) = long_string(b, i) {
            i = end;
            continue;
        }
        match b[i] {
            b'(' | b'{' | b'[' => stack.push(b[i]),
            b')' | b'}' | b']' => {
                let open = stack.pop()?;
                if !matches!((open, b[i]), (b'(', b')') | (b'{', b'}') | (b'[', b']')) {
                    return None;
                }
            }
            c if c == separator && stack.is_empty() => out.push(i),
            _ => {}
        }
        i += 1;
    }
    if stack.is_empty() {
        Some(out)
    } else {
        None
    }
}
fn quoted_end(b: &[u8], start: usize) -> Option<usize> {
    let quote = b[start];
    let mut i = start + 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
        } else if b[i] == quote {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None
}
fn long_string(b: &[u8], start: usize) -> Option<(usize, usize)> {
    if b.get(start) != Some(&b'[') {
        return None;
    }
    let mut opening = start + 1;
    while b.get(opening) == Some(&b'=') {
        opening += 1;
    }
    if b.get(opening) != Some(&b'[') {
        return None;
    }
    let width = opening - start + 1;
    let mut i = opening + 1;
    while i + width <= b.len() {
        if b[i] == b']'
            && b[i + 1..i + width - 1].iter().all(|c| *c == b'=')
            && b[i + width - 1] == b']'
        {
            return Some((opening + 1, i + width));
        }
        i += 1;
    }
    None
}
fn lua_string(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if let Some((start, end)) = long_string(b, 0) {
        if end != b.len() {
            return None;
        }
        let width = start;
        let raw = &s[start..end - width];
        return Some(
            raw.strip_prefix("\r\n")
                .or_else(|| raw.strip_prefix('\n'))
                .unwrap_or(raw)
                .into(),
        );
    }
    if b.is_empty() || !matches!(b[0], b'\'' | b'"') || quoted_end(b, 0)? != b.len() {
        return None;
    }
    let mut result = vec![];
    let mut i = 1;
    while i < b.len() - 1 {
        if b[i] != b'\\' {
            result.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        let c = *b.get(i)?;
        i += 1;
        match c {
            b'n' => result.push(b'\n'),
            b'r' => result.push(b'\r'),
            b't' => result.push(b'\t'),
            b'a' => result.push(7),
            b'b' => result.push(8),
            b'f' => result.push(12),
            b'v' => result.push(11),
            b'\\' | b'\'' | b'"' => result.push(c),
            b'\n' => result.push(b'\n'),
            b'\r' => {
                if b.get(i) == Some(&b'\n') {
                    i += 1;
                }
                result.push(b'\n');
            }
            b'0'..=b'9' => {
                let start = i - 1;
                while i < start + 3 && b.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                let n = s[start..i].parse::<u8>().ok()?;
                result.push(n);
            }
            b'x' => {
                let hex = s.get(i..i + 2)?;
                result.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            b'z' => {
                while b.get(i).is_some_and(u8::is_ascii_whitespace) {
                    i += 1;
                }
            }
            _ => return None,
        }
    }
    String::from_utf8(result).ok()
}

struct Statement {
    code: String,
    line: usize,
}
fn statements(source: &str) -> Vec<Statement> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let b = source.as_bytes();
    let mut out = vec![];
    let mut code = String::new();
    let mut i = 0;
    let mut line = 1;
    let mut start_line = 1;
    let mut stack = vec![];
    while i < b.len() {
        if b[i..].starts_with(b"--") {
            if let Some((_, end)) = long_string(b, i + 2) {
                line += b[i..end].iter().filter(|c| **c == b'\n').count();
                i = end;
                code.push(' ');
                continue;
            }
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if matches!(b[i], b'\'' | b'"') {
            let end = quoted_end(b, i).unwrap_or(b.len());
            code.push_str(&source[i..end]);
            line += b[i..end].iter().filter(|c| **c == b'\n').count();
            i = end;
            continue;
        }
        if let Some((_, end)) = long_string(b, i) {
            code.push_str(&source[i..end]);
            line += b[i..end].iter().filter(|c| **c == b'\n').count();
            i = end;
            continue;
        }
        if (b[i] == b'\n' || b[i] == b';') && stack.is_empty() {
            if !code.trim().is_empty() {
                out.push(Statement {
                    code: std::mem::take(&mut code),
                    line: start_line,
                });
            } else {
                code.clear();
            }
            if b[i] == b'\n' {
                line += 1;
            }
            start_line = line;
            i += 1;
            continue;
        }
        if code.trim().is_empty() && !b[i].is_ascii_whitespace() {
            start_line = line;
        }
        match b[i] {
            b'(' | b'{' | b'[' => stack.push(b[i]),
            b')' | b'}' | b']' => {
                stack.pop();
            }
            _ => {}
        }
        let ch = source[i..].chars().next().unwrap();
        code.push(ch);
        i += ch.len_utf8();
        if ch == '\n' {
            line += 1;
        }
    }
    if !code.trim().is_empty() {
        out.push(Statement {
            code,
            line: start_line,
        });
    }
    out
}
