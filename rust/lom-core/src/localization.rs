//! Story prose localization. IDs, paths and raw Lua are never translated.
use anyhow::{bail, ensure, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const SUPPORTED_LOCALES: [&str; 4] = ["chs", "cht", "ja", "ko"];
pub fn normalize_locale(locale: &str) -> &str {
    match locale {
        "zh_CN" | "zh-CN" | "zh_Hans" | "zh-Hans" => "chs",
        "zh_TW" | "zh-TW" | "zh_Hant" | "zh-Hant" => "cht",
        value => value,
    }
}
pub fn localization_config(story: &Value) -> Option<Value> {
    let mut config = story.get("localization")?.as_object()?.clone();
    for key in ["default_locale", "fallback_locale"] {
        if let Some(locale) = config.get(key).and_then(Value::as_str) {
            let locale = normalize_locale(locale).to_string();
            config.insert(key.into(), Value::String(locale));
        }
    }
    if let Some(translations) = config.get("translations").and_then(Value::as_object) {
        let mut canonical = Map::new();
        for (locale, catalog) in translations {
            let key = normalize_locale(locale).to_string();
            if let (Some(Value::Object(previous)), Value::Object(next)) =
                (canonical.get_mut(&key), catalog)
            {
                previous.extend(next.clone());
            } else {
                canonical.insert(key, catalog.clone());
            }
        }
        config.insert("translations".into(), Value::Object(canonical));
    }
    Some(Value::Object(config))
}
pub fn iter_localizable_texts(story: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    fn add(out: &mut BTreeMap<String, String>, key: String, value: &Value) {
        if let Some(s) = value.as_str().filter(|s| !s.is_empty()) {
            out.insert(key, s.into());
        }
    }
    add(&mut out, "story.title".into(), &story["title"]);
    for node in story["nodes"].as_array().into_iter().flatten() {
        let id = node["id"].as_str().unwrap_or("");
        let kind = node["type"].as_str().unwrap_or("");
        let fields: &[&str] = match kind {
            "say" | "message" => &["text"],
            "intro" => &["title", "name", "text"],
            "goto_scene" => &["title", "desc"],
            "death" => &["title", "text"],
            "dice" => &["header", "bonus_name", "bonus_status"],
            _ => &[],
        };
        for field in fields {
            add(&mut out, format!("{id}.{field}"), &node[*field]);
        }
        if kind == "choice" {
            for (i, option) in node["options"].as_array().into_iter().flatten().enumerate() {
                add(&mut out, format!("{id}.options.{i}.text"), &option["text"]);
            }
        }
        if kind == "dice" {
            if let Some(bands) = node["bands"].as_array() {
                for (i, band) in bands.iter().enumerate() {
                    add(&mut out, format!("{id}.bands.{i}.text"), &band["text"]);
                }
            } else {
                for (i, option) in node["options"].as_array().into_iter().flatten().enumerate() {
                    for (j, value) in option["band_texts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        add(&mut out, format!("{id}.options.{i}.band_texts.{j}"), value);
                    }
                }
            }
        }
    }
    out
}
pub fn validate_story_localization(story: &Value) -> Result<()> {
    let Some(config) = localization_config(story) else {
        ensure!(
            story.get("localization").is_none(),
            "字段 localization 必须是对象"
        );
        return Ok(());
    };
    for key in config.as_object().unwrap().keys() {
        ensure!(
            ["default_locale", "fallback_locale", "translations"].contains(&key.as_str()),
            "localization 含未知字段 {key}"
        );
    }
    let default = config["default_locale"].as_str().unwrap_or("");
    let fallback = config
        .get("fallback_locale")
        .unwrap_or(&config["default_locale"])
        .as_str()
        .unwrap_or("");
    ensure!(
        SUPPORTED_LOCALES.contains(&default),
        "localization.default_locale 必须是 chs/cht/ja/ko 之一"
    );
    ensure!(
        SUPPORTED_LOCALES.contains(&fallback),
        "localization.fallback_locale 必须是 chs/cht/ja/ko 之一"
    );
    let known = iter_localizable_texts(story);
    if let Some(raw) = config.get("translations") {
        let Some(translations) = raw.as_object() else {
            bail!("localization.translations 必须是对象");
        };
        for (locale, catalog) in translations {
            ensure!(
                SUPPORTED_LOCALES.contains(&locale.as_str()),
                "不支持的 locale {locale}"
            );
            ensure!(
                locale != default,
                "默认语言 {default} 不应重复放入 translations"
            );
            let Some(catalog) = catalog.as_object() else {
                bail!("localization.translations.{locale} 必须是对象");
            };
            for (path, value) in catalog {
                ensure!(
                    known.contains_key(path),
                    "localization.translations.{locale} 含不存在或不可翻译的路径 {path}"
                );
                ensure!(
                    value.as_str().is_some_and(|s| !s.is_empty()),
                    "localization.translations.{locale}.{path} 必须是非空字符串"
                );
            }
        }
    }
    Ok(())
}
pub fn resolved_catalog(story: &Value, locale: &str) -> Result<BTreeMap<String, String>> {
    validate_story_localization(story)?;
    let mut source = iter_localizable_texts(story);
    if let Some(config) = localization_config(story) {
        let locale = normalize_locale(locale);
        ensure!(
            SUPPORTED_LOCALES.contains(&locale),
            "不支持的 Story locale: {locale}"
        );
        let fallback = config
            .get("fallback_locale")
            .unwrap_or(&config["default_locale"])
            .as_str()
            .unwrap_or("");
        for (path, value) in &mut source {
            if let Some(translated) = config["translations"][locale]
                .get(path.as_str())
                .or_else(|| config["translations"][fallback].get(path.as_str()))
                .and_then(Value::as_str)
            {
                *value = translated.into();
            }
        }
    }
    Ok(source)
}
pub fn apply_story_locale(story: &Value, locale: &str) -> Result<Value> {
    let mut out = story.clone();
    for (path, value) in resolved_catalog(story, locale)? {
        if path == "story.title" {
            out["title"] = Value::String(value);
            continue;
        }
        let parts: Vec<_> = path.split('.').collect();
        let nodes = out["nodes"]
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("缺少 nodes"))?;
        let mut target = nodes
            .iter_mut()
            .find(|n| n["id"].as_str() == Some(parts[0]))
            .ok_or_else(|| anyhow::anyhow!("翻译节点不存在"))?;
        for part in &parts[1..] {
            target = if target.is_array() {
                target.get_mut(part.parse::<usize>()?)
            } else {
                target.get_mut(*part)
            }
            .ok_or_else(|| anyhow::anyhow!("翻译路径不存在: {path}"))?;
        }
        *target = Value::String(value);
    }
    if let Some(obj) = out.as_object_mut() {
        obj.remove("localization");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn translations_fallback_and_never_touch_ids() {
        let story = json!({"id":"main","title":"标题","nodes":[{"id":"s","type":"say","text":"原文","character":"player"}],"localization":{"default_locale":"chs","fallback_locale":"cht","translations":{"cht":{"s.text":"繁體"},"ja":{"story.title":"日本語"}}}});
        let out = apply_story_locale(&story, "ja").unwrap();
        assert_eq!(out["title"], "日本語");
        assert_eq!(out["nodes"][0]["text"], "繁體");
        assert_eq!(out["nodes"][0]["character"], "player");
        assert!(out.get("localization").is_none());
        let mut bad = story.clone();
        bad["localization"]["translations"]["ja"]["s.character"] = json!("wrong");
        assert!(validate_story_localization(&bad).is_err());
    }
}
