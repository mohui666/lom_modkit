//! Reader-facing tool results. The original data is retained for troubleshooting.
use eframe::egui;
use serde_json::Value;

fn known_label(key: &str) -> Option<String> {
    let text = match key {
        "healthy" => "安装状态",
        "ok" => "检查结果",
        "error" | "validation_error" => "错误原因",
        "severity" => "提示级别",
        "message" | "detail" => "说明",
        "fixable" => "可自动修复",
        "fixable_count" => "可自动修复的问题数",
        "changed" => "是否有更新",
        "rollback_available" => "可恢复上一版",
        "game_tested" => "已在游戏中验证",
        "steam_ready" => "Steam 启动配置",
        "findings" | "issues" => "检查项目",
        "files" | "entries" => "文件清单",
        "path" | "output" | "directory" | "output_dir" | "folder" => "保存位置",
        "paths" => "相关文件",
        "present" => "文件存在",
        "enabled" => "已启用",
        "manifest" => "作品信息",
        "id" | "content_id" | "mod_id" => "内容编号",
        "description" => "作品说明",
        "entry" => "起始章节",
        "campaign" => "战役设置",
        "campaign_id" => "战役存档编号",
        "new_game" => "允许新建战役",
        "triggers" => "剧情触发条件",
        "min_host_version" => "需要的游戏接入组件版本",
        "metadata" => "素材资料",
        "host_locations" => "游戏接入组件的位置",
        "code" => "问题编号",
        "story" | "story_id" => "章节",
        "node" | "node_id" => "剧情步骤",
        "story_title" => "章节名称",
        "character" | "character_id" => "人物",
        "character_label" => "人物名称",
        "name" | "label" => "名称",
        "text" => "内容",
        "version" => "版本",
        "author" => "作者",
        "license" => "使用许可",
        "changes" => "建议的修改",
        "steps" => "处理步骤",
        "backup" => "原文件备份",
        "stories" | "story_count" => "章节",
        "nodes" | "node_count" => "剧情步骤数",
        "story_ids" => "章节编号",
        "node_types" => "各类步骤数量",
        "parallel_actions" => "同时执行组内的动作数",
        "dialogue" | "dialogue_count" => "对白条数",
        "choice_nodes" => "选项步骤数",
        "choice_options" => "选项总数",
        "endings" => "结局数",
        "characters" => "人物数",
        "images" => "图片数",
        "audio" => "音频数",
        "voiced_dialogue" | "voiced" => "已有配音",
        "unvoiced_dialogue" | "unvoiced" => "尚无配音",
        "voice_coverage" | "percent" => "配音覆盖率（%）",
        "unvoiced_dialogues" => "尚无配音的对白",
        "unreachable_nodes" => "无法走到的步骤",
        "unused_assets" => "未使用的素材",
        "total" => "合计",
        "stats" => "统计",
        "conditions" => "条件分析",
        "symbols" => "变量与引用",
        "status" => "结果",
        "visited" => "经过的步骤",
        "variables" => "变量",
        "flags" => "剧情标记",
        "missing_dependencies" => "缺少的配套内容",
        "dependencies" => "配套内容",
        "content_type" | "type" | "kind" => "类型",
        "collision_type" => "同名内容冲突",
        "added" => "已添加",
        "size" => "大小（字节）",
        "expected" => "需要的值",
        "actual" => "当前值",
        "portraits" => "立绘和表情",
        "views" => "背景",
        "exported" => "成功导出的图片数",
        "failed" => "未能导出的图片数",
        "failures" => "未能导出的素材",
        "emotion" => "表情",
        "address" => "素材地址",
        "unavailable_characters" => "默认立绘缺失的人物",
        "resource_platform" => "游戏素材平台",
        "detected" => "检测到来源水印",
        "confidence" => "检测可信度",
        "frames_sampled" => "已检查的视频帧数",
        "sample_interval_seconds" => "检查间隔（秒）",
        "preflight" => "发布检查",
        "errors" => "需要修正的问题数",
        "warnings" => "提醒数",
        "contents" => "素材清单",
        "unused" => "未使用的内容",
        "source" => "来源",
        "field" => "填写项目",
        "subject" => "检查对象",
        "reason" => "原因",
        "reads" => "读取位置",
        "writes" => "设置位置",
        "uses" => "使用位置",
        "access" => "使用方式",
        "possibly_read_before_write" => "可能使用了尚未设置的值",
        "integrity_verified" => "文件完整性检查通过",
        "source_lua_match" => "剧情与编译结果一致",
        "default_namespace" => "默认内容前缀",
        "editor_version" => "编辑器版本",
        "runtime_version" => "游戏接入组件版本",
        "detected_game_version" => "检测到的游戏版本",
        "created_utc" => "生成时间（世界标准时间）",
        "validation_summary" => "检查摘要",
        "project_metadata" => "作品概况",
        "preview_truncated" | "truncated" => "已省略的内容",
        "locales" => "已提供的语言",
        "default_locale" => "默认语言",
        "fallback_locale" => "备用语言",
        "privacy" => "隐私保护",
        "project_story_content_included" => "包含剧情正文",
        "user_content_included" => "包含用户素材",
        "game_files_included" => "包含游戏文件",
        "absolute_paths_redacted" => "已隐去本机完整路径",
        "raw_node_count" => "自行编写代码的步骤数",
        "user_content_reference_count" => "引用用户素材数",
        "campaign_enabled" => "已启用战役",
        "scope" => "统计范围",
        "key" => "标识",
        _ => {
            let node_key = format!("node.{key}");
            let translated = crate::i18n::key(&node_key);
            if translated != node_key {
                return Some(translated);
            }
            return None;
        }
    };
    Some(crate::i18n::tr(text))
}

pub fn label(key: &str) -> String {
    known_label(key).unwrap_or_else(|| crate::i18n::tr("补充信息"))
}

fn scalar(key: &str, value: &Value) -> String {
    match value {
        Value::Null => crate::i18n::tr("无"),
        Value::Bool(v) => crate::i18n::tr(match (key, v) {
            ("healthy", true) => "状态正常",
            ("healthy", false) => "需要处理",
            ("ok", true) => "检查通过",
            ("ok", false) => "检查未通过",
            (_, true) => "是",
            (_, false) => "否",
        }),
        Value::String(s) if matches!(key, "type" | "kind" | "content_type" | "collision_type") => {
            match s.as_str() {
                "audio" => crate::i18n::tr("音频"),
                "character" => crate::i18n::tr("人物"),
                "image" => crate::i18n::tr("图片"),
                "story" => crate::i18n::tr("章节"),
                "node" => crate::i18n::tr("剧情步骤"),
                _ => {
                    let translated = crate::i18n::key(&format!("node.{s}"));
                    if translated == format!("node.{s}") {
                        s.clone()
                    } else {
                        translated
                    }
                }
            }
        }
        Value::String(s) if key == "access" => crate::i18n::tr(match s.as_str() {
            "read" => "读取",
            "write" => "设置",
            "read_write" => "读取与设置",
            _ => s,
        }),
        Value::String(s) => crate::i18n::tr(match s.as_str() {
            "ok" | "passed" | "pass" => "通过",
            "warning" => "提醒",
            "error" | "failed" => "未通过",
            "unsupported" => "需要在游戏中确认",
            "skipped" => "未执行",
            "__narrator__" => "旁白",
            "__unspecified__" => "未指定人物",
            _ => s,
        }),
        _ => value.to_string(),
    }
}

/// Counts real errors/warnings, without treating an unsupported offline step as a pass.
pub fn summary(value: &Value) -> String {
    if let Some(error) = value["error"].as_str() {
        return format!("{}\n{error}", crate::i18n::tr("操作未完成"));
    }
    if value.is_null() {
        return crate::i18n::tr("操作完成");
    }
    if let Some(exported) = value["exported"].as_u64() {
        return format!(
            "{} {exported} · {} {}",
            crate::i18n::tr("成功导出的图片数"),
            crate::i18n::tr("未能导出的图片数"),
            value["failed"].as_u64().unwrap_or(0)
        );
    }
    let mut errors = 0;
    let mut warnings = 0;
    let mut unsupported = 0;
    fn count(v: &Value, e: &mut usize, w: &mut usize, u: &mut usize) {
        match v {
            Value::Object(o) => {
                if o.get("severity").is_some_and(|s| s == "error")
                    || o.get("status").is_some_and(|s| s == "failed")
                    || o.contains_key("error")
                {
                    *e += 1;
                }
                if o.get("severity").is_some_and(|s| s == "warning") {
                    *w += 1;
                }
                if o.get("status").is_some_and(|s| s == "unsupported") {
                    *u += 1;
                }
                for child in o.values() {
                    count(child, e, w, u);
                }
            }
            Value::Array(a) => {
                for child in a {
                    count(child, e, w, u);
                }
            }
            _ => {}
        }
    }
    count(value, &mut errors, &mut warnings, &mut unsupported);
    if errors + warnings + unsupported > 0 {
        return format!(
            "{} {errors} · {} {warnings} · {} {unsupported}",
            crate::i18n::tr("需修正"),
            crate::i18n::tr("提醒"),
            crate::i18n::tr("待游戏确认")
        );
    }
    if value["healthy"] == false || value["ok"] == false {
        return crate::i18n::tr("检查未通过，请查看下面的说明");
    }
    if value["healthy"] == true {
        return crate::i18n::tr("安装状态正常，可以继续下一步");
    }
    if value["changed"] == false {
        return crate::i18n::tr("内容已一致，无需更新，未修改文件");
    }
    if value.as_array().is_some_and(Vec::is_empty) {
        return crate::i18n::tr("检查完成，没有发现问题或待处理内容");
    }
    crate::i18n::tr("操作完成，结果如下")
}

fn finding(code: &str) -> &str {
    match code {
        "architecture_ok" => "游戏位数已识别",
        "architecture_unknown" => "无法识别游戏位数，请重新选择游戏目录",
        "loader_architecture_ok" => "游戏加载组件的位数匹配",
        "loader_architecture_mismatch" => "游戏加载组件位数不匹配，请安装对应版本",
        "loader_architecture_unknown" => "游戏加载组件无法读取，请重新安装",
        "loader_missing" => "缺少游戏加载组件，请先安装 BepInEx",
        "steam_launch_ready" => "Steam 启动配置就绪",
        "steam_launch_required" => "需要配置 Steam 启动，可使用一键配置游戏",
        "bepinex_ok" => "游戏加载组件完整",
        "bepinex_incomplete" => "游戏加载组件不完整，请安装 BepInEx",
        "runtime_current" => "剧情接入组件已是当前版本",
        "runtime_dependency_current" => "配套组件已是当前版本",
        "runtime_missing" | "runtime_obsolete" => {
            "剧情接入组件缺失或需要更新，可点击“修复宿主安装”"
        }
        "runtime_dependency_missing" | "runtime_dependency_obsolete" => {
            "配套组件缺失或需要更新，可点击“修复宿主安装”"
        }
        "bundled_runtime_missing" | "bundled_dependency_missing" => {
            "工具自带的接入组件缺失，请重新选择宿主目录"
        }
        "mods_directory_ok" => "MOD 文件夹已就绪",
        "mods_directory_missing" => "MOD 文件夹尚未建立，可点击“修复宿主安装”",
        "no_duplicate_runtime_dll" => "未发现重复的接入组件",
        "duplicate_runtime_dll" => "发现重复的接入组件，请核对下面的位置",
        _ => code,
    }
}

pub fn show(ui: &mut egui::Ui, value: &Value, depth: usize) {
    if show_fields(ui, value, depth, false) {
        ui.label(crate::i18n::tr("其余资料请展开“技术详情”查看"));
    }
}

fn show_fields(ui: &mut egui::Ui, value: &Value, depth: usize, named_entries: bool) -> bool {
    if depth > 8 {
        ui.label("…");
        return true;
    }
    let mut technical_details = false;
    match value {
        Value::Object(o) => {
            if let Some(code) = o.get("code").and_then(Value::as_str) {
                let text = finding(code);
                if text != code {
                    ui.strong(crate::i18n::tr(text));
                }
            }
            for (key, v) in o {
                // Keep implementation identifiers in the raw report only.
                if matches!(
                    key.as_str(),
                    "code"
                        | "event"
                        | "scope"
                        | "host_locations"
                        | "package_sha256"
                        | "sha256"
                        | "protocol_version"
                        | "algorithm_version"
                        | "mod_hash"
                        | "checksum_status"
                        | "ecc_status"
                        | "ecc_corrections"
                        | "scale_factor"
                        | "sync_score"
                        | "method"
                ) {
                    continue;
                }
                if v.is_null() {
                    continue;
                }
                if !named_entries && known_label(key).is_none() {
                    technical_details = true;
                    if v.is_object() || v.is_array() {
                        show_fields(ui, v, depth + 1, false);
                    }
                    continue;
                }
                // Variable/flag names and chapter IDs are user data, not
                // implementation labels; preserve them even when in English.
                let title = if named_entries {
                    key.clone()
                } else {
                    label(key)
                };
                if v.is_object() || v.is_array() {
                    if v.as_array().is_some_and(Vec::is_empty) {
                        continue;
                    }
                    egui::CollapsingHeader::new(title)
                        .id_salt((depth, key))
                        .default_open(depth < 1)
                        .show(ui, |ui| {
                            technical_details |= show_fields(
                                ui,
                                v,
                                depth + 1,
                                !named_entries
                                    && matches!(key.as_str(), "variables" | "flags" | "stories"),
                            );
                        });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(title);
                        ui.label(scalar(key, v));
                    });
                }
            }
        }
        Value::Array(a) => {
            for (i, item) in a.iter().take(1000).enumerate() {
                ui.push_id(i, |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        technical_details |= show_fields(ui, item, depth + 1, false);
                    });
                });
            }
            if a.len() > 1000 {
                ui.label(crate::i18n::tr("更多内容请查看技术详情"));
            }
        }
        _ => {
            ui.label(scalar("", value));
        }
    }
    technical_details
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn readable_report_keeps_user_names_and_errors_but_hides_unmapped_internal_labels() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let report = json!({"healthy":false,"content_type":"audio",
            "variables":{"myCounter":17},
            "future_internal_field":{"error":"素材无法读取"},
            "package_format":3});
        let original = report.clone();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.strong(summary(&report));
                    show(ui, &report, 0);
                });
            },
        );
        let text = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("素材无法读取"), "{text}");
        assert!(text.contains("myCounter"), "{text}");
        assert!(text.contains("音频"), "{text}");
        assert!(!text.contains("future_internal_field"));
        assert!(!text.contains("package_format"));
        assert!(text.contains("技术详情"), "{text}");
        assert_eq!(report, original);
    }
    #[test]
    fn reports_keep_failures_and_unsupported_results_visible() {
        assert!(summary(&json!({"healthy":false})).contains("未通过"));
        assert!(summary(&json!({"healthy":true})).contains("正常"));
        assert!(summary(
            &json!([{"severity":"error"},{"status":"unsupported"},{"severity":"warning"}])
        )
        .contains("需修正 1 · 提醒 1 · 待游戏确认 1"));
        assert!(summary(&json!({"exported":30,"failed":2})).contains("2"));
        assert_eq!(scalar("healthy", &json!(true)), "状态正常");
        assert_eq!(scalar("game_tested", &json!(false)), "否");
        assert!(summary(&json!({"changed":false})).contains("无需更新"));
        let failed = summary(&json!({"changed":false,"error":"拒绝访问 (os error 5)"}));
        assert!(failed.contains("操作未完成"));
        assert!(failed.contains("拒绝访问"));
        assert!(!failed.contains("无需更新"));
    }
}
