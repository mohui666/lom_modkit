//! Shared project document operations.
use super::*;

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
// JSON or a selected story/ folder. Standalone JSON keeps its import semantics.
pub(super) fn open_project(path: &Path) -> anyhow::Result<Project> {
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
        Project::open(root)
    } else {
        Project::open(&absolute)
    }
}
pub(super) fn compile_with_assets(project: &Project, story: &Value) -> anyhow::Result<String> {
    let root = lom_core::workdir::create()?;
    for (name, bytes) in &project.assets {
        lom_core::package::canonical_archive_name(name)?;
        anyhow::ensure!(name.starts_with("assets/"), "项目素材必须位于 assets/");
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    lom_core::codegen::compile_story(story, Some(&project.manifest), None, Some(root.path()))
}
