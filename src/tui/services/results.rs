//! List and load consolidated results from `outputs/results`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::paths;
use crate::post::model;

/// One consolidated output found on disk.
#[derive(Debug, Clone)]
pub struct OutputEntry {
    /// File path.
    pub path: PathBuf,
    /// File name.
    pub name: String,
    /// Runs in the payload.
    pub total: usize,
    /// Failed runs.
    pub failures: usize,
    /// Solver name.
    pub solver: String,
}

/// List the 1D outputs in the default results directory.
pub fn list() -> Vec<OutputEntry> {
    list_from(&paths::results_dir())
}

/// List the 1D outputs in a directory.
pub fn list_from(directory: &Path) -> Vec<OutputEntry> {
    let mut entries: Vec<OutputEntry> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("output_1d_") && name.ends_with(".json")
        })
        .map(|path| {
            let value = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());
            let total = value
                .as_ref()
                .and_then(|value| value.get("runs"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let failures = value
                .as_ref()
                .and_then(|value| value.get("failures"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let solver = value
                .as_ref()
                .and_then(|value| value.get("solver"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            OutputEntry {
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?")
                    .to_string(),
                path,
                total,
                failures,
                solver,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Load one output payload.
///
/// # Errors
///
/// Propagates read/parse errors from [`model::load`].
pub fn load(path: &Path) -> Result<Value, String> {
    model::load(path)
}
