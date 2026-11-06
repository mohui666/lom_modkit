//! Preserve the data contracts of Binarizer's ModSupport extension hooks.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Extensions {
    pub handled_files: BTreeSet<String>,
    pub unsupported: Vec<(String, String)>,
    equivalents: Value,
    tables: Vec<Value>,
}

pub fn supports_ext(method: &str) -> bool {
    matches!(
        method,
        "GetPlayerStat"
            | "GetEnemyTeamStat"
            | "GetRelationship"
            | "GetSkillLevel"
            | "GetBookLevel"
            | "GetDeveloped"
            | "GetDevelopd"
            | "GetDevelopLevel"
            | "GetFlag"
            | "GetCurrentTime"
            | "GetTimeValue"
            | "GetItemCount"
    )
}

impl Extensions {
    pub fn apply(&self, project: &mut crate::project::Project) -> Result<()> {
        ensure!(
            self.unsupported.is_empty(),
            "旧扩展包含无法转换的文件：{:?}",
            self.unsupported
        );
        if self.handled_files.is_empty() {
            return Ok(());
        }
        if !project.manifest["legacy"].is_object() {
            project.manifest["legacy"] = json!({"schema":1});
        }
        project.manifest["legacy"]["schema"] = json!(1);
        project.manifest["legacy"]["equivalents"] = self.equivalents.clone();
        project.manifest["legacy"]["tables"] = json!(self.tables);
        Ok(())
    }
}

pub fn read(root: &Path) -> Result<Extensions> {
    let mut result = Extensions {
        equivalents: json!({}),
        ..Default::default()
    };
    for kind in ["Condition", "Switch", "Position"] {
        let mut files = Vec::new();
        collect(&root.join("LuaEquivalent").join(kind), &mut files)?;
        for file in files {
            let relative = file
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if !file
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
            {
                continue;
            }
            let name = file
                .file_stem()
                .and_then(|value| value.to_str())
                .context("LuaEquivalent 文件名不是 UTF-8")?;
            ensure!(
                result.equivalents[kind][name].is_null(),
                "LuaEquivalent/{kind} 存在重复名称 {name}，无法确认原来加载的文件"
            );
            let source =
                fs::read_to_string(&file).with_context(|| format!("无法读取 {relative}"))?;
            if !result.equivalents[kind].is_object() {
                result.equivalents[kind] = json!({});
            }
            result.equivalents[kind][name] = json!(source.trim_start_matches('\u{feff}'));
            result.handled_files.insert(relative);
        }
    }
    let mut files = Vec::new();
    collect(&root.join("DataTable"), &mut files)?;
    for file in files {
        let relative = file
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        if !file
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let pieces: Vec<_> = relative.split('/').collect();
        if pieces.len() != 3 {
            result.unsupported.push((
                relative,
                "原版 DataTable 只读取 DataTable/<类型>/<名称>.json".into(),
            ));
            continue;
        }
        let type_name = pieces[1];
        ensure!(
            type_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !type_name.is_empty(),
            "DataTable 类型名不合法：{relative}"
        );
        let fields: Value = serde_json::from_slice(&fs::read(&file)?)
            .with_context(|| format!("DataTable JSON 无效：{relative}"))?;
        ensure!(
            fields.is_object(),
            "DataTable 顶层必须是字段对象：{relative}"
        );
        result.tables.push(json!({"type":type_name, "name":file.file_stem().and_then(|value|value.to_str()).context("DataTable 名称不是 UTF-8")?, "fields":fields}));
        result.handled_files.insert(relative);
    }
    Ok(result)
}

fn collect(path: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "旧扩展目录不支持符号链接：{}",
        path.display()
    );
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "旧扩展目录不支持符号链接：{}",
            entry.path().display()
        );
        if kind.is_dir() {
            collect(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extensions_keep_source_and_merge_with_resource_metadata() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("LuaEquivalent/Condition/sub")).unwrap();
        fs::create_dir_all(root.path().join("DataTable/Book")).unwrap();
        let code = "return ext.GetBookLevel('book_001') >= 2\n";
        fs::write(
            root.path().join("LuaEquivalent/Condition/sub/Test.lua"),
            code,
        )
        .unwrap();
        fs::write(
            root.path().join("DataTable/Book/Test.json"),
            r#"{"_currentMartialLevel":3}"#,
        )
        .unwrap();
        let extensions = read(root.path()).unwrap();
        let mut project = crate::project::Project::new();
        project.manifest["legacy"] = json!({"schema":1,"strings":{"Story/example":"author text"}});
        extensions.apply(&mut project).unwrap();
        assert_eq!(
            project.manifest["legacy"]["equivalents"]["Condition"]["Test"],
            code
        );
        assert_eq!(
            project.manifest["legacy"]["tables"][0]["fields"]["_currentMartialLevel"],
            3
        );
        assert_eq!(
            project.manifest["legacy"]["strings"]["Story/example"],
            "author text"
        );
        assert!(extensions
            .handled_files
            .contains("DataTable/Book/Test.json"));
        assert!(supports_ext("GetDevelopd"));
        assert!(!supports_ext("DeleteSave"));
    }
}
