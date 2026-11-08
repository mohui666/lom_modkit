use lom_core::project::Project;
use serde_json::Value;

pub(crate) fn authoring_schema() -> &'static Value {
    static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!("../data/authoring.json")).expect("authoring schema")
    })
}

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
        .map(|issue| format!("{}\n{}", issue_label(issue, project), issue.message))
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
    // Installing missing preview binaries cannot be fixed by editing a story
    // or choosing a preview library. Keep the install instruction, without a
    // misleading navigation button.
    if message.contains("原版音频试听组件缺失") {
        return Destination::Current;
    }
    if message.starts_with("无法打开") || message.starts_with("打开失败") {
        return Destination::Open;
    }
    if let Some(destination) = operation_destination(message) {
        return destination;
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

pub fn operation_destination(message: &str) -> Option<Destination> {
    crate::error_ui::operation_key(message).map(|key| match key {
        "editor.error.ops_game_directory"
        | "editor.error.ops_game_components"
        | "editor.error.ops_game_busy"
        | "editor.error.ops_game_platform"
        | "editor.error.ops_game_launch" => Destination::Game,
        _ => Destination::Current,
    })
}

pub fn collect(value: &Value, project: &Project, out: &mut Vec<(Destination, String)>) {
    if let Some(object) = value.as_object() {
        let message = ["message", "text", "error", "validation_error"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str))
            .unwrap_or("");
        let has_error = ["error", "validation_error"]
            .iter()
            .any(|key| object.get(*key).is_some_and(|value| !value.is_null()));
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
            || has_error
            || object
                .get("severity")
                .is_some_and(|v| v == "error" || v == "warning")
            || object
                .get("status")
                .is_some_and(|v| v == "fail" || v == "failed" || v == "error")
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
            (has_error || object.get("severity").is_some_and(|v| v == "error")).then(|| {
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
    fn operation_failures_do_not_jump_to_story_settings_and_nullable_messages_keep_the_error() {
        let project = Project::new();
        for raw in [
            "目标 manifest 不属于当前项目，拒绝覆盖",
            "更新文件校验失败：runtime/MortalModHost.dll",
            "共享库已有该 ID；请明确选择替换",
            "无法保存 C:/work/manifest.json (os error 5)",
        ] {
            assert_eq!(resolve(raw, &project), Destination::Current, "{raw}");
        }
        assert_eq!(
            resolve("BepInEx 不完整：core/loader.dll", &project),
            Destination::Game
        );
        let report = json!([
            {"status":"pass","error":null},
            {"story_id":"main","node_id":"say1","status":"fail","message":null,"error":"完整错误内容"}
        ]);
        let mut locations = Vec::new();
        collect(&report, &project, &mut locations);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].1, "完整错误内容");
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
            Destination::Current
        );
        assert_eq!(
            resolve(
                "原版音频试听组件缺失：asset-extractor/lom-asset-extractor.exe",
                &project
            ),
            Destination::Current
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
