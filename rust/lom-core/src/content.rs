//! User-content metadata and confined filesystem resolution shared by compiler and editor.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const MAX_AUDIO_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const ANIMATIONS: &[&str] = &[
    "combat_idle",
    "combat_attack",
    "combat_hurt",
    "combat_defence",
];
pub fn is_user_ref(value: &str) -> bool {
    value.starts_with("user:")
}
pub fn validate_content_id(id: &str) -> Result<()> {
    ensure!(
        regex::Regex::new(r"^[a-z][a-z0-9_]{0,31}\.[a-z0-9][a-z0-9_]{0,47}$")?.is_match(id),
        "内容 ID {id:?} 不合法。必须是 小写命名空间.名称（只含字母、数字、下划线），例如 mohui.boss_theme。"
    );
    Ok(())
}
pub fn parse_content_ref(value: &str) -> Result<Option<String>> {
    let Some(id) = value.strip_prefix("user:") else {
        return Ok(None);
    };
    validate_content_id(id)?;
    Ok(Some(id.into()))
}
pub fn package_content_dir(kind: &str, id: &str) -> Result<String> {
    ensure!(
        regex::Regex::new(r"^[a-z][a-z0-9_]{0,15}$")?.is_match(kind),
        "不支持的内容类型 {kind:?}"
    );
    validate_content_id(id)?;
    Ok(format!("assets/user/{kind}/{id}"))
}
pub fn default_repository_root() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(appdata).join("lom_modkit/repository");
    }
    let home = PathBuf::from(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .unwrap_or_default(),
    );
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/lom_modkit/repository")
    } else {
        home.join("AppData/Roaming/lom_modkit/repository")
    }
}
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn safe_filename(v: &Value, label: &str, audio: bool) -> Result<String> {
    let name = v
        .as_str()
        .filter(|x| !x.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{label}必须是文件名字符串"))?;
    ensure!(
        !name.contains(['/', '\\']) && !name.contains(".."),
        "{label}必须是同目录下的文件名，不能含路径：{name:?}"
    );
    let ext = Path::new(name)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_lowercase();
    ensure!(
        if audio {
            ["ogg", "wav"].contains(&ext.as_str())
        } else {
            ["png", "jpg", "jpeg"].contains(&ext.as_str())
        },
        "{label}扩展名不支持：{name:?}"
    );
    Ok(name.into())
}
fn optional_image(v: &Value, label: &str) -> Result<Option<String>> {
    if v.is_null() || v.as_str().is_some_and(|x| x.trim().is_empty()) {
        Ok(None)
    } else {
        safe_filename(v, label, false).map(Some)
    }
}
fn clamp(v: &Value, default: i64, lo: i64, hi: i64) -> i64 {
    let n = if let Some(n) = v.as_i64() {
        Some(n)
    } else if let Some(n) = v.as_f64() {
        if n.is_finite() {
            Some(n as i64)
        } else {
            None
        }
    } else if let Some(s) = v.as_str() {
        s.trim().parse().ok()
    } else {
        None
    };
    n.unwrap_or(default).clamp(lo, hi)
}
pub fn normalize_character_intro(raw: &Value) -> Result<Value> {
    if raw.is_null() {
        return Ok(Value::Null);
    }
    let obj = raw
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("intro 必须是对象，或省略"))?;
    if obj.is_empty() {
        return Ok(Value::Null);
    }
    for k in ["name", "text"] {
        ensure!(
            !text(raw, k).trim().is_empty(),
            "介绍卡 {k} 必须是非空字符串"
        );
    }
    ensure!(
        raw["title"].is_null() || raw["title"].is_string(),
        "介绍卡 title 必须是字符串"
    );
    Ok(
        json!({"title":text(raw,"title").trim(),"name":text(raw,"name").trim(),"text":text(raw,"text").trim(),"image":optional_image(&raw["image"],"intro.image")?,"image_scale":clamp(&raw["image_scale"],100,40,160),"image_x":clamp(&raw["image_x"],0,-30,30),"image_y":clamp(&raw["image_y"],0,-30,30)}),
    )
}
pub fn normalize_content_metadata(data: &Value) -> Result<Value> {
    ensure!(data.is_object(), "content.json：顶层必须是 JSON 对象");
    let schema = data.get("content_schema").unwrap_or(&data["schema"]);
    ensure!(
        schema.as_f64() == Some(1.0),
        "content_schema 必须是 1（当前用户内容格式版本）"
    );
    if data.get("content_schema").is_some() && data.get("schema").is_some() {
        ensure!(
            data["schema"].as_f64() == Some(1.0),
            "content_schema 与旧字段 schema 的版本声明不一致"
        );
    }
    let id = text(data, "id");
    validate_content_id(id)?;
    let kind = text(data, "type");
    ensure!(
        ["audio", "character", "image"].contains(&kind),
        "type 必须是 audio / character / image 之一"
    );
    let name = text(data, "name").trim();
    ensure!(!name.is_empty(), "name（显示名称）必须是非空字符串");
    let main = safe_filename(&data["files"]["main"], "files.main", kind == "audio")?;
    let mut meta = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":name,"audio_kind":null,"files":{"main":main},"portraits":null,"character":null,"intro":null,"title":null,"scale":null,"art_facing":null});
    if kind == "audio" {
        let audio = text(data, "audio_kind");
        ensure!(
            ["music", "sound", "env"].contains(&audio),
            "音频的 audio_kind 必须是 music / sound / env"
        );
        meta["audio_kind"] = json!(audio);
        if !data["character"].is_null() {
            ensure!(
                data["character"].is_string(),
                "character 必须是字符串，或省略"
            );
            let c = text(data, "character").trim();
            if !c.is_empty() {
                let normalized = if is_user_ref(c) {
                    parse_content_ref(c)?;
                    c.into()
                } else if validate_content_id(c).is_ok() {
                    format!("user:{c}")
                } else {
                    ensure!(
                        regex::Regex::new(r"^[A-Za-z][A-Za-z0-9_]{0,47}$")?.is_match(c),
                        "character 必须是 user:命名空间.名称 或官方人物 id"
                    );
                    c.into()
                };
                meta["character"] = json!(normalized);
            }
        }
    }
    if kind == "character" {
        let mut portraits = if data["portraits"].is_null() {
            Map::new()
        } else {
            let p = data["portraits"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("角色必须提供 portraits（表情 id -> 文件名）"))?;
            ensure!(!p.is_empty(), "角色必须提供 portraits（表情 id -> 文件名）");
            p.clone()
        };
        let portrait_re = regex::Regex::new(r"^[A-Za-z][A-Za-z0-9_]{0,31}$")?;
        for (k, v) in &mut portraits {
            ensure!(portrait_re.is_match(k), "表情 id {k:?} 不合法");
            *v = json!(safe_filename(v, &format!("portraits.{k}"), false)?);
        }
        if !portraits.contains_key("normal") {
            portraits.insert("normal".into(), json!(main));
        }
        if portraits["normal"] != main && !portraits.values().any(|v| v == &main) {
            portraits.insert("normal".into(), json!(main));
        }
        meta["portraits"] = Value::Object(portraits);
        let intro = normalize_character_intro(&data["intro"])?;
        let title = if data["title"].is_null() {
            &intro["title"]
        } else {
            &data["title"]
        };
        if let Some(t) = title.as_str() {
            if !t.trim().is_empty() {
                meta["title"] = json!(t.trim());
            } else {
                ensure!(t.is_empty(), "title（对话称号）必须是字符串");
            }
        } else {
            ensure!(title.is_null(), "title（对话称号）必须是字符串");
        }
        meta["intro"] = intro;
        meta["scale"] = json!(clamp(&data["scale"], 100, 50, 130));
        let facing = if data["art_facing"].is_null() || data["art_facing"] == "" {
            "left".into()
        } else {
            ensure!(
                data["art_facing"].is_string(),
                "art_facing 必须是 left 或 right"
            );
            text(data, "art_facing").trim().to_lowercase()
        };
        ensure!(
            ["left", "right"].contains(&facing.as_str()),
            "art_facing 必须是 left 或 right"
        );
        meta["art_facing"] = json!(facing);
        let idle = optional_image(&data["combat_idle"], "combat_idle")?
            .unwrap_or_else(|| text(&meta["portraits"], "normal").into());
        meta["combat_idle"] = json!(idle);
        for k in &ANIMATIONS[1..] {
            meta[*k] = json!(optional_image(&data[*k], k)?.unwrap_or_else(|| idle.clone()));
        }
    }
    Ok(meta)
}
pub fn listed_content_files(meta: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut add = |v: &Value| {
        if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
            if !out.iter().any(|x| x == s) {
                out.push(s.to_string());
            }
        }
    };
    add(&meta["files"]["main"]);
    if let Some(p) = meta["portraits"].as_object() {
        for v in p.values() {
            add(v);
        }
    }
    for k in ANIMATIONS {
        add(&meta[*k]);
    }
    add(&meta["intro"]["image"]);
    out
}
pub fn content_metadata_payload(meta: &Value) -> Value {
    let mut p = Map::new();
    for k in ["schema", "content_schema", "id", "type", "name", "files"] {
        p.insert(k.into(), meta[k].clone());
    }
    if meta["type"] == "audio" {
        p.insert("audio_kind".into(), meta["audio_kind"].clone());
        if !meta["character"].is_null() {
            p.insert("character".into(), meta["character"].clone());
        }
    }
    if meta["type"] == "character" {
        p.insert("portraits".into(), meta["portraits"].clone());
        let idle = &meta["combat_idle"];
        if !idle.is_null() && idle != &meta["portraits"]["normal"] {
            p.insert("combat_idle".into(), idle.clone());
        }
        for k in &ANIMATIONS[1..] {
            if !meta[*k].is_null() && &meta[*k] != idle {
                p.insert((*k).into(), meta[*k].clone());
            }
        }
        if !meta["title"].is_null() {
            p.insert("title".into(), meta["title"].clone());
        }
        if !meta["scale"].is_null() && meta["scale"] != 100 {
            p.insert("scale".into(), meta["scale"].clone());
        }
        if !meta["art_facing"].is_null() && meta["art_facing"] != "left" {
            p.insert("art_facing".into(), meta["art_facing"].clone());
        }
        if meta["intro"].is_object() {
            p.insert("intro".into(), meta["intro"].clone());
        }
    }
    Value::Object(p)
}
pub fn load_content_metadata(path: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("无法读取 content.json：{}", path.display()))?;
    let data: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .with_context(|| format!("content.json 不是合法 JSON：{}", path.display()))?;
    normalize_content_metadata(&data).with_context(|| path.display().to_string())
}
pub fn resolve_content(root: &Path, kind: &str, id: &str) -> Result<(Value, PathBuf)> {
    let relative = package_content_dir(kind, id)?;
    let folder = root.join(relative);
    ensure!(
        folder.is_dir(),
        "找不到用户内容 user:{id}（类型 {kind}）。请确认已导入用户内容库，或包内 assets/user/ 含有该资源。"
    );
    let root_real = root.canonicalize()?;
    let folder = folder.canonicalize()?;
    ensure!(
        folder.starts_with(&root_real),
        "用户内容目录通过 symlink/junction 指向项目目录外：{id}"
    );
    let metadata_path = folder.join("content.json");
    let metadata_real = metadata_path.canonicalize()?;
    ensure!(
        metadata_real.starts_with(&folder),
        "用户内容 metadata 通过 symlink/junction 路径逃逸：{id}"
    );
    let meta = load_content_metadata(&metadata_real)?;
    ensure!(
        meta["id"] == id,
        "用户内容 {id} 的 content.json id 与目录名不一致"
    );
    ensure!(
        meta["type"] == kind,
        "用户内容 {id} 的 type 与目录类型 {kind} 不一致"
    );
    let files = listed_content_files(&meta);
    let max = if kind == "audio" {
        MAX_AUDIO_BYTES
    } else {
        MAX_IMAGE_BYTES
    };
    let mut main = None;
    for name in files {
        let path = folder
            .join(&name)
            .canonicalize()
            .with_context(|| format!("用户内容 user:{id} 的文件不存在：{name}"))?;
        ensure!(
            path.starts_with(&folder),
            "用户内容 user:{id} 的文件通过 symlink/junction 路径逃逸：{name}"
        );
        let stat = path.metadata()?;
        ensure!(stat.is_file(), "用户内容 user:{id} 的文件不存在：{name}");
        ensure!(
            stat.len() <= max,
            "用户内容 user:{id}/{name} 超过单文件大小限制 {} MB",
            max / 1024 / 1024
        );
        if name == text(&meta["files"], "main") {
            main = Some(path);
        }
    }
    Ok((
        meta,
        main.ok_or_else(|| anyhow::anyhow!("用户内容主文件不存在"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_character_and_payload() {
        let m=normalize_content_metadata(&json!({"schema":1,"id":"test.person","type":"character","name":" 名字 ","files":{"main":"main.png"},"portraits":{"happy":"happy.png"},"scale":500,"intro":{"name":" 人物 ","text":" 介绍 ","image_scale":"130"}})).unwrap();
        assert_eq!(m["portraits"]["normal"], "main.png");
        assert_eq!(m["scale"], 130);
        assert_eq!(m["intro"]["name"], "人物");
        assert_eq!(m["combat_attack"], "main.png");
        let p = content_metadata_payload(&m);
        assert!(p.get("combat_attack").is_none());
        assert_eq!(p["scale"], 130);
    }
    #[test]
    fn rejects_unsafe_content() {
        for id in ["../escape", "a.b/other", "Bad.id", "x..x"] {
            assert!(validate_content_id(id).is_err());
        }
        assert!(normalize_content_metadata(&json!({"schema":1,"id":"test.asset","type":"image","name":"image","files":{"main":"../main.png"}})).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let dir = root.path().join("assets/user/image/test.asset");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("content.json"),r#"{"schema":1,"id":"test.asset","type":"image","name":"image","files":{"main":"main.png"}}"#).unwrap();
        std::fs::write(outside.path().join("main.png"), b"png").unwrap();
        symlink(outside.path().join("main.png"), dir.join("main.png")).unwrap();
        assert!(resolve_content(root.path(), "image", "test.asset").is_err());
    }
}
