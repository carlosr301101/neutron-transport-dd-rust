//! List and load configuration files from `outputs/inputs`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::paths;
use crate::post::model;

/// One configuration found on disk.
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    /// File path.
    pub path: PathBuf,
    /// File name.
    pub name: String,
}

impl ConfigEntry {
    /// Human-readable label.
    pub fn label(&self) -> &str {
        &self.name
    }
}

/// List the JSON configurations in the default inputs directory.
pub fn list() -> Vec<ConfigEntry> {
    list_from(&paths::outputs_dir().join("inputs"))
}

/// List the JSON configurations in a directory.
pub fn list_from(directory: &Path) -> Vec<ConfigEntry> {
    let mut entries: Vec<ConfigEntry> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            Some(ConfigEntry { path, name })
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Load a configuration as JSON.
///
/// # Errors
///
/// Propagates read/parse errors from [`model::load`].
pub fn load(path: &Path) -> Result<Value, String> {
    model::load(path)
}
