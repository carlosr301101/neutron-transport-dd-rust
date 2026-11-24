//! `nts-rs status`: report directories, file counts and solver availability.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::json;

use crate::paths;

/// Arguments of `nts-rs status`.
#[derive(Debug, Args)]
pub struct StatusCommand {
    /// Machine-readable JSON output.
    #[arg(long)]
    json: bool,
}

pub(crate) fn execute(command: StatusCommand) -> Result<u8, String> {
    let work_root = paths::work_root();
    let outputs = paths::outputs_dir();
    let inputs = outputs.join("inputs");
    let results = outputs.join("results");
    let plots = results.join("plots");
    let logs = outputs.join("logs");

    let directories: Vec<(&str, &Path)> = vec![
        ("outputs", &outputs),
        ("inputs", &inputs),
        ("results", &results),
        ("plots", &plots),
        ("logs", &logs),
    ];

    let configs = count_files(&inputs, |path| {
        matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("json" | "txt")
        )
    })?;
    let results_1d = count_files(&results, |path| {
        name_of(path).starts_with("output_1d_") && is_json(path)
    })?;
    let results_2d = count_files(&results, |path| {
        name_of(path).starts_with("output_")
            && !name_of(path).starts_with("output_1d_")
            && is_json(path)
    })?;
    let log_files = count_files(&logs, |_| true)?;

    let solvers_dir = std::env::var_os("NTS_SOLVERS_DIR").map(PathBuf::from);
    let missing: Vec<&str> = match &solvers_dir {
        Some(directory) => paths::AVAILABLE_SOLVERS
            .iter()
            .copied()
            .filter(|name| !directory.join(name).is_file())
            .collect(),
        None => Vec::new(),
    };

    if command.json {
        let payload = json!({
            "work_root": work_root,
            "directories": directories
                .iter()
                .map(|(name, path)| {
                    json!({
                        "name": name,
                        "path": path,
                        "exists": path.is_dir(),
                    })
                })
                .collect::<Vec<_>>(),
            "counts": {
                "configs": configs,
                "results_1d": results_1d,
                "results_2d": results_2d,
                "logs": log_files,
            },
            "solvers_2d": {
                "directory": solvers_dir,
                "available": paths::AVAILABLE_SOLVERS,
                "missing": missing,
            },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
        return Ok(0);
    }

    println!("Work root: {}", work_root.display());
    println!("\nDirectories:");
    for (name, path) in &directories {
        let mark = if path.is_dir() { "✓" } else { "⚠" };
        let state = if path.is_dir() { "" } else { " (missing)" };
        println!("  {mark} {} [{name}]{state}", path.display());
    }

    println!("\nCounts:");
    println!("  configs:      {configs}");
    println!("  results (1D): {results_1d}");
    println!("  results (2D): {results_2d}");
    println!("  log files:    {log_files}");

    println!("\n2D solvers:");
    match solvers_dir {
        Some(directory) => {
            println!("  directory: {}", directory.display());
            for name in paths::AVAILABLE_SOLVERS {
                let mark = if directory.join(name).is_file() {
                    "✓"
                } else {
                    "⚠"
                };
                let state = if directory.join(name).is_file() {
                    ""
                } else {
                    " (missing)"
                };
                println!("  {mark} {name}{state}");
            }
        }
        None => {
            println!("  NTS_SOLVERS_DIR not set (2D workflow lands in step 4)");
        }
    }

    Ok(0)
}

fn count_files(directory: &Path, predicate: impl Fn(&Path) -> bool) -> Result<usize, String> {
    let read = match std::fs::read_dir(directory) {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!("failed to read {}: {error}", directory.display()));
        }
    };

    let mut count = 0;
    for entry in read {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_file() && predicate(&path) {
            count += 1;
        }
    }
    Ok(count)
}

fn name_of(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

fn is_json(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("json")
}
