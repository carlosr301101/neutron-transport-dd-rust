//! `nts-rs expand`: turn a placeholder template into concrete configurations.

use std::path::PathBuf;

use clap::{ArgAction, Args};
use serde_json::Value;

use crate::cli::{split_configs, write_json};
use crate::paths;
use crate::sweep;

/// Arguments of `nts-rs expand`.
#[derive(Debug, Args)]
pub struct ExpandCommand {
    /// JSON template (object or list of objects), usually from `nts-rs config`.
    #[arg(long, value_name = "FILE")]
    json: PathBuf,

    /// Expand a placeholder over START..END with STEP (repeatable).
    #[arg(
        long = "place-holder",
        num_args = 4,
        value_names = ["NAME", "START", "END", "STEP"],
        action = ArgAction::Append
    )]
    place_holder: Vec<String>,

    /// Output JSON list (default: outputs/inputs/inputs_1d.json).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
}

pub(crate) fn execute(command: ExpandCommand) -> Result<u8, String> {
    let mut ranges = Vec::new();
    for group in command.place_holder.chunks(4) {
        if group.len() != 4 {
            return Err("--place-holder expects NAME START END STEP".to_string());
        }
        ranges.push(sweep::SweepRange {
            name: group[0].clone(),
            start: group[1].clone(),
            end: group[2].clone(),
            step: group[3].clone(),
        });
    }
    if ranges.is_empty() {
        return Err("No placeholders provided".to_string());
    }

    let text = std::fs::read_to_string(&command.json)
        .map_err(|error| format!("failed to read {}: {error}", command.json.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid JSON in {}: {error}", command.json.display()))?;
    let bases = split_configs(&value)?;
    let expansion = sweep::expand(&bases, &ranges)?;

    if expansion.steps < expansion.max_len {
        eprintln!(
            "Warning: Placeholder ranges have different lengths; using {} steps",
            expansion.steps
        );
        log::warn!(
            "placeholder ranges differ: {} vs {}",
            expansion.steps,
            expansion.max_len
        );
    }

    if expansion.configs.is_empty() {
        eprintln!("✗ No configurations generated");
        for error in &expansion.errors {
            eprintln!("  • {error}");
        }
        return Ok(1);
    }

    let output = command
        .output
        .unwrap_or_else(|| paths::outputs_dir().join("inputs").join("inputs_1d.json"));
    if output.extension().and_then(|extension| extension.to_str()) == Some("py") {
        return Err("Python module output is not supported; use a .json path".to_string());
    }

    let total = expansion.configs.len();
    write_json(&output, &Value::Array(expansion.configs))?;
    println!(
        "✓ Generated {total} config(s). Output: {}",
        output.display()
    );
    println!("Run: nts-rs run --json {}", output.display());

    if !expansion.errors.is_empty() {
        eprintln!("Warning: {} config(s) skipped", expansion.errors.len());
        for error in &expansion.errors {
            eprintln!("  • {error}");
        }
        return Ok(1);
    }

    Ok(0)
}
