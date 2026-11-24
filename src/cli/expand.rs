//! `nts-rs expand`: turn a placeholder template into concrete configurations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::{ArgAction, Args};
use serde_json::Value;

use crate::cli::{is_two_d_config, split_configs, write_json};
use crate::config::Config;
use crate::paths;
use crate::placeholder::{
    build_values, contains_placeholders, decimal_places, replace_placeholders, round_to,
};

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

struct Range {
    values: Vec<f64>,
}

pub(crate) fn execute(command: ExpandCommand) -> Result<u8, String> {
    let mut ranges: BTreeMap<String, Range> = BTreeMap::new();
    let mut rounding_digits = 0usize;

    for group in command.place_holder.chunks(4) {
        if group.len() != 4 {
            return Err("--place-holder expects NAME START END STEP".to_string());
        }
        let name = &group[0];
        let start = parse_group(name, &group[1])?;
        let end = parse_group(name, &group[2])?;
        let step = parse_group(name, &group[3])?;
        rounding_digits = rounding_digits.max(decimal_places(&group[3]));

        let values = build_values(start, end, step)
            .map_err(|error| format!("invalid placeholder '{name}': {error}"))?;
        ranges.insert(name.clone(), Range { values });
    }

    if ranges.is_empty() {
        return Err("No placeholders provided".to_string());
    }

    let text = std::fs::read_to_string(&command.json)
        .map_err(|error| format!("failed to read {}: {error}", command.json.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid JSON in {}: {error}", command.json.display()))?;
    let bases = split_configs(&value)?;

    let lengths: Vec<usize> = ranges.values().map(|range| range.values.len()).collect();
    let min_len = *lengths.iter().min().unwrap_or(&0);
    let max_len = *lengths.iter().max().unwrap_or(&0);
    if min_len != max_len {
        eprintln!("Warning: Placeholder ranges have different lengths; using {min_len} steps");
        log::warn!("placeholder ranges differ: {} vs {}", min_len, max_len);
    }

    let names: BTreeSet<String> = ranges.keys().cloned().collect();
    let mut results: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for index in 0..min_len {
        let replacements: BTreeMap<String, f64> = ranges
            .iter()
            .map(|(name, range)| (name.clone(), round_to(range.values[index], rounding_digits)))
            .collect();

        for (base_name, base) in &bases {
            let expanded = replace_placeholders(base, &replacements);
            if contains_placeholders(&expanded, &names) {
                errors.push(format!(
                    "Step {} ({base_name}): unresolved placeholders",
                    index + 1
                ));
                continue;
            }
            if is_two_d_config(&expanded) {
                errors.push(format!(
                    "Step {} ({base_name}): 2D expansion lands in step 4",
                    index + 1
                ));
                continue;
            }
            match Config::from_value(&expanded, 1e-5, 2000, None) {
                Ok(_) => results.push(expanded),
                Err(error) => errors.push(format!("Step {} ({base_name}): {error}", index + 1)),
            }
        }
    }

    if results.is_empty() {
        eprintln!("✗ No configurations generated");
        for error in &errors {
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

    let total = results.len();
    write_json(&output, &Value::Array(results))?;
    println!(
        "✓ Generated {total} config(s). Output: {}",
        output.display()
    );
    println!("Run: nts-rs run --json {}", output.display());

    if !errors.is_empty() {
        eprintln!("Warning: {} config(s) skipped", errors.len());
        for error in &errors {
            eprintln!("  • {error}");
        }
        return Ok(1);
    }

    Ok(0)
}

fn parse_group(name: &str, raw: &str) -> Result<f64, String> {
    raw.trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid placeholder '{name}': invalid value {raw:?}"))
}
