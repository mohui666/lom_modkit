//! Native validation of the v3 package and all 63 story node contracts.
//! The declarative field table is shared with the native editor. Semantic rules
//! are implemented here; no Python interpreter or subprocess is involved.
use anyhow::{ensure, Context, Result};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

pub fn node_schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!("../data/schema.json")).expect("embedded node schema")
    })
}
pub fn editor_data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/editor_data.json"))
            .expect("embedded official catalog")
    })
}
const SCRIPT_ID: &str = r"^[a-zA-Z0-9_\-]{1,64}$";
const MOD_ID: &str = r"^[a-z0-9_\-]{1,64}$";
const NODE_ID: &str = r"^[a-zA-Z0-9_]+$";
const USER_ID: &str = r"^[a-z][a-z0-9_]{0,31}\.[a-z0-9][a-z0-9_]{0,47}$";
const DICE_GOTOS: [&str; 3] = ["goto_大成功", "goto_成功", "goto_失败"];
fn matches(pattern: &str, s: &str) -> bool {
    Regex::new(pattern).expect("constant regex").is_match(s)
}
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn string(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn has(v: &Value, key: &str) -> bool {
    v.get(key).is_some()
}
fn array(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn text<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or(default)
}
fn member(table: &str, value: &str) -> bool {
    array(&node_schema()[table])
        .iter()
        .any(|x| x.as_str() == Some(value))
}
fn enumeration(tag: &str, value: &Value) -> bool {
    array(&node_schema()["enums"][tag]).contains(value)
}
fn integer(v: &Value) -> bool {
    v.as_number()
        .is_some_and(|n| !n.to_string().contains(['.', 'e', 'E']))
}
fn positive_integer(v: &Value, allow_zero: bool) -> bool {
    integer(v) && !v.to_string().starts_with('-') && (allow_zero || v.to_string() != "0")
}
fn int_range(v: &Value, low: i64, high: i64) -> bool {
    v.as_i64().is_some_and(|n| n >= low && n <= high)
}
fn finite(v: &Value) -> bool {
    integer(v) || v.as_f64().is_some_and(f64::is_finite)
}
fn nonempty(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty())
}
fn nonblank(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn object<'a>(v: &'a Value, label: &str) -> Result<&'a Map<String, Value>> {
    v.as_object()
        .with_context(|| format!("{label}: 必须是对象"))
}
fn allowed(v: &Value, keys: &[&str], label: &str) -> Result<()> {
    for key in object(v, label)?.keys() {
        ensure!(
            keys.contains(&key.as_str()),
            "{label}: 未知字段 \"{key}\"（允许：{}）",
            keys.join("、")
        );
    }
    Ok(())
}
fn require(v: &Value, keys: &[&str], label: &str) -> Result<()> {
    for key in keys {
        ensure!(has(v, key), "{label}: 缺少必填字段 \"{key}\"");
    }
    Ok(())
}
fn user(v: &Value) -> bool {
    v.as_str().is_some_and(|x| x.starts_with("user:"))
}
fn user_ref(v: &Value, label: &str) -> Result<()> {
    ensure!(user(v), "{label}: 必须是 user: 用户内容引用");
    ensure!(
        matches(USER_ID, &string(v)[5..]),
        "{label}: 用户内容引用不合法，必须是小写命名空间.名称，不能含路径字符"
    );
    Ok(())
}
fn type_valid(tag: &str, v: &Value) -> bool {
    match tag {
        "str" => v.is_string(),
        "idstr" => nonempty(v),
        "num" => finite(v),
        "bool" => v.is_boolean(),
        "list" => v.is_array(),
        "talent_level" => v.as_f64().is_some_and(|n| n == 1.0 || n == -1.0),
        "save_button" => v.as_f64().is_some_and(|n| n == 0.0 || n == 1.0),
        "script_id" => v.as_str().is_some_and(|x| matches(SCRIPT_ID, x)),
        _ => enumeration(tag, v),
    }
}
fn range(node: &Value, field: &str, low: f64, high: f64) -> Result<()> {
    if let Some(v) = node.get(field) {
        ensure!(
            v.as_f64().is_some_and(|n| n >= low && n <= high)
                || (high == f64::INFINITY && positive_integer(v, true) && v.as_f64().is_none()),
            "{field} 必须在 {low}~{high} 之间"
        );
    }
    Ok(())
}
fn int_field(node: &Value, field: &str, low: i64, high: i64) -> Result<()> {
    if let Some(v) = node.get(field) {
        ensure!(
            int_range(v, low, high),
            "{field} 必须是 {low}~{high} 的整数"
        );
    }
    Ok(())
}
fn asset_image(v: &Value, strict_prefix: bool) -> Result<()> {
    ensure!(nonblank(v), "image 必须是非空字符串（包内图片路径）");
    let image = string(v);
    let normalized = image.replace('\\', "/");
    ensure!(
        [".png", ".jpg", ".jpeg"]
            .iter()
            .any(|ext| image.to_lowercase().ends_with(ext)),
        "image 必须是包内 assets/ 下的 .png/.jpg/.jpeg 图片"
    );
    let prefix = if strict_prefix { image } else { &normalized };
    ensure!(
        prefix.starts_with("assets/")
            && prefix != "assets/"
            && !normalized.split('/').any(|p| p == ".."),
        "image 必须是包内 assets/ 相对路径（不得指向包外）"
    );
    Ok(())
}
fn version(v: &Value, field: &str) -> Result<(Vec<u32>, Option<Vec<String>>)> {
    let pattern = Regex::new(
        r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$",
    )?;
    let cap = pattern
        .captures(string(v))
        .with_context(|| format!("字段 \"{field}\" 必须是 SemVer（如 0.6.0 或 0.7.0-beta.1）"))?;
    let mut core = Vec::new();
    for index in 1..=3 {
        let value: u32 = cap[index].parse().context("版本数字不能超过 2147483647")?;
        ensure!(
            value <= i32::MAX as u32,
            "字段 \"{field}\" 的版本数字不能超过 2147483647"
        );
        core.push(value);
    }
    let pre = cap
        .get(4)
        .map(|m| m.as_str().split('.').map(str::to_owned).collect::<Vec<_>>());
    if let Some(parts) = &pre {
        for p in parts {
            if p.bytes().all(|x| x.is_ascii_digit()) {
                ensure!(
                    !(p.len() > 1 && p.starts_with('0')) && p.parse::<i32>().is_ok(),
                    "字段 \"{field}\" 的预发布数字必须无前导零且不超过 2147483647"
                );
            }
        }
    }
    Ok((core, pre))
}
fn version_greater(
    left: &(Vec<u32>, Option<Vec<String>>),
    right: &(Vec<u32>, Option<Vec<String>>),
) -> bool {
    if left.0 != right.0 {
        return left.0 > right.0;
    }
    match (&left.1, &right.1) {
        (None, Some(_)) => true,
        (Some(_), None) | (None, None) => false,
        (Some(l), Some(r)) => {
            for (a, b) in l.iter().zip(r) {
                if a == b {
                    continue;
                }
                return match (a.parse::<u32>(), b.parse::<u32>()) {
                    (Ok(x), Ok(y)) => x > y,
                    (Ok(_), Err(_)) => false,
                    (Err(_), Ok(_)) => true,
                    _ => a > b,
                };
            }
            l.len() > r.len()
        }
    }
}
pub fn validate_manifest(manifest: &Value) -> Result<()> {
    manifest_inner(manifest).map_err(|error| anyhow::anyhow!("manifest.json: {error:#}"))
}
fn manifest_inner(m: &Value) -> Result<()> {
    object(m, "顶层必须是 JSON 对象")?;
    let fmt = m.get("package_format").unwrap_or(&m["format"]);
    ensure!(fmt.as_f64()==Some(3.0),"字段 \"package_format\" 必须是 3；v1/v2 包缺少稳定 campaign_id，与 v3 不兼容，请用 1.0.1 或更高版本 Editor 重新导出");
    if has(m, "package_format") && has(m, "format") {
        ensure!(
            m["format"].as_f64() == Some(3.0),
            "字段 \"package_format\" 与旧字段 \"format\" 的声明不一致"
        );
    }
    for (field, value) in [("story_schema", 2.0), ("content_schema", 1.0)] {
        if has(m, field) {
            ensure!(
                m[field].as_f64() == Some(value),
                "字段 \"{field}\" 必须固定为 {value}"
            );
        }
    }
    for field in ["id", "campaign_id"] {
        ensure!(
            matches(MOD_ID, s(m, field)),
            "缺少必填字段 \"{field}\"（稳定 id，规则 [a-z0-9_-]{{1,64}}）"
        );
    }
    for (field, limit) in [
        ("name", 80),
        ("version", 32),
        ("author", 80),
        ("description", 500),
    ] {
        ensure!(
            nonempty(&m[field]),
            "缺少必填字段 \"{field}\"（非空字符串）"
        );
        ensure!(
            s(m, field).chars().count() <= limit,
            "字段 \"{field}\" 不能超过 {limit} 个字符"
        );
        ensure!(
            !matches(r"[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]", s(m, field)),
            "字段 \"{field}\" 必须是单行可见文本，不能含控制、零宽或双向格式字符"
        );
    }
    let mut versions = BTreeMap::new();
    for field in ["min_host_version", "tested_host_version"] {
        if has(m, field) {
            versions.insert(field, version(&m[field], field)?);
        }
    }
    if let (Some(low), Some(high)) = (
        versions.get("min_host_version"),
        versions.get("tested_host_version"),
    ) {
        ensure!(
            !version_greater(low, high),
            "min_host_version 不能高于 tested_host_version"
        );
    }
    for field in ["game_version", "tested_game_version"] {
        if has(m, field) {
            ensure!(
                matches(r"^[0-9A-Za-z][0-9A-Za-z._+\-]{0,63}$", s(m, field)),
                "字段 \"{field}\" 必须是 1~64 位版本标识（字母、数字、.-_+）"
            );
        }
    }
    if has(m, "game_version") && has(m, "tested_game_version") {
        ensure!(
            s(m, "game_version").to_lowercase() == s(m, "tested_game_version").to_lowercase(),
            "game_version 与 tested_game_version 不能互相矛盾"
        );
    }
    ensure!(
        matches(SCRIPT_ID, s(m, "entry")),
        "缺少必填字段 \"entry\"（入口剧情脚本 id）"
    );
    let campaign = &m["campaign"];
    object(campaign, "缺少必填字段 campaign")?;
    ensure!(
        campaign["new_game"] == true,
        "字段 \"campaign.new_game\" 必须固定为 true"
    );
    if has(campaign, "disable_official_events") {
        ensure!(
            campaign["disable_official_events"].is_boolean(),
            "字段 \"campaign.disable_official_events\" 必须是布尔值"
        );
    }
    if has(campaign, "triggers") {
        ensure!(
            campaign["triggers"].is_array(),
            "字段 \"campaign.triggers\" 必须是数组"
        );
    }
    for (index, trig) in array(&campaign["triggers"]).iter().enumerate() {
        let label = format!("campaign.triggers 第 {} 项", index + 1);
        allowed(
            trig,
            &[
                "type",
                "position",
                "script",
                "when_flag_set",
                "when_flag_clear",
                "when_month",
                "when_stage",
                "when_affinity",
            ],
            &label,
        )?;
        ensure!(
            s(trig, "type") == "position",
            "{label}: 字段 \"type\" 目前只支持 \"position\""
        );
        ensure!(
            member("campaign_positions", s(trig, "position")),
            "{label}: 字段 \"position\" 必须是合法位置 id"
        );
        ensure!(
            matches(SCRIPT_ID, s(trig, "script")),
            "{label}: 字段 \"script\" 必须是合法的同包脚本 id"
        );
        for field in ["when_flag_set", "when_flag_clear"] {
            if has(trig, field) {
                ensure!(
                    trig[field].is_string(),
                    "{label}: 字段 \"{field}\" 必须是字符串"
                );
            }
        }
        for (field, max) in [("when_month", 12), ("when_stage", 3)] {
            if !trig[field].is_null() {
                int_field(trig, field, 1, max).with_context(|| label.clone())?;
            }
        }
        let affinity = &trig["when_affinity"];
        if !affinity.is_null() {
            allowed(
                affinity,
                &["character", "min"],
                &format!("{label}: when_affinity"),
            )?;
            ensure!(
                nonempty(&affinity["character"]),
                "{label}: when_affinity.character 必须是非空字符串（人物 id）"
            );
            ensure!(
                int_range(&affinity["min"], i32::MIN as i64, i32::MAX as i64),
                "{label}: when_affinity.min 必须是 Int32 范围内的整数"
            );
        }
    }
    Ok(())
}
fn fields(node: &Value) -> Result<&str> {
    let ntype = node["type"]
        .as_str()
        .context("缺少必填字段 \"type\"（字符串）")?;
    let spec = &node_schema()["nodes"][ntype];
    ensure!(!spec.is_null(), "未知节点类型 \"{ntype}\"");
    let required = spec["required"].as_object().expect("required table");
    let optional = spec["optional"].as_object().expect("optional table");
    for key in object(node, "节点")?.keys() {
        ensure!(
            ["id", "type", "goto"].contains(&key.as_str())
                || required.contains_key(key)
                || optional.contains_key(key),
            "未知字段 \"{key}\""
        );
    }
    for (name, tag) in required {
        ensure!(has(node, name), "缺少必填字段 \"{name}\"");
        ensure!(
            type_valid(string(tag), &node[name]),
            "字段 \"{name}\" 必须是{}，实际为 {}",
            node_schema()["type_names"][string(tag)],
            node[name]
        );
    }
    for (name, tag) in optional {
        if has(node, name) {
            ensure!(
                type_valid(string(tag), &node[name]),
                "可选字段 \"{name}\" 必须是{}，实际为 {}",
                node_schema()["type_names"][string(tag)],
                node[name]
            );
        }
    }
    if ntype == "free_trigger" {
        int_field(node, "when_affinity_min", i32::MIN as i64, i32::MAX as i64)?;
    }
    Ok(ntype)
}
fn options(
    node: &Value,
    min: usize,
    max: usize,
    required: &[&str],
    optional: &[&str],
) -> Result<()> {
    let opts = array(&node["options"]);
    ensure!(
        (min..=max).contains(&opts.len()),
        "选项数必须在 {min}~{max} 之间，实际为 {}",
        opts.len()
    );
    let keys = required.iter().chain(optional).copied().collect::<Vec<_>>();
    for (index, opt) in opts.iter().enumerate() {
        let label = format!("第 {} 个选项", index + 1);
        allowed(opt, &keys, &label)?;
        require(opt, required, &label)?;
        for key in required {
            ensure!(
                opt[*key].is_string(),
                "{label}: 字段 \"{key}\" 必须是字符串"
            );
        }
    }
    Ok(())
}
fn talents(node: &Value, field: &str, catalog: &Value) -> Result<()> {
    let values = array(&node[field]);
    ensure!(values.len() <= 32, "{field} 最多 32 条");
    let mut seen = BTreeSet::new();
    for (index, talent) in values.iter().enumerate() {
        let label = format!("第 {} 条 {field}", index + 1);
        allowed(talent, &["key", "level"], &label)?;
        let key = s(talent, "key");
        ensure!(matches(SCRIPT_ID, key), "{label} key 无效");
        ensure!(seen.insert(key), "{field} {key} 不得重复");
        let absent = json!({});
        let meta = if catalog["combat_talents"].is_null() {
            &absent
        } else {
            array(&catalog["combat_talents"])
                .iter()
                .find(|x| s(x, "id") == key)
                .with_context(|| format!("{field} {key} 不是原版 CombatSkill"))?
        };
        let default = json!(1);
        let level = talent.get("level").unwrap_or(&default);
        let maximum = meta["max_level"].as_i64().unwrap_or(999);
        ensure!(
            int_range(level, 1, maximum),
            "{field} {key} level 必须在 1~{maximum} 之间"
        );
        if !catalog["combat_talents"].is_null() {
            ensure!(
                array(&meta["effects"])
                    .iter()
                    .any(|x| x["level"] == *level && nonempty(&x["key"])),
                "{field} {key} level {level} 没有已验证的 Combat Effect"
            );
        }
    }
    Ok(())
}
fn combat(n: &Value, catalog: &Value) -> Result<()> {
    let character = &n["character"];
    if user(character) {
        user_ref(character, "character")?;
    } else {
        ensure!(
            matches(SCRIPT_ID, string(character)),
            "character 必须是官方人物 id 或合法 user: 引用"
        );
    }
    ensure!(
        !has(n, "display_name"),
        "display_name 已删除；决斗名称始终由所选人物名称决定"
    );
    for field in [
        "confucianism",
        "buddhism",
        "taoism",
        "xingyi",
        "strategy_level",
    ] {
        ensure!(
            !has(n, field),
            "{field} 已删除；儒学/佛学/道学/形意/战术由「对手决斗技能」的等级写入"
        );
    }
    let background = text(n, "background", "center");
    ensure!(
        matches(r"^[a-zA-Z0-9_\- ]{1,64}$", background),
        "background 必须是安全的官方场景背景 id"
    );
    if !catalog["views"].is_null() {
        ensure!(
            array(&catalog["views"])
                .iter()
                .any(|v| s(v, "id") == background || v.as_str() == Some(background)),
            "background {background:?} 不在 editor_data.views 官方背景清单中"
        );
    }
    for (field, lo, hi) in [
        ("max_health", 1, 10_000_000),
        ("health", 0, 10_000_000),
        ("max_stamina", 0, 100_000),
        ("stamina", 0, 100_000),
        ("weapon_hit_addition", 0, 10_000),
        ("attack_damage_addition", -100_000, 100_000),
        ("defence_addition", -100_000, 100_000),
        ("weapon_damage_addition", -100_000, 100_000),
        ("attack_dice_addition", -1000, 1000),
        ("weapon_dice_addition", -1000, 1000),
        ("player_max_health", 1, 10_000_000),
        ("player_health", 0, 10_000_000),
        ("player_max_stamina", 0, 100_000),
        ("player_stamina", 0, 100_000),
    ] {
        int_field(n, field, lo, hi)?;
    }
    for field in [
        "stamina_power",
        "strength",
        "internal",
        "dexterity",
        "talking",
        "defence",
        "sword",
        "fist",
        "martial_weapon",
        "mental",
        "weapon_poison_value",
        "weapon_paralyzed_value",
        "poison_resist",
        "paralyzed_resist",
        "disposition",
        "behaviour",
        "karma",
        "training",
        "player_stamina_power",
        "player_strength",
        "player_internal",
        "player_dexterity",
        "player_talking",
        "player_defence",
        "player_sword",
        "player_fist",
        "player_martial_weapon",
        "player_mental",
        "player_poison_resist",
        "player_paralyzed_resist",
        "player_disposition",
        "player_behaviour",
        "player_karma",
        "player_training",
    ] {
        int_field(n, field, 0, 10_000)?;
    }
    for field in [
        "attack_parry_addition",
        "block_dodge_addition",
        "block_parry_addition",
    ] {
        range(n, field, -1.0, 1.0)?;
    }
    range(n, "ultimate_damage_rate", 0.0, 100.0)?;
    for field in [
        "talk_rate",
        "attack_rate",
        "weapon_rate",
        "ultimate_rate",
        "block_rate",
    ] {
        range(n, field, 0.0, 1.0)?;
    }
    talents(n, "talents", catalog)?;
    talents(n, "player_talents", catalog)?;
    Ok(())
}
fn battle(n: &Value) -> Result<()> {
    for field in ["friend_faction", "enemy_faction"] {
        ensure!(!has(n, field), "{field} 已改为可附加列表 {field}s");
    }
    for field in ["friend_people", "enemy_people"] {
        ensure!(
            !has(n, field),
            "{field} 已取消；各方总人数由各阵营 people 与具名角色相加"
        );
    }
    for side in ["friend", "enemy"] {
        let field = format!("{side}_factions");
        let mut seen = BTreeSet::new();
        let mut total: u128 = 0;
        for (index, faction) in array(&n[&field]).iter().enumerate() {
            let label = format!("{field} 第 {} 项", index + 1);
            allowed(faction, &["id", "people"], &label)?;
            let id = s(faction, "id");
            ensure!(matches(SCRIPT_ID, id), "{label} id 必须是安全的原版阵营 id");
            ensure!(
                member("battle_factions", id),
                "{label} {id:?} 没有对应的原版 BattleLevel"
            );
            ensure!(seen.insert(id), "{field} 不得重复 {id}");
            ensure!(
                positive_integer(&faction["people"], false),
                "{label} people 必须是至少 1 的整数"
            );
            total += 1;
        }
        let field = format!("{side}_characters");
        let mut seen = BTreeSet::new();
        for (index, c) in array(&n[&field]).iter().enumerate() {
            ensure!(
                member("battle_characters", string(c)),
                "{field} 第 {} 项不是已验证的官方 Battle 人物",
                index + 1
            );
            ensure!(
                seen.insert(string(c).to_lowercase()),
                "{field} 不得重复添加人物 {c}"
            );
            total += 1;
        }
        ensure!(
            total >= 1,
            "{side} 总人数必须至少 1（各阵营 people 与具名角色相加）"
        );
        int_field(n, &format!("{side}_health"), 1, 10_000_000)?;
    }
    if has(n, "title") {
        ensure!(nonblank(&n["title"]), "title 不能为空或纯空白");
        ensure!(
            !s(n, "title").contains([';', '=', ',', '\n', '\r']),
            "title 不能包含分隔符 ; = , 或换行"
        );
    }
    Ok(())
}
fn rewards(entries: &Value) -> Result<()> {
    let entries = entries.as_array().context("entries 必须是数组")?;
    ensure!((1..=32).contains(&entries.len()), "entries 必须有 1~32 条");
    for (index, entry) in entries.iter().enumerate() {
        let label = format!("第 {} 条奖励", index + 1);
        let kind = s(entry, "kind");
        ensure!(
            enumeration("reward_kind", &entry["kind"]),
            "{label}: kind 必须是 stat/affinity/talent/item/flag"
        );
        let mut keys = vec!["kind", "key"];
        if kind != "flag" {
            keys.push("amount");
        }
        if kind == "item" {
            keys.push("category");
        }
        allowed(entry, &keys, &label)?;
        ensure!(nonempty(&entry["key"]), "{label}: key 必须是非空 id");
        if kind == "flag" {
            continue;
        }
        ensure!(finite(&entry["amount"]), "{label}: amount 必须是有限数值");
        if kind == "talent" {
            ensure!(
                type_valid("talent_level", &entry["amount"]),
                "{label}: talent amount 只能是 1 或 -1"
            );
        }
        if kind == "item" {
            ensure!(
                positive_integer(&entry["amount"], false),
                "{label}: item amount 必须是正整数"
            );
            ensure!(
                enumeration("item_kind", &entry["category"]),
                "{label}: item category 必须是 book/misc/special"
            );
        }
    }
    Ok(())
}
fn custom_shop(n: &Value) -> Result<()> {
    let items = array(&n["items"]);
    ensure!((1..=64).contains(&items.len()), "items 必须有 1~64 条");
    let default = json!(0);
    ensure!(
        int_range(n.get("discount").unwrap_or(&default), 0, 1),
        "discount 只能是 0（原价）或 1（原版统一折扣）"
    );
    let mut seen = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let label = format!("第 {} 件商品", index + 1);
        allowed(item, &["category", "item", "count", "condition"], &label)
            .context("原版没有公开的逐商品自定义价格接口")?;
        ensure!(
            enumeration("shop_item_kind", &item["category"]),
            "{label}: category 必须是 book/misc/special"
        );
        ensure!(
            nonempty(&item["item"]),
            "{label}: item 必须是非空原版物品 id"
        );
        int_field(item, "count", 1, 9999)?;
        ensure!(
            seen.insert((s(item, "category"), s(item, "item"))),
            "{label}: 同一类别和 item id 不能重复"
        );
        let c = &item["condition"];
        if c.is_null() {
            continue;
        }
        allowed(
            c,
            &["source", "key", "invert"],
            &format!("{label}: condition"),
        )?;
        ensure!(
            enumeration("shop_condition_source", &c["source"]),
            "{label}: condition.source 必须是 mod/condition"
        );
        ensure!(nonempty(&c["key"]), "{label}: condition.key 必须是非空 id");
        if has(c, "invert") {
            ensure!(
                c["invert"].is_boolean(),
                "{label}: condition.invert 必须是布尔值"
            );
        }
    }
    Ok(())
}
fn dice(n: &Value, catalog: &Value) -> Result<()> {
    let legacy = has(n, "check") || has(n, "options");
    let direct = has(n, "max") || has(n, "header") || has(n, "bands");
    ensure!(legacy!=direct,"必须使用直接参数 max/header/bands；旧 check/options 只作为导入兼容格式，不能与直接参数混用");
    if legacy {
        require(n, &["check", "options"], "旧格式必须同时包含 check/options")?;
        options(n, 1, 1, &DICE_GOTOS, &["band_texts"])?;
        let check = s(n, "check");
        let meta = &catalog["dice_meta"][check];
        ensure!(!meta.is_null(),"骰子检查点 \"{check}\" 缺少官方元数据（骰子范围与结果带未知，游戏内骰子菜单会因此崩溃）。请改用编辑器直接参数。");
        let count = array(&meta["bands"]).len();
        ensure!(count > 0, "骰子检查点 \"{check}\" 的官方元数据没有结果带");
        let opt = &n["options"][0];
        let band_texts = &opt["band_texts"];
        if !band_texts.is_null() {
            ensure!(band_texts.is_array(), "可选字段 \"band_texts\" 必须是数组");
            ensure!(
                array(band_texts).len() == count,
                "\"band_texts\" 条数必须等于检查点结果带数"
            );
            for value in array(band_texts) {
                ensure!(nonempty(value), "\"band_texts\" 每条必须是非空字符串");
            }
        }
        if count < 3 {
            ensure!(nonempty(&opt["goto_成功"]), "必填字段 \"goto_成功\"");
        } else {
            ensure!(nonempty(&opt["goto_大成功"]), "必填字段 \"goto_大成功\"");
        }
        ensure!(nonempty(&opt["goto_失败"]), "必填字段 \"goto_失败\"");
        return Ok(());
    }
    require(n, &["max", "header", "bands"], "直接参数")?;
    ensure!(int_range(&n["max"], 1, 9999), "max 必须是 1~9999 的整数");
    int_field(n, "bonus", -9999, 9999)?;
    ensure!(
        nonblank(&n["header"]) && s(n, "header").chars().count() <= 80,
        "header 必须是 1~80 字符的检定标题"
    );
    let bands = array(&n["bands"]);
    ensure!(
        (2..=4).contains(&bands.len()),
        "bands 必须有 2~4 个结果分段"
    );
    let bonus = n["bonus"].as_i64().unwrap_or(0);
    let maximum = n["max"].as_i64().unwrap();
    let mut previous = None;
    for (index, band) in bands.iter().enumerate() {
        let label = format!("第 {} 个结果分段", index + 1);
        let final_band = index + 1 == bands.len();
        let keys: &[&str] = if final_band {
            &["text", "goto"]
        } else {
            &["upper", "text", "goto"]
        };
        allowed(band, keys, &label)?;
        ensure!(nonblank(&band["text"]), "{label}: text 必须是非空结果文字");
        ensure!(nonempty(&band["goto"]), "{label}: goto 必须是节点 id");
        if !final_band {
            let upper = band["upper"]
                .as_i64()
                .with_context(|| format!("{label}: upper 必须是整数"))?;
            ensure!(
                upper >= bonus && upper < maximum + bonus,
                "{label}: upper 必须在可投出的总点数 {bonus}~{} 之间，且要给后一档留出点数",
                maximum + bonus
            );
            if let Some(old) = previous {
                ensure!(upper > old, "{label}: upper 必须严格递增");
            }
            previous = Some(upper);
        }
    }
    Ok(())
}
fn branch(n: &Value) -> Result<()> {
    let source = text(n, "source", "mod");
    let cases = array(&n["cases"]);
    ensure!(!cases.is_empty(), "cases 至少需要 1 个分支");
    let (required, forbidden) = if source == "stat" {
        ("stat", "flag")
    } else {
        ("flag", "stat")
    };
    ensure!(
        nonempty(&n[required]),
        "source=\"{source}\" 时必填字段 \"{required}\""
    );
    ensure!(
        !has(n, forbidden),
        "source=\"{source}\" 时不支持字段 \"{forbidden}\""
    );
    let mut seen = BTreeSet::new();
    let numeric = ["stat", "flag_value"].contains(&source);
    for (index, case) in cases.iter().enumerate() {
        let label = format!("第 {} 个 case", index + 1);
        let keys: &[&str] = if numeric {
            &["op", "value", "goto"]
        } else {
            &["value", "goto"]
        };
        allowed(case, keys, &label)?;
        ensure!(
            integer(&case["value"]),
            "{label}: 字段 \"value\" 必须是整数"
        );
        if ["mod", "condition"].contains(&source) {
            ensure!(
                int_range(&case["value"], 1, 2),
                "{label}: source=\"{source}\" 时 value 只能是 1 或 2"
            );
        }
        let op = if numeric { text(case, "op", ">=") } else { "" };
        if numeric {
            ensure!(
                enumeration("check_op", &json!(op)),
                "{label}: 字段 \"op\" 必须是 >=/>/<=/</== 之一"
            );
        }
        ensure!(
            seen.insert((op, case["value"].to_string())),
            "{label}: op={op} value={} 与其他 case 重复",
            case["value"]
        );
        ensure!(
            case["goto"].is_string(),
            "{label}: 缺少必填字段 \"goto\"（节点 id 字符串）"
        );
    }
    Ok(())
}
fn mod_death_id(key: &str) -> bool {
    if key.is_empty() || !key.bytes().all(|x| x.is_ascii_digit()) {
        return false;
    }
    let n = key.trim_start_matches('0');
    n.len() > 6 || (n.len() == 6 && n >= "900000")
}
fn extra(n: &Value, ntype: &str, catalog: &Value) -> Result<()> {
    if has(n, "appearance") {
        ensure!(
            s(n, "character") == "player"
                && ["game", "original", "beautified"].contains(&s(n, "appearance")),
            "appearance 仅支持赵活 player 的 game/original/beautified 外观"
        );
    }
    let c = &n["character"];
    let c_used = !(ntype == "intro" && text(n, "intro_source", "official") == "custom");
    if c_used {
        ensure!(
            !matches(r"^.+（[a-zA-Z0-9_\-]+）$", string(c).trim()),
            "人物必须保存内部 ID，不能使用下拉显示文字 {c}；请重新选择该人物（例如 chicken1）"
        );
    }
    if ["show", "say"].contains(&ntype) && nonempty(c) && nonempty(&n["portrait"]) {
        let portrait = s(n, "portrait");
        if user(c) {
            user_ref(c, "character")?;
            ensure!(
                matches(r"^[A-Za-z][A-Za-z0-9_]{0,31}$", portrait),
                "portrait 必须是合法表情 id"
            );
        } else if let Some(meta) = array(&catalog["characters"])
            .iter()
            .find(|v| s(v, "id") == string(c))
        {
            {
                ensure!(array(&meta["portraits"]).iter().any(|v|v.as_str()==Some(portrait)),"角色 {c} 没有表情 \"{portrait}\"（该角色表情：{}）。游戏 LoadCharacterPortrait 对无效表情 key 抛 KeyNotFoundException → Lua 协程死 → 对话冻结，请改用清单内表情。",meta["portraits"]);
            }
        }
    }
    ensure!(
        !(ntype == "affinity" && user(c)),
        "自定义角色暂不支持该步骤，请改用官方角色，或用纯演出节点。"
    );
    if user(c)
        && [
            "show", "say", "hide", "move", "face", "focus", "offset", "shock", "dim", "rotate",
        ]
        .contains(&ntype)
    {
        user_ref(c, "character")?;
    }
    match ntype {
        "background" => {
            range(n, "fade", 0.0, f64::INFINITY)?;
            if ["set", "show", "replace", "fadein"].contains(&s(n, "action")) {
                user_ref(&n["image"], "image")?;
            } else {
                ensure!(
                    n["image"].is_null() || n["image"] == "",
                    "action=\"{}\" 时不能填写 \"image\"",
                    s(n, "action")
                );
            }
        }
        "custom_cg" => {
            range(n, "fade", 0.0, f64::INFINITY)?;
            range(n, "scale", 10.0, 300.0)?;
            range(n, "x", -100.0, 100.0)?;
            range(n, "y", -100.0, 100.0)?;
            if s(n, "action") == "show" {
                user_ref(&n["image"], "image")?;
            }
        }
        "overlay" => {
            ensure!(
                matches(SCRIPT_ID, s(n, "slot")),
                "slot 必须是 [A-Za-z0-9_-]+ 的非空槽位 id"
            );
            range(n, "fade", 0.0, f64::INFINITY)?;
            range(n, "scale", 10.0, 300.0)?;
            range(n, "opacity", 0.0, 100.0)?;
            if s(n, "action") == "show" {
                user_ref(&n["image"], "image")?;
            }
        }
        "say" => {
            if ["character", "think"].contains(&text(n, "mode", "character")) {
                require(n, &["character"], "character/think 模式")?;
            }
            if has(n, "voice") {
                user_ref(&n["voice"], "voice")?;
            }
        }
        "intro" => match text(n, "intro_source", "official") {
            "official" => ensure!(
                nonblank(&n["character"]),
                "使用原版人物资料时必填字段 \"character\""
            ),
            "character" => user_ref(&n["character"], "自定义角色介绍卡 character")?,
            _ => {
                ensure!(nonblank(&n["name"]), "使用自定义人物资料时人物姓名不能为空");
                ensure!(nonblank(&n["text"]), "使用自定义人物资料时人物介绍不能为空");
                if nonempty(&n["image"]) {
                    asset_image(&n["image"], false)?;
                }
                range(n, "image_scale", 40.0, 160.0)?;
                range(n, "image_x", -30.0, 30.0)?;
                range(n, "image_y", -30.0, 30.0)?;
            }
        },
        "choice" => {
            options(n, 2, 4, &["text", "goto"], &[])?;
            ensure!(text(n,"dialog","Options")=="Options","dialog 只支持 \"Options\"；自由场景 break 格式菜单会触发 BreakOptionButton 解析崩溃（IndexOutOfRange，菜单冻结无法点击）");
        }
        "dice" => dice(n, catalog)?,
        "music" | "sound" => {
            if user(&n["name"]) {
                user_ref(&n["name"], "name")?;
            }
            if ntype == "sound" && text(n, "op", "play") == "fadeout" {
                ensure!(
                    text(n, "kind", "sound") == "env",
                    "op=\"fadeout\" 仅支持 kind=\"env\""
                );
            }
        }
        "message" => ensure!(nonblank(&n["text"]), "字段 \"text\" 不能为空"),
        "rotate" => {
            ensure!(
                integer(&n["angle"]),
                "字段 \"angle\" 必须是整数（官方调用点均整数角度）"
            );
            ensure!(
                n["duration"].as_f64().is_some_and(|x| x > 0.0)
                    || positive_integer(&n["duration"], false),
                "字段 \"duration\" 必须是正数（秒）"
            );
        }
        "dayenv" => ensure!(
            int_range(&n["day_type"], 1, 2),
            "字段 \"day_type\" 必须是 1（白天）或 2（晚上）"
        ),
        "cg" => {
            if s(n, "action") == "show" {
                let keys: &[&str] = match s(n, "kind") {
                    "map" => &["key", "key2"],
                    "family" => &["key", "key2", "n1", "n2"],
                    _ => &["key"],
                };
                require(n, keys, "action=show")?;
            } else {
                ensure!(
                    s(n, "kind") != "title",
                    "action=\"hide\" 不支持 kind=\"title\"（官方无对应 API）"
                );
            }
        }
        "item" => ensure!(
            !(n["remove"] == true && s(n, "kind") == "special"),
            "remove 仅支持 kind=\"book\"/\"misc\"（special 无 Remove API）"
        ),
        "enemy" => {
            if s(n, "op") != "id" {
                require(n, &["value"], "op 非 id")?;
            }
            if has(n, "display") {
                ensure!(
                    type_valid("save_button", &n["display"]),
                    "字段 \"display\" 必须是 0 或 1"
                );
            }
        }
        "battle_skill" => {
            if ["set", "active", "level"].contains(&s(n, "op")) {
                require(n, &["key"], "op 非 reset")?;
            }
            if s(n, "op") == "active" && has(n, "active") {
                ensure!(
                    type_valid("save_button", &n["active"]),
                    "字段 \"active\" 必须是 0 或 1"
                );
            }
            if s(n, "op") == "level" && has(n, "level") {
                ensure!(
                    positive_integer(&n["level"], true),
                    "字段 \"level\" 必须是非负整数"
                );
            }
        }
        "combat" => combat(n, catalog)?,
        "battle" => battle(n)?,
        "reward" | "result_screen" => {
            if ntype == "result_screen" {
                ensure!(nonblank(&n["title"]), "title 不能为空或纯空白");
            }
            rewards(&n["entries"])?;
        }
        "custom_shop" => custom_shop(n)?,
        "stat_check" | "affinity_check" | "talent_check" => {
            ensure!(integer(&n["value"]), "value 必须是整数")
        }
        "flag_check" => {
            if s(n, "source") == "flag_value" {
                require(n, &["op", "value"], "source=flag_value")?;
                ensure!(integer(&n["value"]), "value 必须是整数");
                ensure!(!has(n, "invert"), "flag_value 不使用 invert");
            } else {
                ensure!(
                    !has(n, "op") && !has(n, "value"),
                    "source=\"{}\" 只使用 invert，不接受 op/value",
                    s(n, "source")
                );
            }
        }
        "activity" => {
            ensure!(integer(&n["value"]), "value 必须是整数");
            for field in ["success_rewards", "failure_rewards"] {
                let items = array(&n[field]);
                ensure!(items.len() <= 16, "{field} 最多 16 项");
                if !items.is_empty() {
                    rewards(&n[field]).with_context(|| field.to_owned())?;
                }
            }
        }
        "mod_quest" | "quest_check" => ensure!(
            matches(SCRIPT_ID, s(n, "quest")),
            "quest 必须符合 [A-Za-z0-9_-]{{1,64}}"
        ),
        "persistent_var" | "persistent_check" => {
            ensure!(
                matches(SCRIPT_ID, s(n, "key")),
                "key 必须符合 [A-Za-z0-9_-]{{1,64}}"
            );
            ensure!(
                int_range(&n["value"], i32::MIN as i64, i32::MAX as i64),
                "value 必须是 Int32 范围内的整数"
            );
        }
        "time" => match s(n, "op") {
            "set" => require(n, &["year", "month", "stage"], "op=set")?,
            "mission" => require(n, &["name", "year", "month", "stage"], "op=mission")?,
            _ => {}
        },
        "panel" => {
            if ["cg", "cgvideo", "endgame"].contains(&s(n, "panel")) {
                require(n, &["key"], "panel=cg/cgvideo/endgame")?;
            }
        }
        "goto_scene" => {
            let key = s(n, "key");
            let scene = s(n, "scene");
            let has_text = nonblank(&n["title"]) || nonblank(&n["desc"]);
            if scene == "GameOver" && mod_death_id(key) {
                ensure!(has_text,"scene=\"GameOver\" 使用 mod 专属 key=\"{key}\" 时必须提供 title/desc，否则官方死亡画面没有文字；建议改用 death 节点");
            }
            if scene == "End" && (key.is_empty() || mod_death_id(key)) {
                ensure!(has_text||nonblank(&n["image"]),"scene=\"End\" 使用空 key 或 mod 专属 key=\"{key}\" 时必须提供 title/desc/image，否则汗青书结局卡没有内容");
            }
            if has(n, "image") {
                ensure!(scene == "End", "字段 \"image\" 仅 scene=\"End\" 支持");
                asset_image(&n["image"], true)?;
            }
        }
        "block" => {
            for (index, var) in array(&n["vars"]).iter().enumerate() {
                let label = format!("第 {} 个 var", index + 1);
                allowed(var, &["name", "value"], &label)?;
                ensure!(
                    nonempty(&var["name"]),
                    "{label}: 缺少必填字段 \"name\"（非空字符串）"
                );
                ensure!(
                    var["value"].is_string() || finite(&var["value"]),
                    "{label}: 字段 \"value\" 必须是字符串或数值"
                );
            }
        }
        "raw" => ensure!(nonblank(&n["code"]), "字段 \"code\" 不能为空"),
        "death" => {
            ensure!(nonblank(&n["text"]), "字段 \"text\" 不能为空");
            ensure!(mod_death_id(s(n,"death_id")),"字段 \"death_id\" 必须是 ≥900000 的 mod 专属数字 id（官方死亡画面 id 会触发官方结局解锁与记录）");
        }
        "branch" => branch(n)?,
        _ => {}
    }
    Ok(())
}
fn targets(node: &Value) -> Vec<&str> {
    let mut result = Vec::new();
    if let Some(v) = node.get("goto").and_then(Value::as_str) {
        result.push(v);
    }
    for opt in array(&node["options"]) {
        for field in ["goto", "goto_大成功", "goto_成功", "goto_失败"] {
            if let Some(v) = opt.get(field).and_then(Value::as_str) {
                if !v.is_empty() || field == "goto" {
                    result.push(v);
                }
            }
        }
    }
    for case in array(&node["cases"]) {
        if let Some(v) = case.get("goto").and_then(Value::as_str) {
            result.push(v);
        }
    }
    let kind = s(node, "type");
    if kind == "dice" {
        for band in array(&node["bands"]) {
            result.push(s(band, "goto"));
        }
    }
    if ["combat", "battle", "battle_result"].contains(&kind) {
        result.extend([s(node, "win"), s(node, "lose")]);
    }
    if member("check_types", kind) {
        result.extend([s(node, "success"), s(node, "failure")]);
    }
    result
}
fn branch_covered(node: &Value) -> bool {
    ["mod", "condition"].contains(&text(node, "source", "mod"))
        && array(&node["cases"])
            .iter()
            .filter_map(|v| v["value"].as_i64())
            .collect::<BTreeSet<_>>()
            == BTreeSet::from([1, 2])
}
fn successors(nodes: &[Value], index: usize) -> Vec<&str> {
    let node = &nodes[index];
    let kind = s(node, "type");
    let next = nodes.get(index + 1).map(|v| s(v, "id"));
    if ["end", "goto_scene", "death"].contains(&kind) {
        return vec![];
    }
    if ["choice", "dice", "combat", "battle", "battle_result"].contains(&kind)
        || member("check_types", kind)
    {
        return targets(node);
    }
    if kind == "branch" {
        let mut result = targets(node);
        if !branch_covered(node) {
            if let Some(next) = next {
                result.push(next);
            }
        }
        return result;
    }
    node.get("goto")
        .and_then(Value::as_str)
        .or(next)
        .into_iter()
        .collect()
}
fn cg_lifecycle(story: &Value) -> Result<()> {
    let nodes = array(&story["nodes"]);
    let by_id: BTreeMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (s(n, "id"), i))
        .collect();
    let start = s(story, "start");
    let mut incoming: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::from([(start, BTreeSet::new())]);
    let mut queue = VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        let index = by_id[id];
        let node = &nodes[index];
        let mut state = incoming[id].clone();
        match s(node, "type") {
            "cg" => {
                if s(node, "action") == "show" {
                    state.insert(s(node, "kind"));
                } else {
                    state.remove(s(node, "kind"));
                }
            }
            "combat" | "battle" => state.clear(),
            _ => {}
        }
        for next in successors(nodes, index) {
            let merged = if let Some(old) = incoming.get(next) {
                old.intersection(&state).copied().collect()
            } else {
                state.clone()
            };
            if incoming.get(next) != Some(&merged) {
                incoming.insert(next, merged);
                queue.push_back(next);
            }
        }
    }
    for (id, state) in incoming {
        let node = &nodes[by_id[id]];
        let kind = s(node, "kind");
        ensure!(!(s(node,"type")=="cg"&&s(node,"action")=="hide"&&!state.contains(kind)),"节点 \"{id}\"(cg): action=\"hide\" kind=\"{kind}\" 前没有在每条可达路径上成功执行同类 show；原版 Hide 会释放无效的 Addressables handle 并导致演出异常。请先添加同 kind 的 cg show，或删除这个 hide 节点。");
    }
    Ok(())
}
fn collect_warnings(story: &Value, catalog: &Value) -> Vec<String> {
    let nodes = array(&story["nodes"]);
    let mut warnings = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if s(node, "type") != "transition" {
            continue;
        }
        let id = s(node, "id");
        if s(node, "phase") == "in" {
            if !nodes[index + 1..]
                .iter()
                .any(|n| s(n, "type") == "transition" && s(n, "phase") == "out")
            {
                warnings.push(format!("节点 \"{id}\"(transition, phase=in) 之后没有 phase=out 解除：TransitionIn 会隐藏剧情 UI 并盖满黑幕（官方脚本必须成对使用，ch1_1 里相距仅十几行），黑幕将一直覆盖到脚本结尾（含链式脚本）。请在其后补一个 phase=out 节点，或改用 scene 节点做转场。"));
            }
        } else if !nodes[..index]
            .iter()
            .any(|n| s(n, "type") == "transition" && s(n, "phase") == "in")
        {
            warnings.push(format!("节点 \"{id}\"(transition, phase=out) 之前没有 phase=in：无黑幕可撤（官方用法永远是先 in 后 out），该节点不会产生任何视觉效果，可删除。"));
        }
    }
    for node in nodes {
        if s(node, "type") != "dice" || !has(node, "check") {
            continue;
        }
        let check = s(node, "check");
        let opt = &node["options"][0];
        if array(&catalog["dice_meta"][check]["bands"]).len() == 2
            && nonempty(&opt["goto_大成功"])
            && opt["goto_大成功"] != opt["goto_成功"]
        {
            warnings.push(format!("节点 \"{}\"(dice): 检查点 \"{check}\" 只有 2 个结果带（无独立大成功档），goto_大成功 会被忽略（最优带按 goto_成功 分支）。",s(node,"id")));
        }
    }
    let official: BTreeSet<&str> = ["death_ids", "ending_ids"]
        .iter()
        .flat_map(|field| array(&catalog[*field]))
        .map(|v| v.as_str().unwrap_or_else(|| s(v, "id")))
        .collect();
    for node in nodes {
        if s(node, "type") != "goto_scene" {
            continue;
        }
        let id = s(node, "id");
        let scene = s(node, "scene");
        let key = s(node, "key");
        let custom = scene == "End"
            && ["title", "desc", "image"]
                .iter()
                .any(|field| nonblank(&node[*field]));
        if ["GameOver", "End"].contains(&scene)
            && !key.is_empty()
            && official.contains(key)
            && !custom
        {
            warnings.push(format!("节点 \"{id}\"(goto_scene): scene=\"{scene}\" 的 key=\"{key}\" 与官方结局 id 重复，会触发官方结局解锁与记录（LibraryItemData.Add，污染玩家存档）；建议改用 ≥900000 的 mod 专属 id（查不到官方条目，仅展示对应画面，无副作用）。"));
        }
        if scene == "End" && nonempty(&node["title"]) && !nonblank(&node["desc"]) {
            warnings.push(format!("节点 \"{id}\"(goto_scene): scene=\"End\" 给了 title 但未给 desc，结局画面的描述区将显示空白；建议补一个非空 desc。"));
        }
        let next = s(node, "next");
        if scene == "End" && !["", "Title", "Story"].contains(&next) {
            warnings.push(format!("节点 \"{id}\"(goto_scene): 汗青书结局的 next='{}' 不会生效；原版 EndGamePanel 确认后固定返回标题画面，编译器已按 Title 处理。",next));
        }
        if scene == "GameOver" && !["", "Title"].contains(&next) {
            warnings.push(format!("节点 \"{id}\"(goto_scene): 死亡画面的 next='{}' 不会生效；原版按钮固定为读档或标题画面，编译器已按 Title 处理。",next));
        }
    }
    for node in nodes {
        let next = s(node, "next");
        if s(node, "type") == "death" && !["", "Title"].contains(&next) {
            warnings.push(format!("节点 \"{}\"(death): next='{}' 不会生效；原版 GameOverController 只提供读档和返回标题按钮，编译器已按 Title 处理。",s(node,"id"),next));
        }
    }
    warnings
}
pub fn validate_story(story: &Value) -> Result<Vec<String>> {
    validate_story_with_catalog(story, editor_data())
}
/// Explicit catalog injection supports offline authoring and migration parity tests.
/// Normal editor and CLI validation always uses the embedded official catalog.
pub fn validate_story_with_catalog(story: &Value, catalog: &Value) -> Result<Vec<String>> {
    story_inner(story, catalog).map_err(|error| anyhow::anyhow!("story.json: {error:#}"))?;
    Ok(collect_warnings(story, catalog))
}
fn story_inner(story: &Value, catalog: &Value) -> Result<()> {
    object(story, "顶层必须是 JSON 对象")?;
    ensure!(
        !has(story, "battle_presets"),
        "字段 \"battle_presets\" 已废弃；请把模板和数值直接写入每个 combat/battle 节点"
    );
    if has(story, "story_schema") {
        ensure!(
            story["story_schema"].as_f64() == Some(2.0),
            "字段 \"story_schema\" 必须固定为 2"
        );
    }
    ensure!(
        matches(SCRIPT_ID, s(story, "id")),
        "缺少必填字段 \"id\"（剧情脚本 id，规则 [a-zA-Z0-9_-]{{1,64}}）"
    );
    if has(story, "title") {
        ensure!(story["title"].is_string(), "字段 \"title\" 必须是字符串");
    }
    if has(story, "mood") {
        ensure!(
            story["mood"].is_boolean(),
            "字段 \"mood\" 必须是布尔值（true=保留官方心情气泡，false=自动隐藏）"
        );
    }
    ensure!(
        story["start"].is_string(),
        "缺少必填字段 \"start\"（起始节点 id）"
    );
    let nodes = story["nodes"]
        .as_array()
        .filter(|v| !v.is_empty())
        .context("缺少必填字段 \"nodes\"（非空节点数组）")?;
    let mut ids = BTreeSet::new();
    for (index, node) in nodes.iter().enumerate() {
        object(node, &format!("第 {} 个节点", index + 1))?;
        let id = s(node, "id");
        ensure!(
            matches(NODE_ID, id),
            "第 {} 个节点: 节点 id 必须是 [a-zA-Z0-9_]+（会拼进 Lua 函数名 node_<id>）",
            index + 1
        );
        ensure!(ids.insert(id), "节点 id \"{id}\" 重复");
    }
    ensure!(
        ids.contains(s(story, "start")),
        "start 指向不存在的节点 \"{}\"",
        s(story, "start")
    );
    let last = nodes
        .iter()
        .rposition(|n| !member("declaration_types", s(n, "type")));
    for (index, node) in nodes.iter().enumerate() {
        let id = s(node, "id");
        let label = format!("节点 \"{id}\"({})", s(node, "type"));
        let kind = fields(node).with_context(|| label.clone())?;
        extra(node, kind, catalog).with_context(|| label.clone())?;
        if has(node, "goto") {
            ensure!(
                node["goto"].is_string(),
                "{label}: 字段 \"goto\" 必须是节点 id 字符串"
            );
        }
        for target in targets(node) {
            ensure!(
                ids.contains(target),
                "{label}: goto 指向不存在的节点 \"{target}\""
            );
        }
        ensure!(
            !(member("no_goto_types", kind) && has(node, "goto")),
            "{label}: 该类型节点不允许显式 \"goto\"（流转由分支/跳转决定）"
        );
        ensure!(!(Some(index)==last&&!member("terminal_types",kind)&&!has(node,"goto")),"{label}: 是最后一个节点且没有显式 goto，脚本无法正常结束（请改用 end/goto_scene/raw 节点或显式 goto）");
        if kind == "branch" && Some(index) == last {
            ensure!(branch_covered(node),"{label}: 是最后一个节点且存在未覆盖的返回值，else 没有落点（请把它移到最后之前，或补齐 value 1 和 2 两个 case）");
        }
    }
    cg_lifecycle(story)?;
    crate::localization::validate_story_localization(story)?;
    Ok(())
}
