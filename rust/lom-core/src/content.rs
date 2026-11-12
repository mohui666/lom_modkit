//! User-content metadata and confined filesystem resolution shared by compiler and editor.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const MAX_AUDIO_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
pub const CONTENT_TYPES: &[&str] = &[
    "audio",
    "character",
    "image",
    "item",
    "martial",
    "support",
    "skill",
    "talent",
];
pub const GAMEPLAY_TYPES: &[&str] = &["item", "martial", "support", "skill", "talent"];
const ANIMATIONS: &[&str] = &[
    "combat_idle",
    "combat_attack",
    "combat_hurt",
    "combat_defence",
];
pub fn is_user_ref(value: &str) -> bool {
    value.starts_with("user:")
}
pub fn validate_portrait_id(id: &str) -> Result<()> {
    ensure!(
        regex::Regex::new(r"^[A-Za-z][A-Za-z0-9_]{0,31}$")?.is_match(id),
        "表情 id {id:?} 不合法：须以英文字母开头，只含字母、数字和下划线，最多 32 位"
    );
    Ok(())
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
/// Percentages are author controlled; only reject values the game cannot represent.
pub fn positive_scale(value: &Value, label: &str) -> Result<Value> {
    if value.is_null() {
        return Ok(json!(100));
    }
    let number = value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .with_context(|| format!("{label} 必须是正数百分比"))?;
    ensure!(
        number.is_finite() && number > 0.0 && (number as f32).is_finite() && (number as f32) > 0.0,
        "{label} 必须是游戏可表示的正数百分比"
    );
    Ok(if value.is_number() {
        value.clone()
    } else {
        json!(number)
    })
}
/// Original-character additions stay in project assets, never in the preview cache.
pub fn character_extensions(
    assets: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<std::collections::BTreeMap<String, (String, Value)>> {
    let mut extensions = std::collections::BTreeMap::new();
    for (key, bytes) in assets {
        if !key.starts_with("assets/user/character/") || !key.ends_with("/content.json") {
            continue;
        }
        let raw: Value = serde_json::from_slice(bytes).with_context(|| key.clone())?;
        if raw.get("extends").is_none_or(|v| v.is_null() || v == "") {
            continue;
        }
        let meta = normalize_content_metadata(&raw).with_context(|| key.clone())?;
        ensure!(
            meta["type"] == "character",
            "原版人物扩展必须是 character 类型"
        );
        let id = text(&meta, "id");
        ensure!(
            key == &format!("assets/user/character/{id}/content.json"),
            "扩展素材 ID 与目录不一致：{key}"
        );
        let target = text(&meta, "extends").to_owned();
        ensure!(
            !extensions.contains_key(&target),
            "原版人物 {target} 存在多份扩展，请合并到同一素材内"
        );
        extensions.insert(target, (key.clone(), meta));
    }
    Ok(extensions)
}
pub fn character_extensions_at(
    root: &Path,
) -> Result<std::collections::BTreeMap<String, (String, Value)>> {
    let directory = root.join("assets/user/character");
    let mut assets = std::collections::BTreeMap::new();
    if directory.exists() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path().join("content.json");
            if path.is_file() {
                let path = crate::package::resolve_confined_file(root, &path)?;
                let key = path
                    .strip_prefix(root.canonicalize()?)?
                    .to_string_lossy()
                    .replace('\\', "/");
                assets.insert(key, std::fs::read(path)?);
            }
        }
    }
    character_extensions(&assets)
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
        json!({"title":text(raw,"title").trim(),"name":text(raw,"name").trim(),"text":text(raw,"text").trim(),"image":optional_image(&raw["image"],"intro.image")?,"image_scale":positive_scale(&raw["image_scale"],"intro.image_scale")?,"image_x":clamp(&raw["image_x"],0,-30,30),"image_y":clamp(&raw["image_y"],0,-30,30)}),
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
        CONTENT_TYPES.contains(&kind),
        "type 必须是 audio / character / image / item / martial / support / skill / talent 之一"
    );
    if kind != "character" {
        ensure!(
            data.get("initially_known").is_none() && data.get("biography_pages").is_none(),
            "initially_known 与 biography_pages 仅支持 character 人物素材"
        );
    }
    let name = text(data, "name").trim();
    ensure!(!name.is_empty(), "name（显示名称）必须是非空字符串");
    ensure!(
        kind != "talent" || data["files"].is_null() || data["files"].is_object(),
        "files 必须是对象"
    );
    let main =
        if kind == "talent" && (data["files"]["main"].is_null() || data["files"]["main"] == "") {
            String::new()
        } else {
            safe_filename(&data["files"]["main"], "files.main", kind == "audio")?
        };
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
        let extends = data
            .get("extends")
            .map(|v| v.as_str().context("extends 必须是原版人物 ID"))
            .transpose()?
            .unwrap_or("")
            .trim();
        if !extends.is_empty() {
            ensure!(
                crate::validate::editor_data()["characters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|c| c["id"] == extends),
                "找不到要扩展的原版人物：{extends}"
            );
            meta["extends"] = json!(extends);
        }
        let mut portraits = if data["portraits"].is_null() {
            Map::new()
        } else {
            let p = data["portraits"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("角色必须提供 portraits（表情 id -> 文件名）"))?;
            ensure!(!p.is_empty(), "角色必须提供 portraits（表情 id -> 文件名）");
            p.clone()
        };
        for (k, v) in &mut portraits {
            validate_portrait_id(k)?;
            *v = json!(safe_filename(v, &format!("portraits.{k}"), false)?);
        }
        if extends.is_empty() && !portraits.contains_key("normal") {
            portraits.insert("normal".into(), json!(main));
        }
        if extends.is_empty()
            && portraits["normal"] != main
            && !portraits.values().any(|v| v == &main)
        {
            portraits.insert("normal".into(), json!(main));
        }
        meta["portraits"] = Value::Object(portraits);
        if !data["portrait_layouts"].is_null() {
            meta["portrait_layouts"] = crate::portrait_alignment::normalize(
                &data["portrait_layouts"],
                &meta["portraits"],
            )?;
        }
        if !data["portrait_reference"].is_null() {
            let reference = data["portrait_reference"]
                .as_str()
                .context("基准表情必须是表情 ID")?;
            ensure!(
                meta["portraits"].get(reference).is_some(),
                "基准表情不存在：{reference}"
            );
            meta["portrait_reference"] = json!(reference);
        }
        ensure!(
            extends.is_empty() || !meta["portraits"].as_object().unwrap().is_empty(),
            "原版人物扩展必须提供至少一个表情和图片"
        );
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
        if !data["biography"].is_null() {
            ensure!(
                data["biography"].is_string(),
                "biography（状态页人物传记）必须是文字"
            );
            meta["biography"] = json!(text(data, "biography").trim());
        }
        crate::character_profile::normalize_metadata(data, &mut meta)?;
        if let Some(avatar) = optional_image(&data["avatar"], "avatar（好感头像）")? {
            meta["avatar"] = json!(avatar);
        }
        if !data["initial_affinity"].is_null() {
            meta["initial_affinity"] = json!(integer_field(data, "initial_affinity", 0, 100)?);
        }
        meta["scale"] = positive_scale(&data["scale"], "scale")?;
        for (key, default) in [("mood_x", 50), ("mood_y", 90)] {
            if !data[key].is_null() {
                meta[key] = json!(integer_default(data, key, default, 0, 100)?);
            }
        }
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
            .unwrap_or_else(|| meta["portraits"]["normal"].as_str().unwrap_or(&main).into());
        meta["combat_idle"] = json!(idle);
        for k in &ANIMATIONS[1..] {
            meta[*k] = json!(optional_image(&data[*k], k)?.unwrap_or_else(|| idle.clone()));
        }
    }
    if GAMEPLAY_TYPES.contains(&kind) {
        normalize_gameplay_metadata(data, &mut meta)?;
    }
    Ok(meta)
}
fn integer_field(data: &Value, key: &str, low: i64, high: i64) -> Result<i64> {
    let value = data[key]
        .as_i64()
        .with_context(|| format!("{key} 必须是整数"))?;
    ensure!(
        (low..=high).contains(&value),
        "{key} 必须在 {low}~{high} 之间"
    );
    Ok(value)
}
fn integer_default(data: &Value, key: &str, default: i64, low: i64, high: i64) -> Result<i64> {
    if data.get(key).is_none() {
        Ok(default)
    } else {
        integer_field(data, key, low, high)
    }
}
fn combat_effects(data: &Value) -> Result<Value> {
    let empty = json!([]);
    let effects = data
        .get("effects")
        .unwrap_or(&empty)
        .as_array()
        .context("effects 必须是列表")?;
    ensure!(effects.len() <= 16, "effects 最多16项");
    Ok(json!(effects.iter().map(|effect| {
        ensure!(["damage","heal","stamina"].contains(&text(effect,"type")),"效果 type 必须是 damage / heal / stamina");
        ensure!(["self","enemy"].contains(&text(effect,"target")),"效果 target 必须是 self / enemy");
        Ok(json!({"type":effect["type"],"target":effect["target"],"value":integer_field(effect,"value",1,999999)?}))
    }).collect::<Result<Vec<_>>>()?))
}
fn consumable_fields(data: &Value, meta: &mut Value) -> Result<()> {
    let use_in =
        data.get("use_in")
            .and_then(Value::as_str)
            .unwrap_or(if data.get("use_in").is_none() {
                "field"
            } else {
                ""
            });
    ensure!(
        ["field", "combat"].contains(&use_in),
        "use_in 必须是 field / combat"
    );
    let effects = data["effects"]
        .as_array()
        .context("消耗品 effects 必须是列表")?;
    ensure!(
        (1..=16).contains(&effects.len()),
        "消耗品必须有1~16项使用效果"
    );
    meta["use_in"] = json!(use_in);
    meta["effects"] = json!(effects
        .iter()
        .map(|effect| {
            let value = integer_field(
                effect,
                "value",
                if use_in == "field" { -999999 } else { 1 },
                999999,
            )?;
            ensure!(
                effect.get("target").is_none(),
                "消耗品目标固定为玩家，不能指定 target"
            );
            if use_in == "field" {
                ensure!(
                    effect["type"] == "stat" && value != 0,
                    "野外消耗品效果必须是非零 stat 加成"
                );
                let stat = text(effect, "stat");
                ensure!(
                    crate::validate::editor_data()["stats"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row["id"] == stat),
                    "未知玩家属性：{stat}"
                );
                Ok(json!({"type":"stat","stat":stat,"value":value}))
            } else {
                ensure!(
                    ["heal", "stamina"].contains(&text(effect, "type")),
                    "决斗消耗品效果必须是 heal / stamina"
                );
                Ok(json!({"type":effect["type"],"value":value}))
            }
        })
        .collect::<Result<Vec<_>>>()?);
    Ok(())
}
fn stat_rows(data: &Value, field: &str, value_key: &str, low: i64, high: i64) -> Result<Value> {
    if data.get(field).is_none() {
        return Ok(json!([]));
    }
    let rows = data[field]
        .as_array()
        .with_context(|| format!("{field} 必须是列表"))?;
    ensure!(rows.len() <= 32, "{field} 最多 32 项");
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        let stat = text(row, "stat");
        ensure!(
            crate::validate::editor_data()["stats"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == stat),
            "未知玩家属性：{stat}"
        );
        ensure!(seen.insert(stat), "{field} 中属性重复：{stat}");
        let value = integer_field(row, value_key, low, high)?;
        result.push(json!({"stat":stat,value_key:value}));
    }
    Ok(json!(result))
}
fn normalize_gameplay_metadata(data: &Value, meta: &mut Value) -> Result<()> {
    let description = text(data, "description").trim();
    ensure!(
        (data.get("description").is_none() || data["description"].is_string())
            && description.chars().count() <= 4000,
        "description 必须是最多 4000 字的说明"
    );
    meta["description"] = json!(description);
    match text(data, "type") {
        "talent" => {
            for key in ["effects", "bonuses", "combat_skill", "battle_skill"] {
                ensure!(
                    data.get(key).is_none(),
                    "自定义天赋当前仅支持名称与描述，不支持 {key} 效果字段"
                );
            }
        }
        "item" | "martial" => {
            meta["cost"] = json!(if data.get("cost").is_none() {
                0
            } else {
                integer_field(data, "cost", 0, 999999)?
            });
            if data["type"] == "item" {
                let category = data.get("category").and_then(Value::as_str).unwrap_or(
                    if data.get("category").is_none() {
                        "misc"
                    } else {
                        ""
                    },
                );
                ensure!(
                    ["misc", "special", "consumable"].contains(&category),
                    "category 必须是 misc / special / consumable"
                );
                meta["category"] = json!(category);
                if category == "consumable" {
                    consumable_fields(data, meta)?;
                }
            } else {
                let levels = data["levels"].as_array().context("levels 必须是列表")?;
                ensure!((1..=20).contains(&levels.len()), "功法必须有 1~20 级");
                meta["levels"] = json!(levels
                    .iter()
                    .enumerate()
                    .map(|(index, level)| {
                        let mut normalized = json!({
                        "points":integer_field(level,"points",1,999999)?,
                        "requirements":stat_rows(level,"requirements","min",-999999,999999)?,
                        "bonuses":stat_rows(level,"bonuses","delta",-999999,999999)?
                        });
                        if let Some(grants) = level.get("talents") {
                            let grants =
                                grants.as_array().context("升级天赋 talents 必须是列表")?;
                            ensure!(grants.len() <= 32, "升级天赋最多 32 项");
                            let mut seen = std::collections::BTreeSet::new();
                            for grant in grants {
                                let id = text(grant, "talent");
                                if let Some(id) = id.strip_prefix("user:") {
                                    validate_content_id(id)?;
                                } else {
                                    ensure!(
                                        crate::validate::editor_data()["talents"]
                                            .as_array()
                                            .unwrap()
                                            .iter()
                                            .any(|t| t["id"] == id),
                                        "第 {} 级的升级天赋不存在：{id}",
                                        index + 1
                                    );
                                }
                                ensure!(seen.insert(id), "第 {} 级的升级天赋重复：{id}", index + 1);
                                let delta = integer_field(grant, "level", -1, 1)?;
                                ensure!(
                                    delta != 0,
                                    "第 {} 级的天赋等级变化只能为 +1/-1",
                                    index + 1
                                );
                            }
                            normalized["talents"] = json!(grants);
                        }
                        Ok(normalized)
                    })
                    .collect::<Result<Vec<_>>>()?);
            }
        }
        "support" => {
            let trigger = data.get("trigger").and_then(Value::as_str).unwrap_or(
                if data.get("trigger").is_none() {
                    "round_start"
                } else {
                    ""
                },
            );
            ensure!(
                ["round_start", "after_action", "after_hit"].contains(&trigger),
                "trigger 必须是 round_start / after_action / after_hit"
            );
            meta["trigger"] = json!(trigger);
            meta["chance"] = json!(if data.get("chance").is_none() {
                100
            } else {
                integer_field(data, "chance", 0, 100)?
            });
            meta["uses"] = json!(if data.get("uses").is_none() {
                1
            } else {
                integer_field(data, "uses", 1, 99)?
            });
            let effects = data["effects"].as_array().context("effects 必须是列表")?;
            ensure!((1..=16).contains(&effects.len()), "援助必须有 1~16 项效果");
            meta["effects"] = json!(effects.iter().map(|effect| {
                ensure!(["damage","heal","stamina"].contains(&text(effect,"type")), "援助效果 type 必须是 damage / heal / stamina");
                ensure!(["self","enemy"].contains(&text(effect,"target")), "援助效果 target 必须是 self / enemy");
                Ok(json!({"type":effect["type"],"target":effect["target"],"value":integer_field(effect,"value",1,999999)?}))
            }).collect::<Result<Vec<_>>>()?);
        }
        "skill" => {
            let action = data.get("action").and_then(Value::as_str).unwrap_or(
                if data.get("action").is_none() {
                    "attack"
                } else {
                    ""
                },
            );
            ensure!(
                ["attack", "weapon", "ultimate"].contains(&action),
                "action 必须是 attack / weapon / ultimate"
            );
            meta["action"] = json!(action);
            for (key, default, max) in [
                ("power", 0, 999999),
                ("stamina_cost", 0, 9999),
                ("accuracy", 100, 100),
                ("cooldown", 0, 99),
                ("ai_chance", 100, 100),
            ] {
                meta[key] = json!(integer_default(data, key, default, 0, max)?);
            }
            meta["effects"] = combat_effects(data)?;
            if !data["animation"].is_null() {
                let animation = &data["animation"];
                ensure!(animation.is_object(), "animation 必须是对象");
                let frames = animation["frames"]
                    .as_array()
                    .context("animation.frames 必须是列表")?;
                ensure!((1..=120).contains(&frames.len()), "动画必须有1~120帧");
                let frames = frames
                    .iter()
                    .map(|frame| {
                        let name = safe_filename(frame, "动画帧", false)?;
                        ensure!(
                            name.to_ascii_lowercase().ends_with(".png"),
                            "动画帧必须是 PNG 文件"
                        );
                        Ok(json!(name))
                    })
                    .collect::<Result<Vec<_>>>()?;
                meta["animation"] = json!({"frames":frames,"fps":integer_default(animation,"fps",12,1,60)?,"hit_frame":integer_default(animation,"hit_frame",0,0,frames.len() as i64-1)?,"scale":integer_default(animation,"scale",100,50,200)?});
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// References which participate in the 1.4 gameplay content contract.
pub fn gameplay_references(node: &Value) -> Vec<(&str, &'static str, &str)> {
    let mut refs = Vec::new();
    match text(node, "type") {
        "affinity" | "affinity_check" | "character_profile" => {
            refs.push((text(node, "character"), "character", ""))
        }
        "free_trigger" => refs.push((text(node, "when_affinity"), "character", "")),
        "camera" => {
            for id in node["exclude_characters"].as_array().into_iter().flatten() {
                if let Some(id) = id.as_str() {
                    refs.push((id, "character", ""));
                }
            }
        }
        "talent" | "talent_check" => refs.push((text(node, "talent"), "talent", "")),
        "item" | "item_check" | "use_item" => {
            if node["type"] == "use_item" {
                refs.push((text(node, "item"), "item", "consumable"));
                return refs;
            }
            let category = node["category"].as_str().unwrap_or(text(node, "kind"));
            refs.push((
                text(node, "item"),
                if category == "book" {
                    "martial"
                } else {
                    "item"
                },
                category,
            ));
        }
        "custom_shop" => {
            if let Some(items) = node["items"].as_array() {
                for item in items {
                    let category = text(item, "category");
                    refs.push((
                        text(item, "item"),
                        if category == "book" {
                            "martial"
                        } else {
                            "item"
                        },
                        category,
                    ));
                }
            }
        }
        "combat" => {
            for field in [
                "player_supports",
                "enemy_supports",
                "player_skills",
                "enemy_skills",
            ] {
                if let Some(items) = node[field].as_array() {
                    for item in items {
                        if let Some(raw) = item.as_str() {
                            refs.push((
                                raw,
                                if field.ends_with("skills") {
                                    "skill"
                                } else {
                                    "support"
                                },
                                "",
                            ));
                        }
                    }
                }
            }
        }
        "reward" | "result_screen" | "activity" => {
            for field in ["entries", "success_rewards", "failure_rewards"] {
                if let Some(items) = node[field].as_array() {
                    for item in items.iter().filter(|i| i["kind"] == "affinity") {
                        refs.push((text(item, "key"), "character", ""));
                    }
                    for item in items.iter().filter(|i| i["kind"] == "talent") {
                        refs.push((text(item, "key"), "talent", ""));
                    }
                    for item in items.iter().filter(|i| i["kind"] == "item") {
                        let category = text(item, "category");
                        refs.push((
                            text(item, "key"),
                            if category == "book" {
                                "martial"
                            } else {
                                "item"
                            },
                            category,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    refs
}

pub fn gameplay_defaults(kind: &str) -> Value {
    match kind {
        "talent" => json!({"description":""}),
        "item" => json!({"description":"","category":"misc","cost":100}),
        "martial" => {
            json!({"description":"","cost":100,"levels":[{"points":10,"requirements":[],"bonuses":[{"stat":"internal","delta":1}]}]})
        }
        "support" => {
            json!({"description":"","trigger":"round_start","chance":100,"uses":3,"effects":[{"type":"heal","target":"self","value":10}]})
        }
        "skill" => {
            json!({"description":"","action":"attack","power":20,"stamina_cost":5,"accuracy":100,"cooldown":0,"ai_chance":100,"effects":[]})
        }
        _ => json!({}),
    }
}
/// Present event result item actions in the same shape as story item nodes.
pub fn scenario_gameplay_nodes(config: &Value) -> Vec<Value> {
    fn collect(value: &Value, result: &mut Vec<Value>) {
        match value {
            Value::Object(object) => {
                if value["type"] == "item" && text(value, "key").starts_with("user:") {
                    result.push(json!({"type":"item","kind":value["kind"],"item":value["key"]}));
                }
                if value["type"] == "talent" && text(value, "key").starts_with("user:") {
                    result.push(json!({"type":"talent","talent":value["key"]}));
                }
                if (value["type"] == "affinity" || value["source"] == "affinity")
                    && text(value, "key").starts_with("user:")
                {
                    result.push(json!({"type":"affinity","character":value["key"],"delta":0}));
                }
                for child in object.values() {
                    collect(child, result);
                }
            }
            Value::Array(array) => {
                for child in array {
                    collect(child, result);
                }
            }
            _ => {}
        }
    }
    let mut nodes = Vec::new();
    collect(config, &mut nodes);
    nodes
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
    if let Some(frames) = meta["animation"]["frames"].as_array() {
        for frame in frames {
            add(frame);
        }
    }
    add(&meta["intro"]["image"]);
    add(&meta["avatar"]);
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
        if meta["extends"].is_string() {
            p.insert("extends".into(), meta["extends"].clone());
        }
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
        for (key, default) in [("mood_x", 50), ("mood_y", 90)] {
            if !meta[key].is_null() && meta[key] != default {
                p.insert(key.into(), meta[key].clone());
            }
        }
        if meta["intro"].is_object() {
            p.insert("intro".into(), meta["intro"].clone());
        }
        for key in [
            "avatar",
            "initial_affinity",
            "biography",
            "initially_known",
            "biography_pages",
        ] {
            if !meta[key].is_null() {
                p.insert(key.into(), meta[key].clone());
            }
        }
        for key in ["portrait_layouts", "portrait_reference"] {
            if !meta[key].is_null() {
                p.insert(key.into(), meta[key].clone());
            }
        }
    }
    if GAMEPLAY_TYPES.contains(&text(meta, "type")) {
        for key in [
            "description",
            "category",
            "cost",
            "levels",
            "trigger",
            "chance",
            "uses",
            "effects",
            "use_in",
            "action",
            "power",
            "stamina_cost",
            "accuracy",
            "cooldown",
            "ai_chance",
            "animation",
        ] {
            if !meta[key].is_null() {
                p.insert(key.into(), meta[key].clone());
            }
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
        if kind == "talent" {
            main.unwrap_or(metadata_real)
        } else {
            main.ok_or_else(|| anyhow::anyhow!("用户内容主文件不存在"))?
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portrait_ids_have_the_same_bounds_as_character_metadata() {
        for id in ["normal", "Happy_2", "abcdefghijklmnopqrstuvwxyzabcdef"] {
            validate_portrait_id(id).unwrap();
        }
        for id in [
            "",
            "26_04_23_17_53_52",
            "中文",
            "face-smile",
            "abcdefghijklmnopqrstuvwxyzabcdefg",
            "happy\n",
        ] {
            assert!(validate_portrait_id(id).is_err(), "accepted {id:?}");
        }
    }
    #[test]
    fn normalize_character_and_payload() {
        let m=normalize_content_metadata(&json!({"schema":1,"id":"test.person","type":"character","name":" 名字 ","files":{"main":"main.png"},"portraits":{"happy":"happy.png"},"scale":500,"intro":{"name":" 人物 ","text":" 介绍 ","image_scale":"130"}})).unwrap();
        assert_eq!(m["portraits"]["normal"], "main.png");
        assert_eq!(m["scale"], 500);
        assert_eq!(m["intro"]["name"], "人物");
        assert_eq!(m["combat_attack"], "main.png");
        let p = content_metadata_payload(&m);
        assert!(p.get("combat_attack").is_none());
        assert_eq!(p["scale"], 500);
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
