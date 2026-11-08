//! Shared error capture and presentation. Callers keep their operation state and locations.
use eframe::egui;
#[cfg(test)]
use serde_json::Value;
use std::fmt::Display;
#[path = "error_operations.rs"]
mod operations;
pub(crate) use operations::key as operation_key;
#[cfg(test)]
#[path = "error_operations_tests.rs"]
mod operation_tests;

/// Preserve the complete cause chain before storing an error as UI text.
pub fn capture(error: impl Display) -> String {
    format!("{error:#}")
}

/// Show the next action in user language. The original error remains in diagnostics.
pub fn show(ui: &mut egui::Ui, message: &str) {
    ui.add(egui::Label::new(text(message)).wrap().selectable(true));
}

pub fn title(message: &str) -> String {
    let key = if message.starts_with("无法打开") || message.starts_with("打开失败") {
        "editor.error.title.open"
    } else if message.contains("试听") || message.contains("原版音频") {
        "editor.error.title.audio"
    } else if message.contains("保存") {
        "editor.error.title.save"
    } else if message.contains("检查错误") || message.contains("节点 \"") {
        "editor.error.title.validation"
    } else {
        return crate::i18n::tr("操作未完成");
    };
    crate::i18n::key(key)
}

/// Keep complete technical diagnostics available for forwarding.
pub fn copy_button(ui: &mut egui::Ui, diagnostics: impl FnOnce() -> String) {
    if ui
        .small_button(crate::i18n::key("editor.workspace.copy_diagnostics"))
        .clicked()
    {
        ui.ctx().copy_text(diagnostics());
    }
}

fn field_label(message: &str, field: &str) -> String {
    let kind = message
        .split_once("\"(")
        .and_then(|(_, tail)| tail.split_once("):"))
        .map(|(kind, _)| kind);
    let label = kind
        .and_then(|kind| {
            crate::error_location::authoring_schema()["NODE_SCHEMAS"][kind]["fields"].as_array()
        })
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

pub fn text(message: &str) -> String {
    readable_summary(message)
}

fn readable_summary(message: &str) -> String {
    if let Some((_, detail)) = message.split_once("包内 ") {
        if let Some((lua, detail)) = detail.split_once(" 不是由对应 ") {
            if let Some((story, _)) = detail.split_once(" 编译生成") {
                if lua.starts_with("lua/")
                    && lua.ends_with(".lua")
                    && story.starts_with("story/")
                    && story.ends_with(".json")
                {
                    return crate::i18n::key("editor.error.story_lua_mismatch");
                }
            }
        }
    }
    if (message.starts_with("无法打开") || message.starts_with("打开失败"))
        && message.contains("scenario.json")
    {
        return crate::i18n::key("common.error.no_scenario");
    }
    let lower = message.to_lowercase();
    if message.contains("原版音频试听组件缺失") {
        return crate::i18n::key("editor.error.audio_components_missing");
    }
    if message.contains("集合中没有资源") {
        return crate::i18n::key("editor.error.audio_missing");
    }
    if message.contains("动态混音")
        || message.contains("动态播放规则")
        || message.contains("多段") && message.contains("试听")
    {
        return crate::i18n::key("editor.error.audio_dynamic");
    }
    if message.contains("试听只支持") {
        return crate::i18n::key("editor.error.audio_format");
    }
    if message.contains("保存")
        && (operations::os_error(&lower, 5)
            || ["permission denied", "拒绝访问"]
                .iter()
                .any(|pattern| lower.contains(pattern)))
    {
        return crate::i18n::key("editor.error.save_permission");
    }
    for (patterns, text) in [
        (
            &["permission denied", "拒绝访问"][..],
            "无法访问文件。请选择有读写权限的位置后重试。",
        ),
        (
            &["另一个程序正在使用", "being used by another process"][..],
            "文件正在被其他程序使用。关闭占用它的程序后重试。",
        ),
        (
            &["no space left", "磁盘空间不足"][..],
            "磁盘空间不足。请更换保存位置或清理空间后重试。",
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
    for (code, key) in [
        (5, "editor.error.ops_permission"),
        (32, "editor.error.ops_locked"),
        (112, "editor.error.ops_disk_full"),
    ] {
        if operations::os_error(&lower, code) {
            return crate::i18n::key(key);
        }
    }
    if ["timed out", "timeout", "超时"]
        .iter()
        .any(|word| lower.contains(word))
    {
        let network = ["连接", "下载", "http", "request", "connect", "network"]
            .iter()
            .any(|word| lower.contains(word));
        return if network {
            crate::i18n::tr("连接超时。请检查网络连接后重试。")
        } else {
            crate::i18n::key("editor.error.operation_timeout")
        };
    }
    if message.contains("原版特效预览缓存无效") || message.contains("特效预览缓存损坏")
    {
        return crate::i18n::key("editor.error.effect_cache");
    }
    if let Some(key) = operation_key(message) {
        return crate::i18n::key(key);
    }
    if message.contains("音频试听失败") || message.contains("无法试听") {
        return crate::i18n::key("editor.error.audio_playback");
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
    // Known validator outcomes need instructions for the actual controls, rather
    // than losing their meaning when an identifier makes the text technical.
    let validation_key = if message.contains("同时动作都会控制")
        || message.contains("全场气泡开关和单个人物气泡")
    {
        Some("parallel_conflict")
    } else if message.contains("节点 id 必须") {
        Some("step_id")
    } else if message.contains("节点 id") && message.contains("重复") {
        Some("duplicate_step")
    } else if message.contains("是最后一个节点且存在未覆盖的返回值") {
        Some("branch_ending")
    } else if message.contains("是最后一个节点且没有显式 goto") {
        Some("missing_ending")
    } else if message.contains("该类型节点不允许显式") {
        Some("unexpected_next")
    } else if message.contains("未知节点类型") {
        Some("unsupported_step")
    } else if message.contains("campaign.new_game") {
        Some("campaign_mode")
    } else if message.contains("作品 ID格式不合法") {
        Some("work_id")
    } else if message.contains("min_host_version 不能高于 tested_host_version") {
        Some("host_versions")
    } else if message.contains("game_version 与 tested_game_version") {
        Some("game_versions")
    } else if message.contains("upper 必须在可投出的总点数") || message.contains("band_texts")
    {
        Some("dice_results")
    } else if message.contains("骰子检查点") && message.contains("缺少官方元数据") {
        Some("dice_checkpoint")
    } else if message.contains("when_affinity") && message.contains("必须是对象") {
        Some("trigger_condition")
    } else if message.contains("人物必须保存内部 ID") {
        Some("character_selection")
    } else if message.contains("用户内容引用不合法") {
        return crate::i18n::key("common.error.invalid_reference");
    } else if message.contains("friend 总人数必须至少 1") {
        Some("battle_friends")
    } else if message.contains("enemy_faction 已改") || message.contains("friend_people 已取消")
    {
        Some("battle_roster")
    } else if message.contains("display_name 已删除") {
        Some("combat_name")
    } else if message.contains("已删除") && message.contains("对手决斗技能") {
        Some("combat_skills")
    } else if message.contains("同一类别和 item id 不能重复") {
        Some("duplicate_goods")
    } else if message.contains("condition.source 必须是") {
        Some("shop_condition")
    } else if message.contains("不支持的 locale") || message.contains("不应重复放入 translations")
    {
        Some("language_settings")
    } else {
        None
    };
    if let Some(key) = validation_key {
        return crate::i18n::key(&format!("editor.error.{key}"));
    }
    let field = crate::error_location::field(message);
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
    if let Some(label) = &label {
        if message.contains("整数") {
            return crate::i18n::message("common.error.integer_field", &[("field", label.clone())]);
        }
        if message.contains("数字") || message.contains("数值") {
            return crate::i18n::message("common.error.number_field", &[("field", label.clone())]);
        }
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

pub fn show_with_actions(ui: &mut eframe::egui::Ui, message: &str) {
    show(ui, message);
    ui.horizontal(|ui| {
        let locatable = crate::error_location::operation_destination(message)
            != Some(crate::error_location::Destination::Current)
            && !message.contains("原版音频试听组件缺失")
            && text(message) != crate::i18n::key("common.error.generic");
        if locatable
            && ui
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
        copy_button(ui, || message.to_owned());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_operation_errors_keep_the_action_specific_and_unknown_errors_stay_honest() {
        for (raw, expected) in [
            ("音频试听失败：当前游戏的普通音效集合中没有资源“missing_001”", "重新选择"),
            ("原版特效预览缓存无效：C:/cache/fx.png: invalid signature", "提取当前特效预览"),
            ("保存失败：等待操作超时", "重新执行"),
            ("下载失败：request timed out", "网络连接"),
            ("打开失败：文件被占用 (os error 32)", "关闭占用"),
            ("导出失败：磁盘空间不足 (os error 112)", "更换保存位置"),
            ("无法保存：C:/work.json: Permission denied (os error 5)", "另存为"),
            ("story.json: 节点 \"g\"(parallel): 第 1 和第 2 个同时动作都会控制人物 player，请拆成先后步骤", "先后步骤"),
        ] {
            let shown = text(raw);
            assert!(shown.contains(expected), "{raw}\n{shown}");
            assert!(!shown.contains("C:/") && !shown.contains("os error") && !shown.contains("player"), "{shown}");
        }
        let unknown = "unexpected subsystem error: opaque-482";
        assert_eq!(text(unknown), crate::i18n::key("common.error.generic"));
        assert_eq!(capture(unknown), unknown);
    }
    #[test]
    fn component_shows_a_next_step_and_keeps_the_complete_chain_for_diagnostics() {
        let error = anyhow::anyhow!("Permission denied (os error 5)")
            .context("C:/作品/story/chapter1.json")
            .context("保存章节失败");
        let detail = capture(error);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(ui, &detail);
                copy_button(ui, || detail.clone());
            });
        });
        let update = output.platform_output.accesskit_update.unwrap();
        let visible = update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()))
            .collect::<Vec<_>>()
            .join("\n");
        for cause in [
            "保存章节失败",
            "C:/作品/story/chapter1.json",
            "Permission denied (os error 5)",
        ] {
            assert!(detail.contains(cause), "{detail}");
        }
        assert!(visible.contains("另存为"), "{visible}");
        assert!(!visible.contains("Permission denied"), "{visible}");
        assert!(output.platform_output.commands.is_empty());
    }
    #[test]
    fn audio_errors_tell_the_user_what_to_do_without_process_or_component_details() {
        for (message, action) in [
            ("无法试听音频“子彈咻咻_001”：原版音频试听组件缺失：C:/Modkit/audio-decoder/vgmstream-cli.exe", "重新打开"),
            ("音频试听失败：原版普通音效“子彈咻咻_001”：准备音频试听失败（exit code: 1）：此音频事件包含多段或动态混音，目前不能作为单段试听；请在游戏中确认", "试玩"),
            ("音频试听失败（exit code: 1）：调用 PlaySync 失败", "换一段音频"),
        ] {
            let visible = text(message);
            assert!(visible.contains(action), "{visible}");
            assert!(!visible.contains("exit code"));
            assert!(!visible.contains("vgmstream"));
            assert_ne!(visible, crate::i18n::key("common.error.generic"));
        }
    }
    #[test]
    fn package_script_mismatch_gives_actions_for_authors_and_players() {
        let message =
            "打开失败\n包内 lua/chapter1.lua 不是由对应 story/chapter1.json 编译生成，拒绝导入";
        let visible = text(message);
        assert!(visible.contains("原工程"));
        assert!(visible.contains("联系作者"));
        assert!(visible.contains("编辑器版本"));
        assert!(!visible.contains(".json") && !visible.contains(".lua"));
        assert_ne!(visible, crate::i18n::key("common.error.generic"));
    }

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
                let readable = readable_summary(&raw);
                assert_ne!(
                    readable,
                    crate::i18n::key("common.error.generic"),
                    "Missing actionable explanation: {raw}"
                );
                assert!(
                    !readable.chars().any(|c| c.is_ascii_alphabetic()),
                    "{raw}\n{readable}"
                );
                assert!(
                    !readable.contains('{') && !readable.contains('}'),
                    "{readable}"
                );
                assert_eq!(text(&raw), readable);
                checked += 1;
            }
        }
        assert!(checked >= 267);
        assert_eq!(
            readable_summary("story.json: 节点 \"s\"(say): 缺少必填字段 \"text\""),
            "请填写文本。"
        );
    }
}
