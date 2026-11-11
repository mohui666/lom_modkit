//! Shared project document operations.
use super::*;

pub(super) fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub(super) fn short(s: &str, n: usize) -> String {
    let mut out = s.replace('\n', " ").chars().take(n).collect::<String>();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}
pub(crate) fn retarget(value: &mut Value, map: &BTreeMap<String, String>) {
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "goto"
                        | "win"
                        | "lose"
                        | "success"
                        | "failure"
                        | "resume_node"
                        | "start"
                        | "end"
                        | "goto_大成功"
                        | "goto_成功"
                        | "goto_失败"
                ) {
                    if let Some(new) = v.as_str().and_then(|s| map.get(s)) {
                        *v = json!(new);
                    }
                } else {
                    retarget(v, map);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                retarget(v, map)
            }
        }
        _ => {}
    }
}
pub(super) fn rename_translations(story: &mut Value, old: &str, new: &str) {
    if let Some(translations) = story
        .get_mut("localization")
        .and_then(|v| v.get_mut("translations"))
        .and_then(Value::as_object_mut)
    {
        for catalog in translations.values_mut() {
            if let Some(o) = catalog.as_object_mut() {
                let updates: Vec<_> = o
                    .keys()
                    .filter(|k| k.starts_with(&format!("{old}.")))
                    .cloned()
                    .collect();
                for key in updates {
                    if let Some(v) = o.remove(&key) {
                        o.insert(format!("{new}{}", &key[old.len()..]), v);
                    }
                }
            }
        }
    }
}
pub(super) fn rename_story_refs(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(k.as_str(), "next_script" | "script" | "story")
                    && v.as_str() == Some(old)
                {
                    *v = json!(new);
                } else {
                    rename_story_refs(v, old, new);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                rename_story_refs(v, old, new)
            }
        }
        _ => {}
    }
}
pub(super) fn chapter_group(story: &Value) -> &str {
    story["_editor"]["chapter_group"]
        .as_str()
        .unwrap_or("")
        .trim()
}
pub(super) fn set_chapter_group(story: &mut Value, group: &str) -> anyhow::Result<()> {
    if group.is_empty() && story.get("_editor").is_none() {
        return Ok(());
    }
    let editor = story
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("章节格式错误"))?
        .entry("_editor")
        .or_insert_with(|| json!({}));
    if editor.is_null() {
        *editor = json!({});
    }
    let fields = editor
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("章节 _editor 必须是对象，分组未修改"))?;
    if group.is_empty() {
        fields.remove("chapter_group");
    } else {
        fields.insert("chapter_group".into(), json!(group));
    }
    if fields.is_empty() {
        story.as_object_mut().unwrap().remove("_editor");
    }
    Ok(())
}
pub(super) fn story_reference_count(value: &Value, chapter: &str) -> usize {
    match value {
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| {
                if matches!(key.as_str(), "next_script" | "script" | "story")
                    && value.as_str() == Some(chapter)
                {
                    1
                } else {
                    story_reference_count(value, chapter)
                }
            })
            .sum(),
        Value::Array(values) => values
            .iter()
            .map(|value| story_reference_count(value, chapter))
            .sum(),
        _ => 0,
    }
}
pub(super) fn replace_prose(value: &mut Value, find: &str, replace: &str) -> usize {
    let mut count = 0;
    if find.is_empty() {
        return 0;
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "text"
                        | "title"
                        | "description"
                        | "desc"
                        | "header"
                        | "bonus_name"
                        | "bonus_status"
                ) {
                    if let Some(s) = v.as_str() {
                        let n = s.matches(find).count();
                        if n > 0 {
                            *v = json!(s.replace(find, replace));
                            count += n;
                        }
                    }
                } else if k != "localization" && k != "_editor" {
                    count += replace_prose(v, find, replace);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                count += replace_prose(v, find, replace)
            }
        }
        _ => {}
    }
    count
}
