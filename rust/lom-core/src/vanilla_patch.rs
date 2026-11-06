//! Small source edits applied to the installed vanilla script. Original Lua is
//! never stored in the project or exported package.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const MIN_HOST: &str = "1.4.9";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub find: String,
    pub occurrence: usize,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_condition: Option<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_condition: Option<Condition>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub source: String,
    pub flag: String,
    pub op: String,
    pub value: i64,
    pub invert: bool,
}
impl Default for Condition {
    fn default() -> Self {
        Self {
            source: "condition".into(),
            flag: String::new(),
            op: ">=".into(),
            value: 1,
            invert: false,
        }
    }
}
impl Condition {
    pub(crate) fn node(&self) -> Value {
        let mut node = json!({"id":"check","type":"flag_check","source":self.source,
            "flag":self.flag,"success":"end","failure":"end"});
        if self.source == "flag_value" {
            node["op"] = json!(self.op);
            node["value"] = json!(self.value);
        } else {
            node["invert"] = json!(self.invert);
        }
        node
    }
    fn validate(&self) -> Result<()> {
        crate::validate::validate_story(&json!({"id":"check","start":"check","nodes":[
            self.node(), {"id":"end","type":"end"}]}))?;
        Ok(())
    }
}

impl Edit {
    pub fn replacement(&self) -> String {
        self.replacement_with_prefix(None)
    }
    pub fn replacement_with_prefix(&self, prefix: Option<&str>) -> String {
        let key = |field: &str, text: &str| {
            prefix
                .map(|prefix| format!("{prefix}_{field}"))
                .unwrap_or_else(|| text.into())
        };
        let original = self
            .text
            .as_ref()
            .map(|text| {
                if self.kind == "option" {
                    crate::codegen::lua_str(&key("text", text))
                } else {
                    format!(
                        "say(luamanager.GetStoryText({}))",
                        crate::codegen::lua_str(&key("text", text))
                    )
                }
            })
            .unwrap_or_else(|| self.find.clone());
        if self.kind == "option" {
            return original;
        }
        let mut calls = Vec::new();
        for (index, (text, condition)) in [
            (&self.before, &self.before_condition),
            (&self.after, &self.after_condition),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 1 {
                calls.push(original.clone());
            }
            if text.is_empty() {
                continue;
            }
            let say = format!(
                "say(luamanager.GetStoryText({}))",
                crate::codegen::lua_str(&key(if index == 0 { "before" } else { "after" }, text))
            );
            calls.push(if let Some(condition) = condition {
                format!(
                    "if {} then\n{say}\nend",
                    crate::codegen::flag_check_expression(&condition.node())
                )
            } else {
                say
            });
        }
        calls.join("\n")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub target: String,
    pub edits: Vec<Edit>,
}

pub fn valid_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 64
        && !target.starts_with("MOD_")
        && target
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

pub fn parse(story: &Value) -> Result<Option<Patch>> {
    let Some(value) = story.get("vanilla_patch") else {
        return Ok(None);
    };
    let patch: Patch = serde_json::from_value(value.clone()).context("原版剧情补丁格式错误")?;
    ensure!(
        story["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.len() == 1
                && nodes[0]["type"] == "end"
                && nodes[0]["next_script"].as_str().unwrap_or("").is_empty()
                && story["start"] == nodes[0]["id"]),
        "原版补丁请在原文位置编辑，不支持普通剧情步骤"
    );
    ensure!(valid_target(&patch.target), "原版章节编号无效");
    ensure!(patch.edits.len() <= 4096, "单章补丁最多包含 4096 处修改");
    let mut used = BTreeSet::new();
    for edit in &patch.edits {
        ensure!(
            !edit.find.is_empty() && edit.find.len() <= 65_536,
            "补丁定位原文不能为空或超过 64 KiB"
        );
        ensure!(
            matches!(edit.kind.as_str(), "say" | "option"),
            "补丁只能修改对白或选项文字"
        );
        ensure!(
            edit.text.as_ref().map_or(0, String::len) + edit.before.len() + edit.after.len()
                <= 262_144
                && edit.occurrence > 0,
            "补丁替换文本或匹配序号无效"
        );
        ensure!(
            edit.kind == "say"
                || (edit.before.is_empty()
                    && edit.after.is_empty()
                    && edit.before_condition.is_none()
                    && edit.after_condition.is_none()),
            "选项仅修改文字，不插入对白"
        );
        ensure!(
            edit.text
                .as_ref()
                .is_none_or(|text| !text.trim().is_empty()),
            "修改后的原文不能为空"
        );
        ensure!(
            (edit.before.is_empty() || !edit.before.trim().is_empty())
                && (edit.after.is_empty() || !edit.after.trim().is_empty()),
            "新增对白不能只有空白"
        );
        for condition in [&edit.before_condition, &edit.after_condition]
            .into_iter()
            .flatten()
        {
            condition.validate()?;
        }
        ensure!(
            used.insert((&edit.find, edit.occurrence)),
            "同一处原文不能重复修改"
        );
    }
    Ok(Some(patch))
}

pub fn texts(story: &Value, mod_id: &str) -> Result<BTreeMap<String, String>> {
    let mut texts = BTreeMap::new();
    if let Some(patch) = parse(story)? {
        let id = story["id"].as_str().context("章节缺少编号")?;
        for (index, edit) in patch.edits.iter().enumerate() {
            let prefix = format!("MOD_{mod_id}_{id}_patch_{index}");
            for (field, value) in [
                ("text", edit.text.as_deref()),
                ("before", Some(edit.before.as_str())),
                ("after", Some(edit.after.as_str())),
            ] {
                if let Some(value) = value.filter(|value| !value.is_empty()) {
                    texts.insert(format!("{prefix}_{field}"), value.to_owned());
                }
            }
        }
    }
    Ok(texts)
}

pub fn apply(source: &str, edits: &[Edit]) -> Result<String> {
    let mut replacements = Vec::new();
    for edit in edits {
        ensure!(!edit.find.is_empty() && edit.occurrence > 0, "补丁定位无效");
        let start = source
            .match_indices(&edit.find)
            .nth(edit.occurrence - 1)
            .map(|(i, _)| i)
            .context("补丁定位原文不存在，游戏章节可能已更新")?;
        replacements.push((start, start + edit.find.len(), edit.replacement()));
    }
    replacements.sort_by_key(|item| item.0);
    ensure!(
        replacements.windows(2).all(|pair| pair[0].1 <= pair[1].0),
        "补丁修改范围重叠"
    );
    let mut result = source.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    Ok(result)
}

/// Derived declarations only: a chapter retains its original game trigger.
pub fn targets(stories: &BTreeMap<String, Value>) -> Result<BTreeMap<String, String>> {
    let mut targets = BTreeMap::new();
    let mut unique = BTreeSet::new();
    for (id, story) in stories {
        let target = parse(story)?
            .map(|patch| patch.target)
            .or(crate::vanilla_import::parse(story)?.map(|imported| imported.target));
        if let Some(target) = target {
            ensure!(
                unique.insert(target.to_ascii_lowercase()),
                "同一原版章节只能设置一个补丁"
            );
            targets.insert(target, id.clone());
        }
    }
    Ok(targets)
}

pub fn new_story(id: &str, target: &str) -> Result<Value> {
    ensure!(valid_target(target), "原版章节编号无效");
    Ok(
        json!({"story_schema":2,"id":id,"title":format!("原版章节 · {target}"),
        "start":"patch_entry","nodes":[{"id":"patch_entry","type":"end"}],
        "vanilla_patch":{"target":target,"edits":[]}}),
    )
}

#[derive(Clone, Debug)]
pub struct TextSite {
    pub kind: &'static str,
    pub original: String,
    pub find: String,
    pub occurrence: usize,
    pub key: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Token {
    pub start: usize,
    pub end: usize,
    pub value: String,
    pub string: bool,
}

/// Lex strings/comments first: text inside Lua comments and quoted code is never
/// offered as an executable dialogue. Branches and local scopes are left intact.
pub(crate) fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"--") {
            at = crate::lua_import::long_string(bytes, at + 2)
                .map(|(_, end)| end)
                .unwrap_or_else(|| {
                    source[at..]
                        .find('\n')
                        .map(|n| at + n + 1)
                        .unwrap_or(bytes.len())
                });
            continue;
        }
        let start = at;
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else {
                    let end = bytes[at] == quote;
                    at += 1;
                    if end {
                        break;
                    }
                }
            }
        } else if let Some((_, end)) = crate::lua_import::long_string(bytes, at) {
            at = end;
        } else if bytes[at].is_ascii_digit() {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_digit() || bytes[at] == b'.') {
                at += 1;
            }
        } else if ["==", "~=", ">=", "<=", ".."]
            .iter()
            .any(|op| source[at..].starts_with(op))
        {
            at += 2;
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
        } else {
            at += source[at..].chars().next().unwrap().len_utf8();
        }
        let raw = &source[start..at];
        let literal = crate::lua_import::lua_string(raw);
        tokens.push(Token {
            start,
            end: at,
            string: literal.is_some(),
            value: literal.unwrap_or_else(|| raw.into()),
        });
    }
    tokens
}

pub fn text_sites(source: &str) -> Vec<TextSite> {
    let tokens = tokens(source);
    let mut sites = Vec::new();
    for i in 0..tokens.len() {
        let token = &tokens[i];
        // say("text") and say(luamanager.GetStoryText("key")), the
        // confirmed forms in the current vanilla scripts.
        if token.value == "say"
            && !token.string
            && tokens.get(i + 1).is_some_and(|t| t.value == "(")
            && (i == 0 || !matches!(tokens[i - 1].value.as_str(), "." | ":" | "function"))
        {
            let (literal, end, key) = if tokens.get(i + 2).is_some_and(|t| t.string)
                && tokens.get(i + 3).is_some_and(|t| t.value == ")")
            {
                (i + 2, i + 3, false)
            } else if tokens.get(i + 2).is_some_and(|t| t.value == "luamanager")
                && tokens.get(i + 3).is_some_and(|t| t.value == ".")
                && tokens.get(i + 4).is_some_and(|t| t.value == "GetStoryText")
                && tokens.get(i + 5).is_some_and(|t| t.value == "(")
                && tokens.get(i + 6).is_some_and(|t| t.string)
                && tokens.get(i + 7).is_some_and(|t| t.value == ")")
                && tokens.get(i + 8).is_some_and(|t| t.value == ")")
            {
                (i + 6, i + 8, true)
            } else {
                continue;
            };
            let find = source[token.start..tokens[end].end].to_owned();
            let original = tokens[literal].value.clone();
            sites.push(TextSite {
                kind: "say",
                original: original.clone(),
                occurrence: source[..token.start].matches(&find).count() + 1,
                find,
                key: key.then_some(original),
            });
        }
        // Vanilla choose() tables store option labels as localized O_ keys.
        if token.string && token.value.starts_with("O_") && i > 0 && tokens[i - 1].value == "=" {
            let find = source[token.start..token.end].to_owned();
            sites.push(TextSite {
                kind: "option",
                original: token.value.clone(),
                occurrence: source[..token.start].matches(&find).count() + 1,
                find,
                key: Some(token.value.clone()),
            });
        }
    }
    sites
}
