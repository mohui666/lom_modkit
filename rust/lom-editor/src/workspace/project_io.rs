//! Shared project document operations.
use super::*;

pub(super) struct ProjectChoice {
    pub directory: PathBuf,
    pub candidates: Vec<PathBuf>,
    pub selected: usize,
}

pub(super) fn template_project(key: &str) -> anyhow::Result<Project> {
    let templates: Value = serde_json::from_str(include_str!("../../data/project-templates.json"))?;
    let mut project = Project::new_scenario("sandbox");
    let template = templates
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("未知项目模板 {key}"))?;
    let campaign = project.manifest["campaign_id"]
        .as_str()
        .unwrap_or("my_campaign");
    let encoded = serde_json::to_string(&template["project"])?;
    let data: Value = serde_json::from_str(&encoded.replace("__NEW_CAMPAIGN_ID__", campaign))?;
    project.stories = serde_json::from_value(data["stories"].clone())?;
    for (k, v) in data["manifest"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("模板manifest无效"))?
    {
        project.manifest[k] = v.clone();
    }
    if let Some(config) = data.get("scenario") {
        project.scenario = Some(config.clone());
    }
    project.manifest["name"] = template["name"].clone();
    project.paths.clear();
    project.source = None;
    Ok(project)
}
// All editor entry points open the owning project, including a recent chapter
// JSON or a selected story/ folder. Unowned loose chapters keep standalone
// ownership; opening their folder explicitly loads all its chapters together.
pub(super) fn project_sources(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let absolute = path.canonicalize()?;
    let parent = if absolute.is_dir() {
        absolute.as_path()
    } else {
        absolute.parent().context("无效项目路径")?
    };
    let root = if parent.file_name().and_then(|s| s.to_str()) == Some("story") {
        parent.parent().unwrap_or(parent)
    } else {
        parent
    };
    let project_document =
        absolute.is_dir() || absolute.extension().and_then(|s| s.to_str()) == Some("json");
    if project_document && root.join("manifest.json").is_file() {
        return Ok(vec![root.to_owned()]);
    }
    let chapter = absolute.is_file()
        && absolute.extension().and_then(|s| s.to_str()) == Some("json")
        && is_chapter_file(&absolute)?;
    let inside_story = parent.file_name().and_then(|s| s.to_str()) == Some("story");
    if project_document
        && root.join("story").is_dir()
        && (absolute.is_dir() || chapter && inside_story)
    {
        return Ok(vec![root.to_owned()]);
    }
    if !absolute.is_dir() || is_project_directory(&absolute)? {
        return Ok(vec![absolute]);
    }
    // Only inspect the selected folder and its immediate children. Do not load
    // packages, collect assets or migrate documents before the user chooses one.
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&absolute)? {
        let path = entry?.path();
        if (path.is_dir() && is_project_directory(&path)?)
            || (path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("lommod"))
        {
            candidates.push(path.canonicalize()?);
        }
    }
    candidates.sort();
    candidates.dedup();
    anyhow::ensure!(
        !candidates.is_empty(),
        "目录中没有可打开的工程或 .lommod：{}\n请选择工程目录（包含 manifest.json、story/ 或剧情 JSON），或包含工程子目录／.lommod 的外层目录。",
        absolute.display()
    );
    Ok(candidates)
}

fn is_chapter_file(path: &Path) -> anyhow::Result<bool> {
    if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Ok(false);
    }
    let bytes = std::fs::read(path)?;
    Ok(
        serde_json::from_slice::<Value>(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
            .is_ok_and(|value| value.get("nodes").is_some_and(Value::is_array)),
    )
}

fn is_project_directory(path: &Path) -> anyhow::Result<bool> {
    if path.join("manifest.json").is_file()
        || path.join("story").is_dir()
        || lom_core::legacy_loose::has_content(path)?
    {
        return Ok(true);
    }
    for entry in std::fs::read_dir(path)? {
        let file = entry?.path();
        if !file.is_file() || file.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        // A loose chapter folder is also a project. Other JSON (including
        // unrelated configuration files) is not an engineering directory marker.
        let bytes = std::fs::read(&file)?;
        if serde_json::from_slice::<Value>(&bytes)
            .is_ok_and(|value| value.get("nodes").is_some() || value.get("story_schema").is_some())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn open_project(path: &Path) -> anyhow::Result<Project> {
    let candidates = project_sources(path)?;
    anyhow::ensure!(
        candidates.len() == 1,
        "目录中有多个工程，请选择具体工程目录或 .lommod：\n{}",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
    Project::open(&candidates[0])
}

#[cfg(test)]
#[path = "project_io_tests.rs"]
mod tests;
pub(super) fn compile_with_assets(project: &Project, story: &Value) -> anyhow::Result<String> {
    compile_lua(story, &project.manifest, &project.assets)
}

pub(super) fn compile_lua(
    story: &Value,
    manifest: &Value,
    assets: &BTreeMap<String, Vec<u8>>,
) -> anyhow::Result<String> {
    let root = lom_core::workdir::create()?;
    for (name, bytes) in assets {
        lom_core::package::canonical_archive_name(name)?;
        anyhow::ensure!(name.starts_with("assets/"), "项目素材必须位于 assets/");
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    lom_core::codegen::compile_story(story, Some(manifest), None, Some(root.path()))
}
