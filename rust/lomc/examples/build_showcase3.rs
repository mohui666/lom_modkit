//! Rebuild the acceptance sample from its independent authored source.
//! Run from any directory: cargo run -p lomc --example build_showcase3 -- [output]
use anyhow::{ensure, Result};
use lom_core::{project::Project, validate};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf};
fn main() -> Result<()> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = lom_core::load_json(repo.join("samples/showcase3/source.json"))?;
    let out = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("samples/showcase3"));
    let mut project = Project::new();
    project.manifest = source["manifest"].clone();
    project.stories.clear();
    for story in source["stories"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source.stories 必须是数组"))?
    {
        validate::validate_story(story)?;
        let id = story["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少章节 ID"))?;
        ensure!(
            project.stories.insert(id.into(), story.clone()).is_none(),
            "章节 ID 重复"
        );
    }
    let schema: Value = serde_json::from_str(include_str!("../../lom-editor/data/authoring.json"))?;
    let expected: BTreeSet<_> = schema["NODE_SCHEMAS"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let used: BTreeSet<_> = project
        .stories
        .values()
        .flat_map(|s| {
            s["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|n| n["type"].as_str())
        })
        .collect();
    ensure!(expected == used, "样例必须覆盖当前全部节点类型");
    copy_assets(
        &repo.join("samples/showcase3"),
        &repo.join("samples/showcase3/assets"),
        &mut project,
    )?;
    project.save_to(&out)?;
    project.export(&out.join("showcase3.lommod"))?;
    println!(
        "{} chapters, {} node types, JSON + Lua package: {}",
        project.stories.len(),
        expected.len(),
        out.display()
    );
    Ok(())
}

fn copy_assets(root: &std::path::Path, dir: &std::path::Path, project: &mut Project) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        ensure!(!entry.file_type()?.is_symlink(), "样例素材不能是符号链接");
        if path.is_dir() {
            copy_assets(root, &path, project)?;
        } else {
            lom_core::package::resolve_confined_file(root, &path)?;
            let key = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            project.assets.insert(key, std::fs::read(path)?);
        }
    }
    Ok(())
}
