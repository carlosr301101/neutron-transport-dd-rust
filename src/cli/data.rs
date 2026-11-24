//! `nts-rs data`: extract scalar fluxes at cell positions into `datos.json`.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{Map, Value};

use crate::cli::write_json;
use crate::paths;
use crate::post::{extract, model};

/// Arguments of `nts-rs data`.
#[derive(Debug, Args)]
pub struct DataCommand {
    /// Result file (`output_1d_###.json`); ignored with --latest.
    #[arg(conflicts_with = "result")]
    source: Option<PathBuf>,

    /// Result file (same as the positional argument).
    #[arg(short = 'r', long = "result", value_name = "FILE")]
    result: Option<PathBuf>,

    /// Use the most recent output_1d_*.json in outputs/results.
    #[arg(long)]
    latest: bool,

    /// Cell positions to extract.
    #[arg(long, num_args = 1.., default_values_t = [0usize, 100])]
    positions: Vec<usize>,

    /// Optional JSON with configurations (object of name -> config or list).
    #[arg(long)]
    inputs: Option<PathBuf>,

    /// Output path (default: outputs/results/datos.json).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
}

pub(crate) fn execute(command: DataCommand) -> Result<u8, String> {
    let source = resolve_source(&command)?;
    let payload = model::load(&source)?;

    let fallback = match &command.inputs {
        Some(path) => Some(load_inputs(&resolve(path))?),
        None => None,
    };

    let source_label = source.to_string_lossy().to_string();
    let extraction = extract::extract(
        &payload,
        &source_label,
        &command.positions,
        fallback.as_ref(),
    )?;

    let output = command
        .output
        .map(|path| resolve(&path))
        .unwrap_or_else(|| paths::results_dir().join("datos.json"));
    write_json(&output, &extraction.payload)?;

    println!(
        "✓ Data extracted: {} ({} row(s), {} position(s))",
        output.display(),
        extraction.rows,
        command.positions.len()
    );
    if !extraction.errors.is_empty() {
        eprintln!("Warning: {} run(s) skipped", extraction.errors.len());
        for error in &extraction.errors {
            eprintln!("  • {error}");
        }
    }

    Ok(0)
}

fn resolve_source(command: &DataCommand) -> Result<PathBuf, String> {
    if command.latest {
        return latest_output();
    }
    let source = command
        .result
        .as_ref()
        .or(command.source.as_ref())
        .ok_or_else(|| "provide a source path or use --latest".to_string())?;
    Ok(resolve(source))
}

fn latest_output() -> Result<PathBuf, String> {
    let directory = paths::results_dir();
    let read = match std::fs::read_dir(&directory) {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "no output_1d_*.json files in {}",
                directory.display()
            ));
        }
        Err(error) => {
            return Err(format!("failed to read {}: {error}", directory.display()));
        }
    };

    let mut candidates: Vec<PathBuf> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("output_1d_") && name.ends_with(".json")
        })
        .collect();
    candidates.sort();

    candidates
        .pop()
        .ok_or_else(|| format!("no output_1d_*.json files in {}", directory.display()))
}

fn load_inputs(path: &Path) -> Result<Map<String, Value>, String> {
    let value = model::load(path)?;
    match value {
        Value::Object(map) if map.values().all(Value::is_object) => Ok(map),
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => Ok(items
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("json_config_{:03}", index + 1), item.clone()))
            .collect()),
        _ => Err(format!(
            "{} must contain an object of configurations or a list of objects",
            path.display()
        )),
    }
}

fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths::work_root().join(path)
    }
}
