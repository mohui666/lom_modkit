//! Lossless project documents. Opening is transactional; writes preserve file ownership.
use crate::{load_json, package, stable_json};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub manifest: Value,
    pub stories: BTreeMap<String, Value>,
    /// Story ID -> original filename relative to story/ (or standalone parent).
    pub paths: BTreeMap<String, PathBuf>,
    pub source: Option<PathBuf>,
    pub assets: BTreeMap<String, Vec<u8>>,
    pub story_subdir: bool,
    pub has_manifest: bool,
}
impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}
impl Project {
    pub fn new() -> Self {
        let mut project = Self::draft();
        project.manifest["campaign_id"] =
            json!(format!("campaign_{}", uuid::Uuid::new_v4().simple()));
        project
    }
    // Existing documents without a manifest must not acquire a new save identity on import.
    fn draft() -> Self {
        let story = json!({"story_schema":2,"id":"main","title":"新的剧情","start":"say1","nodes":[{"id":"say1","type":"say","mode":"narrative","text":"从这里开始你的故事。"},{"id":"end1","type":"end"}]});
        Self {
            manifest: json!({"format":3,"package_format":3,"story_schema":2,"content_schema":1,"id":"my_mod","name":"新的作品","version":"1.0.0","author":"作者","description":"原创剧情","entry":"main","campaign":{"new_game":true,"triggers":[]}}),
            stories: BTreeMap::from([("main".into(), story)]),
            paths: BTreeMap::new(),
            source: None,
            assets: BTreeMap::new(),
            story_subdir: true,
            has_manifest: true,
        }
    }
    pub fn open(path: &Path) -> Result<Self> {
        let path = path.canonicalize()?;
        let mut project = Self::draft();
        project.stories.clear();
        project.paths.clear();
        project.source = Some(path.clone());
        let mut migrations = Vec::new();
        if path.is_dir() {
            project.has_manifest = path.join("manifest.json").is_file();
            if project.has_manifest {
                project.manifest = load_json(path.join("manifest.json"))?;
                ensure!(project.manifest.is_object(), "manifest 必须是对象");
                ensure!(
                    project.manifest.get("entry").is_none_or(Value::is_string),
                    "manifest.entry 必须是字符串"
                );
            }
            project.story_subdir = path.join("story").is_dir();
            let folder = if project.story_subdir {
                path.join("story")
            } else {
                path.clone()
            };
            let mut files = fs::read_dir(&folder)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            files.sort();
            for file in files {
                if file.extension().and_then(|s| s.to_str()) != Some("json") || !file.is_file() {
                    continue;
                }
                if file.file_name().and_then(|s| s.to_str()) == Some("manifest.json") {
                    continue;
                }
                package::resolve_confined_file(&path, &file)?;
                let value =
                    load_json(&file).with_context(|| format!("无法读取章节 {}", file.display()))?;
                if !value.get("nodes").is_some_and(Value::is_array)
                    && !value.get("id").is_some_and(Value::is_string)
                {
                    continue;
                }
                let migrated = crate::migration::migrate_story(&value)?;
                if migrated.changed {
                    migrations.push((file.clone(), migrated.document.clone()));
                }
                project
                    .insert_story(migrated.document, PathBuf::from(file.file_name().unwrap()))?;
            }
            collect_assets(&path, &path.join("assets"), &mut project.assets)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("lommod") {
            let entries = package::read_package(&path)?;
            project.manifest = serde_json::from_slice(&entries["manifest.json"])?;
            for (name, bytes) in &entries {
                if name.starts_with("story/") && name.ends_with(".json") {
                    let rel = &name[6..];
                    ensure!(!rel.contains('/'), "story/ 不允许嵌套章节");
                    project.insert_story(serde_json::from_slice(bytes)?, PathBuf::from(rel))?;
                } else if name.starts_with("assets/") {
                    project.assets.insert(name.clone(), bytes.clone());
                }
            }
        } else {
            project.has_manifest = false;
            let migrated = crate::migration::migrate_story(&load_json(&path)?)?;
            if migrated.changed {
                migrations.push((path.clone(), migrated.document.clone()));
            }
            let story = migrated.document;
            let id = story["id"].as_str().context("剧情缺少 id")?.to_owned();
            project.manifest["entry"] = json!(id);
            project.insert_story(story, PathBuf::from(path.file_name().unwrap()))?;
            let parent = path.parent().unwrap();
            let root = if parent.file_name().and_then(|s| s.to_str()) == Some("story") {
                parent.parent().unwrap_or(parent)
            } else {
                parent
            };
            collect_assets(root, &root.join("assets"), &mut project.assets)?;
        }
        ensure!(!project.stories.is_empty(), "项目没有剧情章节");
        if !project.has_manifest {
            let entry = if project.stories.contains_key("main") {
                "main"
            } else {
                project.stories.keys().next().unwrap()
            };
            project.manifest["entry"] = json!(entry);
        }
        // Read and shape-check every chapter and asset before touching any source.
        // Each changed source keeps an exact backup and is replaced atomically.
        for (path, expected) in migrations {
            crate::migration::migrate_json_file(
                &path,
                "story",
                Some(&|actual| {
                    ensure!(actual == &expected, "章节在读取期间发生变化，拒绝迁移");
                    Ok(())
                }),
            )?;
        }
        Ok(project)
    }
    fn insert_story(&mut self, story: Value, filename: PathBuf) -> Result<()> {
        let id = story["id"].as_str().context("剧情缺少 id")?.to_string();
        check_id(&id)?;
        ensure!(story["nodes"].is_array(), "章节 {id} 的 nodes 必须是数组");
        ensure!(
            story["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|node| node.is_object()
                    && node.get("id").is_some()
                    && node.get("type").is_some()),
            "章节 {id} 的节点缺少 id/type"
        );
        ensure!(
            story
                .get("story_schema")
                .is_none_or(|s| s.as_u64() == Some(2)),
            "章节 {id} 的 story_schema 必须为 2"
        );
        ensure!(
            !self.stories.contains_key(&id),
            "重复章节 ID {id}: {} / {}",
            self.paths
                .get(&id)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            filename.display()
        );
        self.paths.insert(id.clone(), filename);
        self.stories.insert(id, story);
        Ok(())
    }
    pub fn save_to(&mut self, target: &Path) -> Result<()> {
        if target.extension().and_then(|s| s.to_str()) == Some("lommod") {
            self.export(target)?;
            self.source = Some(target.to_path_buf());
            return Ok(());
        }
        if target.extension().and_then(|s| s.to_str()) == Some("json") {
            ensure!(self.stories.len() == 1, "多章节项目请保存为目录");
            let (id, story) = self.stories.first_key_value().unwrap();
            check_id(id)?;
            ensure!(story["id"] == *id, "章节键与内部 id 不一致: {id}");
            let absolute = if target.is_absolute() {
                target.to_path_buf()
            } else {
                std::env::current_dir()?.join(target)
            };
            let parent = absolute.parent().context("无效保存路径")?;
            let asset_root = if parent.file_name().and_then(|v| v.to_str()) == Some("story") {
                parent.parent().unwrap_or(parent)
            } else {
                parent
            };
            let same_source = self
                .source
                .as_ref()
                .and_then(|p| p.canonicalize().ok())
                .zip(absolute.canonicalize().ok())
                .is_some_and(|(source, target)| source == target);
            let mut writes = vec![(absolute.clone(), stable_json(story)?)];
            for (name, bytes) in &self.assets {
                package::canonical_archive_name(name)?;
                ensure!(name.starts_with("assets/"), "项目资源路径必须在 assets/ 下");
                let destination = asset_root.join(name);
                ensure!(
                    !destination.exists() || same_source,
                    "目标资源不属于当前项目，拒绝覆盖: {name}"
                );
                writes.push((destination, bytes.clone()));
            }
            write_confined_files(asset_root, &writes)?;
            self.paths = BTreeMap::from([(
                id.clone(),
                PathBuf::from(target.file_name().context("无效文件名")?),
            )]);
            self.source = Some(target.canonicalize()?);
            return Ok(());
        }
        let target = if target.exists() {
            target.canonicalize()?
        } else {
            std::env::current_dir()?.join(target)
        };
        let same_source = self.source.as_ref().is_some_and(|p| p == &target);
        let mut writes = Vec::new();
        let mut names = BTreeSet::new();
        let mut new_paths = BTreeMap::new();
        for (id, story) in &self.stories {
            check_id(id)?;
            ensure!(story["id"] == *id, "章节键与内部 id 不一致: {id}");
            let filename = self
                .paths
                .get(id)
                .cloned()
                .unwrap_or_else(|| PathBuf::from(format!("{id}.json")));
            let raw = filename.to_str().context("章节文件名无效")?;
            package::canonical_archive_name(raw)?;
            ensure!(
                !raw.contains('/') && filename.extension().and_then(|s| s.to_str()) == Some("json"),
                "章节文件名无效"
            );
            ensure!(names.insert(raw.to_lowercase()), "章节文件名冲突: {raw}");
            let path = if self.story_subdir {
                target.join("story").join(&filename)
            } else {
                target.join(&filename)
            };
            if path.exists() {
                ensure!(
                    same_source && self.paths.get(id) == Some(&filename),
                    "目标文件不属于当前项目，拒绝覆盖: {}",
                    path.display()
                );
            }
            writes.push((path, stable_json(story)?));
            new_paths.insert(id.clone(), filename);
        }
        let manifest_path = target.join("manifest.json");
        ensure!(
            !manifest_path.exists() || same_source,
            "目标 manifest 不属于当前项目，拒绝覆盖"
        );
        writes.push((manifest_path, stable_json(&self.manifest)?));
        for (name, bytes) in &self.assets {
            package::canonical_archive_name(name)?;
            ensure!(name.starts_with("assets/"), "项目资源路径必须在 assets/ 下");
            let dest = target.join(name);
            ensure!(!dest.exists() || same_source, "目标资源已存在: {name}");
            writes.push((dest, bytes.clone()));
        }
        write_confined_files(&target, &writes)?;
        let root = target.canonicalize()?;
        self.paths = new_paths;
        self.source = Some(root);
        self.has_manifest = true;
        Ok(())
    }
    pub fn export(&self, target: &Path) -> Result<PathBuf> {
        let temp = tempfile::tempdir()?;
        let mut project = self.clone();
        project.paths.clear();
        project.source = None;
        project.story_subdir = true;
        project.has_manifest = true;
        project.save_to(temp.path())?;
        package::pack_mod(temp.path(), Some(target))
    }
}
/// Prepare all temporary files before replacing any owned document or asset.
fn write_confined_files(root: &Path, writes: &[(PathBuf, Vec<u8>)]) -> Result<()> {
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    for (path, _) in writes {
        ensure!(
            !path.is_symlink(),
            "拒绝通过符号链接保存: {}",
            path.display()
        );
        ensure!(
            !path.exists() || path.is_file(),
            "保存目标不是文件: {}",
            path.display()
        );
        let parent = path.parent().context("无效保存路径")?;
        let existing_parent = parent
            .ancestors()
            .find(|p| p.exists())
            .context("保存目录不存在")?;
        ensure!(
            existing_parent.canonicalize()?.starts_with(&root),
            "保存目录逃逸: {}",
            path.display()
        );
    }
    for (path, _) in writes {
        fs::create_dir_all(path.parent().context("无效保存路径")?)?;
        ensure!(
            path.parent()
                .context("无效保存路径")?
                .canonicalize()?
                .starts_with(&root),
            "保存目录逃逸: {}",
            path.display()
        );
    }
    let mut prepared = Vec::new();
    for (path, bytes) in writes {
        let mut file = tempfile::NamedTempFile::new_in(path.parent().context("无效保存路径")?)?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        prepared.push((file, path));
    }
    for (file, path) in prepared {
        file.persist(path)?;
    }
    Ok(())
}
fn check_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "无效章节 id: {id}"
    );
    Ok(())
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    ensure!(!path.is_symlink(), "拒绝通过符号链接保存");
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
fn collect_assets(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    ensure!(
        !dir.is_symlink(),
        "资源目录不能是符号链接: {}",
        dir.display()
    );
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        ensure!(!path.is_symlink(), "资源不能是符号链接: {}", path.display());
        if path.is_dir() {
            collect_assets(root, &path, out)?;
        } else if path.is_file() {
            let path = package::resolve_confined_file(root, &path)?;
            let rel = path
                .strip_prefix(root)?
                .to_str()
                .context("资源名不是 UTF-8")?
                .replace('\\', "/");
            ensure!(
                fs::metadata(&path)?.len() <= package::MAX_ENTRY_BYTES,
                "资源超过 32 MiB: {rel}"
            );
            out.insert(rel, fs::read(path)?);
            ensure!(
                out.values().map(Vec::len).sum::<usize>() <= 128 * 1024 * 1024,
                "项目资源超过 128 MiB"
            );
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_save_preserves_names_manifest_and_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Project::new();
        p.manifest["author"] = json!("测试");
        p.paths.insert("main".into(), "original-name.json".into());
        p.save_to(dir.path()).unwrap();
        fs::write(dir.path().join("keep.txt"), "keep").unwrap();
        let mut loaded = Project::open(dir.path()).unwrap();
        loaded.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("修改");
        loaded.save_to(dir.path()).unwrap();
        assert!(dir.path().join("story/original-name.json").exists());
        assert!(!dir.path().join("story/main.json").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
            "keep"
        );
        assert_eq!(
            Project::open(dir.path()).unwrap().manifest["author"],
            "测试"
        );
    }
    #[test]
    fn save_refuses_unowned_collision_and_duplicate_open() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Project::new();
        p.save_to(dir.path()).unwrap();
        assert!(Project::new().save_to(dir.path()).is_err());
        fs::copy(
            dir.path().join("story/main.json"),
            dir.path().join("story/duplicate.json"),
        )
        .unwrap();
        assert!(Project::open(dir.path()).is_err());
    }
    #[test]
    fn flat_folder_saves_every_chapter_without_moving_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Project::new();
        p.story_subdir = false;
        p.has_manifest = false;
        let mut second = p.stories["main"].clone();
        second["id"] = json!("second");
        p.stories.insert("second".into(), second);
        p.paths.insert("second".into(), "original.json".into());
        p.save_to(dir.path()).unwrap();
        fs::remove_file(dir.path().join("manifest.json")).unwrap();
        let mut loaded = Project::open(dir.path()).unwrap();
        assert_eq!(loaded.stories.len(), 2);
        assert!(!loaded.story_subdir);
        assert!(!loaded.has_manifest);
        for story in loaded.stories.values_mut() {
            story["title"] = json!("修改");
        }
        loaded.save_to(dir.path()).unwrap();
        assert!(!dir.path().join("story").exists());
        assert!(dir.path().join("manifest.json").exists());
        assert!(dir.path().join("original.json").exists());
        assert!(Project::open(dir.path())
            .unwrap()
            .stories
            .values()
            .all(|s| s["title"] == "修改"));
    }
    #[test]
    fn draft_manifest_is_preserved_without_export_validation() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Project::new();
        p.manifest = json!({"entry":"main","campaign_id":"unchanged","name":"原有项目"});
        p.save_to(dir.path()).unwrap();
        let mut loaded = Project::open(dir.path()).unwrap();
        assert_eq!(loaded.manifest, p.manifest);
        loaded.save_to(dir.path()).unwrap();
        assert_eq!(
            load_json(dir.path().join("manifest.json")).unwrap(),
            p.manifest
        );
    }
}
