//! A fixed-allowlist ZIP: never traverse a project or game tree.
use super::{nodes, text, PreflightIssue};
use crate::{project::Project, stable_json};
use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub const MAX_LOG_READ_BYTES: usize = 1024 * 1024;
pub const MAX_LOG_OUTPUT_CHARS: usize = 256 * 1024;
pub const MAX_COLLECTION_ITEMS: usize = 1000;
pub const MAX_STRING_CHARS: usize = 8192;
const MAX_DEPTH: usize = 8;
#[derive(Clone, Debug)]
pub struct DiagnosticOptions {
    pub game_root: Option<PathBuf>,
    pub bundled_runtime: Option<PathBuf>,
    pub crash_log: Option<PathBuf>,
    pub runtime_log: Option<PathBuf>,
    pub editor_version: String,
    pub runtime_version: String,
    /// Additional known roots (for example the authoring project directory).
    pub private_roots: Vec<(PathBuf, String)>,
}
impl Default for DiagnosticOptions {
    fn default() -> Self {
        Self {
            game_root: None,
            bundled_runtime: None,
            crash_log: None,
            runtime_log: None,
            editor_version: env!("CARGO_PKG_VERSION").into(),
            runtime_version: "1.1.2".into(),
            private_roots: Vec::new(),
        }
    }
}
fn roots(options: &DiagnosticOptions) -> Vec<(String, String)> {
    let mut paths = options.private_roots.clone();
    for (env, label) in [
        ("HOME", "<user-home>"),
        ("USERPROFILE", "<user-home>"),
        ("APPDATA", "<appdata>"),
    ] {
        if let Some(value) = std::env::var_os(env) {
            if !value.is_empty() {
                paths.push((value.into(), label.into()));
            }
        }
    }
    paths.push((std::env::temp_dir(), "<temp>".into()));
    if let Ok(path) = std::env::current_dir() {
        paths.push((path, "<project-dir>".into()));
    }
    if let Some(root) = &options.game_root {
        paths.push((root.clone(), "<game-dir>".into()));
    }
    let mut roots = Vec::new();
    for (path, label) in paths {
        for variant in std::iter::once(path.clone()).chain(path.canonicalize().ok()) {
            let root = variant.to_string_lossy().into_owned();
            if root.len() > 1 {
                roots.push((root.clone(), label.clone()));
                roots.push((root.replace('\\', "/"), label.clone()));
            }
        }
    }
    roots.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
    roots.dedup();
    roots
}
struct Sanitizer {
    replacements: Vec<(regex::Regex, String)>,
    unc: regex::Regex,
    windows: regex::Regex,
    posix: regex::Regex,
}
impl Sanitizer {
    fn new(options: &DiagnosticOptions) -> Self {
        let mut replacements: Vec<_> = roots(options)
            .into_iter()
            .map(|(root, label)| {
                (
                    regex::RegexBuilder::new(&regex::escape(&root))
                        .case_insensitive(true)
                        .build()
                        .unwrap(),
                    label,
                )
            })
            .collect();
        if let Some(name) = std::env::var("USERNAME")
            .ok()
            .or_else(|| std::env::var("USER").ok())
            .filter(|s| !s.is_empty())
        {
            replacements.push((
                regex::RegexBuilder::new(&regex::escape(&name))
                    .case_insensitive(true)
                    .build()
                    .unwrap(),
                "<user>".into(),
            ));
        }
        Self {
            replacements,
            unc: regex::Regex::new(r#"\\\\[^\\/\s]+[\\/][^\r\n\"<>|]*"#).unwrap(),
            windows: regex::Regex::new(r#"(?i)(^|[^A-Za-z0-9_])[A-Z]:[\\/][^\r\n\"<>|]*"#).unwrap(),
            posix: regex::Regex::new(
                r#"(^|[^A-Za-z0-9_:])/(?:Users|home|tmp|var/tmp)/[^\r\n\"<>|]*"#,
            )
            .unwrap(),
        }
    }
    fn text(&self, value: &str) -> String {
        let mut result = value.to_owned();
        for (re, label) in &self.replacements {
            result = re.replace_all(&result, regex::NoExpand(label)).into_owned();
        }
        result = self.unc.replace_all(&result, "<network-path>").into_owned();
        result = self
            .windows
            .replace_all(&result, "${1}<local-path>")
            .into_owned();
        result = self
            .posix
            .replace_all(&result, "${1}<local-path>")
            .into_owned();
        if result.chars().count() > MAX_STRING_CHARS {
            result = result.chars().take(MAX_STRING_CHARS).chain(['…']).collect();
        }
        result
    }
}
pub fn sanitize_text(value: &str, options: &DiagnosticOptions) -> String {
    Sanitizer::new(options).text(value)
}
fn sanitize_value(value: &Value, sanitizer: &Sanitizer, depth: usize, budget: &mut usize) -> Value {
    if depth >= MAX_DEPTH {
        return json!("<depth-limit>");
    }
    if *budget == 0 {
        return json!("<collection-limit>");
    }
    *budget -= 1;
    match value {
        Value::String(s) => json!(sanitizer.text(s)),
        Value::Array(values) => {
            let mut items: Vec<_> = values
                .iter()
                .take(MAX_COLLECTION_ITEMS)
                .map(|v| sanitize_value(v, sanitizer, depth + 1, budget))
                .collect();
            if values.len() > MAX_COLLECTION_ITEMS {
                items.push(json!(format!(
                    "<{} more items>",
                    values.len() - MAX_COLLECTION_ITEMS
                )));
            }
            json!(items)
        }
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for key in keys.into_iter().take(MAX_COLLECTION_ITEMS) {
                out.insert(
                    sanitizer.text(key),
                    sanitize_value(&values[key], sanitizer, depth + 1, budget),
                );
            }
            if values.len() > MAX_COLLECTION_ITEMS {
                out.insert(
                    "<truncated>".into(),
                    json!(values.len() - MAX_COLLECTION_ITEMS),
                );
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}
fn read_tail(path: Option<&Path>, limit: usize) -> String {
    let read = || -> std::io::Result<String> {
        let Some(path) = path else {
            return Ok(String::new());
        };
        let mut f = fs::File::open(path)?;
        let length = f.metadata()?.len();
        f.seek(SeekFrom::Start(length.saturating_sub(limit as u64)))?;
        let mut data = Vec::new();
        f.take(limit as u64).read_to_end(&mut data)?;
        Ok(String::from_utf8_lossy(&data).into_owned())
    };
    read().unwrap_or_default()
}
fn digest(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut stream = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().to_vec())
}
pub fn detect_runtime_version(options: &DiagnosticOptions, log: &str) -> String {
    let installed = options
        .game_root
        .as_ref()
        .map(|p| p.join("BepInEx/plugins/MortalModHost/MortalModHost.dll"));
    if let (Some(installed), Some(bundled)) = (&installed, &options.bundled_runtime) {
        if let (Ok(a), Ok(b)) = (digest(installed), digest(bundled)) {
            if a == b {
                return options.runtime_version.clone();
            }
        }
    }
    let re = regex::Regex::new(r"MortalModHost\s+([0-9]+(?:\.[0-9A-Za-z_-]+)+)\s+启动").unwrap();
    if let Some(found) = re.captures_iter(log).last() {
        return found[1].into();
    }
    if installed.is_some_and(|p| p.is_file()) {
        format!(
            "unknown (installed runtime differs from bundled {})",
            options.runtime_version
        )
    } else {
        "not installed".into()
    }
}
pub fn detect_game_version(game_root: Option<&Path>) -> String {
    let Some(root) = game_root else {
        return "not configured".into();
    };
    let Some(steamapps) = root.parent().and_then(Path::parent) else {
        return "unknown".into();
    };
    let log = read_tail(Some(&steamapps.join("appmanifest_1859910.acf")), 512 * 1024);
    let re = regex::Regex::new(r#""buildid"\s+"([0-9]+)""#).unwrap();
    re.captures_iter(&log)
        .last()
        .map(|m| format!("Steam build {}", &m[1]))
        .unwrap_or_else(|| "unknown".into())
}
fn filtered_runtime_log(raw: &str, sanitizer: &Sanitizer) -> String {
    let mut lines = Vec::new();
    let mut continuation = 0;
    for line in raw.lines() {
        let lower = line.to_lowercase();
        let marked = [
            "mortalmodhost",
            "mod-runtime-error",
            "mortalmodhost.dll",
            "玩家内容披露",
            "mod 演出",
            ".lommod",
        ]
        .iter()
        .any(|marker| lower.contains(marker));
        if marked {
            lines.push(line);
            continuation = 8;
        } else if continuation > 0
            && (line.starts_with([' ', '\t'])
                || ["at ", "--- End of", "Caused by:"]
                    .iter()
                    .any(|prefix| line.trim_start().starts_with(prefix)))
        {
            lines.push(line);
            continuation -= 1;
        } else {
            continuation = 0;
        }
    }
    let text = lines[lines.len().saturating_sub(600)..]
        .iter()
        .map(|line| sanitizer.text(line))
        .collect::<Vec<_>>()
        .join("\n");
    let skip = text.chars().count().saturating_sub(MAX_LOG_OUTPUT_CHARS);
    text.chars().skip(skip).collect()
}
fn created_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Civil calendar conversion of days since 1970-01-01, valid for UTC dates.
    let z = seconds as i64 / 86400 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}
pub fn export_diagnostic_bundle(
    output: &Path,
    project: &Project,
    issues: &[PreflightIssue],
    options: &DiagnosticOptions,
) -> Result<PathBuf> {
    let mut destination = output.to_path_buf();
    if destination
        .extension()
        .is_none_or(|s| !s.eq_ignore_ascii_case("zip"))
    {
        destination.set_extension("zip");
    }
    let mut options = options.clone();
    if let Some(source) = &project.source {
        options.private_roots.push((
            if source.is_dir() {
                source.clone()
            } else {
                source.parent().unwrap_or(source).to_owned()
            },
            "<project-dir>".into(),
        ));
    }
    let sanitizer = Sanitizer::new(&options);
    let errors = issues.iter().filter(|i| i.severity == "error").count();
    let warnings = issues.iter().filter(|i| i.severity == "warning").count();
    let validation = json!({"errors":errors,"warnings":warnings,"truncated":issues.len().saturating_sub(MAX_COLLECTION_ITEMS),"issues":issues.iter().take(MAX_COLLECTION_ITEMS).map(|i|json!({"severity":sanitizer.text(&i.severity),"code":sanitizer.text(&i.code),"story":sanitizer.text(&i.story_id),"node":sanitizer.text(&i.node_id),"message":sanitizer.text(&i.message),"fixable":i.fixable})).collect::<Vec<_>>()});
    let mut types: BTreeMap<String, usize> = BTreeMap::new();
    let mut user_refs = BTreeSet::new();
    let mut count = 0;
    let mut raw = 0;
    for n in project
        .stories
        .values()
        .flat_map(nodes)
        .filter(|n| n.is_object())
    {
        count += 1;
        let kind = if text(n, "type").is_empty() {
            "unknown"
        } else {
            text(n, "type")
        };
        *types.entry(kind.into()).or_default() += 1;
        if kind == "raw" {
            raw += 1;
        }
        for v in n.as_object().unwrap().values() {
            if let Some(s) = v.as_str().filter(|s| s.starts_with("user:")) {
                user_refs.insert(s);
            }
        }
    }
    let metadata = json!({"story_count":project.stories.len(),"story_ids":project.stories.keys().take(MAX_COLLECTION_ITEMS).collect::<Vec<_>>(),"node_count":count,"node_types":types,"raw_node_count":raw,"user_content_reference_count":user_refs.len(),"editor_data_schema":crate::validate::editor_data()["schema"],"campaign_enabled":project.manifest["campaign"].is_object()});
    let runtime_path = options.runtime_log.clone().or_else(|| {
        options
            .game_root
            .as_ref()
            .map(|root| root.join("BepInEx/LogOutput.log"))
    });
    let runtime_raw = read_tail(runtime_path.as_deref(), MAX_LOG_READ_BYTES);
    let mut manifest = sanitize_value(&project.manifest, &sanitizer, 0, &mut 10_000);
    if serde_json::to_vec(&manifest)?.len() > 4 * 1024 * 1024 {
        manifest = json!({"truncated":"<metadata-size-limit>"});
    }
    let diagnostic = json!({"diagnostic_format":1,"created_utc":created_utc(),"editor_version":sanitizer.text(&options.editor_version),"runtime_version":detect_runtime_version(&options,&runtime_raw),"detected_game_version":detect_game_version(options.game_root.as_deref()),"manifest":manifest,"validation_summary":{"errors":errors,"warnings":warnings,"truncated":issues.len().saturating_sub(MAX_COLLECTION_ITEMS)},"project_metadata":sanitize_value(&metadata,&sanitizer,0,&mut 10_000),"privacy":{"fixed_allowlist":true,"project_story_content_included":false,"user_content_included":false,"game_files_included":false,"absolute_paths_redacted":true}});
    let editor_log = sanitizer.text(&read_tail(options.crash_log.as_deref(), MAX_LOG_READ_BYTES));
    let runtime_log = filtered_runtime_log(&runtime_raw, &sanitizer);
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut archive = zip::ZipWriter::new(temp.as_file_mut());
        let zip_options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        // Do not add user-supplied paths to this allowlist.
        let entries=[("diagnostic.json",stable_json(&diagnostic)?),("validation.json",stable_json(&validation)?),("logs/editor-crash.log",editor_log.into_bytes()),("logs/runtime.log",runtime_log.into_bytes()),("README.txt",b"lom_modkit diagnostic bundle\nContains only version/manifest/project counts, validation, and bounded relevant logs.\nDoes not copy stories, user content, saves, mods, or game files. Private directory prefixes are redacted.\n".to_vec())];
        for (name, data) in entries {
            archive.start_file(name, zip_options)?;
            archive.write_all(&data)?;
        }
        archive.finish()?;
    }
    temp.as_file().sync_all()?;
    temp.persist(&destination)?;
    Ok(destination)
}
