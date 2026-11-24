//! `nts-rs show`: summarise a result file.

use std::path::PathBuf;

use clap::Args;
use serde_json::Value;

/// Arguments of `nts-rs show`.
#[derive(Debug, Args)]
pub struct ShowCommand {
    /// Result JSON file written by `nts-rs run`.
    pub result: PathBuf,

    /// Print the raw result payload.
    #[arg(long)]
    json: bool,
}

pub(crate) fn execute(command: ShowCommand) -> Result<u8, String> {
    let path = &command.result;
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;

    if command.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return Ok(0);
    }

    let Some(runs) = value.get("runs").and_then(Value::as_array) else {
        if value.get("STATUS").is_some() || value.get("MFLUX").is_some() {
            return Err(
                "2D result files are supported in step 4; use --json to dump them".to_string(),
            );
        }
        return Err(format!(
            "unrecognized result file: {}",
            command.result.display()
        ));
    };

    println!("File:      {}", command.result.display());
    println!(
        "Solver:    {}",
        value
            .get("solver")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "Generated: {}",
        value
            .get("generated_at")
            .and_then(Value::as_str)
            .unwrap_or("-")
    );
    let failures = value.get("failures").and_then(Value::as_u64).unwrap_or(0);
    println!("Runs:      {} (failures: {failures})", runs.len());

    if let Some(schemes) = value.get("spatial_schemes").and_then(Value::as_array) {
        let schemes: Vec<&str> = schemes.iter().filter_map(Value::as_str).collect();
        if !schemes.is_empty() {
            println!("Schemes:   {}", schemes.join(", "));
        }
    }
    println!();

    for (index, run) in runs.iter().enumerate() {
        let name = run
            .get("config_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");

        if let Some(error) = run.get("error").and_then(Value::as_str) {
            println!("  {}. {name} | ERROR: {error}", index + 1);
            continue;
        }

        let mode = run.get("mode").and_then(Value::as_str).unwrap_or("-");
        let scheme = run
            .get("spatial_scheme")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let converged = run
            .get("converged")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let iteration = run
            .get("iteration")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let outer = run.get("outer_iterations").and_then(Value::as_u64);
        let k_eff = run.get("k_eff").and_then(Value::as_f64);
        let k_balance = run.get("k_balance").and_then(Value::as_f64);
        let power_w = run.get("power_w").and_then(Value::as_f64);

        let status = if converged { "converged" } else { "not converged" };
        let outer = outer.map_or_else(|| "-".to_string(), |value| value.to_string());

        println!(
            "  {}. {name} | {mode} | {scheme} | {status} | iter={iteration} (outer {outer}) | k_eff={} | k_balance={} | power={}",
            index + 1,
            format_optional(k_eff, 8),
            format_optional(k_balance, 8),
            format_optional(power_w, 6),
        );
    }

    Ok(0)
}

fn format_optional(value: Option<f64>, precision: usize) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{value:.precision$}"))
}
