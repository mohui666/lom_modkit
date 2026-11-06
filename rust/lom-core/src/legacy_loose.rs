//! Read the directory contract used by Binarizer's Mortal ModSupport branch.
//! The selected directory and its scripts remain read-only during import.
#[path = "legacy_resources.rs"]
mod resources;
use anyhow::{ensure, Context, Result};
pub use resources::references as resource_references;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryEntry {
    pub script: String,
    pub title: String,
    pub author: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnsupportedFile {
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyMod {
    pub name: String,
    pub scripts: BTreeMap<String, String>,
    pub entries: Vec<StoryEntry>,
    /// Full LeanLocalization keys, including prefixes such as Story/.
    pub strings: BTreeMap<String, String>,
    pub unsupported: Vec<UnsupportedFile>,
    pub resources: resources::Resources,
    pub extensions: crate::legacy_extensions::Extensions,
}

impl LegacyMod {
    pub fn story_translations(&self) -> BTreeMap<String, String> {
        self.strings
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix("Story/")
                    .map(|key| (key.to_owned(), value.clone()))
            })
            .collect()
    }

    pub fn require_supported(&self) -> Result<()> {
        ensure!(
            self.unsupported.is_empty(),
            "旧 MOD 含尚未接入的内容，不能生成可玩包：{}",
            self.unsupported
                .iter()
                .map(|file| format!("{}（{}）", file.path, file.reason))
                .collect::<Vec<_>>()
                .join("、")
        );
        Ok(())
    }

    /// A saved copy uses the current package contract and retains the old script keys.
    /// Its entry is the first valid CustomStory row, matching the old story menu order.
    pub fn into_project(self) -> Result<crate::project::Project> {
        self.require_supported()?;
        let entry = self
            .entries
            .first()
            .map(|entry| entry.script.as_str())
            .or_else(|| self.scripts.keys().next().map(String::as_str))
            .unwrap_or("main");
        let mut project = if self.entries.is_empty() && !self.scripts.is_empty() {
            crate::project::Project::new_vanilla_patch(entry)?
        } else {
            crate::project::Project::new()
        };
        project.stories.clear();
        project.manifest["id"] = json!(format!("legacy_{}", uuid::Uuid::new_v4().simple()));
        project.manifest["name"] = json!(self.name);
        project.manifest["description"] = json!("从旧 Mods 散装剧情目录导入");
        project.manifest["entry"] = json!(entry);
        if let Some(custom) = self.entries.first() {
            project.manifest["name"] = json!(custom.title);
            project.manifest["author"] = json!(custom.author);
        }
        let translations = self.story_translations();
        for (key, source) in &self.scripts {
            let title = self
                .entries
                .iter()
                .find(|entry| &entry.script == key)
                .map(|entry| entry.title.as_str())
                .unwrap_or(key);
            let mut story = crate::native_flow::import(source, key, title, key, &translations)
                .with_context(|| format!("旧脚本 story/{key}.lua 无法完整转换"))?;
            story["vanilla_import"]["external"] = json!(true);
            let voices: BTreeMap<usize, String> = story["vanilla_import"]["origins"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(index, origin)| {
                    voice_reference(&origin["operations"], &self.resources.voices)
                        .map(|voice| (index, voice))
                })
                .collect();
            for node in story["nodes"].as_array_mut().unwrap() {
                if node["type"] == "say" {
                    if let Some(voice) = crate::vanilla_import::origin_index(node)
                        .and_then(|index| voices.get(&index))
                    {
                        node["voice"] = json!(voice);
                    }
                    convert_voice_marker(node, &self.resources.voices);
                }
                translate_node(node, &translations);
            }
            for (index, origin) in story["vanilla_import"]["origins"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                if origin["node"]["type"] == "say" {
                    if let Some(voice) = voices.get(&index) {
                        origin["node"]["voice"] = json!(voice);
                    }
                    convert_voice_marker(&mut origin["node"], &self.resources.voices);
                }
                if let Some(ops) = origin.get_mut("operations") {
                    replace_translations(ops, &translations);
                }
                translate_node(&mut origin["node"], &translations);
            }
            crate::validate::validate_story(&story)?;
            project.stories.insert(key.clone(), story);
        }
        self.resources.apply(&mut project, &self.strings)?;
        self.extensions.apply(&mut project)?;
        crate::legacy_activation::bootstrap(&mut project)?;
        Ok(project)
    }
}

fn voice_reference(value: &Value, voices: &BTreeMap<String, String>) -> Option<String> {
    if value["kind"] == "call"
        && value["function"]["kind"] == "member"
        && value["function"]["object"]["kind"] == "name"
        && value["function"]["object"]["name"] == "luamanager"
        && value["function"]["field"] == "GetStoryText"
        && value["args"][0]["kind"] == "literal"
    {
        return value["args"][0]["value"]
            .as_str()
            .and_then(|key| voices.get(key))
            .cloned();
    }
    match value {
        Value::Array(values) => values
            .iter()
            .find_map(|value| voice_reference(value, voices)),
        Value::Object(values) => values
            .values()
            .find_map(|value| voice_reference(value, voices)),
        _ => None,
    }
}

fn convert_voice_marker(node: &mut Value, voices: &BTreeMap<String, String>) {
    let Some(text) = node["text"].as_str() else {
        return;
    };
    let Some(rest) = text.strip_prefix("ac<") else {
        return;
    };
    let Some((key, text)) = rest.split_once('>') else {
        return;
    };
    if let Some(voice) = voices.get(key) {
        let text = text.to_owned();
        node["voice"] = json!(voice);
        node["text"] = json!(text);
    }
}

fn translate_node(node: &mut Value, translations: &BTreeMap<String, String>) {
    for key in ["operations", "condition"] {
        if let Some(value) = node.get_mut(key) {
            replace_translations(value, translations);
        }
    }
}

fn replace_translations(value: &mut Value, translations: &BTreeMap<String, String>) {
    if value["kind"] == "call" {
        let function = &value["function"];
        if function["kind"] == "member"
            && function["object"]["kind"] == "name"
            && function["object"]["name"] == "luamanager"
            && function["field"] == "GetStoryText"
            && value["args"].as_array().is_some_and(|args| args.len() == 1)
            && value["args"][0]["kind"] == "literal"
        {
            if let Some(text) = value["args"][0]["value"]
                .as_str()
                .and_then(|key| translations.get(key))
            {
                *value = json!({"kind":"literal","value":text});
                return;
            }
        }
    }
    if value["kind"] == "literal" {
        if let Some(text) = value["value"]
            .as_str()
            .filter(|key| key.starts_with("O_"))
            .and_then(|key| translations.get(key))
        {
            value["value"] = json!(text);
        }
    }
    match value {
        Value::Array(values) => {
            for child in values {
                replace_translations(child, translations);
            }
        }
        Value::Object(fields) => {
            for child in fields.values_mut() {
                replace_translations(child, translations);
            }
        }
        _ => {}
    }
}

pub fn has_scripts(root: &Path) -> Result<bool> {
    if !root.join("story").is_dir() {
        return Ok(false);
    }
    let mut files = Vec::new();
    collect_files(root, &root.join("story"), &mut files)?;
    Ok(files.iter().any(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
    }))
}

pub fn has_content(root: &Path) -> Result<bool> {
    if root.join("StringTable.csv").is_file() || has_scripts(root)? {
        return Ok(true);
    }
    for (directory, extensions) in [
        ("Portraits", &["png", "jpg", "gif"][..]),
        ("Voice", &["mp3"][..]),
        ("DataTable", &["json"][..]),
        ("LuaEquivalent", &["lua"][..]),
    ] {
        let directory = root.join(directory);
        if !directory.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_files(root, &directory, &mut files)?;
        if files.iter().any(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extensions
                        .iter()
                        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                })
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn read(root: &Path) -> Result<LegacyMod> {
    let root = root.canonicalize().context("无法打开旧 MOD 目录")?;
    ensure!(root.is_dir(), "请选择 Mods 下的单个旧 MOD 目录");
    let mut files = Vec::new();
    collect_files(&root, &root, &mut files)?;
    files.sort();
    let resources = resources::Resources::read(&root, &files)?;
    let extensions = crate::legacy_extensions::read(&root)?;
    let handled_files: std::collections::BTreeSet<_> = resources
        .handled_files
        .iter()
        .chain(&extensions.handled_files)
        .map(|path| path.to_ascii_lowercase())
        .collect();
    let mut result = LegacyMod {
        name: root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("旧 MOD")
            .to_owned(),
        scripts: BTreeMap::new(),
        entries: Vec::new(),
        strings: BTreeMap::new(),
        unsupported: extensions
            .unsupported
            .iter()
            .map(|(path, reason)| UnsupportedFile {
                path: path.clone(),
                reason: reason.clone(),
            })
            .collect(),
        resources,
        extensions,
    };
    let mut entry_rows = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        let lower = relative.to_ascii_lowercase();
        if lower.starts_with("story/") && lower.ends_with(".lua") {
            // The old loader keys scripts by basename, independently of subdirectories.
            let key = path
                .file_stem()
                .and_then(|name| name.to_str())
                .context("旧脚本文件名无效")?;
            ensure!(
                crate::vanilla_patch::valid_target(key),
                "旧脚本编号无法接入：{key}"
            );
            ensure!(
                !result.scripts.contains_key(key),
                "旧 MOD 含同名脚本，无法确定优先文件：{key}"
            );
            let source = read_text(&path)?;
            let tokens = crate::vanilla_patch::tokens(&source);
            for window in tokens.windows(3) {
                if !window[0].string
                    && window[0].value == "ext"
                    && window[1].value == "."
                    && !crate::legacy_extensions::supports_ext(&window[2].value)
                {
                    result.unsupported.push(UnsupportedFile {
                        path: relative.clone(),
                        reason: format!("未识别的旧扩展 ext.{}", window[2].value),
                    });
                }
            }
            result.scripts.insert(key.to_owned(), source);
        } else if lower == "stringtable.csv" {
            for row in csv_rows(&read_text(&path)?)? {
                if row.first().is_none_or(|key| key.is_empty()) {
                    continue;
                }
                ensure!(row.len() >= 2, "StringTable.csv 的非空行缺少文字列");
                // HookMods.AddStringTable keeps the first occurrence, with exact casing.
                result
                    .strings
                    .entry(row[0].clone())
                    .or_insert_with(|| row[1].clone());
            }
        } else if lower == "customstory.csv" {
            entry_rows = csv_rows(&read_text(&path)?)?;
        } else if handled_files.contains(&lower)
            || result
                .extensions
                .unsupported
                .iter()
                .any(|(file, _)| file.eq_ignore_ascii_case(&relative))
            || is_readme(&lower)
        {
            continue;
        } else {
            result.unsupported.push(UnsupportedFile {
                path: relative,
                reason: if lower.starts_with("datatable/") {
                    "未识别的 DataTable 文件结构"
                } else if lower.starts_with("luaequivalent/") {
                    "未识别的 LuaEquivalent 文件"
                } else if lower.starts_with("portraits/") {
                    "未识别的旧立绘文件"
                } else if lower.starts_with("voice/") {
                    "未识别的旧配音文件，旧加载器仅读取 MP3"
                } else {
                    "未识别的旧资源文件"
                }
                .into(),
            });
        }
    }
    ensure!(
        !result.scripts.is_empty()
            || !result.resources.handled_files.is_empty()
            || !result.extensions.handled_files.is_empty()
            || !result.strings.is_empty(),
        "目录中没有可识别的旧 MOD 剧情、资源、数据表或本地化"
    );
    for row in entry_rows {
        if row.len() < 3
            || row[0].is_empty()
            || !result.scripts.contains_key(&row[0])
            || result.entries.iter().any(|entry| entry.script == row[0])
        {
            continue;
        }
        result.entries.push(StoryEntry {
            script: row[0].clone(),
            title: row[1].clone(),
            author: row[2].clone(),
        });
    }
    Ok(result)
}

fn is_readme(path: &str) -> bool {
    !path.contains('/')
        && (path.ends_with(".md") || path.starts_with("readme") || path.starts_with("license"))
}

fn read_text(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("无法读取 {}", path.display()))?;
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        ensure!(
            (bytes.len() - 2) % 2 == 0,
            "旧 MOD 的 UTF-16 文件长度无效：{}",
            path.display()
        );
        let little = bytes.starts_with(&[0xff, 0xfe]);
        let units = bytes[2..].chunks_exact(2).map(|pair| {
            if little {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        });
        return char::decode_utf16(units)
            .collect::<std::result::Result<String, _>>()
            .with_context(|| format!("旧 MOD 的 UTF-16 编码无效：{}", path.display()));
    }
    let text = std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
        .with_context(|| format!("旧 MOD 文件需要 UTF-8 编码：{}", path.display()))?;
    Ok(text.into())
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "旧 MOD 不能通过链接读取目录外文件：{}",
            entry.path().display()
        );
        if kind.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if kind.is_file() {
            let path = crate::package::resolve_confined_file(root, &entry.path())?;
            files.push(path);
        }
    }
    Ok(())
}

/// Standard comma-separated records. Quoted fields retain embedded newlines and commas.
fn csv_rows(source: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut after_quote = false;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    after_quote = true;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        if ch == '"' && field.is_empty() && !after_quote {
            quoted = true;
            continue;
        }
        ensure!(
            !after_quote || matches!(ch, ',' | '\r' | '\n'),
            "CSV 引号结束后有意外字符"
        );
        match ch {
            ',' => {
                row.push(std::mem::take(&mut field));
                after_quote = false;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                after_quote = false;
            }
            _ => field.push(ch),
        }
    }
    ensure!(!quoted, "CSV 存在未闭合的引号");
    if !field.is_empty() || !row.is_empty() || after_quote {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_csv_supports_quoted_newlines_and_keeps_the_first_key() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("story")).unwrap();
        fs::write(
            root.path().join("story/demo.lua"),
            "say(luamanager.GetStoryText(\"D_TEST\"))\n",
        )
        .unwrap();
        fs::write(
            root.path().join("StringTable.csv"),
            "\u{feff}Story/D_TEST,\"第一行,有逗号\r\n第二行\"\r\nStory/D_TEST,忽略后项\r\n",
        )
        .unwrap();
        fs::write(
            root.path().join("CustomStory.csv"),
            "demo,\"标题,引号\"\"\",作者\nmissing,不存在,作者\ndemo,重复,作者\n",
        )
        .unwrap();
        let imported = read(root.path()).unwrap();
        imported.require_supported().unwrap();
        assert_eq!(
            imported.entries,
            vec![StoryEntry {
                script: "demo".into(),
                title: "标题,引号\"".into(),
                author: "作者".into()
            }]
        );
        assert_eq!(
            imported.story_translations()["D_TEST"],
            "第一行,有逗号\r\n第二行"
        );
    }
    #[test]
    fn unsupported_unknown_files_block_playable_conversion() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("story/nested")).unwrap();
        fs::write(root.path().join("story/nested/demo.lua"), "wait(1)").unwrap();
        fs::create_dir_all(root.path().join("DataTable/Book")).unwrap();
        fs::write(root.path().join("DataTable/Book/test.json"), "{}").unwrap();
        fs::create_dir(root.path().join("Portraits")).unwrap();
        fs::write(root.path().join("Portraits/player_normal.xyz"), b"fixture").unwrap();
        let imported = read(root.path()).unwrap();
        assert_eq!(imported.scripts.len(), 1);
        assert_eq!(imported.unsupported.len(), 1);
        assert!(imported.require_supported().is_err());
    }
    #[test]
    fn duplicate_basename_and_malformed_csv_are_rejected() {
        assert!(csv_rows("key,\"unterminated").is_err());
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("story/sub")).unwrap();
        fs::write(root.path().join("story/demo.lua"), "wait(1)").unwrap();
        fs::write(root.path().join("story/sub/demo.lua"), "wait(2)").unwrap();
        assert!(read(root.path())
            .unwrap_err()
            .to_string()
            .contains("同名脚本"));
    }
    #[test]
    fn story_only_directory_converts_exports_and_preserves_original_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("story")).unwrap();
        let source="setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nlocal options={}\noptions[1]=\"O_LEFT\"\noptions[2]=\"O_RIGHT\"\nlocal selected=choose(options)\nif selected==1 then\nsay(luamanager.GetStoryText(\"D_LEFT\"))\nelseif selected==2 then\nsay(luamanager.GetStoryText(\"D_RIGHT\"))\nend\n";
        let path = root.path().join("story/demo.lua");
        fs::write(&path, source).unwrap();
        fs::write(root.path().join("StringTable.csv"),"Story/O_LEFT,向左\nStory/O_RIGHT,向右\nStory/D_LEFT,左边对白\nStory/D_RIGHT,右边对白\n").unwrap();
        fs::write(root.path().join("CustomStory.csv"), "demo,旧故事,旧作者\n").unwrap();
        let project = read(root.path()).unwrap().into_project().unwrap();
        assert_eq!(project.manifest["entry"], "demo");
        assert_eq!(project.manifest["author"], "旧作者");
        assert_eq!(project.stories["demo"]["vanilla_import"]["external"], true);
        assert!(project.stories["demo"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|node| node["type"] != "raw"));
        assert!(project.stories["demo"].to_string().contains("向左"));
        let output = tempfile::tempdir().unwrap();
        let package = output.path().join("legacy.lommod");
        project.export(&package).unwrap();
        let reopened = crate::project::Project::open(&package).unwrap();
        assert_eq!(reopened.stories, project.stories);
        assert_eq!(fs::read_to_string(path).unwrap(), source);
    }
    #[test]
    fn utf16_source_and_old_extension_calls_are_detected() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("story")).unwrap();
        let source = "local flag=ext.GetFlag(\"X\")\n";
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(source.encode_utf16().flat_map(u16::to_le_bytes));
        fs::write(root.path().join("story/demo.lua"), bytes).unwrap();
        let imported = read(root.path()).unwrap();
        assert_eq!(imported.scripts["demo"], source);
        imported.require_supported().unwrap();
        fs::write(root.path().join("story/demo.lua"), "ext.UnknownMethod()\n").unwrap();
        assert!(read(root.path())
            .unwrap()
            .require_supported()
            .unwrap_err()
            .to_string()
            .contains("UnknownMethod"));
    }
}
