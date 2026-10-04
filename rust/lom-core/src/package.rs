//! Confined resources, deterministic v3 packages and read-only integrity checks.
use crate::{codegen::compile_story, content, load_json, localization, stable_json, validate};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use unicode_casefold::UnicodeCaseFold;
use zip::{write::SimpleFileOptions, CompressionMethod, DateTime, ZipArchive, ZipWriter};

pub const MAX_PACKAGE_BYTES: u64 = 160 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL: u64 = 128 * 1024 * 1024;
const CONTENT_HASH: &str = "package-content.sha256";
const STORY_HASH: &str = "story-lua.sha256";
pub type Entries = BTreeMap<String, Vec<u8>>;
pub fn canonical_archive_name(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty() && !name.contains(['\0', '\\']) && !name.starts_with('/'),
        "包内不安全路径: {name:?}"
    );
    let body = name.strip_suffix('/').unwrap_or(name);
    ensure!(!body.is_empty(), "包内空路径");
    for part in body.split('/') {
        ensure!(
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.contains(':')
                && !part.ends_with(['.', ' ']),
            "包内非规范路径: {name:?}"
        );
        let stem = part.split('.').next().unwrap().to_uppercase();
        let numbered = (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9');
        ensure!(
            !["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) && !numbered,
            "包内 Windows 设备名: {name}"
        );
    }
    Ok(name.into())
}
pub fn resolve_confined_file(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let real = path
        .canonicalize()
        .with_context(|| format!("无法读取 {}", path.display()))?;
    ensure!(
        real.starts_with(root) && real.is_file(),
        "文件指向项目目录外或不是普通文件: {}",
        path.display()
    );
    Ok(real)
}
fn folded(name: &str) -> String {
    name.case_fold().collect()
}
fn check_names<'a>(names: impl IntoIterator<Item = (&'a str, u64)>) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    let mut total = 0;
    for (name, size) in names {
        canonical_archive_name(name)?;
        ensure!(seen.len() < 2048, "包内条目超过 2048 个");
        ensure!(size <= MAX_ENTRY_BYTES, "包内条目超过 32 MiB: {name}");
        let lower = name.to_lowercase();
        ensure!(
            ![".json", ".lua", ".sha256", ".txt"]
                .iter()
                .any(|ext| lower.ends_with(ext))
                || size <= MAX_TEXT_BYTES,
            "文本条目超过 4 MiB: {name}"
        );
        total += size;
        ensure!(total <= MAX_TOTAL, "包解压后超过 128 MiB");
        let fold = folded(name);
        ensure!(
            seen.insert(fold.clone()),
            "包内重复或大小写冲突路径: {name}"
        );
        if name.ends_with('/') {
            dirs.insert(fold.trim_end_matches('/').to_string());
        } else {
            files.insert(fold);
        }
    }
    for name in &seen {
        let parts: Vec<_> = name.trim_end_matches('/').split('/').collect();
        for end in 1..parts.len() {
            ensure!(
                !files.contains(&parts[..end].join("/")),
                "包内路径同时作为文件和目录: {name}"
            );
        }
    }
    ensure!(files.is_disjoint(&dirs), "包内路径同时作为文件和目录");
    Ok(())
}
pub fn content_hash(entries: &Entries) -> String {
    let mut hash = Sha256::new();
    for (name, data) in entries
        .iter()
        .filter(|(n, _)| n.as_str() != CONTENT_HASH && !n.ends_with('/'))
    {
        hash.update((name.len() as u32).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update((data.len() as u64).to_be_bytes());
        hash.update(data);
    }
    format!("{:X}", hash.finalize())
}
fn sha(data: &[u8]) -> String {
    format!("{:X}", Sha256::digest(data))
}
pub fn verify_integrity(entries: &Entries) -> Result<()> {
    let record = entries
        .get(CONTENT_HASH)
        .context("缺少 package-content.sha256")?;
    let expected = format!(
        "algorithm=lom-entry-sha256-v1\nsha256={}\n",
        content_hash(entries)
    );
    ensure!(
        record == expected.as_bytes(),
        "package-content.sha256 校验失败"
    );
    let record = std::str::from_utf8(entries.get(STORY_HASH).context("缺少 story-lua.sha256")?)?;
    let mut lines = record.lines();
    ensure!(
        lines.next() == Some("algorithm=lom-story-lua-sha256-v1"),
        "story-lua.sha256 algorithm 无效"
    );
    let mut linked = BTreeSet::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let cols: Vec<_> = line.split('\t').collect();
        ensure!(cols.len() == 4, "story-lua.sha256 必须四列");
        ensure!(
            cols[0].starts_with("story/")
                && cols[0].ends_with(".json")
                && cols[2].starts_with("lua/")
                && cols[2].ends_with(".lua"),
            "Story/Lua 记录路径无效"
        );
        ensure!(linked.insert(cols[2].to_string()), "重复 Lua 完整性记录");
        for (path, hash) in [(cols[0], cols[1]), (cols[2], cols[3])] {
            ensure!(
                entries.get(path).is_some_and(|bytes| sha(bytes) == hash),
                "Story/Lua 完整性校验失败: {path}"
            );
        }
    }
    let expected: BTreeSet<_> = entries
        .keys()
        .filter(|n| n.starts_with("lua/") && n.ends_with(".lua"))
        .cloned()
        .collect();
    ensure!(
        !linked.is_empty() && linked == expected,
        "Story/Lua 完整性记录与 Lua 条目集合不一致"
    );
    Ok(())
}
// zip 2.x stores central-directory entries in a name-keyed map. Duplicate
// names disappear before `len`/`by_index`, so inspect the raw record count as
// well. ZIP64 extra fields do not change the fixed central header or its
// variable-name/extra/comment lengths.
fn validate_raw_directory(file: &mut File, start: u64, unique_count: usize) -> Result<()> {
    file.seek(SeekFrom::Start(start))?;
    let limit = file.metadata()?.len();
    let mut count = 0;
    loop {
        let mut signature = [0u8; 4];
        file.read_exact(&mut signature)
            .context("ZIP 中央目录被截断")?;
        if signature != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }
        count += 1;
        ensure!(count <= 2048, "包内条目超过 2048 个");
        let mut header = [0u8; 42];
        file.read_exact(&mut header)
            .context("ZIP 中央目录头被截断")?;
        let name = u16::from_le_bytes([header[24], header[25]]) as u64;
        let extra = u16::from_le_bytes([header[26], header[27]]) as u64;
        let comment = u16::from_le_bytes([header[28], header[29]]) as u64;
        let next = file
            .stream_position()?
            .checked_add(name + extra + comment)
            .context("ZIP 中央目录长度溢出")?;
        ensure!(next <= limit, "ZIP 中央目录长度超出文件");
        file.seek(SeekFrom::Start(next))?;
    }
    ensure!(
        count == unique_count,
        "包内存在重复路径或中央目录条目数量不一致"
    );
    Ok(())
}
pub fn read_package(path: &Path) -> Result<Entries> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_PACKAGE_BYTES,
        "包文件超过 160 MiB"
    );
    let mut raw_directory = file.try_clone()?;
    let mut archive = ZipArchive::new(file)?;
    validate_raw_directory(
        &mut raw_directory,
        archive.central_directory_start(),
        archive.len(),
    )?;
    ensure!(archive.len() <= 2048, "包内条目超过 2048 个");
    let mut listing = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        ensure!(!entry.is_symlink(), "包内不允许符号链接: {}", entry.name());
        listing.push((entry.name().to_owned(), entry.size()));
    }
    check_names(listing.iter().map(|(n, s)| (n.as_str(), *s)))?;
    let mut entries = Entries::new();
    for (i, (name, size)) in listing.into_iter().enumerate() {
        let entry = archive.by_index(i)?;
        let mut bytes = Vec::new();
        entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 == size, "包内条目大小不符: {name}");
        if !name.ends_with('/') {
            entries.insert(name, bytes);
        }
    }
    let manifest: Value = serde_json::from_slice(
        entries
            .get("manifest.json")
            .context("包缺少 manifest.json")?,
    )?;
    validate::validate_manifest(&manifest)?;
    verify_integrity(&entries)?;
    verify_story_lua_pairs(&entries, &manifest)?;
    Ok(entries)
}
/// Hashes establish byte consistency only. Independently compile the authored
/// documents so rewriting both hashes cannot conceal substituted executable Lua.
/// Bundled resources are confined to a disposable directory, never registered.
fn verify_story_lua_pairs(entries: &Entries, manifest: &Value) -> Result<()> {
    let mut stories = BTreeMap::new();
    for (name, bytes) in entries {
        if let Some(id) = name
            .strip_prefix("story/")
            .and_then(|s| s.strip_suffix(".json"))
        {
            ensure!(
                !id.is_empty() && !id.contains('/'),
                "包内 Story 路径层级无效: {name}"
            );
            let story: Value = serde_json::from_slice(bytes)
                .with_context(|| format!("包内 {name} 不是合法 JSON"))?;
            ensure!(
                story["id"].as_str() == Some(id),
                "包内 {name} 的 id 与文件名不一致"
            );
            ensure!(
                entries.contains_key(&format!("lua/{id}.lua")),
                "包内 Story 缺少对应 Lua: {name}"
            );
            stories.insert(id, story);
        }
    }
    ensure!(!stories.is_empty(), "包内没有 story/*.json");
    let resources = tempfile::tempdir()?;
    for (name, bytes) in entries
        .iter()
        .filter(|(name, _)| name.starts_with("assets/"))
    {
        let destination = resources.path().join(name);
        fs::create_dir_all(destination.parent().context("资源路径无效")?)?;
        fs::write(destination, bytes)?;
    }
    for (path, packaged) in entries
        .iter()
        .filter(|(path, _)| path.starts_with("lua/") && path.ends_with(".lua"))
    {
        let parts: Vec<_> = path.split('/').collect();
        let (id, locale) = match parts.as_slice() {
            ["lua", file] => (&file[..file.len() - 4], None),
            ["lua", locale, file] => {
                let locale = localization::normalize_locale(locale);
                ensure!(
                    localization::SUPPORTED_LOCALES.contains(&locale),
                    "包内包含不支持的 locale Lua 路径: {path}"
                );
                (&file[..file.len() - 4], Some(locale))
            }
            _ => anyhow::bail!("包内 Lua 路径层级无效: {path}"),
        };
        let source = format!("story/{id}.json");
        let story = stories
            .get(id)
            .with_context(|| format!("包内 Lua 没有对应 Story: {path}"))?;
        let localized;
        let story = if let Some(locale) = locale {
            localized = localization::apply_story_locale(story, locale)?;
            &localized
        } else {
            story
        };
        let generated = compile_story(story, Some(manifest), Some(&source), Some(resources.path()))
            .with_context(|| format!("无法复编译 {source} 以验证 {path}"))?;
        ensure!(
            generated.as_bytes() == packaged,
            "包内 {path} 不是由对应 {source} 编译生成，拒绝导入"
        );
    }
    Ok(())
}
pub fn write_package(mut entries: Entries, output: &Path) -> Result<PathBuf> {
    ensure!(
        !entries.keys().any(|name| name.ends_with('/')),
        "打包器不接受目录条目"
    );
    ensure!(
        !entries.contains_key(CONTENT_HASH),
        "package-content.sha256 由打包器保留"
    );
    entries.insert(
        CONTENT_HASH.into(),
        format!(
            "algorithm=lom-entry-sha256-v1\nsha256={}\n",
            content_hash(&entries)
        )
        .into_bytes(),
    );
    check_names(entries.iter().map(|(n, b)| (n.as_str(), b.len() as u64)))?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut archive = ZipWriter::new(temp.as_file_mut());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(9))
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644);
        for (name, bytes) in entries {
            archive.start_file(name, options)?;
            archive.write_all(&bytes)?;
        }
        archive.finish()?;
    }
    ensure!(
        temp.as_file().metadata()?.len() <= MAX_PACKAGE_BYTES,
        "输出包超过 160 MiB"
    );
    temp.as_file().sync_all()?;
    temp.persist(output)?;
    Ok(output.to_path_buf())
}

pub fn free_trigger_from_node(node: &Value) -> Option<Value> {
    if node["type"] != "free_trigger"
        || !node["position"].as_str().is_some_and(|s| !s.is_empty())
        || !node["script"].as_str().is_some_and(|s| !s.is_empty())
    {
        return None;
    }
    let mut trigger =
        json!({"type":"position","position":node["position"],"script":node["script"]});
    for key in ["when_flag_set", "when_flag_clear"] {
        if node[key].as_str().is_some_and(|s| !s.trim().is_empty()) {
            trigger[key] = json!(node[key].as_str().unwrap().trim());
        }
    }
    for key in ["when_month", "when_stage"] {
        let raw = &node[key];
        if raw.is_null() || raw == "" || raw == "any" {
            continue;
        }
        let rendered = raw
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| raw.to_string());
        if let Ok(n) = rendered.trim().parse::<i64>() {
            trigger[key] = json!(n);
        } else {
            return None;
        }
    }
    if let Some(character) = node["when_affinity"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
    {
        trigger["when_affinity"] = json!({"character":character.trim(),"min":node.get("when_affinity_min").unwrap_or(&json!(0))});
    }
    Some(trigger)
}
fn merge_triggers(manifest: &mut Value, stories: &BTreeMap<String, Value>) -> Result<()> {
    let generated: Vec<_> = stories
        .values()
        .flat_map(|s| s["nodes"].as_array().into_iter().flatten())
        .filter_map(free_trigger_from_node)
        .collect();
    let previous = manifest
        .get("node_free_triggers")
        .cloned()
        .unwrap_or(json!([]));
    ensure!(
        previous
            .as_array()
            .is_some_and(|v| v.iter().all(Value::is_object)),
        "node_free_triggers 必须为对象数组"
    );
    if generated.is_empty() && previous.as_array().unwrap().is_empty() {
        return Ok(());
    }
    ensure!(
        manifest["campaign"].is_object(),
        "自由模式触发缺少 campaign"
    );
    let mut current = manifest["campaign"]["triggers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for old in previous.as_array().unwrap().iter().rev() {
        if let Some(i) = current.iter().rposition(|v| v == old) {
            current.remove(i);
        }
    }
    let mut tracked = Vec::new();
    for trigger in generated {
        if !current.contains(&trigger) {
            current.push(trigger.clone());
            tracked.push(trigger);
        }
    }
    manifest["campaign"]["triggers"] = json!(current);
    if tracked.is_empty() {
        manifest
            .as_object_mut()
            .unwrap()
            .remove("node_free_triggers");
    } else {
        manifest["node_free_triggers"] = json!(tracked);
    }
    Ok(())
}
fn add_user_content(
    entries: &mut Entries,
    root: &Path,
    stories: &BTreeMap<String, Value>,
) -> Result<()> {
    let mut referenced: BTreeMap<String, String> = BTreeMap::new();
    for story in stories.values() {
        for node in story["nodes"].as_array().into_iter().flatten() {
            let kind = node["type"].as_str().unwrap_or("");
            let mut refs = Vec::new();
            if kind == "music" {
                refs.push(("name", "audio", Some("music")));
            }
            if kind == "sound" {
                refs.push((
                    "name",
                    "audio",
                    Some(node["kind"].as_str().unwrap_or("sound")),
                ));
            }
            if kind == "say" {
                refs.push(("voice", "audio", None));
            }
            if [
                "show", "move", "face", "hide", "focus", "offset", "say", "shock", "dim", "rotate",
                "intro", "combat",
            ]
            .contains(&kind)
            {
                refs.push(("character", "character", None));
            }
            let image_active = !((kind == "custom_cg" || kind == "overlay")
                && node["action"] == "hide"
                || kind == "background"
                    && ["fadeout", "clear"].contains(&node["action"].as_str().unwrap_or("")));
            if image_active {
                refs.push(("image", "image", None));
            }
            for (field, ctype, expected) in refs {
                let Some(id) = node[field].as_str().and_then(|s| s.strip_prefix("user:")) else {
                    continue;
                };
                if let Some(previous) = referenced.get(id) {
                    ensure!(
                        previous == ctype,
                        "用户内容 {id} 同时被当作 {previous}/{ctype}"
                    );
                }
                let (meta, _main) = content::resolve_content(root, ctype, id)?;
                if let Some(expected) = expected {
                    ensure!(
                        meta["audio_kind"] == expected,
                        "用户音频 {id} 类型不匹配: 需要 {expected}"
                    );
                }
                if ctype == "character" && ["show", "say"].contains(&kind) {
                    if let Some(portrait) = node["portrait"].as_str().filter(|s| !s.is_empty()) {
                        ensure!(
                            meta["portraits"].get(portrait).is_some(),
                            "自定义人物 {id} 缺少表情 {portrait}"
                        );
                    }
                }
                if referenced.insert(id.into(), ctype.into()).is_none() {
                    let dir = format!("assets/user/{ctype}/{id}");
                    entries.insert(
                        format!("{dir}/content.json"),
                        stable_json(&content::content_metadata_payload(&meta))?,
                    );
                    for file in content::listed_content_files(&meta) {
                        canonical_archive_name(&file)?;
                        let path = resolve_confined_file(root, &root.join(&dir).join(&file))?;
                        entries.insert(format!("{dir}/{file}"), fs::read(path)?);
                    }
                }
            }
        }
    }
    Ok(())
}
pub fn pack_mod(root: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let default = if let Some(name) = root.file_name() {
        let mut name = name.to_os_string();
        name.push(".lommod");
        root.with_file_name(name)
    } else {
        let canonical = root.canonicalize().context("mod 目录不存在")?;
        let mut name = canonical.as_os_str().to_os_string();
        name.push(".lommod");
        PathBuf::from(name)
    };
    let root = root.canonicalize().context("mod 目录不存在")?;
    let mut manifest = load_json(resolve_confined_file(&root, &root.join("manifest.json"))?)?;
    validate::validate_manifest(&manifest)?;
    let story_dir = root.join("story");
    let mut stories = BTreeMap::new();
    for entry in fs::read_dir(&story_dir).context("缺少 story/ 目录")? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") || !path.is_file() {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap()
            .to_str()
            .context("剧情文件名不是 UTF-8")?
            .to_string();
        let path = resolve_confined_file(&root, &path)?;
        let mut story = load_json(&path)?;
        validate::validate_story(&story)?;
        story["story_schema"] = json!(2);
        if let Some(config) = localization::localization_config(&story) {
            story["localization"] = config;
        }
        ensure!(
            story["id"] == stem,
            "story/{stem}.json 的 id 必须与文件名一致"
        );
        stories.insert(stem, story);
    }
    ensure!(!stories.is_empty(), "story/ 下没有剧情 JSON");
    merge_triggers(&mut manifest, &stories)?;
    validate::validate_manifest(&manifest)?;
    for (key, val) in [
        ("format", 3),
        ("package_format", 3),
        ("story_schema", 2),
        ("content_schema", 1),
    ] {
        manifest[key] = json!(val);
    }
    if stories.values().any(|s| {
        s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["appearance"].as_str().is_some_and(|s| !s.is_empty()))
    }) {
        let version = manifest
            .get("min_host_version")
            .and_then(Value::as_str)
            .unwrap_or("1.1.2");
        let core: Vec<_> = version
            .split(['.', '-', '+'])
            .take(3)
            .map(|v| v.parse::<u32>().unwrap_or(0))
            .collect();
        let prerelease = version.split('+').next().unwrap_or(version).contains('-');
        ensure!(
            core.as_slice() > [1, 1, 2].as_slice()
                || (core.as_slice() == [1, 1, 2].as_slice() && !prerelease),
            "appearance 要求 min_host_version 至少 1.1.2"
        );
        if manifest.get("min_host_version").is_none() {
            manifest["min_host_version"] = json!("1.1.2");
        }
        validate::validate_manifest(&manifest)?;
    }
    let exists = |target: &Value| target.as_str().is_some_and(|s| stories.contains_key(s));
    ensure!(exists(&manifest["entry"]), "entry 指向不存在的章节");
    for trigger in manifest["campaign"]["triggers"]
        .as_array()
        .into_iter()
        .flatten()
    {
        ensure!(exists(&trigger["script"]), "自由模式触发指向不存在的章节");
    }
    let mut entries = Entries::new();
    let mut texts = json!({});
    let mut loc_pair: Option<(String, String)> = None;
    for (id, story) in &stories {
        if let Some(config) = localization::localization_config(story) {
            let default = config["default_locale"].as_str().unwrap_or("");
            let fallback = config
                .get("fallback_locale")
                .and_then(Value::as_str)
                .unwrap_or(default);
            let pair = (default.into(), fallback.into());
            ensure!(
                loc_pair.as_ref().is_none_or(|prev| prev == &pair),
                "同包章节本地化 default/fallback 不一致"
            );
            loc_pair = Some(pair);
        }
        let source = format!("story/{id}.json");
        entries.insert(source.clone(), stable_json(story)?);
        entries.insert(
            format!("lua/{id}.lua"),
            compile_story(story, Some(&manifest), Some(&source), Some(&root))?.into_bytes(),
        );
        for node in story["nodes"].as_array().unwrap() {
            if node["type"] == "say" {
                texts[format!(
                    "MOD_{}_{}_{}",
                    manifest["id"].as_str().unwrap(),
                    id,
                    node["id"].as_str().unwrap()
                )] = node["text"].clone();
            }
            if node["type"] == "end" && node["next_script"].as_str().is_some_and(|s| !s.is_empty())
            {
                ensure!(exists(&node["next_script"]), "next_script 指向不存在的章节");
            }
            if (node["type"] == "goto_scene" && node["scene"] == "End")
                || (node["type"] == "intro" && node["intro_source"] == "custom")
            {
                if let Some(image) = node["image"]
                    .as_str()
                    .filter(|s| !s.is_empty() && !s.starts_with("user:"))
                {
                    canonical_archive_name(image)?;
                    ensure!(image.starts_with("assets/"), "image 必须位于 assets/");
                    let lower = image.to_lowercase();
                    ensure!(
                        [".png", ".jpg", ".jpeg"].iter().any(|e| lower.ends_with(e)),
                        "image 必须 PNG/JPEG"
                    );
                    let path = resolve_confined_file(&root, &root.join(image))?;
                    ensure!(
                        fs::metadata(&path)?.len() <= 8 * 1024 * 1024,
                        "image 超过 8 MiB"
                    );
                    entries.insert(image.into(), fs::read(path)?);
                }
            }
        }
    }
    entries.insert("manifest.json".into(), stable_json(&manifest)?);
    entries.insert("texts.json".into(), stable_json(&texts)?);
    if let Some((default, fallback)) = loc_pair {
        entries.insert("localization.json".into(),stable_json(&json!({"schema":1,"default_locale":default,"fallback_locale":fallback,"locales":localization::SUPPORTED_LOCALES}))?);
        for locale in localization::SUPPORTED_LOCALES {
            let mut texts = json!({});
            for (id, story) in &stories {
                let localized = localization::apply_story_locale(story, locale)?;
                entries.insert(
                    format!("lua/{locale}/{id}.lua"),
                    compile_story(
                        &localized,
                        Some(&manifest),
                        Some(&format!("story/{id}.json")),
                        Some(&root),
                    )?
                    .into_bytes(),
                );
                for node in localized["nodes"].as_array().unwrap() {
                    if node["type"] == "say" {
                        texts[format!(
                            "MOD_{}_{}_{}",
                            manifest["id"].as_str().unwrap(),
                            id,
                            node["id"].as_str().unwrap()
                        )] = node["text"].clone();
                    }
                }
            }
            entries.insert(format!("texts/{locale}.json"), stable_json(&texts)?);
        }
    }
    let mut integrity = "algorithm=lom-story-lua-sha256-v1\n".to_string();
    for (path, bytes) in entries
        .iter()
        .filter(|(p, _)| p.starts_with("lua/") && p.ends_with(".lua"))
    {
        let id = Path::new(path).file_stem().unwrap().to_str().unwrap();
        let source = format!("story/{id}.json");
        integrity.push_str(&format!(
            "{source}\t{}\t{path}\t{}\n",
            sha(&entries[&source]),
            sha(bytes)
        ));
    }
    entries.insert(STORY_HASH.into(), integrity.into_bytes());
    add_user_content(&mut entries, &root, &stories)?;
    write_package(entries, output.unwrap_or(&default))
}
