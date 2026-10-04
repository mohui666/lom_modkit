//! Offline user-content library and deterministic .lomcontent exchange.
//! Dependencies are reported; this module never downloads or resolves them automatically.
use crate::{content, package, project, stable_json};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};
use unicode_casefold::UnicodeCaseFold;

pub const MAX_CONTENT_PACK_BYTES: u64 = 128 * 1024 * 1024;
const MANIFEST: &str = "content-pack.json";
const HASH: &str = "package-content.sha256";
const SHARE_META: &str = ".lomcontent.json";
const KINDS: [&str; 3] = ["audio", "character", "image"];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContentRecord {
    pub content_id: String,
    pub content_type: String,
    pub name: String,
    pub folder: PathBuf,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContentPackInfo {
    pub path: PathBuf,
    pub content_id: String,
    pub content_type: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub name: String,
    pub files: Vec<Value>,
    pub package_sha256: String,
    pub logical_content_hash: String,
    pub dependencies: Vec<String>,
    pub missing_dependencies: Vec<String>,
    pub collision_type: Option<String>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
fn collision_type(root: &Path, id: &str) -> Result<Option<String>> {
    content::validate_content_id(id)?;
    for kind in KINDS {
        let path = root.join(content::package_content_dir(kind, id)?);
        if path.exists() || path.is_symlink() {
            return Ok(Some(kind.into()));
        }
    }
    Ok(None)
}
pub fn get_content(root: &Path, id: &str) -> Result<ContentRecord> {
    content::validate_content_id(id)?;
    let mut found = None;
    for kind in KINDS {
        let folder = root.join(content::package_content_dir(kind, id)?);
        if folder.exists() {
            ensure!(found.is_none(), "内容 ID user:{id} 同时存在于多个类型");
            let (metadata, _) = content::resolve_content(root, kind, id)?;
            found = Some(ContentRecord {
                content_id: id.into(),
                content_type: kind.into(),
                name: metadata["name"].as_str().unwrap_or("").into(),
                folder,
                metadata,
            });
        }
    }
    found.with_context(|| format!("找不到用户内容 user:{id}"))
}
pub fn list_contents(root: &Path) -> Result<Vec<ContentRecord>> {
    let mut ids = BTreeSet::new();
    for kind in KINDS {
        let folder = root.join(format!("assets/user/{kind}"));
        if !folder.exists() {
            continue;
        }
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            if path.is_dir() {
                if let Some(id) = path.file_name().and_then(|v| v.to_str()) {
                    ids.insert(id.to_owned());
                }
            }
        }
    }
    Ok(ids
        .into_iter()
        .filter_map(|id| get_content(root, &id).ok())
        .collect())
}
fn default_namespace(root: &Path) -> String {
    let index = crate::load_json(root.join("registry.json")).unwrap_or(json!({}));
    if let Some(value) = index["default_namespace"]
        .as_str()
        .filter(|s| content::validate_content_id(&format!("{s}.x")).is_ok())
    {
        return value.into();
    }
    let name = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "custom".into());
    let mut name: String = name
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    if name.is_empty() {
        name = "custom".into();
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        name.insert(0, 'u');
    }
    name.truncate(32);
    name
}
pub fn rebuild_index(root: &Path) -> Result<()> {
    let records = list_contents(root)?;
    let data = json!({"schema":1,"default_namespace":default_namespace(root),"contents":records.iter().map(|r|json!({"id":r.content_id,"type":r.content_type})).collect::<Vec<_>>()});
    project::atomic_write(&root.join("registry.json"), &stable_json(&data)?)
}
pub fn normalize_dependencies(raw: &Value, own_id: &str) -> Result<Vec<String>> {
    if raw.is_null() {
        return Ok(vec![]);
    }
    let values = raw.as_array().context("dependencies 必须是内容 ID 列表")?;
    let mut found = BTreeSet::new();
    for value in values {
        let id = value
            .as_str()
            .context("dependencies 的每一项必须是内容 ID 文本")?
            .trim();
        let id = id.strip_prefix("user:").unwrap_or(id);
        content::validate_content_id(id)?;
        ensure!(id != own_id, "内容包不能依赖自己");
        found.insert(id.to_owned());
        ensure!(found.len() <= 128, "直接依赖不能超过 128 项");
    }
    Ok(found.into_iter().collect())
}
fn visible_text(value: &Value, label: &str, limit: usize) -> Result<String> {
    let text = value
        .as_str()
        .context(format!("{label} 必须是非空文本"))?
        .trim();
    ensure!(
        !text.is_empty() && text.chars().count() <= limit,
        "{label} 必须是 1~{limit} 个字符"
    );
    ensure!(
        !regex::Regex::new(r"[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]")?.is_match(text),
        "{label} 不能含换行、控制、零宽或双向格式字符"
    );
    Ok(text.into())
}
fn validate_version(value: &Value) -> Result<String> {
    let value = value.as_str().context("内容版本必须是 SemVer")?;
    ensure!(value.len()<=128 && regex::Regex::new(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$")?.is_match(value),"内容版本必须是 SemVer");
    let prerelease = value
        .split('+')
        .next()
        .unwrap_or(value)
        .split_once('-')
        .map(|v| v.1)
        .unwrap_or("");
    ensure!(
        !prerelease
            .split('.')
            .any(|v| v.len() > 1 && v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit())),
        "SemVer 的数字预发布标识不能有前导零"
    );
    Ok(value.into())
}
pub fn content_pack_defaults(root: &Path, id: &str) -> Result<Value> {
    let record = get_content(root, id)?;
    let stored = crate::load_json(record.folder.join(SHARE_META)).unwrap_or(json!({}));
    let dependencies = normalize_dependencies(stored.get("dependencies").unwrap_or(&json!([])), id)
        .unwrap_or_default();
    Ok(
        json!({"version":stored["version"].as_str().filter(|s|!s.is_empty()).unwrap_or("1.0.0"),"author":stored["author"].as_str().filter(|s|!s.is_empty()).map(str::to_owned).unwrap_or_else(||default_namespace(root)),"license":stored["license"].as_str().filter(|s|!s.is_empty()).unwrap_or("All Rights Reserved"),"dependencies":dependencies}),
    )
}
pub fn export_content_pack(
    root: &Path,
    path: &Path,
    id: &str,
    version: &str,
    author: &str,
    license: &str,
    dependencies: &Value,
) -> Result<ContentPackInfo> {
    let version = validate_version(&json!(version))?;
    let author = visible_text(&json!(author), "作者", 96)?;
    let license = visible_text(&json!(license), "许可证", 128)?;
    let record = get_content(root, id)?;
    let name = visible_text(&json!(record.name), "显示名称", 128)?;
    let dependencies = normalize_dependencies(dependencies, id)?;
    let mut entries = package::Entries::new();
    let mut files = vec![];
    for filename in content::listed_content_files(&record.metadata) {
        let source =
            package::resolve_confined_file(&record.folder, &record.folder.join(&filename))?;
        let bytes = fs::read(source)?;
        let key = format!("files/{filename}");
        files.push(json!({"path":key,"size":bytes.len(),"sha256":sha(&bytes)}));
        entries.insert(key, bytes);
    }
    entries.insert(MANIFEST.into(),stable_json(&json!({"content_pack_format":1,"content_schema":1,"id":id,"type":record.content_type,"name":name,"version":version,"author":author,"license":license,"dependencies":dependencies,"metadata":content::content_metadata_payload(&record.metadata),"files":files}))?);
    // Prepare and validate the complete package before replacing the requested destination.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let stage = tempfile::tempdir_in(parent)?;
    let staged = stage.path().join("export.lomcontent");
    package::write_package(entries, &staged)?;
    let mut info = inspect_content_pack(root, &staged)?;
    ensure!(!path.is_symlink(), "拒绝通过符号链接导出内容包");
    fs::rename(&staged, path)?;
    info.path = path.into();
    info.collision_type = None;
    info.missing_dependencies.clear();
    Ok(info)
}
fn archive_entries(path: &Path) -> Result<(Vec<u8>, package::Entries)> {
    let mut file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_CONTENT_PACK_BYTES,
        "内容包超过 128 MiB 上限"
    );
    let mut bytes = vec![];
    file.by_ref()
        .take(MAX_CONTENT_PACK_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONTENT_PACK_BYTES,
        "内容包超过 128 MiB 上限"
    );
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes))?;
    // zip 2.x deduplicates its directory map: count raw central headers before trusting it.
    let mut offset = archive.central_directory_start() as usize;
    let mut count = 0;
    while bytes.get(offset..offset + 4) == Some(b"PK\x01\x02") {
        let header = bytes
            .get(offset..offset + 46)
            .context("内容包中央目录截断")?;
        let u16_at = |i| u16::from_le_bytes([header[i], header[i + 1]]) as usize;
        offset = offset
            .checked_add(46 + u16_at(28) + u16_at(30) + u16_at(32))
            .context("中央目录长度越界")?;
        ensure!(offset <= bytes.len(), "内容包中央目录长度越界");
        count += 1;
        ensure!(count <= 2048, "内容包条目过多（最多 2048）");
    }
    ensure!(count == archive.len(), "内容包存在重复路径或无效中央目录");
    let mut entries = package::Entries::new();
    let mut folded = BTreeSet::new();
    let mut all_paths = vec![];
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = package::canonical_archive_name(entry.name())?;
        ensure!(!entry.is_symlink(), "内容包不允许符号链接");
        let key: String = name.case_fold().collect();
        ensure!(
            folded.insert(key.clone()),
            "内容包重复或大小写冲突路径：{name}"
        );
        let size = entry.size();
        total = total.checked_add(size).context("内容包长度越界")?;
        ensure!(
            size <= content::MAX_AUDIO_BYTES && total <= MAX_CONTENT_PACK_BYTES,
            "内容包解压大小超限"
        );
        all_paths.push((key, entry.is_dir()));
        let mut data = vec![];
        entry.take(size + 1).read_to_end(&mut data)?;
        ensure!(data.len() as u64 == size, "内容包条目大小不一致：{name}");
        if !name.ends_with('/') {
            entries.insert(name, data);
        }
    }
    let files: BTreeSet<_> = all_paths
        .iter()
        .filter(|(_, dir)| !*dir)
        .map(|(name, _)| name.as_str())
        .collect();
    for (name, dir) in &all_paths {
        ensure!(
            !*dir || !files.contains(name.trim_end_matches('/')),
            "内容包路径同时是文件和目录"
        );
        let parts: Vec<_> = name.trim_end_matches('/').split('/').collect();
        for n in 1..parts.len() {
            ensure!(
                !files.contains(parts[..n].join("/").as_str()),
                "内容包路径同时是文件和目录"
            );
        }
    }
    drop(archive);
    Ok((bytes, entries))
}
fn inspect_loaded(
    root: &Path,
    path: &Path,
    raw: &[u8],
    entries: &package::Entries,
) -> Result<(ContentPackInfo, Value)> {
    let bytes = entries
        .get(MANIFEST)
        .context("内容包缺少 content-pack.json")?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "content-pack.json 超过 4 MiB 上限"
    );
    let manifest: Value =
        serde_json::from_str(std::str::from_utf8(bytes)?.trim_start_matches('\u{feff}'))?;
    for key in ["content_pack_format", "content_schema"] {
        ensure!(manifest[key].as_f64() == Some(1.0), "只支持 {key}=1");
    }
    let id = manifest["id"].as_str().context("内容包缺少 id")?;
    content::validate_content_id(id)?;
    let kind = manifest["type"].as_str().context("内容包缺少 type")?;
    ensure!(KINDS.contains(&kind), "内容包 type 无效");
    let version = validate_version(&manifest["version"])?;
    let author = visible_text(&manifest["author"], "作者", 96)?;
    let license = visible_text(&manifest["license"], "许可证", 128)?;
    let name = visible_text(&manifest["name"], "显示名称", 128)?;
    let metadata = content::normalize_content_metadata(&manifest["metadata"])?;
    ensure!(
        metadata["id"] == id && metadata["type"] == kind,
        "内容包 id/type 与 metadata 不一致"
    );
    let dependencies =
        normalize_dependencies(manifest.get("dependencies").unwrap_or(&Value::Null), id)?;
    let rows = manifest["files"]
        .as_array()
        .filter(|a| !a.is_empty())
        .context("内容包 files 必须是非空列表")?;
    let expected: BTreeSet<_> = content::listed_content_files(&metadata)
        .into_iter()
        .collect();
    let mut declared = BTreeSet::new();
    let mut normalized = vec![];
    let mut allowed = BTreeSet::from([MANIFEST.to_owned(), HASH.to_owned()]);
    for row in rows {
        let path = row["path"].as_str().context("files 条目缺少 path")?;
        package::canonical_archive_name(path)?;
        let parts: Vec<_> = path.split('/').collect();
        ensure!(
            parts.len() == 2 && parts[0] == "files" && !parts[1].is_empty(),
            "内容文件必须位于 files/ 且不能有子目录"
        );
        ensure!(declared.insert(parts[1].to_owned()), "内容包重复声明文件");
        let size = row["size"].as_u64().context("size 必须是非负整数")?;
        let hash = row["sha256"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .context("sha256 必须是 64 位十六进制")?
            .to_uppercase();
        let data = entries.get(path).context("内容包缺少已声明文件")?;
        ensure!(data.len() as u64 == size, "{path} 的实际大小与声明不一致");
        ensure!(
            size <= if kind == "audio" {
                content::MAX_AUDIO_BYTES
            } else {
                content::MAX_IMAGE_BYTES
            },
            "{path} 大小超限"
        );
        ensure!(sha(data) == hash, "{path} 的 SHA-256 与声明不一致");
        normalized.push(json!({"path":path,"size":size,"sha256":hash}));
        allowed.insert(path.into());
    }
    ensure!(declared == expected, "files 与 metadata 不一致");
    ensure!(
        entries.keys().all(|k| allowed.contains(k)),
        "内容包含未声明条目"
    );
    let hash = entries
        .get(HASH)
        .filter(|v| v.len() <= 512)
        .context("内容包缺少有效的 package-content.sha256")?;
    let fields: std::collections::BTreeMap<_, _> = std::str::from_utf8(hash)?
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();
    let logical = package::content_hash(entries);
    ensure!(
        fields.get("algorithm") == Some(&"lom-entry-sha256-v1")
            && fields
                .get("sha256")
                .is_some_and(|s| s.to_uppercase() == logical),
        "内容包逻辑内容哈希无效或不匹配"
    );
    let missing_dependencies = dependencies
        .iter()
        .filter(|id| get_content(root, id).is_err())
        .cloned()
        .collect();
    Ok((
        ContentPackInfo {
            path: path.into(),
            content_id: id.into(),
            content_type: kind.into(),
            version,
            author,
            license,
            name,
            files: normalized,
            package_sha256: sha(raw),
            logical_content_hash: logical,
            dependencies,
            missing_dependencies,
            collision_type: collision_type(root, id)?,
        },
        metadata,
    ))
}
pub fn inspect_content_pack(root: &Path, path: &Path) -> Result<ContentPackInfo> {
    let (raw, entries) = archive_entries(path)?;
    Ok(inspect_loaded(root, path, &raw, &entries)?.0)
}
pub fn import_content_pack(root: &Path, path: &Path) -> Result<ContentPackInfo> {
    // One immutable snapshot is both validated and installed; later path changes cannot alter it.
    let (raw, entries) = archive_entries(path)?;
    let (mut info, metadata) = inspect_loaded(root, path, &raw, &entries)?;
    ensure!(
        info.collision_type.is_none(),
        "内容 ID user:{} 已存在；不会覆盖旧版本或跨类型条目",
        info.content_id
    );
    fs::create_dir_all(root)?;
    let canonical = root.canonicalize()?;
    let stage = tempfile::Builder::new()
        .prefix(".lomcontent-")
        .tempdir_in(&canonical)?;
    let folder = stage.path().join("content");
    fs::create_dir(&folder)?;
    fs::write(
        folder.join("content.json"),
        stable_json(&content::content_metadata_payload(&metadata))?,
    )?;
    fs::write(
        folder.join(SHARE_META),
        stable_json(
            &json!({"content_pack_format":1,"version":info.version,"author":info.author,"license":info.license,"dependencies":info.dependencies,"package_sha256":info.package_sha256,"logical_content_hash":info.logical_content_hash}),
        )?,
    )?;
    for row in &info.files {
        let key = row["path"].as_str().unwrap();
        let name = key.strip_prefix("files/").unwrap();
        fs::write(folder.join(name), &entries[key])?;
    }
    let target = canonical.join(content::package_content_dir(
        &info.content_type,
        &info.content_id,
    )?);
    let parent = target.parent().unwrap();
    let existing = parent
        .ancestors()
        .find(|p| p.exists())
        .context("内容库路径无效")?;
    ensure!(
        existing.canonicalize()?.starts_with(&canonical),
        "内容库目录指向根目录外"
    );
    fs::create_dir_all(parent)?;
    ensure!(
        parent.canonicalize()?.starts_with(&canonical),
        "内容库目录指向根目录外"
    );
    ensure!(
        collision_type(&canonical, &info.content_id)?.is_none(),
        "内容 ID 在导入期间发生冲突，未覆盖文件"
    );
    fs::rename(&folder, &target).context("无法安装内容包")?;
    if let Err(error) = rebuild_index(&canonical) {
        // The final directory was created by this operation. A failed registry
        // update must not leave a supposedly failed import installed.
        fs::remove_dir_all(&target).context("更新索引失败，且无法撤销本次内容导入")?;
        return Err(error).context("更新内容库索引失败，已撤销本次导入");
    }
    info.collision_type = None;
    Ok(info)
}
