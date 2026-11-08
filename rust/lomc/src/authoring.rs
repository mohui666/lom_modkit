//! Declarative CLI input uses the editor's existing documents, not a second format.
use anyhow::{ensure, Context, Result};
use lom_core::{load_json, project::Project, stable_json, validate};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

pub fn schema(section: &str) -> Result<Value> {
    Ok(match section {
        "nodes" => validate::node_schema().clone(),
        "catalog" => validate::editor_data().clone(),
        "forms" => serde_json::from_str(include_str!("../../lom-editor/data/authoring.json"))?,
        "defaults" => serde_json::from_str(include_str!("../../lom-core/data/node-defaults.json"))?,
        "project" => json!({
            "version":1,
            "fields":{
                "source":"可选：已有工程目录、章节 JSON、Lua 或 .lommod；相对请求文件解析",
                "mode":"新建时可选 story/addon/sandbox/vanilla_patch，默认 story",
                "target":"vanilla_patch 的原版章节编号",
                "manifest":"可选对象：合并作品配置；未提供的旧字段保留",
                "stories":"可选对象：章节 ID -> 完整章节对象；提供时替换整个章节集合",
                "scenario":"可选对象：完整事件和任务配置；null 表示移除（须另存）",
                "assets":"可选对象：assets/ 包内相对路径 -> 本地文件路径；合并原资源",
                "remove_assets":"可选数组：要移除的包内 assets/ 路径"
            },
            "node_object":"{id, type, ...fields}; 字段和枚举见 schema nodes",
            "write":"project REQUEST.json -o DIRECTORY；完整体检成功才开始保存"
        }),
        _ => anyhow::bail!("schema 需要 nodes/catalog/forms/defaults/project"),
    })
}

pub fn sdk(target: &Path) -> Result<Value> {
    let package = target.join("lom_modkit");
    // Do not overwrite a user's modified SDK or scripts.
    let files = [
        (
            package.join("authoring.py"),
            include_bytes!("../../../python/lom_modkit/authoring.py").to_vec(),
        ),
        (
            package.join("__init__.py"),
            include_bytes!("../../../python/lom_modkit/__init__.py").to_vec(),
        ),
        (
            package.join("contract.json"),
            stable_json(&json!({
                "version":env!("CARGO_PKG_VERSION"), "schema":schema("nodes")?,
                "parallel_actions":lom_core::parallel::ACTION_TYPES
            }))?,
        ),
        (
            target.join("example.py"),
            include_bytes!("../../../python/example.py").to_vec(),
        ),
        (
            target.join("complex_story.py"),
            include_bytes!("../../../samples/python-api-complex/build_story.py").to_vec(),
        ),
        (
            target.join("prepare_assets.py"),
            include_bytes!("../../../samples/python-api-complex/prepare_assets.py").to_vec(),
        ),
        (
            target.join("complex-brief.md"),
            include_bytes!("../../../samples/python-api-complex/BRIEF.md").to_vec(),
        ),
        (
            target.join("README.md"),
            include_bytes!("../../../docs/chs/ai_cli.md").to_vec(),
        ),
        (
            target.join("python_api.md"),
            include_bytes!("../../../docs/chs/python_api.md").to_vec(),
        ),
        (
            target.join("cli_parameters.md"),
            include_bytes!("../../../docs/chs/cli_parameters.md").to_vec(),
        ),
        (
            target.join("skill/lom-modkit-authoring/SKILL.md"),
            include_bytes!("../../../skills/lom-modkit-authoring/SKILL.md").to_vec(),
        ),
        (
            target.join("skill/lom-modkit-authoring/agents/openai.yaml"),
            include_bytes!("../../../skills/lom-modkit-authoring/agents/openai.yaml").to_vec(),
        ),
    ];
    for (path, _) in &files {
        ensure!(
            !path.exists(),
            "目标已存在，请选择空目录：{}",
            path.display()
        );
    }
    for (path, bytes) in files {
        lom_core::project::atomic_write(&path, &bytes)?;
    }
    Ok(
        json!({"ok":true,"output":target,"node_types":validate::node_schema()["nodes"].as_object().unwrap().len()}),
    )
}

pub fn read(path: &Path) -> Result<Value> {
    let project = Project::open(path)?;
    let source = path.canonicalize()?;
    Ok(json!({"ok":true,"project":{
        "source":source,"manifest":project.manifest,"stories":project.stories,
        "scenario":project.scenario
    },"assets":project.assets.keys().collect::<Vec<_>>()}))
}

fn asset_name(name: &str) -> Result<()> {
    ensure!(
        lom_core::package::canonical_archive_name(name)? == name && name.starts_with("assets/"),
        "资源目标必须是 assets/ 下的规范相对路径：{name}"
    );
    Ok(())
}

pub fn build(request: &Path, output: &Path) -> Result<Value> {
    ensure!(
        !output.is_file()
            && !matches!(
                output.extension().and_then(|s| s.to_str()),
                Some("json" | "lommod")
            ),
        "project 的输出必须是工程目录；打包请使用 export"
    );
    let recipe = load_json(request)?;
    let fields = recipe.as_object().context("工程请求必须是对象")?;
    for key in fields.keys() {
        ensure!(
            [
                "source",
                "mode",
                "target",
                "manifest",
                "stories",
                "scenario",
                "assets",
                "remove_assets"
            ]
            .contains(&key.as_str()),
            "工程请求含未知字段：{key}"
        );
    }
    let base = request.canonicalize()?.parent().unwrap().to_path_buf();
    let mut project = if let Some(source) = recipe.get("source") {
        ensure!(
            recipe.get("mode").is_none() && recipe.get("target").is_none(),
            "修改已有工程时请保留原模式，在 manifest 中明确修改配置"
        );
        Project::open(&base.join(source.as_str().context("source 必须是路径字符串")?))?
    } else {
        match recipe
            .get("mode")
            .map(|v| v.as_str().context("mode 必须是字符串"))
            .transpose()?
            .unwrap_or("story")
        {
            "story" => Project::new(),
            mode @ ("addon" | "sandbox") => Project::new_scenario(mode),
            "vanilla_patch" => {
                Project::new_vanilla_patch(recipe["target"].as_str().context("缺少 target")?)?
            }
            other => anyhow::bail!("未知工程模式：{other}"),
        }
    };
    let mut removed_documents = false;
    if let Some(manifest) = recipe.get("manifest") {
        project
            .manifest
            .as_object_mut()
            .unwrap()
            .extend(manifest.as_object().context("manifest 必须是对象")?.clone());
    }
    if let Some(stories) = recipe.get("stories") {
        let stories: BTreeMap<String, Value> = serde_json::from_value(stories.clone())
            .context("stories 必须是章节 ID 到章节对象的映射")?;
        ensure!(!stories.is_empty(), "工程至少需要一个章节");
        for (id, story) in &stories {
            ensure!(
                story.is_object() && story["id"] == *id,
                "章节 {id} 的键和内部 id 必须一致"
            );
        }
        removed_documents |= project.stories.keys().any(|id| !stories.contains_key(id));
        project.paths.retain(|id, _| stories.contains_key(id));
        project.stories = stories;
    }
    if let Some(scenario) = recipe.get("scenario") {
        ensure!(
            scenario.is_null() || scenario.is_object(),
            "scenario 必须是对象或 null"
        );
        removed_documents |= scenario.is_null() && project.scenario.is_some();
        project.scenario = (!scenario.is_null()).then(|| scenario.clone());
    }
    if let Some(removed) = recipe.get("remove_assets") {
        for name in removed.as_array().context("remove_assets 必须是数组")? {
            let name = name
                .as_str()
                .context("remove_assets 必须是路径字符串数组")?;
            asset_name(name)?;
            removed_documents |= project.assets.remove(name).is_some();
        }
    }
    if let Some(assets) = recipe.get("assets") {
        for (name, file) in assets.as_object().context("assets 必须是路径映射对象")? {
            asset_name(name)?;
            if let Some(value) = file.as_object() {
                ensure!(
                    value.len() == 1 && value.contains_key("json"),
                    "内嵌资源只接受 {{json: 对象}}，来源文件使用路径字符串"
                );
                ensure!(
                    name.ends_with(".json") && file["json"].is_object(),
                    "内嵌资源必须是 JSON 对象和 .json 路径"
                );
                project
                    .assets
                    .insert(name.clone(), stable_json(&file["json"])?);
                continue;
            }
            let file = base.join(
                file.as_str()
                    .context("资源来源必须是本地文件路径或内嵌 JSON")?,
            );
            project.assets.insert(
                name.clone(),
                fs::read(&file).with_context(|| format!("读取资源失败：{}", file.display()))?,
            );
        }
    }
    if removed_documents && output.exists() {
        ensure!(
            project.source.as_ref().and_then(|s| s.canonicalize().ok())
                != output.canonicalize().ok(),
            "移除章节、事件或资源后请 -o 另存新目录，原工程不会被删除"
        );
    }
    validate::validate_manifest(&project.manifest)?;
    let issues = lom_core::release::run_preflight(
        &project,
        lom_core::release::Profile::Editing,
        env!("CARGO_PKG_VERSION"),
    );
    if issues.iter().any(|i| i.severity == "error") {
        return Ok(json!({"ok":false,"issues":issues}));
    }
    // Exercise the real compiler, resource collector and package reader before
    // touching the requested output. A JSON/schema check alone is insufficient.
    let verification = lom_core::workdir::create()?;
    let package = project.export(&verification.path().join("check.lommod"))?;
    Project::open(&package).context("生成结果未通过编辑器包读取检查")?;
    // Save uses the same file ownership and confined-path checks as the editor.
    project.save_to(output)?;
    Project::open(output).context("生成结果未通过编辑器工程读取检查")?;
    Ok(
        json!({"ok":true,"output":output,"chapters":project.stories.len(),"issues":issues,
        "checks":["preflight","compile","package_reopen","project_reopen"]}),
    )
}
