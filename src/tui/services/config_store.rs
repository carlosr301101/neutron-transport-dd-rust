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
    /// Item index when the file contains a list of configurations.
    pub index: Option<usize>,
}

impl ConfigEntry {
    /// Human-readable label (`file.json` or `file.json #3`).
    pub fn label(&self) -> String {
        match self.index {
            Some(index) => format!("{} #{}", self.name, index + 1),
            None => self.name.clone(),
        }
    }
}

/// List the JSON configurations in the default inputs directory.
pub fn list() -> Vec<ConfigEntry> {
    list_from(&paths::outputs_dir().join("inputs"))
}

/// List the JSON configurations in a directory.
pub fn list_from(directory: &Path) -> Vec<ConfigEntry> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    files.sort();

    let mut entries = Vec::new();
    for path in files {
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let contents = std::fs::read_to_string(&path).ok();
        let value = contents
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok());

        match value {
            Some(Value::Array(items)) => {
                for index in 0..items.len() {
                    entries.push(ConfigEntry {
                        path: path.clone(),
                        name: name.clone(),
                        index: Some(index),
                    });
                }
            }
            _ => entries.push(ConfigEntry {
                path,
                name,
                index: None,
            }),
        }
    }
    entries
}

/// Load the configuration selected by an entry (whole file or list item).
///
/// # Errors
///
/// Propagates read/parse errors and invalid list indices.
pub fn load_entry(entry: &ConfigEntry) -> Result<Value, String> {
    let value = load(&entry.path)?;
    match entry.index {
        None => Ok(value),
        Some(index) => value
            .as_array()
            .and_then(|items| items.get(index))
            .cloned()
            .ok_or_else(|| format!("{} has no item #{}", entry.name, index + 1)),
    }
}

/// Load a configuration as JSON.
///
/// # Errors
///
/// Propagates read/parse errors from [`model::load`].
pub fn load(path: &Path) -> Result<Value, String> {
    model::load(path)
}
