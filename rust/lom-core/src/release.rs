//! Offline authoring preflight, reversible repairs and private release artifacts.
mod diagnostic;
mod flow;
mod play_bundle;
mod stage;
mod statistics;
use crate::project::Project;
use anyhow::{ensure, Context, Result};
pub use diagnostic::{
    detect_game_version, detect_runtime_version, export_diagnostic_bundle, sanitize_text,
    DiagnosticOptions, MAX_COLLECTION_ITEMS, MAX_LOG_OUTPUT_CHARS, MAX_LOG_READ_BYTES,
    MAX_STRING_CHARS,
};
pub use play_bundle::{
    build_play_bundle, install_play_bundle, validate_loader_archive, PlayBundleResult,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub use stage::{
    ensure_stage, find_stage_issues, missing_stage_linear, propose_safe_fixes, required_character,
    FixProposal,
};
pub use statistics::{
    calculate_project_statistics, calculate_voice_coverage, referenced_asset_paths,
    unused_asset_paths,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
pub type Stories = BTreeMap<String, Value>;
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn nodes(value: &Value) -> &[Value] {
    value["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[])
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightIssue {
    pub severity: String,
    pub code: String,
    pub story_id: String,
    pub node_id: String,
    pub message: String,
    pub fixable: bool,
}
impl PreflightIssue {
    fn new(
        severity: &str,
        code: &str,
        story: &str,
        node: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: severity.into(),
            code: code.into(),
            story_id: story.into(),
            node_id: node.into(),
            message: message.into(),
            fixable: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Editing,
    Release,
}
struct ContentSpec<'a> {
    field: &'static str,
    raw: &'a str,
    kind: &'static str,
    audio_kind: Option<&'a str>,
    portrait: Option<&'a str>,
}
fn content_specs(node: &Value) -> Vec<ContentSpec<'_>> {
    let kind = text(node, "type");
    let mut refs = Vec::new();
    if kind == "cg"
        && matches!(text(node, "kind"), "event" | "picture" | "item" | "big")
        && node["action"] == "show"
        && node["background_only"] != true
        && text(node, "key").starts_with("user:")
    {
        refs.push(ContentSpec {
            field: "key",
            raw: text(node, "key"),
            kind: "picture",
            audio_kind: None,
            portrait: None,
        });
    }
    if kind == "say"
        && !text(node, "mood_icon").is_empty()
        && !text(node, "mood_character").is_empty()
    {
        refs.push(ContentSpec {
            field: "mood_character",
            raw: text(node, "mood_character"),
            kind: "character",
            audio_kind: None,
            portrait: None,
        });
    }
    if ["music", "sound"].contains(&kind) {
        refs.push(ContentSpec {
            field: "name",
            raw: text(node, "name"),
            kind: "audio",
            audio_kind: Some(if kind == "music" {
                "music"
            } else if node["kind"] == "env" {
                "env"
            } else {
                "sound"
            }),
            portrait: None,
        });
    }
    if kind == "say" && !text(node, "voice").is_empty() {
        refs.push(ContentSpec {
            field: "voice",
            raw: text(node, "voice"),
            kind: "audio",
            audio_kind: None,
            portrait: None,
        });
    }
    if [
        "show",
        "say",
        "portrait",
        "hide",
        "move",
        "face",
        "focus",
        "offset",
        "shock",
        "dim",
        "rotate",
        "intro",
        "combat",
        "character_title",
    ]
    .contains(&kind)
        && !text(node, "character").is_empty()
    {
        refs.push(ContentSpec {
            field: "character",
            raw: text(node, "character"),
            kind: "character",
            audio_kind: None,
            portrait: (["show", "say", "portrait"].contains(&kind))
                .then(|| text(node, "portrait"))
                .filter(|s| !s.is_empty()),
        });
    }
    let image_active = !(["custom_cg", "overlay"].contains(&kind) && node["action"] == "hide"
        || kind == "background" && ["fadeout", "clear"].contains(&text(node, "action")));
    if image_active && !text(node, "image").is_empty() {
        refs.push(ContentSpec {
            field: "image",
            raw: text(node, "image"),
            kind: "image",
            audio_kind: None,
            portrait: None,
        });
    }
    for (raw, content_kind, _) in crate::content::gameplay_references(node) {
        if raw.starts_with("user:") {
            refs.push(ContentSpec {
                field: "gameplay",
                raw,
                kind: content_kind,
                audio_kind: None,
                portrait: None,
            });
        }
    }
    refs
}
fn illegal_path(raw: &str) -> bool {
    let s = raw.trim().replace('\\', "/");
    let body = s.strip_prefix("user:").unwrap_or(&s);
    s.starts_with('/')
        || s.as_bytes().get(1) == Some(&b':')
        || body.split('/').any(|p| p == "..")
        || s.starts_with("user:") && body.contains('/')
}
fn missing_code(r: &ContentSpec<'_>) -> &'static str {
    if r.field == "voice" {
        "missing_voice"
    } else if matches!(r.kind, "image" | "picture") {
        "missing_image"
    } else {
        "impossible_content_reference"
    }
}
fn audit_content(project: &Project, root: &Path) -> Vec<PreflightIssue> {
    let mut out = Vec::new();
    let mut referenced = BTreeSet::new();
    if let Ok(extensions) = crate::content::character_extensions(&project.assets) {
        for (target, (_, meta)) in extensions {
            if project.stories.values().any(|s| {
                crate::parallel::story_actions(s).any(|(_, n)| {
                    content_specs(n)
                        .iter()
                        .any(|r| r.kind == "character" && r.raw == target)
                })
            }) {
                referenced.insert(("character".into(), text(&meta, "id").to_owned()));
            }
        }
    }
    for (sid, story) in &project.stories {
        for (parent, node) in crate::parallel::story_actions(story) {
            for mut r in content_specs(node) {
                let nid = text(parent, "id");
                if let Some(id) = r.raw.strip_prefix("user:") {
                    if let Err(e) = crate::content::validate_content_id(id) {
                        out.push(PreflightIssue::new(
                            "error",
                            if illegal_path(r.raw) {
                                "illegal_content_path"
                            } else {
                                "impossible_content_reference"
                            },
                            sid,
                            nid,
                            e.to_string(),
                        ));
                        continue;
                    }
                    if r.kind == "picture" {
                        match crate::content::picture_content_kind(root, id) {
                            Ok(kind) => r.kind = kind,
                            Err(error) => {
                                out.push(PreflightIssue::new(
                                    "error",
                                    "missing_image",
                                    sid,
                                    nid,
                                    format!("{error:#}"),
                                ));
                                continue;
                            }
                        }
                    }
                    referenced.insert((r.kind.to_owned(), id.to_owned()));
                    let base = root.join(format!("assets/user/{}/{id}", r.kind));
                    if !base.is_dir() {
                        let other = crate::content::CONTENT_TYPES
                            .iter()
                            .any(|kind| root.join(format!("assets/user/{kind}/{id}")).is_dir());
                        out.push(PreflightIssue::new(
                            "error",
                            if other {
                                "wrong_user_content_type"
                            } else {
                                missing_code(&r)
                            },
                            sid,
                            nid,
                            format!("用户内容 {} 缺失或类型不匹配（需要 {}）", r.raw, r.kind),
                        ));
                        continue;
                    }
                    match crate::content::resolve_content(root, r.kind, id) {
                        Ok((meta, _)) => {
                            if node["type"] == "character_profile" {
                                if let Err(error) =
                                    crate::character_profile::validate_reference(node, &meta)
                                {
                                    out.push(PreflightIssue::new(
                                        "error",
                                        "invalid_character_profile",
                                        sid,
                                        nid,
                                        format!("{error:#}"),
                                    ));
                                }
                            }
                            if node["type"] == "use_item" && meta["use_in"] != "field" {
                                out.push(PreflightIssue::new(
                                    "error",
                                    "wrong_user_content_type",
                                    sid,
                                    nid,
                                    "use_item 只能同步使用 field 消耗品",
                                ));
                            }
                            if r.field == "gameplay" && r.kind == "item" {
                                for (raw, _, category) in crate::content::gameplay_references(node)
                                {
                                    if raw == r.raw && meta["category"] != category {
                                        out.push(PreflightIssue::new(
                                            "error",
                                            "wrong_user_content_type",
                                            sid,
                                            nid,
                                            format!("{} 的物品类别与 {category} 不一致", r.raw),
                                        ));
                                    }
                                }
                            }
                            if r.audio_kind.is_some_and(|k| meta["audio_kind"] != k) {
                                out.push(PreflightIssue::new(
                                    "error",
                                    "wrong_user_content_type",
                                    sid,
                                    nid,
                                    format!("{} 不能用在当前音频类型", r.raw),
                                ));
                            }
                            if r.portrait
                                .is_some_and(|p| meta["portraits"].get(p).is_none())
                            {
                                out.push(PreflightIssue::new(
                                    "error",
                                    "missing_portrait",
                                    sid,
                                    nid,
                                    format!("{} 没有表情 {}", r.raw, r.portrait.unwrap()),
                                ));
                            }
                        }
                        Err(e) => {
                            let detail = format!("{e:#}");
                            let code = if detail.contains("路径")
                                || detail.contains("文件名")
                                || detail.contains("同目录")
                            {
                                "illegal_content_path"
                            } else if detail.contains("不存在") || detail.contains("空的") {
                                if r.kind == "character" {
                                    "missing_portrait"
                                } else {
                                    missing_code(&r)
                                }
                            } else {
                                "stale_content_metadata"
                            };
                            out.push(PreflightIssue::new("error", code, sid, nid, detail));
                        }
                    }
                } else if illegal_path(r.raw) || r.field == "voice" {
                    out.push(PreflightIssue::new(
                        "error",
                        if illegal_path(r.raw) {
                            "illegal_content_path"
                        } else {
                            "impossible_content_reference"
                        },
                        sid,
                        nid,
                        format!("非法内容引用: {}", r.raw),
                    ));
                } else if r.kind == "image" {
                    match crate::package::resolve_confined_file(root, &root.join(r.raw)) {
                        Ok(file) => {
                            if fs::metadata(file)
                                .is_ok_and(|m| m.len() > crate::content::MAX_IMAGE_BYTES)
                            {
                                out.push(PreflightIssue::new(
                                    "error",
                                    "large_image",
                                    sid,
                                    nid,
                                    format!("图片超过 8 MiB: {}", r.raw),
                                ));
                            }
                        }
                        Err(_) => out.push(PreflightIssue::new(
                            "error",
                            "missing_image",
                            sid,
                            nid,
                            format!("图片不存在: {}", r.raw),
                        )),
                    }
                }
            }
        }
    }
    if let Some(config) = &project.scenario {
        for node in crate::content::scenario_gameplay_nodes(config) {
            for (raw, kind, category) in crate::content::gameplay_references(&node) {
                let Some(id) = raw.strip_prefix("user:") else {
                    continue;
                };
                referenced.insert((kind.to_owned(), id.to_owned()));
                let check =
                    crate::content::resolve_content(root, kind, id).and_then(|(meta, _)| {
                        ensure!(
                            kind != "item" || meta["category"] == category,
                            "{raw} 的物品类别不匹配"
                        );
                        Ok(())
                    });
                if let Err(error) = check {
                    out.push(PreflightIssue::new(
                        "error",
                        "impossible_content_reference",
                        "scenario",
                        "",
                        format!("{error:#}"),
                    ));
                }
            }
        }
    }
    let mut valid = BTreeSet::new();
    let mut folders = BTreeSet::new();
    for name in project
        .assets
        .keys()
        .filter(|p| p.starts_with("assets/user/"))
    {
        let parts: Vec<_> = name.split('/').collect();
        if parts.len() < 5 || !crate::content::CONTENT_TYPES.contains(&parts[2]) {
            out.push(PreflightIssue::new(
                "error",
                "illegal_content_path",
                "",
                "",
                format!("非法内容目录: {name}"),
            ));
            continue;
        }
        folders.insert((parts[2], parts[3]));
    }
    for (kind, id) in folders {
        if let Err(error) = crate::content::validate_content_id(id) {
            out.push(PreflightIssue::new(
                "error",
                "illegal_content_path",
                "",
                "",
                error.to_string(),
            ));
            continue;
        }
        match crate::content::resolve_content(root, kind, id) {
            Ok((meta, _)) => {
                if kind == "character" {
                    if let Err(error) = crate::character_profile::validate_for_export(&meta) {
                        out.push(PreflightIssue::new(
                            "error",
                            "conflicting_biography",
                            "",
                            "",
                            format!("assets/user/character/{id}/content.json: {error:#}"),
                        ));
                    }
                }
                if meta["id"] != id || meta["type"] != kind {
                    out.push(PreflightIssue::new(
                        "error",
                        "stale_content_metadata",
                        "",
                        "",
                        format!("user:{id} 的 metadata 与目录身份不一致"),
                    ));
                } else {
                    valid.insert((kind.to_owned(), id.to_owned()));
                    if kind == "character"
                        && meta["extends"].as_str().unwrap_or("").is_empty()
                        && crate::character_profile::has_profile(&meta)
                    {
                        referenced.insert((kind.to_owned(), id.to_owned()));
                    }
                }
            }
            Err(e) => out.push(PreflightIssue::new(
                "error",
                "stale_content_metadata",
                "",
                "",
                format!("{e:#}"),
            )),
        }
    }
    for (kind, id) in valid.difference(&referenced) {
        let label = match kind.as_str() {
            "image" => "图片",
            "audio" => "音频",
            "character" => "人物",
            "item" => "物品",
            "martial" => "功法",
            "support" => "支援",
            "skill" => "技能",
            "talent" => "天赋",
            _ => kind.as_str(),
        };
        out.push(PreflightIssue::new(
            "warning",
            "unused_content",
            "",
            "",
            format!("已导入{label} user:{id}，但未找到作品中的引用，可保留备用。此提醒本身不阻止试玩或导出，无需重新提取；要使用时，请在对应步骤中选择这项内容。"),
        ));
    }
    out
}
fn asset_root(project: &Project) -> Result<tempfile::TempDir> {
    let temp = crate::workdir::create()?;
    for (name, bytes) in &project.assets {
        crate::package::canonical_archive_name(name)?;
        ensure!(name.starts_with("assets/"), "项目资源路径必须在 assets/ 下");
        let path = temp.path().join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, bytes)?;
    }
    Ok(temp)
}
fn ordered(mut issues: Vec<PreflightIssue>) -> Vec<PreflightIssue> {
    issues.sort_by(|a, b| {
        (&a.severity, &a.story_id, &a.node_id, &a.code, &a.message).cmp(&(
            &b.severity,
            &b.story_id,
            &b.node_id,
            &b.code,
            &b.message,
        ))
    });
    issues.dedup();
    issues
}
/// Compiler diagnostics shared by the editor and project preflight.
pub fn compiler_issues(story_id: &str, story: &Value) -> Vec<PreflightIssue> {
    compiler_issues_with_catalog(story_id, story, crate::validate::editor_data())
}
pub fn compiler_issues_with_catalog(
    story_id: &str,
    story: &Value,
    catalog: &Value,
) -> Vec<PreflightIssue> {
    let (severity, code, messages) =
        match crate::validate::validate_story_with_catalog(story, catalog) {
            Ok(warnings) => ("warning", "compiler_warning", warnings),
            Err(error) => ("error", "compiler_error", vec![format!("{error:#}")]),
        };
    messages
        .into_iter()
        .map(|message| {
            // validate_story currently returns strings. Only its known node
            // label prefix establishes a location; never guess from prose or
            // from a missing destination mentioned later in the message.
            let node_id = message
                .strip_prefix("story.json: ")
                .unwrap_or(&message)
                .strip_prefix("节点 \"")
                .and_then(|rest| rest.split_once("\"("))
                .map(|(id, _)| id)
                .filter(|id| nodes(story).iter().any(|node| text(node, "id") == *id))
                .unwrap_or("");
            PreflightIssue::new(severity, code, story_id, node_id, message.clone())
        })
        .collect()
}

pub fn run_preflight(
    project: &Project,
    profile: Profile,
    runtime_version: &str,
) -> Vec<PreflightIssue> {
    let mut issues = Vec::new();
    let catalog = match crate::validate::catalog_with_assets(&project.assets) {
        Ok(catalog) => Some(catalog),
        Err(error) => {
            issues.push(PreflightIssue::new(
                "error",
                "stale_content_metadata",
                "",
                "",
                format!("{error:#}"),
            ));
            None
        }
    };
    if crate::scenario::enabled(&project.manifest) {
        let result = project
            .scenario
            .as_ref()
            .context("缺少 scenario.json")
            .and_then(|c| crate::scenario::validate(c, &project.manifest, &project.stories));
        if let Err(e) = result {
            issues.push(PreflightIssue::new(
                "error",
                "invalid_scenario",
                "",
                "",
                format!("{e:#}"),
            ));
        }
    }
    let entry = text(&project.manifest, "entry");
    if entry.is_empty() || !project.stories.contains_key(entry) {
        issues.push(PreflightIssue::new(
            "error",
            "invalid_entry",
            entry,
            "",
            "入口章节不存在",
        ));
    }
    match asset_root(project) {
        Ok(root) => issues.extend(audit_content(project, root.path())),
        Err(e) => issues.push(PreflightIssue::new(
            "error",
            "illegal_content_path",
            "",
            "",
            format!("{e:#}"),
        )),
    }
    for (sid, story) in &project.stories {
        issues.extend(match &catalog {
            Some(catalog) => compiler_issues_with_catalog(sid, story, catalog),
            None => compiler_issues(sid, story),
        });
        for (nid, cid) in find_stage_issues(story) {
            let mut issue = PreflightIssue::new(
                "warning",
                "stage_missing",
                sid,
                &nid,
                format!("{cid} 在某条路径上尚未登场或已经退场；请先添加人物登场"),
            );
            issue.fixable = true;
            issues.push(issue);
        }
        for node in nodes(story) {
            let mut values: Vec<_> = ["text", "title", "desc", "name"]
                .into_iter()
                .filter_map(|k| node[k].as_str())
                .collect();
            values.extend(
                node["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o["text"].as_str()),
            );
            if values.iter().any(|v| {
                ["在这里填写", "选项一", "选项二", "新的结局"]
                    .iter()
                    .any(|marker| v.contains(marker))
            }) {
                issues.push(PreflightIssue::new(
                    "warning",
                    "placeholder_text",
                    sid,
                    text(node, "id"),
                    "仍然存在模板占位文字",
                ));
            }
            if node["type"] == "show"
                && ["LB2", "RB2"].contains(&text(node, "position"))
                && node["fadeDuration"].as_f64().unwrap_or(0.0) == 0.0
                && node["moveDuration"].as_f64().unwrap_or(0.0) == 0.0
            {
                issues.push(PreflightIssue::new(
                    "warning",
                    "back_stage_position",
                    sid,
                    text(node, "id"),
                    "人物位于后景边缘，可能被遮挡",
                ));
            }
        }
    }
    let analysis = crate::analysis::analyze_project(&project.stories, &project.manifest);
    issues.extend(flow::flow_issues(&project.stories));
    for trigger in project.manifest["campaign"]["triggers"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if !project.stories.contains_key(text(trigger, "script")) {
            issues.push(PreflightIssue::new(
                "error",
                "invalid_cross_story_goto",
                text(trigger, "script"),
                "",
                "触发器章节不存在",
            ));
        }
    }
    for symbol in analysis["symbols"].as_array().into_iter().flatten() {
        if symbol["kind"] == "mod_flag" && symbol["possibly_read_before_write"] == true {
            let use_ = symbol["uses"]
                .as_array()
                .and_then(|a| a.iter().find(|u| u["access"] == "read"));
            issues.push(PreflightIssue::new(
                "warning",
                "possible_read_before_write",
                use_.map(|v| text(v, "story")).unwrap_or(""),
                use_.map(|v| text(v, "node")).unwrap_or(""),
                format!("Flag {} 可能在写入前读取", text(symbol, "name")),
            ));
        }
    }
    let mut seen = BTreeSet::new();
    let mut pending = vec![entry.to_owned()];
    if let Some(config) = &project.scenario {
        pending.extend(
            config["events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["script"].as_str().map(str::to_owned)),
        );
    }
    while let Some(sid) = pending.pop() {
        if seen.insert(sid.clone()) {
            if let Some(s) = project.stories.get(&sid) {
                for n in nodes(s) {
                    if n["type"] == "end" && !text(n, "next_script").is_empty() {
                        pending.push(text(n, "next_script").to_owned());
                    }
                }
            }
        }
    }
    if project.stories.contains_key(entry) {
        for sid in project.stories.keys().filter(|sid| !seen.contains(*sid)) {
            issues.push(PreflightIssue::new(
                "warning",
                "unreachable_story",
                sid,
                "",
                format!("从入口章节 {entry} 无法到达章节 {sid}"),
            ));
        }
    }
    if profile == Profile::Release {
        let used_extensions: Vec<_> = crate::content::character_extensions(&project.assets)
            .unwrap_or_default()
            .into_iter()
            .filter(|(target, _)| {
                project.stories.values().any(|s| {
                    crate::parallel::story_actions(s).any(|(_, n)| {
                        content_specs(n)
                            .iter()
                            .any(|r| r.kind == "character" && r.raw == *target)
                    })
                })
            })
            .map(|(_, (key, _))| key.trim_end_matches("content.json").to_owned())
            .collect();
        let assets: Vec<_> = project
            .assets
            .keys()
            .filter(|name| {
                !used_extensions
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            })
            .cloned()
            .collect();
        apply_release_profile(
            issues,
            &project.stories,
            &project.manifest,
            runtime_version,
            Some(&assets),
        )
    } else {
        ordered(issues)
    }
}

fn version(raw: &str) -> Option<(Vec<u64>, Option<Vec<String>>)> {
    let re=regex::Regex::new(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$").unwrap();
    let c = re.captures(raw)?;
    Some((
        (1..=3).map(|i| c[i].parse().unwrap_or(u64::MAX)).collect(),
        c.get(4)
            .map(|m| m.as_str().split('.').map(str::to_owned).collect()),
    ))
}
pub fn validate_release_version(raw: &str) -> Option<String> {
    let Some((core, pre)) = version(raw) else {
        return Some("manifest.version 必须是 SemVer（例如 1.2.3 或 2.0.0-beta.1）".into());
    };
    if core.iter().any(|n| *n > 2147483647) {
        return Some("manifest.version 的版本数字不能超过 2147483647".into());
    }
    if pre.iter().flatten().any(|s| {
        s.bytes().all(|c| c.is_ascii_digit())
            && (s.len() > 1 && s.starts_with('0')
                || s.parse::<u64>().unwrap_or(u64::MAX) > 2147483647)
    }) {
        return Some("manifest.version 的预发布数字必须无前导零且不超过 2147483647".into());
    }
    None
}
fn version_greater(left: &str, right: &str) -> bool {
    let (Some((a, ap)), Some((b, bp))) = (version(left), version(right)) else {
        return false;
    };
    if a != b {
        return a > b;
    }
    match (ap, bp) {
        (None, Some(_)) => true,
        (Some(_), None) | (None, None) => false,
        (Some(a), Some(b)) => {
            for (a, b) in a.iter().zip(&b) {
                if a == b {
                    continue;
                }
                let an = a.bytes().all(|c| c.is_ascii_digit());
                let bn = b.bytes().all(|c| c.is_ascii_digit());
                return match (an, bn) {
                    (true, true) => {
                        a.parse::<u64>().unwrap_or(u64::MAX) > b.parse::<u64>().unwrap_or(u64::MAX)
                    }
                    (true, false) => false,
                    (false, true) => true,
                    _ => a > b,
                };
            }
            a.len() > b.len()
        }
    }
}
fn release_manifest(manifest: &Value) -> Value {
    let mut out = json!({"format":3,"package_format":3,"story_schema":2,"content_schema":1});
    if let Some(fields) = manifest.as_object() {
        out.as_object_mut().unwrap().extend(fields.clone());
    }
    out
}
pub fn apply_release_profile(
    mut issues: Vec<PreflightIssue>,
    stories: &Stories,
    manifest: &Value,
    runtime_version: &str,
    bundled_assets: Option<&[String]>,
) -> Vec<PreflightIssue> {
    for i in &mut issues {
        if i.code == "placeholder_text" && i.severity == "warning" {
            i.severity = "error".into();
        }
    }
    let labels = [
        ("id", "Mod 标识"),
        ("name", "Mod 名称"),
        ("version", "版本号"),
        ("author", "作者"),
        ("description", "简介"),
        ("entry", "开始章节"),
    ];
    let missing: Vec<_> = labels
        .into_iter()
        .filter(|(field, _)| text(manifest, field).trim().is_empty())
        .map(|(_, label)| label)
        .collect();
    if !missing.is_empty() {
        issues.push(PreflightIssue::new(
            "error",
            "missing_release_metadata",
            "",
            "",
            format!(
                "发布信息还没有填写完整：{}。请在 Mod 信息中填写。",
                missing.join("、")
            ),
        ));
    } else if let Err(e) = crate::validate::validate_manifest(&release_manifest(manifest)) {
        issues.push(PreflightIssue::new(
            "error",
            "invalid_release_manifest",
            "",
            "",
            format!("{e:#}"),
        ));
    }
    if !text(manifest, "version").is_empty() {
        if let Some(error) = validate_release_version(text(manifest, "version")) {
            issues.push(PreflightIssue::new(
                "error",
                "invalid_release_version",
                "",
                "",
                error,
            ));
        }
    }
    if version_greater(text(manifest, "min_host_version"), runtime_version) {
        issues.push(PreflightIssue::new(
            "error",
            "incompatible_runtime_requirement",
            "",
            "",
            format!(
                "项目要求 MortalModHost {}，但当前随附 Runtime 是 {runtime_version}",
                text(manifest, "min_host_version")
            ),
        ));
    }
    for (sid, s) in stories {
        if let Some(config) = crate::localization::localization_config(s) {
            for locale in ["chs", "cht", "ja", "ko"] {
                if config["default_locale"] != locale
                    && config["translations"].get(locale).is_none()
                {
                    issues.push(PreflightIssue::new(
                        "warning",
                        "missing_locale",
                        sid,
                        "",
                        format!("发布本地化缺少 {locale}；运行时将使用 fallback/default 文本"),
                    ));
                }
            }
        }
    }
    if let Some(assets) = bundled_assets {
        for asset in unused_asset_paths(stories, assets) {
            if Path::new(&asset)
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| {
                    ["png", "jpg", "jpeg", "wav", "ogg", "mp3", "flac"]
                        .contains(&ext.to_ascii_lowercase().as_str())
                })
            {
                issues.push(PreflightIssue::new(
                    "warning",
                    "unused_critical_asset",
                    "",
                    "",
                    format!("发布包含未使用的图片/音频资产：{asset}"),
                ));
            }
        }
    }
    ordered(issues)
}
#[derive(Clone, Debug, Serialize)]
pub struct ReleaseBuildResult {
    pub package_path: PathBuf,
    pub checksum_path: PathBuf,
    pub package_sha256: String,
    pub package_size: u64,
    pub story_count: usize,
    pub node_count: usize,
    pub warnings: Vec<PreflightIssue>,
    pub compile_report: Vec<String>,
    pub readme_path: Option<PathBuf>,
}
#[derive(Debug)]
pub struct ReleaseBuildBlocked {
    pub issues: Vec<PreflightIssue>,
}
impl std::fmt::Display for ReleaseBuildBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "发布构建已停止：发现 {} 个错误",
            self.issues.iter().filter(|i| i.severity == "error").count()
        )
    }
}
impl std::error::Error for ReleaseBuildBlocked {}
pub fn build_release(
    project: &Project,
    output: &Path,
    runtime_version: &str,
) -> Result<ReleaseBuildResult> {
    let mut release = project.clone();
    release.manifest = release_manifest(&project.manifest);
    let issues = run_preflight(&release, Profile::Release, runtime_version);
    if issues.iter().any(|i| i.severity == "error") {
        return Err(ReleaseBuildBlocked { issues }.into());
    }
    let mut destination = output.to_path_buf();
    if destination
        .extension()
        .is_none_or(|s| !s.eq_ignore_ascii_case("lommod"))
    {
        destination.set_extension("lommod");
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = tempfile::tempdir_in(parent)?;
    let name = destination.file_name().context("发布路径缺少文件名")?;
    let staged = temp.path().join(name);
    release.export(&staged)?;
    let data = fs::read(&staged)?;
    let hash = format!("{:X}", Sha256::digest(&data));
    let checksum = destination.with_file_name(format!("{}.sha256", name.to_string_lossy()));
    let checksum_text = format!("{hash}  {}\n", name.to_string_lossy());
    // Both artifacts are fully prepared before the first destination is replaced.
    let staged_checksum = temp.path().join("checksum.tmp");
    fs::write(&staged_checksum, &checksum_text)?;
    fs::rename(&staged, &destination)?;
    crate::project::atomic_write(&checksum, checksum_text.as_bytes())?;
    Ok(ReleaseBuildResult {
        package_path: destination.canonicalize()?,
        checksum_path: checksum.canonicalize()?,
        package_sha256: hash,
        package_size: data.len() as u64,
        story_count: release.stories.len(),
        node_count: release.stories.values().map(|s| nodes(s).len()).sum(),
        warnings: issues
            .into_iter()
            .filter(|i| i.severity == "warning")
            .collect(),
        compile_report: release
            .stories
            .keys()
            .map(|s| format!("已编译 story/{s}.json"))
            .collect(),
        readme_path: None,
    })
}
pub fn build_release_directory(
    project: &Project,
    directory: &Path,
    runtime_version: &str,
) -> Result<ReleaseBuildResult> {
    crate::validate::validate_manifest(&release_manifest(&project.manifest))?;
    if let Some(error) = validate_release_version(text(&project.manifest, "version")) {
        anyhow::bail!(error);
    }
    let stem = format!(
        "{}-{}",
        text(&project.manifest, "id"),
        text(&project.manifest, "version")
    );
    let mut result = build_release(
        project,
        &directory.join(format!("{stem}.lommod")),
        runtime_version,
    )?;
    let readme = directory.join(format!("{stem}-README.txt"));
    let content=format!("{} {}\n作者：{}\n\n{}\n\n文件：{}\nSHA-256：{}\n章节：{}\n节点：{}\n最低 MortalModHost：{}\n\n将 .lommod 导入支持此格式的 MortalModHost。此目录是离线构建产物，未执行 Windows 或游戏实机测试。\n",text(&project.manifest,"name"),text(&project.manifest,"version"),text(&project.manifest,"author"),text(&project.manifest,"description"),result.package_path.file_name().unwrap().to_string_lossy(),result.package_sha256,result.story_count,result.node_count,text(&project.manifest,"min_host_version"));
    crate::project::atomic_write(&readme, content.as_bytes())?;
    result.readme_path = Some(readme.canonicalize()?);
    Ok(result)
}
