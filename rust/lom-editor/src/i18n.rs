//! The existing four offline dictionaries, shared by the native widgets.
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
        [
            serde_json::from_str(include_str!("../../../editor/i18n/locales/chs.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/locales/cht.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/locales/ja.json")).unwrap(),
            serde_json::from_str(include_str!("../../../editor/i18n/locales/ko.json")).unwrap(),
        ]
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
pub fn help_text() -> String {
    let html = match locale() {
        "cht" => include_str!("../../../editor/i18n/help/cht.html"),
        "ja" => include_str!("../../../editor/i18n/help/ja.html"),
        "ko" => include_str!("../../../editor/i18n/help/ko.html"),
        _ => include_str!("../../../editor/i18n/help/chs.html"),
    };
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
    #[test]
    fn four_dictionaries_and_help_are_embedded() {
        for value in super::dictionaries() {
            assert!(value["app.title"].is_string());
            assert!(value["node.say"].is_string());
        }
        assert!(super::help_text().len() > 1000);
        assert_eq!(super::normalize("文件(&F)…"), "文件");
    }
}

#[cfg(test)]
mod native_tests {
    #[test]
    fn important_menus_and_all_node_labels_translate_in_three_locales() {
        for index in 1..4 {
            for label in [
                "新建项目",
                "打开目录…",
                "保存全部章节  ⌘S",
                "创作工具",
                "作品设置",
                "发布体检与打包",
                "批量修改选中步骤字段",
                "从模板新建",
                "共享内容库与 .lomcontent",
                "界面语言 / Language",
            ] {
                let translated = super::tr_index(label, index);
                assert!(!translated.is_empty());
                assert_ne!(
                    translated,
                    label,
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
