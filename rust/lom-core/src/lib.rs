//! Native Rust authoring core for the existing Legend of Mortal package contract.
pub mod analysis;
pub mod codegen;
pub mod content;
pub mod content_library;
pub mod localization;
pub mod migration;
pub mod package;
pub mod project;
pub mod release;
pub mod validate;
pub mod watermark;

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
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|key| (key.clone(), sorted(&map[key])))
                        .collect(),
                )
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&sorted(value))?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub mod game_tools;
