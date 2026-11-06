//! An inert package entry for old add-ons that only replace resources or tables.
use crate::project::Project;
use anyhow::{ensure, Result};
use serde_json::{json, Value};

/// Only actual old-framework changes qualify as a passive original-game add-on.
pub fn has_original_content(manifest: &Value) -> bool {
    let legacy = &manifest["legacy"];
    legacy["schema"] == 1
        && (["sprites", "voices"].iter().any(|key| {
            legacy["resources"][key]
                .as_object()
                .is_some_and(|values| !values.is_empty())
        }) || legacy["strings"]
            .as_object()
            .is_some_and(|values| !values.is_empty())
            || legacy["tables"]
                .as_array()
                .is_some_and(|values| !values.is_empty())
            || legacy["equivalents"].as_object().is_some_and(|kinds| {
                kinds
                    .values()
                    .any(|files| files.as_object().is_some_and(|files| !files.is_empty()))
            }))
}

pub fn bootstrap(project: &mut Project) -> Result<()> {
    if !project.stories.is_empty() {
        return Ok(());
    }
    ensure!(
        project.manifest["legacy"].is_object(),
        "资源附加包缺少旧 MOD 内容"
    );
    project.manifest["play_mode"] = json!("addon");
    project.manifest["entry"] = json!("main");
    project.manifest["min_host_version"] = json!(env!("CARGO_PKG_VERSION"));
    project.manifest["tested_host_version"] = json!(env!("CARGO_PKG_VERSION"));
    project.manifest["campaign"] =
        json!({"new_game":false,"disable_official_events":false,"triggers":[]});
    project.scenario = Some(json!({"schema":1,"events":[],"tasks":[]}));
    project.stories.insert(
        "main".into(),
        json!({
            "story_schema":2,"id":"main","title":"旧 MOD 资源与数据","start":"done",
            "nodes":[{"id":"done","type":"end"}]
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_only_directory_opens_and_exports_as_an_original_game_addon() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("DataTable/GameStat")).unwrap();
        std::fs::write(
            root.path().join("DataTable/GameStat/Player_Money.json"),
            br#"{"_max":99999}"#,
        )
        .unwrap();
        let project = Project::open(root.path()).unwrap();
        assert_eq!(project.stories.len(), 1);
        assert_eq!(project.manifest["play_mode"], "addon");
        assert_eq!(
            project.manifest["legacy"]["tables"][0]["name"],
            "Player_Money"
        );
        assert!(project.manifest.get("vanilla_overrides").is_none());
        assert!(project.stories["main"].get("vanilla_import").is_none());
        let output = root.path().join("converted.lommod");
        project.export(&output).unwrap();
        let entries = crate::package::read_package(&output).unwrap();
        assert!(entries.contains_key("lua/main.lua"));
        assert_eq!(
            entries
                .keys()
                .filter(|path| path.starts_with("lua/"))
                .count(),
            1
        );
    }

    #[test]
    fn string_only_directory_uses_the_same_activation_without_invented_story_text() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("StringTable.csv"),
            "Story/D_ORIGINAL,替换对白\n",
        )
        .unwrap();
        let project = Project::open(root.path()).unwrap();
        assert_eq!(
            project.manifest["legacy"]["strings"]["Story/D_ORIGINAL"],
            "替换对白"
        );
        assert_eq!(
            project.stories["main"]["nodes"].as_array().unwrap().len(),
            1
        );
        assert_eq!(project.stories["main"]["nodes"][0]["type"], "end");
        let mut ordinary = project.manifest.clone();
        ordinary.as_object_mut().unwrap().remove("legacy");
        assert!(crate::scenario::validate(
            project.scenario.as_ref().unwrap(),
            &ordinary,
            &project.stories
        )
        .is_err());
        ordinary["legacy"] =
            json!({"schema":1,"resources":{"sprites":{},"voices":{}},"tables":[],"equivalents":{}});
        assert!(!has_original_content(&ordinary));
        assert!(crate::scenario::validate(
            project.scenario.as_ref().unwrap(),
            &ordinary,
            &project.stories
        )
        .is_err());
    }

    #[test]
    fn portrait_only_directory_retains_the_original_replacement_resource() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Portraits")).unwrap();
        let portrait = root.path().join("Portraits/player_normal.png");
        image::RgbaImage::new(2, 2).save(&portrait).unwrap();
        let original = std::fs::read(&portrait).unwrap();
        let project = Project::open(root.path()).unwrap();
        assert_eq!(project.stories.len(), 1);
        assert_eq!(
            project.manifest["legacy"]["resources"]["sprites"]["Portraits/player_normal.png"]
                ["frames"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(project.assets.values().any(|bytes| bytes == &original));
        assert_eq!(std::fs::read(&portrait).unwrap(), original);
        assert!(!root.path().join("story").exists());
    }
}
