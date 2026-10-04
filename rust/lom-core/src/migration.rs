//! Explicit, lossless authoring migrations. Unsupported old identities are never guessed.
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const MAX_MIGRATION_JSON_BYTES: u64 = 4 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MigrationResult {
    pub kind: String,
    pub document: Value,
    pub changed: bool,
    pub from_version: u32,
    pub to_version: u32,
    pub steps: Vec<String>,
}
fn result(kind: &str, document: Value, version: u32, steps: Vec<String>) -> MigrationResult {
    MigrationResult {
        kind: kind.into(),
        document,
        changed: !steps.is_empty(),
        from_version: version,
        to_version: version,
        steps,
    }
}
fn require_version(value: &Value, field: &str, version: u32) -> Result<()> {
    ensure!(
        value.as_f64() == Some(version as f64),
        "无法迁移 {field}={value}：当前工具仅支持版本 {version}"
    );
    Ok(())
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
pub fn migrate_manifest(document: &Value) -> Result<MigrationResult> {
    let mut out = document
        .as_object()
        .context("manifest 顶层必须是 JSON 对象")?
        .clone();
    let explicit = out.contains_key("package_format");
    ensure!(
        explicit || out.contains_key("format"),
        "manifest 缺少 package_format/format，无法判断来源版本"
    );
    require_version(
        &out[if explicit { "package_format" } else { "format" }],
        "package_format",
        3,
    )?;
    if explicit && out.contains_key("format") {
        require_version(&out["format"], "format", 3)?;
    }
    let mut steps = vec![];
    for (field, step) in [
        ("package_format", "upgrade package_format to 3"),
        ("format", "upgrade legacy format to 3"),
    ] {
        if !out.contains_key(field) {
            out.insert(field.into(), json!(3));
            steps.push(step.into());
        }
    }
    for (field, version) in [("story_schema", 2), ("content_schema", 1)] {
        if let Some(value) = out.get(field) {
            require_version(value, field, version)?;
        } else {
            out.insert(field.into(), json!(version));
            steps.push(format!("add {field}"));
        }
    }
    ensure!(
        out.get("campaign_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty()),
        "manifest 缺少稳定 campaign_id；旧项目不会自动补 ID"
    );
    ensure!(
        out.get("campaign").and_then(|v| v.get("new_game")) == Some(&json!(true)),
        "manifest 必须包含 campaign.new_game=true"
    );
    Ok(result("manifest", Value::Object(out), 3, steps))
}
pub fn migrate_content(document: &Value) -> Result<MigrationResult> {
    let mut out = document
        .as_object()
        .context("content 顶层必须是 JSON 对象")?
        .clone();
    ensure!(
        out.contains_key("content_schema") || out.contains_key("schema"),
        "content.json 缺少 content_schema/schema"
    );
    for key in ["content_schema", "schema"] {
        if let Some(v) = out.get(key) {
            require_version(v, key, 1)?;
        }
    }
    let mut steps = vec![];
    if !out.contains_key("content_schema") {
        out.insert("content_schema".into(), json!(1));
        steps.push("add content_schema".into());
    }
    if !out.contains_key("schema") {
        out.insert("schema".into(), json!(1));
        steps.push("add legacy schema".into());
    }
    Ok(result("content", Value::Object(out), 1, steps))
}
fn integer_or_one(value: &Value) -> Result<i64> {
    if !truthy(value) {
        return Ok(1);
    }
    if let Some(value) = value.as_i64() {
        return Ok(value.max(1));
    }
    if let Some(value) = value.as_f64() {
        ensure!(
            value.is_finite() && value < i64::MAX as f64,
            "Battle 人数过大，无法无损迁移"
        );
        return Ok((value as i64).max(1));
    }
    if let Some(value) = value.as_str() {
        let value = value.trim();
        if let Ok(value) = value.parse::<i64>() {
            return Ok(value.max(1));
        }
        let digits = value.trim_start_matches(['+', '-']);
        ensure!(
            value.starts_with('-')
                || digits.is_empty()
                || !digits.bytes().all(|b| b.is_ascii_digit()),
            "Battle 人数过大，无法无损迁移"
        );
    }
    Ok(1)
}
pub fn migrate_story(document: &Value) -> Result<MigrationResult> {
    migrate_story_with_metadata(document, &crate::validate::editor_data()["dice_meta"])
}
/// Metadata override supports reproducible migration audits without reading game files.
pub fn migrate_story_with_metadata(document: &Value, metadata: &Value) -> Result<MigrationResult> {
    ensure!(document.is_object(), "story 顶层必须是 JSON 对象");
    require_version(&document["story_schema"], "story_schema", 2)?;
    let nodes = document.get("nodes").and_then(Value::as_array);
    ensure!(
        !document.as_object().unwrap().contains_key("battle_presets")
            && !nodes
                .into_iter()
                .flatten()
                .any(|n| n.get("preset").is_some()),
        "旧 Combat/Battle 预设不能自动迁移；请明确配置"
    );
    let mut out = document.clone();
    let mut changed = [false; 7];
    let mut index_maps = BTreeMap::new();
    let mut has_dice = false;
    if let Some(nodes) = out.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            let Some(o) = node.as_object_mut() else {
                continue;
            };
            match o.get("type").and_then(Value::as_str) {
                Some("combat") => {
                    changed[0] |= o.remove("display_name").is_some();
                    for field in ["ultimate_one", "ultimate_two", "ultimate_three"] {
                        changed[1] |= o.remove(field).is_some();
                    }
                    for field in [
                        "confucianism",
                        "buddhism",
                        "taoism",
                        "xingyi",
                        "strategy_level",
                    ] {
                        changed[2] |= o.remove(field).is_some();
                    }
                    if o.get("background").is_none_or(|v| !truthy(v)) {
                        o.insert("background".into(), json!("center"));
                        changed[3] = true;
                    }
                }
                Some("battle") => {
                    let valid = |field: &str, v: &Value| {
                        crate::validate::node_schema()[field]
                            .as_array()
                            .unwrap()
                            .contains(v)
                    };
                    for (singular, plural) in [
                        ("friend_faction", "friend_factions"),
                        ("enemy_faction", "enemy_factions"),
                    ] {
                        if let Some(faction) = o.remove(singular) {
                            changed[5] = true;
                            let mut rows = o
                                .get(plural)
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default();
                            if faction.as_str().is_some_and(|v| !v.is_empty()) {
                                if !valid("battle_factions", &faction) {
                                    changed[6] = true;
                                } else if !rows.contains(&faction) {
                                    rows.push(faction);
                                }
                            }
                            o.insert(plural.into(), json!(rows));
                        }
                    }
                    for field in ["friend_factions", "enemy_factions"] {
                        let Some(old) = o.get(field).and_then(Value::as_array).cloned() else {
                            continue;
                        };
                        let mut rows = vec![];
                        let mut seen = BTreeSet::new();
                        for item in &old {
                            let (id, people) = if item.is_object() {
                                (&item["id"], integer_or_one(&item["people"])?)
                            } else {
                                (item, 1)
                            };
                            if !valid("battle_factions", id)
                                || !seen.insert(id.as_str().unwrap_or("").to_owned())
                            {
                                changed[6] = true;
                                continue;
                            }
                            rows.push(json!({"id":id,"people":people}));
                        }
                        let side = if field.starts_with("friend") {
                            "friend"
                        } else {
                            "enemy"
                        };
                        let legacy_value = o.get(&format!("{side}_people"));
                        if let Some(value) =
                            legacy_value.filter(|v| v.is_number() && v.as_i64().is_none())
                        {
                            let repr = value.to_string();
                            ensure!(
                                repr.starts_with('-') || repr.contains(['.', 'e', 'E']),
                                "Battle 人数过大，无法无损迁移"
                            );
                        }
                        let legacy = legacy_value.and_then(Value::as_i64);
                        let named = o
                            .get(&format!("{side}_characters"))
                            .and_then(Value::as_array)
                            .map_or(0, Vec::len) as i64;
                        let current = rows.iter().try_fold(0i64, |sum, v| {
                            sum.checked_add(v["people"].as_i64().unwrap())
                                .context("Battle 人数总和过大，无法无损迁移")
                        })?;
                        if let Some(legacy) = legacy.filter(|&v| v > current.saturating_add(named))
                        {
                            if !rows.is_empty() && old.iter().all(|v| v.get("people").is_none()) {
                                rows[0]["people"] = json!(rows[0]["people"]
                                    .as_i64()
                                    .unwrap()
                                    .saturating_add(legacy - current - named));
                            }
                        }
                        changed[5] |= rows != old;
                        o.insert(field.into(), json!(rows));
                    }
                    for field in ["friend_people", "enemy_people"] {
                        changed[5] |= o.remove(field).is_some();
                    }
                    for field in ["friend_characters", "enemy_characters"] {
                        if let Some(old) = o.get_mut(field).and_then(Value::as_array_mut) {
                            let len = old.len();
                            old.retain(|v| valid("battle_characters", v));
                            changed[4] |= old.len() != len;
                        }
                    }
                }
                Some("dice") if o.contains_key("check") => {
                    let id = o.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
                    let map = inline_legacy_dice(node, metadata)?;
                    index_maps.insert(id, map);
                    has_dice = true;
                }
                _ => {}
            }
        }
    }
    let mut steps = vec![];
    for (changed, step) in changed.into_iter().zip([
        "remove obsolete combat display_name; selected character now owns the name",
        "remove unused combat ultimate slots; original Combat never reads them",
        "remove combat proficiency stats; original Combat writes them from talents",
        "add explicit combat background=center; background is independent of character",
        "remove Battle named characters without a verified spawnable NPC prefab",
        "convert Battle faction to an attachable list; roster is no longer replaced",
        "drop Battle factions that have no official BattleLevel troop presets",
    ]) {
        if changed {
            steps.push(step.into());
        }
    }
    if has_dice {
        let re = regex::Regex::new(r"^([A-Za-z0-9_-]+)\.options\.0\.band_texts\.(\d+)$")?;
        if let Some(translations) = out
            .get_mut("localization")
            .and_then(|v| v.get_mut("translations"))
            .and_then(Value::as_object_mut)
        {
            for catalog in translations.values_mut().filter_map(Value::as_object_mut) {
                let mut replacements = BTreeMap::new();
                let mut removals = vec![];
                for (path, value) in catalog.iter() {
                    let Some(caps) = re.captures(path) else {
                        continue;
                    };
                    let Some(map) = index_maps.get(&caps[1]) else {
                        continue;
                    };
                    let old: usize = caps[2].parse()?;
                    let new = map.get(&old).context("旧骰子翻译路径超出结果带范围")?;
                    let key = format!("{}.bands.{new}.text", &caps[1]);
                    ensure!(
                        !catalog.contains_key(&key) && !replacements.contains_key(&key),
                        "旧骰子翻译迁移后路径冲突：{key}"
                    );
                    replacements.insert(key, value.clone());
                    removals.push(path.clone());
                }
                for path in removals {
                    catalog.remove(&path);
                }
                catalog.extend(replacements);
            }
        }
        steps.push("inline original dice checkpoints into direct dice parameters".into());
    }
    Ok(result("story", out, 2, steps))
}
fn inline_legacy_dice(node: &mut Value, metadata: &Value) -> Result<BTreeMap<usize, usize>> {
    let check = node["check"].as_str().context("旧骰子检查点无效")?;
    let meta = metadata
        .get(check)
        .filter(|v| v.is_object())
        .context("旧骰子检查点缺少元数据，无法安全展开")?;
    let source = meta["bands"]
        .as_array()
        .filter(|a| (2..=4).contains(&a.len()))
        .context("旧骰子结果带无效")?;
    let maximum = meta
        .get("max")
        .unwrap_or(&json!(99))
        .as_i64()
        .filter(|v| (1..=9999).contains(v))
        .context("旧骰子随机范围无效")?;
    let option = node["options"]
        .as_array()
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or(json!({}));
    ensure!(option.is_object(), "旧骰子选项无效");
    let re = regex::Regex::new(r"(<=|>=|<|>|=)\s*(-?\d+)")?;
    let mut parsed = vec![];
    for (index, band) in source.iter().enumerate() {
        let condition = band["cond"].as_str().unwrap_or("");
        let caps = re.captures(condition).context("旧骰子条件无法安全展开")?;
        let op = &caps[1];
        let raw: i64 = caps[2].parse()?;
        let low = if ["<", "<="].contains(&op) {
            -1_000_000_000
        } else {
            raw.checked_add(i64::from(op == ">"))
                .context("骰子条件越界")?
        };
        let high = if [">", ">="].contains(&op) {
            1_000_000_000
        } else {
            raw.checked_sub(i64::from(op == "<"))
                .context("骰子条件越界")?
        };
        let value = option["band_texts"]
            .as_array()
            .and_then(|v| v.get(index))
            .unwrap_or(&band["text"]);
        let text = if !truthy(value) {
            "结果".into()
        } else {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        };
        parsed.push((low, high, text, index));
    }
    parsed.sort_by_key(|v| (v.0, v.1));
    let mut map = BTreeMap::new();
    let mut bands = vec![];
    for (index, (_, high, text, old)) in parsed.iter().enumerate() {
        map.insert(*old, index);
        let key = if index == 0 {
            "goto_失败"
        } else if index == parsed.len() - 1 && parsed.len() >= 3 {
            "goto_大成功"
        } else {
            "goto_成功"
        };
        let mut row = json!({"text":text,"goto":option[key]});
        if index < parsed.len() - 1 {
            ensure!(
                *high >= 0 && *high < maximum,
                "旧骰子可能含动态加值，无法无损转换，请手动改为直接骰子参数"
            );
            row["upper"] = json!(high);
        }
        bands.push(row);
    }
    let out = node.as_object_mut().unwrap();
    out.remove("check");
    out.remove("options");
    out.extend([
        ("max".into(), json!(maximum)),
        ("header".into(), json!("命运检定")),
        ("bonus".into(), json!(0)),
        ("bands".into(), json!(bands)),
    ]);
    Ok(map)
}
pub fn migrate_document(document: &Value, kind: &str) -> Result<MigrationResult> {
    match kind {
        "manifest" => migrate_manifest(document),
        "story" => migrate_story(document),
        "content" => migrate_content(document),
        _ => bail!("未知迁移类型：{kind}"),
    }
}
fn read_document(path: &Path) -> Result<(Vec<u8>, Value)> {
    ensure!(!path.is_symlink(), "拒绝迁移符号链接文件");
    let mut file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_MIGRATION_JSON_BYTES,
        "文件超过 4 MiB，拒绝自动迁移"
    );
    let mut bytes = vec![];
    use std::io::Read;
    std::io::Read::by_ref(&mut file)
        .take(MAX_MIGRATION_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_MIGRATION_JSON_BYTES,
        "文件超过 4 MiB，拒绝自动迁移"
    );
    let text = std::str::from_utf8(&bytes)?.trim_start_matches('\u{feff}');
    let document: Value = serde_json::from_str(text)?;
    ensure!(document.is_object(), "JSON 顶层必须是对象");
    Ok((bytes, document))
}
fn backup_path(source: &Path, bytes: &[u8], label: &str) -> Result<PathBuf> {
    let base = format!(
        "{}.pre-migration-{label}.bak",
        source.file_name().context("无效文件名")?.to_string_lossy()
    );
    for index in 0..1000 {
        let path = source.with_file_name(if index == 0 {
            base.clone()
        } else {
            format!("{base}.{index}")
        });
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if !path.is_symlink() && fs::read(&path).is_ok_and(|old| old == bytes) {
                    return Ok(path);
                }
            }
            Err(e) => return Err(e).context("无法创建迁移备份"),
        }
    }
    bail!("迁移备份文件过多，请先整理")
}
pub type DocumentValidator<'a> = &'a dyn Fn(&Value) -> Result<()>;
pub fn migrate_json_file(
    path: &Path,
    kind: &str,
    validator: Option<DocumentValidator<'_>>,
) -> Result<(MigrationResult, Option<PathBuf>)> {
    let (original, document) = read_document(path)?;
    let result = migrate_document(&document, kind)?;
    if let Some(validate) = validator {
        validate(&result.document)?;
    }
    if !result.changed {
        return Ok((result, None));
    }
    let mut payload = serde_json::to_vec_pretty(&result.document)?;
    payload.push(b'\n');
    let backup = backup_path(path, &original, &format!("v{}", result.from_version))?;
    crate::project::atomic_write(path, &payload)?;
    Ok((result, Some(backup)))
}
pub fn restore_migration_backup(source: &Path, backup: &Path) -> Result<PathBuf> {
    let (original, _) = read_document(backup)?;
    ensure!(!source.is_symlink(), "拒绝恢复到符号链接文件");
    let current = if source.exists() {
        fs::read(source)?
    } else {
        vec![]
    };
    let recovery = backup_path(source, &current, "before-recovery")?;
    crate::project::atomic_write(source, &original)?;
    Ok(recovery)
}
