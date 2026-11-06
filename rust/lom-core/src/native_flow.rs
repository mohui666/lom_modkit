//! Structured expressions and explicit control flow for generated vanilla chapters.
use crate::vanilla_import::{Imported, Origin};
use crate::vanilla_patch::Token;
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit())
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

/// Emit only the parsed expression grammar. No source-code strings are executable fields.
pub fn expression(v: &Value) -> Result<String> {
    Ok(match text(v, "kind") {
        "literal" => match &v["value"] {
            Value::Null => "nil".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => crate::codegen::lua_str(s),
            _ => bail!("原版表达式的字面量类型无效"),
        },
        "name" => {
            let n = text(v, "name");
            ensure!(name(n), "原版名称无效");
            n.into()
        }
        "local" => {
            let symbol = text(v, "symbol");
            ensure!(name(symbol) && name(text(v, "name")), "原版局部变量无效");
            format!("__lom_locals[{}]", crate::codegen::lua_str(symbol))
        }
        "member" => {
            let field = text(v, "field");
            ensure!(name(field), "原版字段名无效");
            format!("{}.{}", expression(&v["object"])?, field)
        }
        "index" => format!(
            "{}[{}]",
            expression(&v["object"])?,
            expression(&v["index"])?
        ),
        "unary" => {
            let op = text(v, "op");
            ensure!(["not", "-", "#"].contains(&op), "原版一元运算符无效");
            format!("({op} {})", expression(&v["value"])?)
        }
        "binary" => {
            let op = text(v, "op");
            ensure!(precedence(op).is_some(), "原版比较或运算符无效");
            format!(
                "({} {op} {})",
                expression(&v["left"])?,
                expression(&v["right"])?
            )
        }
        "call" => {
            let args = v["args"]
                .as_array()
                .context("原版调用缺少参数数组")?
                .iter()
                .map(expression)
                .collect::<Result<Vec<_>>>()?;
            format!("{}({})", expression(&v["function"])?, args.join(", "))
        }
        "table" => {
            let entries = v["entries"].as_array().context("原版列表缺少条目")?;
            let fields = entries
                .iter()
                .map(|entry| {
                    let value = expression(&entry["value"])?;
                    Ok(if entry["key"].is_null() {
                        value
                    } else {
                        format!("[{}] = {value}", expression(&entry["key"])?)
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            format!("{{{}}}", fields.join(", "))
        }
        _ => bail!("未知原版表达式类型 {}", text(v, "kind")),
    })
}
pub fn operation(v: &Value) -> Result<String> {
    match text(v, "kind") {
        "call" => {
            ensure!(v["expression"]["kind"] == "call", "原版动作必须是调用");
            expression(&v["expression"])
        }
        "assign" => {
            if let Some(declare) = v.get("declare") {
                ensure!(declare.is_boolean(), "局部变量声明标识必须是布尔值");
                ensure!(
                    declare != true || v["target"]["kind"] == "local",
                    "声明必须写入局部变量"
                );
            }
            ensure!(
                ["local", "name", "member", "index"].contains(&text(&v["target"], "kind")),
                "原版赋值目标无效"
            );
            Ok(format!(
                "{} = {}",
                expression(&v["target"])?,
                expression(&v["value"])?
            ))
        }
        "return" => Ok(format!(
            "__lom_result = {}; __lom_finished = true; return nil",
            expression(&v["value"])?
        )),
        _ => bail!("未知原版指令类型"),
    }
}
pub fn operations(values: &[Value]) -> Result<String> {
    Ok(values
        .iter()
        .map(operation)
        .collect::<Result<Vec<_>>>()?
        .join("\n"))
}
fn precedence(op: &str) -> Option<u8> {
    Some(match op {
        "or" => 1,
        "and" => 2,
        "==" | "~=" | "<" | ">" | "<=" | ">=" => 3,
        ".." => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        "^" => 8,
        _ => return None,
    })
}

enum Item {
    Action {
        operation: Value,
        visual: Option<Value>,
        end: usize,
    },
    Branch {
        arms: Vec<Arm>,
        otherwise: Vec<Item>,
        else_end: Option<usize>,
        end: usize,
    },
}
struct Arm {
    condition: Value,
    body: Vec<Item>,
    end: usize,
}
struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    at: usize,
    symbols: usize,
    scopes: Vec<BTreeMap<String, String>>,
    recognizer: crate::lua_import::VanillaRecognizer,
}
impl<'a> Parser<'a> {
    fn peek(&self) -> &str {
        self.tokens
            .get(self.at)
            .filter(|t| !t.string)
            .map(|t| t.value.as_str())
            .unwrap_or("")
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.peek() == s {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<()> {
        ensure!(
            self.eat(s),
            "需要 {s}，在第 {} 字节读到 {}",
            self.tokens
                .get(self.at)
                .map_or(self.source.len(), |t| t.start),
            self.peek()
        );
        Ok(())
    }
    fn resolve(&self, n: &str) -> Value {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(n))
            .map(|symbol| json!({"kind":"local","name":n,"symbol":symbol}))
            .unwrap_or_else(|| json!({"kind":"name","name":n}))
    }
    fn expr(&mut self, minimum: u8) -> Result<Value> {
        let token = self.tokens.get(self.at).context("表达式意外结束")?.clone();
        self.at += 1;
        let mut value = if token.string {
            json!({"kind":"literal","value":token.value})
        } else if let Ok(number) = token.value.parse::<serde_json::Number>() {
            json!({"kind":"literal","value":number})
        } else {
            match token.value.as_str() {
                "true" => json!({"kind":"literal","value":true}),
                "false" => json!({"kind":"literal","value":false}),
                "nil" => json!({"kind":"literal","value":null}),
                "not" | "-" | "#" => json!({"kind":"unary","op":token.value,"value":self.expr(7)?}),
                "(" => {
                    let inner = self.expr(0)?;
                    self.expect(")")?;
                    inner
                }
                "{" => {
                    let mut entries = Vec::new();
                    while !self.eat("}") {
                        let (key, field) = if self.eat("[") {
                            let key = self.expr(0)?;
                            self.expect("]")?;
                            self.expect("=")?;
                            (key, self.expr(0)?)
                        } else if self
                            .tokens
                            .get(self.at + 1)
                            .is_some_and(|t| t.value == "=" && !t.string)
                        {
                            let field = self.tokens[self.at].value.clone();
                            self.at += 2;
                            (json!({"kind":"literal","value":field}), self.expr(0)?)
                        } else {
                            (Value::Null, self.expr(0)?)
                        };
                        entries.push(json!({"key":key,"value":field}));
                        if self.peek() != "}" {
                            ensure!(self.eat(",") || self.eat(";"), "列表条目之间缺少逗号");
                        }
                    }
                    json!({"kind":"table","entries":entries})
                }
                n if name(n) => self.resolve(n),
                _ => bail!("无法转换表达式 {}，字节 {}", token.value, token.start),
            }
        };
        loop {
            if self.eat(".") {
                let field = self
                    .tokens
                    .get(self.at)
                    .context("字段名缺失")?
                    .value
                    .clone();
                ensure!(name(&field), "字段名无效");
                self.at += 1;
                value = json!({"kind":"member","object":value,"field":field});
            } else if self.eat("[") {
                let index = self.expr(0)?;
                self.expect("]")?;
                value = json!({"kind":"index","object":value,"index":index});
            } else if self.eat("(") {
                let mut args = Vec::new();
                if !self.eat(")") {
                    loop {
                        args.push(self.expr(0)?);
                        if self.eat(")") {
                            break;
                        }
                        self.expect(",")?;
                    }
                }
                value = json!({"kind":"call","function":value,"args":args});
            } else if self.peek() == "{" {
                let table = self.expr(9)?;
                value = json!({"kind":"call","function":value,"args":[table]});
            } else if let Some(priority) = precedence(self.peek()) {
                if priority < minimum {
                    break;
                }
                let op = self.peek().to_owned();
                self.at += 1;
                let right = self.expr(if ["^", ".."].contains(&op.as_str()) {
                    priority
                } else {
                    priority + 1
                })?;
                value = json!({"kind":"binary","op":op,"left":value,"right":right});
            } else {
                break;
            }
        }
        Ok(value)
    }
    fn block(&mut self) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        while self.at < self.tokens.len() && !["elseif", "else", "end"].contains(&self.peek()) {
            if self.eat(";") {
                continue;
            }
            if self.eat("if") {
                let mut arms = Vec::new();
                loop {
                    let condition = self.expr(0)?;
                    self.expect("then")?;
                    let end = self.tokens[self.at - 1].end;
                    self.scopes.push(BTreeMap::new());
                    let body = self.block()?;
                    self.scopes.pop();
                    arms.push(Arm {
                        condition,
                        body,
                        end,
                    });
                    if !self.eat("elseif") {
                        break;
                    }
                }
                let mut else_end = None;
                let otherwise = if self.eat("else") {
                    else_end = Some(self.tokens[self.at - 1].end);
                    self.scopes.push(BTreeMap::new());
                    let body = self.block()?;
                    self.scopes.pop();
                    body
                } else {
                    Vec::new()
                };
                self.expect("end")?;
                items.push(Item::Branch {
                    arms,
                    otherwise,
                    else_end,
                    end: self.tokens[self.at - 1].end,
                });
                continue;
            }
            let begin = self.tokens[self.at].start;
            if self.eat("return") {
                let value = if self.at == self.tokens.len()
                    || ["end", "else", "elseif"].contains(&self.peek())
                {
                    json!({"kind":"literal","value":null})
                } else {
                    self.expr(0)?
                };
                items.push(Item::Action {
                    operation: json!({"kind":"return","value":value}),
                    visual: None,
                    end: self.tokens[self.at - 1].end,
                });
                continue;
            }
            let local = self.eat("local");
            let operation = if local {
                let variable = self
                    .tokens
                    .get(self.at)
                    .context("局部变量名缺失")?
                    .value
                    .clone();
                ensure!(name(&variable), "局部变量名无效");
                self.at += 1;
                self.expect("=")?;
                let value = self.expr(0)?;
                let symbol = format!("v{}", self.symbols);
                self.symbols += 1;
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(variable.clone(), symbol.clone());
                json!({"kind":"assign","declare":true,"target":{"kind":"local","name":variable,"symbol":symbol},"value":value})
            } else {
                let target = self.expr(0)?;
                if self.eat("=") {
                    json!({"kind":"assign","target":target,"value":self.expr(0)?})
                } else {
                    ensure!(
                        target["kind"] == "call",
                        "只支持原版调用、赋值和条件分支，字节 {begin}"
                    );
                    json!({"kind":"call","expression":target})
                }
            };
            let end = self.tokens[self.at - 1].end;
            let visual = self.recognizer.node(&self.source[begin..end]);
            items.push(Item::Action {
                operation,
                visual,
                end,
            });
        }
        Ok(items)
    }
}

struct Builder<'a> {
    source: &'a str,
    cursor: usize,
    nodes: Vec<Value>,
    origins: Vec<Origin>,
}
impl Builder<'_> {
    fn push(&mut self, mut node: Value, ops: Option<Vec<Value>>, end: usize) -> usize {
        let index = self.nodes.len();
        let id = format!("original_{}", index + 1);
        node["id"] = json!(id);
        self.origins.push(Origin {
            code: self.source[self.cursor..end].into(),
            node: node.clone(),
            operations: ops,
        });
        self.cursor = end;
        node["_editor"] = json!({"vanilla_origin":index});
        self.nodes.push(node);
        index
    }
    fn link(&mut self, indices: &[usize], target: usize) {
        let id = self.nodes[target]["id"].clone();
        for &index in indices {
            self.nodes[index]["goto"] = id.clone();
        }
    }
    fn block(&mut self, items: Vec<Item>) -> (Option<usize>, Vec<usize>) {
        let mut first = None;
        let mut exits = Vec::new();
        let mut pending = Vec::new();
        let mut pending_end = self.cursor;
        for item in items {
            if let Item::Action {
                operation,
                visual: None,
                end,
            } = item
            {
                pending.push(operation);
                pending_end = end;
                continue;
            }
            if !pending.is_empty() {
                let index = self.push(
                    json!({"type":"native_action","operations":pending}),
                    None,
                    pending_end,
                );
                first.get_or_insert(index);
                self.link(&exits, index);
                exits = vec![index];
                pending = Vec::new();
            }
            match item {
                Item::Action {
                    operation,
                    visual: Some(node),
                    end,
                } => {
                    let index = self.push(node, Some(vec![operation]), end);
                    first.get_or_insert(index);
                    self.link(&exits, index);
                    exits = vec![index];
                }
                Item::Branch {
                    arms,
                    otherwise,
                    else_end,
                    end,
                } => {
                    let mut failures: Vec<usize> = Vec::new();
                    let mut ends = Vec::new();
                    for arm in arms {
                        let branch=self.push(json!({"type":"native_branch","condition":arm.condition,"success":"","failure":""}),None,arm.end);
                        if failures.is_empty() {
                            first.get_or_insert(branch);
                            self.link(&exits, branch);
                        }
                        for &prior in &failures {
                            self.nodes[prior]["failure"] = self.nodes[branch]["id"].clone();
                        }
                        let (entry, body_exits) = self.block(arm.body);
                        if let Some(entry) = entry {
                            self.nodes[branch]["success"] = self.nodes[entry]["id"].clone();
                            ends.extend(body_exits);
                        } else {
                            ends.push(branch);
                        }
                        failures = vec![branch];
                    }
                    if let Some(else_end) = else_end {
                        let marker = self.push(
                            json!({"type":"native_action","operations":[]}),
                            None,
                            else_end,
                        );
                        for &prior in &failures {
                            self.nodes[prior]["failure"] = self.nodes[marker]["id"].clone();
                        }
                        failures.clear();
                        let (entry, body_exits) = self.block(otherwise);
                        if let Some(entry) = entry {
                            self.link(&[marker], entry);
                            ends.extend(body_exits);
                        } else {
                            ends.push(marker);
                        }
                    }
                    let join =
                        self.push(json!({"type":"native_action","operations":[]}), None, end);
                    let id = self.nodes[join]["id"].clone();
                    for prior in failures {
                        self.nodes[prior]["failure"] = id.clone();
                    }
                    for prior in ends {
                        if self.nodes[prior]["type"] == "native_branch" {
                            self.nodes[prior]["success"] = id.clone();
                        } else {
                            self.nodes[prior]["goto"] = id.clone();
                        }
                    }
                    exits = vec![join];
                }
                _ => unreachable!(),
            }
        }
        if !pending.is_empty() {
            let index = self.push(
                json!({"type":"native_action","operations":pending}),
                None,
                pending_end,
            );
            first.get_or_insert(index);
            self.link(&exits, index);
            exits = vec![index];
        }
        (first, exits)
    }
}
pub fn import(
    source: &str,
    id: &str,
    title: &str,
    target: &str,
    translations: &BTreeMap<String, String>,
) -> Result<Value> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let tokens = crate::vanilla_patch::tokens(source);
    let needed = tokens
        .iter()
        .filter(|token| token.string)
        .filter_map(|token| {
            translations
                .get(&token.value)
                .map(|text| (token.value.clone(), text.clone()))
        })
        .collect();
    let mut parser = Parser {
        source,
        tokens,
        at: 0,
        symbols: 0,
        scopes: vec![BTreeMap::new()],
        recognizer: crate::lua_import::VanillaRecognizer::new(&needed),
    };
    let items = parser.block()?;
    ensure!(
        parser.at == parser.tokens.len(),
        "原版章节存在未配对的条件结束语句"
    );
    let mut builder = Builder {
        source,
        cursor: 0,
        nodes: Vec::new(),
        origins: Vec::new(),
    };
    let (first, exits) = builder.block(items);
    let end = builder.push(
        json!({"type":"end","resume_original":true}),
        None,
        source.len(),
    );
    builder.link(&exits, end);
    for node in &mut builder.nodes {
        for field in ["operations", "condition"] {
            if let Some(value) = node.get_mut(field) {
                annotate_text(value, translations);
            }
        }
    }
    annotate_branches(&mut builder.nodes, translations);
    for (node, origin) in builder.nodes.iter().zip(builder.origins.iter_mut()) {
        origin.node = node.clone();
        origin.node.as_object_mut().unwrap().remove("_editor");
    }
    let story = json!({"story_schema":2,"id":id,"title":title,"mood":true,
        "start":builder.nodes[first.unwrap_or(end)]["id"],"nodes":builder.nodes,
        "vanilla_import":{"schema":2,"target":target,"origins":builder.origins}});
    crate::validate::validate_story(&story)?;
    Ok(story)
}

/// Refresh derived branch captions after editing structured options or checks.
pub fn refresh_branch_labels(story: &mut Value) {
    if story["vanilla_import"]["schema"] != 2 {
        return;
    }
    if let Some(nodes) = story["nodes"].as_array_mut() {
        annotate_branches(nodes, &BTreeMap::new());
    }
}

fn annotate_branches(nodes: &mut [Value], translations: &BTreeMap<String, String>) {
    let mut bindings = BTreeMap::new();
    let mut choices: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for node in nodes.iter() {
        for op in node["operations"].as_array().into_iter().flatten() {
            if op["kind"] != "assign" {
                continue;
            }
            let target = &op["target"];
            if target["kind"] == "local" {
                bindings.insert(text(target, "symbol").to_owned(), op["value"].clone());
            }
            if target["kind"] == "index" && target["object"]["kind"] == "local" {
                if let Some(label) = option_text(&op["value"], translations) {
                    let key = (
                        text(&target["object"], "symbol").to_owned(),
                        target["index"]["value"].to_string(),
                    );
                    let labels = choices.entry(key).or_default();
                    if !labels.contains(&label) {
                        labels.push(label);
                    }
                }
            }
        }
    }
    for node in nodes.iter_mut().filter(|n| n["type"] == "native_branch") {
        if let Some(editor) = node.get_mut("_editor").and_then(Value::as_object_mut) {
            editor.remove("native_condition");
        }
        let condition = &node["condition"];
        let (value, result) = if condition["kind"] == "binary" {
            (&condition["left"], condition["right"]["value"].to_string())
        } else {
            (condition, String::new())
        };
        let binding = if value["kind"] == "local" {
            bindings.get(text(value, "symbol")).unwrap_or(value)
        } else {
            value
        };
        if binding["kind"] != "call" {
            continue;
        }
        let function = &binding["function"];
        if function["kind"] == "name" && function["name"] == "choose" {
            let table = &binding["args"][0];
            if let Some(labels) = choices.get(&(text(table, "symbol").to_owned(), result.clone())) {
                node["_editor"]["native_condition"] =
                    json!({"kind":"choice","index":result,"labels":labels});
            }
        } else if function["kind"] == "member" && function["object"]["name"] == "checkpointmanager"
        {
            let key = binding["args"][0]["value"].as_str().unwrap_or("");
            node["_editor"]["native_condition"] = json!({"kind":"check","key":key,"result":result});
        }
    }
}

fn option_text(value: &Value, translations: &BTreeMap<String, String>) -> Option<String> {
    if value["kind"] == "literal" {
        return value["display"].as_str().map(str::to_owned).or_else(|| {
            value["value"]
                .as_str()
                .map(|key| translations.get(key).cloned().unwrap_or_else(|| key.into()))
        });
    }
    if value["kind"] == "binary" {
        return option_text(&value["left"], translations)
            .or_else(|| option_text(&value["right"], translations));
    }
    None
}

fn annotate_text(value: &mut Value, translations: &BTreeMap<String, String>) {
    if value["kind"] == "literal" {
        if let Some(display) = value["value"]
            .as_str()
            .and_then(|key| translations.get(key))
        {
            value["display"] = json!(display);
        }
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                annotate_text(item, translations);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                annotate_text(item, translations);
            }
        }
        _ => {}
    }
}
/// A copied local declaration owns a fresh cell. References to outer locals stay bound.
pub fn rebind_copied_locals(copied: &mut [Value], story: &Value) {
    fn visit(value: &Value, callback: &mut impl FnMut(&Value)) {
        callback(value);
        match value {
            Value::Array(items) => {
                for item in items {
                    visit(item, callback);
                }
            }
            Value::Object(fields) => {
                for item in fields.values() {
                    visit(item, callback);
                }
            }
            _ => {}
        }
    }
    let mut used = std::collections::BTreeSet::new();
    visit(story, &mut |value| {
        if value["kind"] == "local" {
            used.insert(text(value, "symbol").to_owned());
        }
    });
    let mut mapping = BTreeMap::new();
    for node in copied.iter() {
        visit(node, &mut |value| {
            if value["kind"] == "assign" && value["declare"] == true {
                let old = text(&value["target"], "symbol");
                if !mapping.contains_key(old) {
                    let fresh = (1..)
                        .map(|n| format!("copied_v{n}"))
                        .find(|name| !used.contains(name))
                        .unwrap();
                    used.insert(fresh.clone());
                    mapping.insert(old.to_owned(), fresh);
                }
            }
        });
    }
    fn rewrite(value: &mut Value, mapping: &BTreeMap<String, String>) {
        if value["kind"] == "local" {
            if let Some(symbol) = mapping.get(text(value, "symbol")) {
                value["symbol"] = json!(symbol);
            }
            return;
        }
        match value {
            Value::Array(items) => {
                for item in items {
                    rewrite(item, mapping);
                }
            }
            Value::Object(fields) => {
                for item in fields.values_mut() {
                    rewrite(item, mapping);
                }
            }
            _ => {}
        }
    }
    for node in copied {
        rewrite(node, &mapping);
    }
}

pub fn validate(story: &Value, imported: &Imported) -> Result<()> {
    let ns = story["nodes"].as_array().context("原版步骤缺失")?;
    ensure!(
        ns.first().is_some_and(|n| n["id"] == story["start"]),
        "原版覆盖章节必须从首个步骤进入"
    );
    for origin in &imported.origins {
        if let Some(ops) = &origin.operations {
            operations(ops)?;
        }
    }
    for node in ns {
        ensure!(
            !matches!(text(node, "type"), "combat" | "battle"),
            "原版整章局部状态无法跨战斗场景续接"
        );
        if node["type"] == "native_action" {
            operations(
                node["operations"]
                    .as_array()
                    .context("原版指令缺少操作列表")?,
            )?;
        }
        if node["type"] == "native_branch" {
            expression(&node["condition"])?;
        }
    }
    Ok(())
}
