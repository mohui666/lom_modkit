//! Embedded UI catalogs shared by native widgets.
//!
//! New UI uses stable `key` identifiers or `message` templates with named values.
//! Feature catalogs in `data/i18n` provide all four locales together. Author text,
//! resource IDs and Lua values are arguments, so translation never rewrites them.
//! `tr` retains the existing source-label catalog for older widgets. Tests check
//! catalog parity, placeholders, static references and legacy literal coverage.
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
};
static LOCALE: AtomicUsize = AtomicUsize::new(0);
pub const LOCALES: [&str; 4] = ["chs", "cht", "ja", "ko"];
pub const NAMES: [&str; 4] = ["简体中文", "繁體中文", "日本語", "한국어"];
pub fn locale() -> &'static str {
    LOCALES[LOCALE.load(Ordering::Relaxed)]
}
pub fn set_locale(name: &str) {
    if let Some(index) = LOCALES.iter().position(|s| *s == name) {
        LOCALE.store(index, Ordering::Relaxed);
    }
}
fn dictionaries() -> &'static [Value; 4] {
    static DICTS: OnceLock<[Value; 4]> = OnceLock::new();
    DICTS.get_or_init(|| {
        let mut dictionaries: [Value; 4] = [
            serde_json::from_str(include_str!("../../../editor/i18n/locales/chs.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/locales/cht.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/locales/ja.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/locales/ko.json")).unwrap(),
        ];
        for source in [
            include_str!("../data/i18n/native.json"),
            include_str!("../data/i18n/routes.json"),
            include_str!("../data/i18n/flow.json"),
            include_str!("../data/i18n/vanilla.json"),
            include_str!("../data/i18n/editor.json"),
            include_str!("../data/i18n/common.json"),
            include_str!("../data/i18n/workspace.json"),
        ] {
            let messages: Value = serde_json::from_str(source).expect("feature UI translations");
            for (key, translations) in messages.as_object().expect("message dictionary") {
                for (index, locale) in LOCALES.iter().enumerate() {
                    let translated = translations[*locale]
                        .as_str()
                        .expect("all four translations");
                    assert!(
                        !translated.trim().is_empty(),
                        "Empty translation {locale}/{key}"
                    );
                    if let Some(existing) = dictionaries[index].get(key) {
                        assert_eq!(
                            existing.as_str(),
                            Some(translated),
                            "Conflicting translation {locale}/{key}"
                        );
                    } else {
                        dictionaries[index][key] = translated.into();
                    }
                }
            }
        }
        dictionaries
    })
}
fn normalize(text: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\(&[A-Za-z]\)|&").unwrap());
    re.replace_all(text, "")
        .trim()
        .trim_end_matches(['…', '.', '：', ':'])
        .trim()
        .to_owned()
}
pub fn key(name: &str) -> String {
    let all = dictionaries();
    let i = LOCALE.load(Ordering::Relaxed);
    all[i][name]
        .as_str()
        .or_else(|| all[0][name].as_str())
        .unwrap_or(name)
        .to_owned()
}

/// Named values stay separate from translated text and are substituted once.
pub fn message(name: &str, arguments: &[(&str, String)]) -> String {
    message_for_locale(name, arguments, locale())
}
pub(crate) fn message_for_locale(name: &str, arguments: &[(&str, String)], locale: &str) -> String {
    message_index(
        name,
        arguments,
        LOCALES
            .iter()
            .position(|value| *value == locale)
            .unwrap_or(0),
    )
}
fn message_index(name: &str, arguments: &[(&str, String)], index: usize) -> String {
    let all = dictionaries();
    let template = all[index][name]
        .as_str()
        .or_else(|| all[0][name].as_str())
        .unwrap_or(name);
    static PARAMETER: OnceLock<regex::Regex> = OnceLock::new();
    PARAMETER
        .get_or_init(|| regex::Regex::new(r"\{([a-zA-Z_][a-zA-Z_0-9]*)\}").unwrap())
        .replace_all(template, |capture: &regex::Captures<'_>| {
            arguments
                .iter()
                .find(|(key, _)| *key == &capture[1])
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| capture[0].to_owned())
        })
        .into_owned()
}
pub fn tr(source: &str) -> String {
    tr_index(source, LOCALE.load(Ordering::Relaxed))
}
fn tr_index(source: &str, index: usize) -> String {
    if index == 0 {
        return source.into();
    }
    static NATIVE: OnceLock<Value> = OnceLock::new();
    let native = NATIVE.get_or_init(|| {
        serde_json::from_str(include_str!("../data/native-translations.json"))
            .expect("native UI translations")
    });
    if let Some(translated) = native[source][index - 1].as_str() {
        return translated.into();
    }
    static REVERSE: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let reverse = REVERSE.get_or_init(|| {
        dictionaries()[0]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(k, v)| Some((normalize(v.as_str()?), k.clone())))
            .collect()
    });
    let alias = match source {
        "文件" => "menu.file",
        "打开目录" => "menu.open_folder",
        "作品设置" => "menu.release_settings",
        "多语言" => "menu.localization",
        "内容库" => "menu.content_library",
        "导出 Mod" => "toolbar.export",
        "检查项目" => "menu.preflight",
        "节点参考与帮助" => "menu.node_reference",
        "演出" => "preview.tab.stage",
        "流程" => "preview.tab.flow",
        "统计" => "menu.project_stats",
        _ => "",
    };
    let translated = if !alias.is_empty() && dictionaries()[0].get(alias).is_some() {
        dictionaries()[index][alias].as_str().map(str::to_owned)
    } else {
        reverse
            .get(&normalize(source))
            .and_then(|k| dictionaries()[index][k].as_str().map(str::to_owned))
    };
    translated
        .map(|s| {
            static MENU: OnceLock<regex::Regex> = OnceLock::new();
            MENU.get_or_init(|| regex::Regex::new(r"\(&[A-Za-z]\)|&").unwrap())
                .replace_all(&s, "")
                .into_owned()
        })
        .unwrap_or_else(|| source.into())
}
pub fn term(category: &str, id: &str, fallback: &str) -> String {
    term_index(category, id, fallback, LOCALE.load(Ordering::Relaxed))
}

/// Official resource labels can fall back to names from Traditional assets.
/// The embedded character table is loaded once and preserves Unicode symbols.
/// Ambiguous characters used in simplified names, such as 著 in 著作, stay intact.
pub fn simplified_label(text: &str) -> String {
    if text.is_ascii() {
        return text.into();
    }
    static CHARACTERS: OnceLock<std::collections::HashMap<char, char>> = OnceLock::new();
    let characters = CHARACTERS.get_or_init(|| {
        serde_json::from_str(include_str!("../data/chs-characters.json"))
            .expect("simplified resource characters")
    });
    let mut result = String::with_capacity(text.len());
    result.extend(
        text.chars()
            .map(|character| characters.get(&character).copied().unwrap_or(character)),
    );
    result
}

fn term_index(category: &str, id: &str, fallback: &str, index: usize) -> String {
    static TERMS: OnceLock<[Value; 4]> = OnceLock::new();
    let tables = TERMS.get_or_init(|| {
        let mut simplified: Value =
            serde_json::from_str(include_str!("../../../editor/i18n/terms/chs.json")).unwrap();
        for category in simplified
            .as_object_mut()
            .expect("resource categories")
            .values_mut()
        {
            for label in category
                .as_object_mut()
                .expect("resource terms")
                .values_mut()
            {
                *label = Value::String(simplified_label(label.as_str().expect("resource label")));
            }
        }
        [
            simplified,
            serde_json::from_str(include_str!("../../../editor/i18n/terms/cht.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/terms/ja.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/terms/ko.json")).unwrap(),
        ]
    });
    if id.starts_with("user:") {
        return fallback.into();
    }
    let translated = tables[index][category][id]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| tables[0][category][id].as_str().filter(|s| !s.is_empty()));
    match translated {
        Some(translated) => translated.into(),
        None if index == 0 => simplified_label(fallback),
        None => fallback.into(),
    }
}
pub fn help_html(locale: &str) -> &'static str {
    match locale {
        "cht" => include_str!("../../../editor/i18n/help/cht.html"),
        "ja" => include_str!("../../../editor/i18n/help/ja.html"),
        "ko" => include_str!("../../../editor/i18n/help/ko.html"),
        _ => include_str!("../../../editor/i18n/help/chs.html"),
    }
}
#[cfg(test)]
pub fn help_text() -> String {
    let html = help_html(locale());
    let without_style = regex::Regex::new(r"(?s)<style[^>]*>.*?</style>")
        .unwrap()
        .replace_all(html, "");
    let breaks = regex::Regex::new(r"</(?:p|li|h[1-6]|tr)>|<br\s*/?>")
        .unwrap()
        .replace_all(&without_style, "\n");
    regex::Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&breaks, "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    fn production_sources() -> Vec<std::path::PathBuf> {
        fn collect(folder: &std::path::Path, paths: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                        collect(&path, paths);
                    }
                } else if path.extension().is_some_and(|extension| extension == "rs")
                    && !path
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .ends_with("_tests")
                {
                    paths.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        collect(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut paths,
        );
        paths
    }

    fn placeholders(value: &str) -> BTreeSet<String> {
        regex::Regex::new(r"\{[a-zA-Z_][a-zA-Z_0-9]*\}|\{\}")
            .unwrap()
            .find_iter(value)
            .map(|value| value.as_str().to_owned())
            .collect()
    }

    #[test]
    fn every_locale_has_matching_keys_nonempty_messages_and_placeholders() {
        let dictionaries = super::dictionaries();
        let source = dictionaries[0].as_object().unwrap();
        let keys: BTreeSet<_> = source.keys().collect();
        for (index, dictionary) in dictionaries.iter().enumerate() {
            let messages = dictionary.as_object().unwrap();
            assert_eq!(messages.keys().collect::<BTreeSet<_>>(), keys);
            for (key, value) in messages {
                let value = value.as_str().expect("text message");
                assert!(!value.trim().is_empty(), "{}/{key}", super::LOCALES[index]);
                assert_eq!(
                    placeholders(value),
                    placeholders(source[key].as_str().unwrap()),
                    "placeholder mismatch {}/{key}",
                    super::LOCALES[index]
                );
            }
        }
        let native: serde_json::Value =
            serde_json::from_str(include_str!("../data/native-translations.json")).unwrap();
        for (source, values) in native.as_object().unwrap() {
            let values = values.as_array().unwrap();
            assert_eq!(values.len(), 3, "{source}");
            for value in values {
                let value = value.as_str().unwrap();
                assert!(!value.trim().is_empty(), "{source}");
                assert_eq!(placeholders(value), placeholders(source), "{source}");
            }
        }
    }

    #[test]
    fn named_arguments_are_substituted_once_in_each_locale() {
        for locale in super::LOCALES {
            let result = super::message_for_locale(
                "editor.form.count",
                &[
                    ("label", "作者 {count} · 😀".into()),
                    ("count", "12".into()),
                ],
                locale,
            );
            assert!(result.contains("作者 {count} · 😀"), "{locale}/{result}");
            assert!(result.contains("12"));
            assert!(!result.contains("{label}"));
        }
    }

    #[test]
    fn static_message_references_exist_in_all_locales() {
        let references = regex::Regex::new(
            r#"(?:i18n::(?:key|message|message_for_locale)|\bkey)\(\s*"([a-z][a-z0-9_.]+)""#,
        )
        .unwrap();
        for path in production_sources() {
            let source = std::fs::read_to_string(&path).unwrap();
            for capture in references.captures_iter(&source) {
                for (index, dictionary) in super::dictionaries().iter().enumerate() {
                    assert!(
                        dictionary[&capture[1]].is_string(),
                        "{}/{}/{key}",
                        path.display(),
                        super::LOCALES[index],
                        key = &capture[1]
                    );
                }
            }
        }
    }

    #[test]
    fn legacy_literal_messages_have_catalog_entries() {
        let references = regex::Regex::new(r#"\btr\(\s*"([^"\\]*)"\s*,?\s*\)"#).unwrap();
        let native: serde_json::Value =
            serde_json::from_str(include_str!("../data/native-translations.json")).unwrap();
        let known: BTreeSet<_> = super::dictionaries()[0]
            .as_object()
            .unwrap()
            .values()
            .filter_map(|value| value.as_str())
            .map(super::normalize)
            .collect();
        for path in production_sources() {
            let source = std::fs::read_to_string(&path).unwrap();
            for capture in references.captures_iter(&source) {
                let text = &capture[1];
                if !text
                    .chars()
                    .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character))
                {
                    continue;
                }
                assert!(
                    native.get(text).is_some() || known.contains(&super::normalize(text)),
                    "missing legacy message {}/{text}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn widget_literals_and_templates_use_translation_lookups() {
        let patterns = [
            r#"(?:\.(?:label|heading|small|strong|button|small_button|hint_text|on_hover_text|collapsing)|Button::new|CollapsingHeader::new|Window::new)\(\s*(?:format!\(\s*)?"([^"\\]*)""#,
            r#"\.(?:checkbox|selectable_label)\(\s*[^,\n]+,\s*"([^"\\]*)""#,
        ].map(|pattern| regex::Regex::new(pattern).unwrap());
        let test_module = regex::Regex::new(r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{").unwrap();
        for path in production_sources() {
            let source = std::fs::read_to_string(&path).unwrap();
            let production = test_module
                .find(&source)
                .map_or(source.as_str(), |module| &source[..module.start()]);
            for pattern in &patterns {
                for capture in pattern.captures_iter(production) {
                    assert!(
                        !capture[1]
                            .chars()
                            .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)),
                        "untranslated widget literal {}/{}",
                        path.display(),
                        &capture[1]
                    );
                }
            }
        }
    }

    #[test]
    fn resource_term_catalogs_have_the_same_nonempty_entries() {
        let sources = [
            include_str!("../../../editor/i18n/terms/chs.json"),
            include_str!("../../../editor/i18n/terms/cht.json"),
            include_str!("../../../editor/i18n/terms/ja.json"),
            include_str!("../../../editor/i18n/terms/ko.json"),
        ];
        let tables: Vec<serde_json::Value> = sources
            .iter()
            .map(|source| serde_json::from_str(source).unwrap())
            .collect();
        let original = tables[0].as_object().unwrap();
        for (index, table) in tables.iter().enumerate() {
            let categories = table.as_object().unwrap();
            assert_eq!(
                categories.keys().collect::<BTreeSet<_>>(),
                original.keys().collect()
            );
            for (category, entries) in categories {
                let entries = entries.as_object().unwrap();
                assert_eq!(
                    entries.keys().collect::<BTreeSet<_>>(),
                    original[category].as_object().unwrap().keys().collect()
                );
                for (id, text) in entries {
                    assert!(
                        text.as_str().is_some_and(|text| !text.trim().is_empty()),
                        "{}/{category}/{id}",
                        super::LOCALES[index]
                    );
                }
            }
        }
    }

    #[test]
    fn four_dictionaries_and_help_are_embedded() {
        for value in super::dictionaries() {
            assert!(value["app.title"].is_string());
            assert!(value["node.say"].is_string());
            let schema: serde_json::Value =
                serde_json::from_str(include_str!("../data/authoring.json")).unwrap();
            for kind in schema["NODE_SCHEMAS"].as_object().unwrap().keys() {
                assert!(
                    value[format!("help.{kind}")]
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty()),
                    "Missing reader-facing explanation: {kind}"
                );
            }
        }
        assert!(super::help_text().len() > 1000);
        assert_eq!(super::normalize("文件(&F)…"), "文件");
    }
}

#[cfg(test)]
mod native_tests {
    #[test]
    fn important_menus_and_all_node_labels_translate_in_three_locales() {
        let native: serde_json::Value =
            serde_json::from_str(include_str!("../data/native-translations.json")).unwrap();
        for index in 1..4 {
            for label in [
                "新建项目",
                "打开目录…",
                "保存全部章节  ⌘S",
                "剧情工具",
                "剧情编辑",
                "素材管理",
                "游戏与 MOD",
                "发布",
                "作品素材",
                "共享内容库",
                "一键配置游戏",
                "批量修改与模板",
                "高级维护",
                "安装当前作品",
                "查看安装检查结果",
                "作品设置",
                "发布体检与打包",
                "批量修改选中步骤字段",
                "从模板新建",
                "共享内容库与 .lomcontent",
                "编辑器",
                "设置",
                "界面语言",
                "一键提取游戏资产",
                "游戏预览素材",
                "查看进度或结果",
                "技术详情（排查问题时使用）",
                "状态正常",
            ] {
                let translated = super::tr_index(label, index);
                assert!(!translated.is_empty());
                // Some Chinese/Japanese labels share the same spelling. An
                // explicit entry distinguishes that from a missing fallback.
                assert!(
                    translated != label || native[label][index - 1].as_str() == Some(label),
                    "locale {}: {label}",
                    super::LOCALES[index]
                );
            }
            let source = super::dictionaries();
            for key in [
                "node.say",
                "node.show",
                "node.combat",
                "field.combat.max_health",
                "field.character",
                "field.position",
            ] {
                assert!(source[index][key].is_string(), "{key}");
                assert!(!source[index][key].as_str().unwrap().is_empty());
            }
        }
    }
}

#[cfg(test)]
mod term_tests {
    #[test]
    fn simplified_resource_names_cover_fallbacks_and_preserve_unicode_and_user_names() {
        assert_eq!(
            super::simplified_label("繁體資源·乾坤著作😀‧✓한국어"),
            "繁体资源·乾坤著作😀‧✓한국어"
        );
        assert_eq!(
            super::term_index("characters", "unknown_prop", "雜項_破損屋樑", 0),
            "杂项_破损屋梁"
        );
        assert_eq!(super::term_index("talents", "1207", "犯劍", 0), "犯剑");
        assert_eq!(
            super::term_index("characters", "user:demo.hero", "原創人物", 0),
            "原創人物"
        );
        assert_eq!(
            super::term_index("characters", "unknown_prop", "雜項_樹枝", 1),
            "雜項_樹枝"
        );
    }
    #[test]
    fn resource_codes_have_display_names_in_every_ui_language() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/editor_data.json")).unwrap();
        for category in [
            "characters",
            "views",
            "sounds",
            "env_sounds",
            "stats",
            "effects",
            "menu_dialogs",
        ] {
            let entries: Vec<(String, String)> = if let Some(rows) = data[category].as_array() {
                rows.iter()
                    .filter_map(|row| {
                        Some((row["id"].as_str()?.into(), row["name"].as_str()?.into()))
                    })
                    .collect()
            } else {
                data[category]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(id, name)| (id.clone(), name.as_str().unwrap().into()))
                    .collect()
            };
            for (id, name) in entries {
                if name.is_ascii() {
                    for locale in 0..4 {
                        let translated = super::term_index(category, &id, &name, locale);
                        assert!(!translated.is_empty());
                        assert_ne!(
                            translated,
                            id,
                            "untranslated {category}/{id} in {}",
                            super::LOCALES[locale]
                        );
                    }
                }
            }
        }
        for portrait in data["characters"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|row| row["portraits"].as_array().unwrap().iter())
        {
            let id = portrait.as_str().unwrap();
            for locale in 0..4 {
                assert_ne!(
                    super::term_index("portraits", id, id, locale),
                    id,
                    "portrait {id} in {}",
                    super::LOCALES[locale]
                );
            }
        }
        assert_eq!(super::term_index("characters", "bun", "bun", 0), "包子");
        assert_eq!(
            super::term_index("characters", "carriage1", "carriage1", 0),
            "马车"
        );
        assert_eq!(
            super::term_index("characters", "checkerboard", "checkerboard", 0),
            "棋盘"
        );
    }
    #[test]
    fn translations_preserve_user_names_and_lookup_game_ids() {
        assert_eq!(
            super::term_index("characters", "artist1", "fallback", 3),
            "무사"
        );
        assert_eq!(
            super::term_index("characters", "user:demo.hero", "My hero", 3),
            "My hero"
        );
        assert_eq!(
            super::term_index("characters", "future_hero", "Future hero", 1),
            "Future hero"
        );
    }
}
