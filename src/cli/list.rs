//! `nts-rs list`: list generated inputs and results.

use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use serde_json::{Value, json};

use crate::paths;

/// What to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListKind {
    /// Configuration files under `outputs/inputs`.
    Inputs,
    /// Result files under `outputs/results`.
    Outputs,
}

/// Arguments of `nts-rs list`.
#[derive(Debug, Args)]
pub struct ListCommand {
    /// What to list.
    pub kind: ListKind,

    /// Machine-readable JSON output.
    #[arg(long)]
    json: bool,
}

struct Entry {
    name: String,
    path: PathBuf,
    size: u64,
    modified: Option<chrono::DateTime<chrono::Local>>,
    summary: Option<Value>,
}

pub(crate) fn execute(command: ListCommand) -> Result<u8, String> {
    let entries = match command.kind {
        ListKind::Inputs => collect_inputs()?,
        ListKind::Outputs => collect_outputs()?,
    };

    if command.json {
        let payload = json!({
            "kind": match command.kind {
                ListKind::Inputs => "inputs",
                ListKind::Outputs => "outputs",
            },
            "entries": entries.iter().map(entry_json).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    } else {
        print_text(command.kind, &entries);
    }

    Ok(0)
}

fn collect_inputs() -> Result<Vec<Entry>, String> {
    let directory = paths::outputs_dir().join("inputs");
    let mut entries = read_directory(&directory)?
        .into_iter()
        .filter(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("json" | "txt")
            )
        })
        .map(|path| build_entry(&path, None))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn collect_outputs() -> Result<Vec<Entry>, String> {
    let directory = paths::results_dir();
    let mut entries = read_directory(&directory)?
        .into_iter()
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("output_") && name.ends_with(".json")
        })
        .map(|path| {
            let summary = summarize_output(&path);
            build_entry(&path, summary)
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn read_directory(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let read = match std::fs::read_dir(directory) {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!("failed to read {}: {error}", directory.display()));
        }
    };

    let mut paths = Vec::new();
    for entry in read {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_file() {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn build_entry(path: &Path, summary: Option<Value>) -> Entry {
    let metadata = std::fs::metadata(path).ok();
    Entry {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?")
            .to_string(),
        path: path.to_path_buf(),
        size: metadata.as_ref().map_or(0, std::fs::Metadata::len),
        modified: metadata
            .and_then(|metadata| metadata.modified().ok())
            .map(chrono::DateTime::<chrono::Local>::from),
        summary,
    }
}

fn summarize_output(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;

    if let Some(runs) = value.get("runs").and_then(Value::as_array) {
        return Some(json!({
            "format": "1d",
            "solver": value.get("solver").and_then(Value::as_str).unwrap_or("unknown"),
            "runs": runs.len(),
            "failures": value.get("failures").and_then(Value::as_u64).unwrap_or(0),
        }));
    }

    if value.get("STATUS").is_some() || value.get("MFLUX").is_some() {
        return Some(json!({"format": "2d"}));
    }

    Some(json!({"format": "unknown"}))
}

fn entry_json(entry: &Entry) -> Value {
    let mut object = json!({
        "name": entry.name,
        "path": entry.path,
        "size": entry.size,
        "modified": entry.modified.map(|stamp| stamp.format("%Y-%m-%d %H:%M:%S").to_string()),
    });
    if let (Some(summary), Some(map)) = (&entry.summary, object.as_object_mut())
        && let Some(fields) = summary.as_object()
    {
        for (key, value) in fields {
            map.insert(key.clone(), value.clone());
        }
    }
    object
}

fn print_text(kind: ListKind, entries: &[Entry]) {
    let title = match kind {
        ListKind::Inputs => "INPUTS",
        ListKind::Outputs => "OUTPUTS",
    };
    println!("{title} ({})", entries.len());
    if entries.is_empty() {
        println!("  (empty)");
        return;
    }

    for entry in entries {
        let modified = entry
            .modified
            .map(|stamp| stamp.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());
        let mut line = format!(
            "  {:<28} {:>8}  {}",
            entry.name,
            human_size(entry.size),
            modified
        );
        if let Some(summary) = &entry.summary {
            if let Some(runs) = summary.get("runs").and_then(Value::as_u64) {
                line.push_str(&format!(
                    "  {} runs={} failures={}",
                    summary
                        .get("solver")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown"),
                    runs,
                    summary.get("failures").and_then(Value::as_u64).unwrap_or(0)
                ));
            } else if summary.get("format").and_then(Value::as_str) == Some("2d") {
                line.push_str("  (2D result)");
            }
        }
        println!("{line}");
    }
}

fn human_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let value = bytes as f64;
    if value < KIB {
        format!("{bytes} B")
    } else if value < MIB {
        format!("{:.1} kB", value / KIB)
    } else {
        format!("{:.1} MB", value / MIB)
    }
}
