//! `nts-rs excel`: export a consolidated output as an Excel workbook.

use std::path::{Path, PathBuf};

use clap::Args;

use crate::paths;
use crate::post::{excel, model};

/// Arguments of `nts-rs excel`.
#[derive(Debug, Args)]
pub struct ExcelCommand {
    /// Consolidated output (`output_1d_###.json`).
    source: PathBuf,

    /// Output workbook (default: next to the JSON with .xlsx).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Only export these runs (name, 0-based index or all).
    #[arg(long, num_args = 1.., value_name = "NAME|INDEX")]
    run: Vec<String>,
}

pub(crate) fn execute(command: ExcelCommand) -> Result<u8, String> {
    let source = resolve(&command.source);
    let payload = model::load(&source)?;
    let runs = model::runs(&payload)?;
    let indices = model::select_runs(runs, &command.run)?;
    if indices.is_empty() {
        return Err("no runs selected".to_string());
    }
    let selected: Vec<serde_json::Value> =
        indices.iter().map(|&index| runs[index].clone()).collect();

    let output = command
        .output
        .map(|path| resolve(&path))
        .unwrap_or_else(|| source.with_extension("xlsx"));

    excel::write_workbook(&payload, &selected, &output)?;
    println!(
        "✓ Excel saved: {} ({} run(s))",
        output.display(),
        selected.len()
    );

    Ok(0)
}

fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths::work_root().join(path)
    }
}
