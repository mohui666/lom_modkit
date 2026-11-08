use lom_core::project::Project;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Story(String, Option<String>),
    Manifest,
    Asset(String),
    Assets,
    Events,
    Game,
    PreviewLibrary,
    Settings,
    Open,
    Current,
}

pub fn issue_destination(
    issue: &lom_core::release::PreflightIssue,
    project: &Project,
) -> Destination {
    if project.stories.contains_key(&issue.story_id) {
        return Destination::Story(
            issue.story_id.clone(),
            (!issue.node_id.is_empty()).then(|| issue.node_id.clone()),
        );
    }
    resolve(&format!("{}: {}", issue.code, issue.message), project)
}

pub fn issue_label(issue: &lom_core::release::PreflightIssue, project: &Project) -> String {
    let Some(story) = project.stories.get(&issue.story_id) else {
        return issue_destination(issue, project).label();
    };
    let chapter = story["title"]
        .as_str()
        .filter(|title| !title.is_empty())
        .unwrap_or(&issue.story_id);
    if let Some((index, node)) = story["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .find(|(_, node)| node["id"] == issue.node_id)
    {
        let note = node["_editor"]["note"]
            .as_str()
            .filter(|note| !note.is_empty())
            .unwrap_or(&issue.node_id);
        crate::i18n::message(
            "editor.error.issue_location",
            &[
                ("chapter", chapter.into()),
                ("step", (index + 1).to_string()),
                ("note", note.into()),
            ],
        )
    } else {
        chapter.to_owned()
    }
}

pub fn issue_report(issues: &[lom_core::release::PreflightIssue], project: &Project) -> String {
    issues
        .iter()
        .map(|issue| {
            format!(
                "{}\n{}",
                issue_label(issue, project),
                explanation(&issue.message)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

impl Destination {
    pub fn label(&self) -> String {
        match self {
            Self::Story(_, Some(_)) => crate::i18n::key("common.error.fix_step"),
            Self::Story(_, None) => crate::i18n::key("common.error.fix_story"),
            Self::Manifest => crate::i18n::key("common.error.fix_manifest"),
            Self::Asset(_) => crate::i18n::key("common.error.fix_asset"),
            Self::Assets => crate::i18n::key("common.error.open_assets"),
            Self::Events => crate::i18n::key("common.error.fix_events"),
            Self::Game => crate::i18n::key("common.error.open_game"),
            Self::PreviewLibrary => crate::i18n::key("common.error.open_preview"),
            Self::Settings => crate::i18n::key("common.error.open_settings"),
            Self::Open => crate::i18n::key("common.error.reopen"),
            Self::Current => crate::i18n::key("common.error.current"),
        }
    }
}

fn authoring_schema() -> &'static Value {
    static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!("../data/authoring.json")).expect("authoring schema")
    })
}

fn field_label(message: &str, field: &str) -> String {
    let kind = message
        .split_once("\"(")
        .and_then(|(_, tail)| tail.split_once("):"))
        .map(|(kind, _)| kind);
    let label = kind
        .and_then(|kind| authoring_schema()["NODE_SCHEMAS"][kind]["fields"].as_array())
        .and_then(|fields| fields.iter().find(|entry| entry[0] == field))
        .and_then(|entry| entry[1].as_str());
    let label = label
        .unwrap_or(match field {
            "id" => "作品编号",
            "campaign_id" => "存档身份",
            "name" => "名称",
            "version" => "版本",
            "author" => "作者",
            "description" => "简介",
            "entry" => "入口章节",
            "min_host_version" => "最低游戏接入组件版本",
            "tested_host_version" => "已测试的游戏接入组件版本",
            "game_version" => "游戏版本",
            "tested_game_version" => "已测试的游戏版本",
            "character" => "人物",
            "image" => "图片",
            "portrait" => "表情",
            "text" => "文字",
            "goto" => "后续步骤",
            "duration" => "持续时间",
            "position" => "站位",
            "key" => "所选内容",
            _ => "此项设置",
        })
        .replace(" ID", "编号")
        .replace("ID", "编号")
        .replace("Host", "游戏接入组件")
        .replace("CG", "插图");
    crate::i18n::tr(&label)
}

pub fn explanation(message: &str) -> String {
    if (message.starts_with("无法打开") || message.starts_with("打开失败"))
        && message.contains("scenario.json")
    {
        return crate::i18n::key("common.error.no_scenario");
    }
    let lower = message.to_lowercase();
    for (patterns, text) in [
        (
            &["os error 5", "permission denied", "拒绝访问"][..],
            "无法访问文件。请选择有读写权限的位置后重试。",
        ),
        (
            &[
                "os error 32",
                "另一个程序正在使用",
                "being used by another process",
            ][..],
            "文件正在被其他程序使用。关闭占用它的程序后重试。",
        ),
        (
            &["os error 112", "no space left", "磁盘空间不足"][..],
            "磁盘空间不足。请更换保存位置或清理空间后重试。",
        ),
        (
            &["timed out", "timeout", "超时"][..],
            "连接超时。请检查网络连接后重试。",
        ),
        (
            &["error sending request", "connection refused", "dns error"][..],
            "无法连接下载服务。请检查网络连接后重试。",
        ),
    ] {
        if patterns.iter().any(|pattern| lower.contains(pattern)) {
            return crate::i18n::tr(text);
        }
    }
    if (message.contains("asset-extractor") || message.contains("素材提取组件"))
        && ["缺少", "缺失", "不存在", "找不到"]
            .iter()
            .any(|word| message.contains(word))
    {
        return crate::i18n::key("common.error.extractor_missing");
    }
    if message.contains("campaign_id") || message.contains("存档身份") {
        if message.contains("缺少必填") || message.contains("未填写") {
            return crate::i18n::key("common.error.identity_missing");
        }
        if message.contains("格式不合法") || message.contains("只能使用") {
            return crate::i18n::key("common.error.identity_format");
        }
    }
    let field = field(message);
    let label = field.as_deref().map(|field| field_label(message, field));
    if message.contains("缺少必填字段") {
        return crate::i18n::message(
            "common.error.fill_field",
            &[(
                "field",
                label
                    .clone()
                    .unwrap_or_else(|| crate::i18n::tr("当前必填项")),
            )],
        );
    }
    if message.contains("SemVer") {
        return crate::i18n::message(
            "common.error.version_format",
            &[(
                "field",
                label.clone().unwrap_or_else(|| crate::i18n::tr("版本号")),
            )],
        );
    }
    if message.contains("package_format")
        || message.contains("content_schema")
        || message.contains("story_schema")
    {
        return crate::i18n::key("common.error.package_format");
    }
    if message.contains("JSON")
        || lower.contains("expected value")
        || lower.contains("expected ident")
        || lower.contains("invalid type:")
    {
        return crate::i18n::key("common.error.unreadable");
    }
    let mut text = message.trim_start_matches("导出失败\n").to_owned();
    if let Some((_, rest)) = text.split_once("节点 \"") {
        if let Some((_, detail)) = rest.split_once("): ") {
            text = detail.to_owned();
        }
    }
    for prefix in [
        "manifest.json: ",
        "story.json: ",
        "content.json：",
        "content.json: ",
    ] {
        text = text.trim_start_matches(prefix).to_owned();
    }
    if let (Some(field), Some(label)) = (&field, &label) {
        if text.starts_with(field) {
            text = text.replacen(field, label, 1);
        }
        text = text
            .replace(&format!("字段 \"{field}\""), label)
            .replace("（非空字符串）", "");
    }
    // Replace only diagnostic vocabulary. User-authored file names and identifiers
    // remain available in copied diagnostics, rather than leaking into prose.
    for (raw, readable) in [
        ("非空字符串", "已填写的文字"),
        ("字符串", "文字"),
        ("布尔值", "开关值"),
        ("JSON 对象", "设置内容"),
        ("数组", "列表"),
        ("null", "空值"),
        ("Host", "游戏接入组件"),
        ("MOD", "作品包"),
        ("Mod", "作品包"),
        ("Lua", "脚本"),
        ("PNG/JPEG", "图片"),
        ("PNG", "图片"),
        ("JPEG", "图片"),
        ("ZIP", "压缩包"),
        ("true", "开启"),
        ("false", "关闭"),
        ("NaN", "无效数值"),
        ("Infinity", "无限大"),
    ] {
        text = text.replace(raw, readable);
    }
    if !text.chars().any(|c| c.is_ascii_alphabetic()) && !text.contains('{') && !text.contains('}')
    {
        return crate::i18n::tr(&text);
    }
    if (message.contains("goto") || message.contains("跳转")) && message.contains("不存在") {
        return crate::i18n::key("common.error.missing_next");
    }
    if message.contains("start") && message.contains("不存在") {
        return crate::i18n::key("common.error.missing_start");
    }
    if message.contains("没有表情") {
        return crate::i18n::key("common.error.unregistered_portrait");
    }
    if message.contains("portrait") && (message.contains("不存在") || message.contains("不在"))
    {
        return crate::i18n::key("common.error.invalid_portrait");
    }
    if message.contains("user:") && message.contains("引用") {
        return crate::i18n::key("common.error.invalid_reference");
    }
    if let Some(label) = label {
        if message.contains("整数") {
            return crate::i18n::message("common.error.integer_field", &[("field", label)]);
        }
        if message.contains("数字") || message.contains("数值") {
            return crate::i18n::message("common.error.number_field", &[("field", label)]);
        }
        return crate::i18n::message("common.error.invalid_field", &[("field", label)]);
    }
    if message.contains("不存在") || message.contains("找不到") || lower.contains("not found")
    {
        return crate::i18n::key("common.error.not_found");
    }
    if message.contains("图片") || message.contains("素材") || message.contains("content") {
        return crate::i18n::key("common.error.invalid_asset");
    }
    crate::i18n::key("common.error.generic")
}

pub fn show(ui: &mut eframe::egui::Ui, message: &str) {
    ui.colored_label(
        eframe::egui::Color32::from_rgb(179, 55, 49),
        explanation(message),
    );
    ui.horizontal(|ui| {
        if ui
            .small_button(crate::i18n::key("common.error.locate"))
            .clicked()
        {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    eframe::egui::Id::new("locate-error-message"),
                    message.to_owned(),
                )
            });
        }
        if ui.small_button(crate::i18n::tr("复制诊断信息")).clicked() {
            ui.ctx().copy_text(message.to_owned());
        }
    });
}

pub fn field(message: &str) -> Option<String> {
    if message.contains("存档身份") {
        return Some("campaign_id".into());
    }
    let quoted = message
        .split_once("字段 \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(field, _)| field.to_owned());
    if quoted.is_some() {
        return quoted;
    }
    let detail = message
        .rsplit_once(": ")
        .map(|(_, detail)| detail)
        .unwrap_or(message);
    let prefix: String = detail
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '_')
        .collect();
    let known = authoring_schema()["NODE_SCHEMAS"]
        .as_object()
        .unwrap()
        .values()
        .any(|node| {
            node["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|field| field[0] == prefix)
        });
    (!prefix.is_empty() && known).then_some(prefix)
}

pub fn focus_field(ctx: &eframe::egui::Context, message: &str) {
    ctx.data_mut(|data| {
        let id = eframe::egui::Id::new("problem-field");
        data.remove::<String>(id);
        if let Some(field) = field(message) {
            data.insert_temp(id, field);
        }
    });
}

pub fn action_label(message: &str, destination: &Destination) -> String {
    if *destination == Destination::Manifest
        && (message.contains("campaign_id") || message.contains("存档身份"))
    {
        crate::i18n::key("common.error.fill_identity")
    } else {
        destination.label()
    }
}

pub fn resolve(message: &str, project: &Project) -> Destination {
    if message.starts_with("无法打开") || message.starts_with("打开失败") {
        return Destination::Open;
    }
    // Only recognized validator prefixes identify nodes. A destination mentioned
    // inside an error must never be mistaken for the node that needs fixing.
    let source = project.stories.keys().find(|id| {
        message.contains(&format!("story/{id}.json"))
            || message.contains(&format!("story\\{id}.json"))
    });
    let node_id = message
        .split_once("节点 \"")
        .and_then(|(_, rest)| rest.split_once("\"("))
        .map(|(id, _)| id);
    if let Some(node_id) = node_id {
        let matches: Vec<_> = project
            .stories
            .iter()
            .filter(|(id, _)| source.is_none_or(|source| source == *id))
            .filter(|(_, story)| {
                story["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|node| node["id"] == node_id)
            })
            .collect();
        if let [(story, _)] = matches.as_slice() {
            return Destination::Story((*story).clone(), Some(node_id.into()));
        }
    }
    if message.contains("manifest.json")
        || message.contains("manifest.")
        || message.contains("campaign_id")
        || message.contains("invalid_release_manifest")
        || message.contains("invalid_release_version")
        || message.contains("missing_release_metadata")
        || message.contains("incompatible_runtime_requirement")
        || message.contains("invalid_entry")
        || message.contains("min_host_version")
        || message.contains("tested_host_version")
    {
        return Destination::Manifest;
    }
    for path in project
        .assets
        .keys()
        .filter(|path| path.ends_with("/content.json"))
    {
        let parent = path.trim_end_matches("/content.json");
        let id = parent.rsplit('/').next().unwrap_or("");
        if message.contains(parent)
            || message.contains(&format!("user:{id}"))
            || message.contains(&format!("用户内容 {id}"))
        {
            return Destination::Asset(path.clone());
        }
    }
    if message.contains("scenario.json") || message.contains("invalid_scenario") {
        return Destination::Events;
    }
    if let Some(story) = source {
        return Destination::Story(story.clone(), None);
    }
    if ["asset-extractor", "预览素材", "提取", "preview_map"]
        .iter()
        .any(|word| message.contains(word))
    {
        return Destination::PreviewLibrary;
    }
    if [
        "游戏目录",
        "游戏安装",
        "Host",
        "BepInEx",
        "runtime_",
        "dependency_",
        "mods_directory",
    ]
    .iter()
    .any(|word| message.contains(word))
    {
        return Destination::Game;
    }
    if ["preferences.json", "偏好设置", "设置文件"]
        .iter()
        .any(|word| message.contains(word))
    {
        return Destination::Settings;
    }
    if ["assets/", "素材", "用户内容"]
        .iter()
        .any(|word| message.contains(word))
    {
        return Destination::Assets;
    }
    Destination::Current
}

pub fn collect(value: &Value, project: &Project, out: &mut Vec<(Destination, String)>) {
    if let Some(object) = value.as_object() {
        let message = object
            .get("message")
            .or_else(|| object.get("text"))
            .or_else(|| object.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let source = object
            .get("story_id")
            .or_else(|| object.get("story"))
            .and_then(Value::as_str);
        let node = object
            .get("node_id")
            .or_else(|| object.get("node"))
            .and_then(Value::as_str)
            .filter(|node| !node.is_empty());
        let target = if let Some(story_id) = source.filter(|s| !s.is_empty()) {
            project.stories.get(story_id).and_then(|story| {
                node.is_none_or(|id| {
                    story["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|n| n["id"] == id)
                })
                .then(|| Destination::Story(story_id.into(), node.map(str::to_owned)))
            })
        } else if source.is_some()
            || object.contains_key("error")
            || object
                .get("severity")
                .is_some_and(|v| v == "error" || v == "warning")
            || object.get("status").is_some_and(|v| v == "failed")
        {
            Some(resolve(
                &format!(
                    "{} {message}",
                    object.get("code").and_then(Value::as_str).unwrap_or("")
                ),
                project,
            ))
        } else {
            None
        };
        let target = target.or_else(|| {
            (object.contains_key("error") || object.get("severity").is_some_and(|v| v == "error"))
                .then(|| {
                    source
                        .filter(|id| project.stories.contains_key(*id))
                        .map(|id| Destination::Story(id.into(), None))
                        .unwrap_or_else(|| resolve(message, project))
                })
        });
        if let Some(target) = target {
            let entry = (target, message.into());
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
        for child in object
            .values()
            .filter(|child| child.is_object() || child.is_array())
        {
            collect(child, project, out);
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            collect(child, project, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validation_errors_use_chinese_controls_instead_of_serialized_fields() {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../lom-core/tests/test_validate_fixtures.json"
        ))
        .unwrap();
        let mut checked = 0;
        for case in fixtures["cases"].as_array().unwrap() {
            let input: Value = serde_json::from_str(case["input_json"].as_str().unwrap()).unwrap();
            let result = if case["kind"] == "manifest" {
                lom_core::validate::validate_manifest(&input).map(|_| vec![])
            } else {
                lom_core::validate::validate_story(&input)
            };
            if let Err(error) = result {
                let raw = format!("{error:#}");
                let text = explanation(&raw);
                assert!(
                    !text.chars().any(|c| c.is_ascii_alphabetic()),
                    "{raw}\n{text}"
                );
                assert!(!text.contains('{') && !text.contains('}'), "{text}");
                checked += 1;
            }
        }
        assert!(checked > 100);
        assert_eq!(
            explanation("story.json: 节点 \"s\"(say): 缺少必填字段 \"text\""),
            "请填写文本。"
        );
    }

    #[test]
    fn errors_resolve_to_owned_fields_and_never_to_a_missing_goto() {
        let mut project = Project::new();
        project.stories.insert(
            "other".into(),
            json!({"nodes":[{"id":"broken","type":"wait"}]}),
        );
        assert_eq!(
            resolve(
                "story/other.json: 节点 \"broken\"(wait): goto 指向不存在的 destination",
                &project
            ),
            Destination::Story("other".into(), Some("broken".into()))
        );
        assert_eq!(
            resolve("manifest.json: 缺少必填字段 \"campaign_id\"", &project),
            Destination::Manifest
        );
        assert_eq!(
            resolve("安装包缺少 asset-extractor", &project),
            Destination::PreviewLibrary
        );
        project
            .assets
            .insert("assets/user/image/demo.pic/content.json".into(), vec![]);
        assert_eq!(
            resolve("用户内容 demo.pic 缺少图片", &project),
            Destination::Asset("assets/user/image/demo.pic/content.json".into())
        );
        assert_eq!(
            resolve("scenario.json: 事件不存在", &project),
            Destination::Events
        );
        assert_eq!(
            resolve("无法写入文件 (os error 5)", &project),
            Destination::Current
        );
        assert_eq!(
            resolve("无法打开作品：缺少 scenario.json", &project),
            Destination::Open
        );
    }
}
