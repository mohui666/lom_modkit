//! Native Rust authoring core for the existing Legend of Mortal package contract.
pub mod analysis;
pub mod character_profile;
pub mod codegen;
pub mod content;
pub mod content_library;
pub mod legacy_activation;
pub mod legacy_extensions;
pub mod legacy_loose;
pub mod localization;
pub mod lua_import;
pub mod migration;
pub mod native_flow;
pub mod native_upgrade;
pub mod package;
pub mod parallel;
pub mod portrait_alignment;
pub mod project;
pub mod project_merge;
pub mod release;
pub mod story_bookmarks;
pub mod story_routes;
pub mod validate;
pub mod vanilla_import;
pub mod vanilla_patch;
pub mod vanilla_routes;
pub mod watermark;
pub mod workdir;

pub use anyhow::{Error, Result};
pub use serde_json::Value;

pub const PACKAGE_FORMAT: u32 = 3;
pub const STORY_SCHEMA: u32 = 2;
pub const CONTENT_SCHEMA: u32 = 1;

pub fn load_json(path: impl AsRef<std::path::Path>) -> Result<Value> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(
        bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes),
    )?)
}

pub fn stable_json(value: &Value) -> Result<Vec<u8>> {
    use serde::{Serialize, Serializer};

    struct Sorted<'a>(&'a Value);
    impl Serialize for Sorted<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
            match self.0 {
                Value::Object(map) => {
                    let mut entries: Vec<_> = map.iter().collect();
                    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
                    serializer
                        .collect_map(entries.into_iter().map(|(key, value)| (key, Sorted(value))))
                }
                Value::Array(values) => serializer.collect_seq(values.iter().map(Sorted)),
                other => other.serialize(serializer),
            }
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&Sorted(value))?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub mod content_edit;
pub mod editing;
pub mod editor_update;
pub mod game_discovery;
pub mod game_tools;
pub mod render_priority;
pub mod scenario;
pub mod scenario_edit;
pub mod story_api;
