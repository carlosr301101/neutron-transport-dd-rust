//! `nts-rs validate`: check one or more configuration files.

use std::path::PathBuf;

use clap::Args;
use serde_json::{Value, json};

use crate::cli::{is_two_d_config, split_configs};
use crate::config::{Config, RunOverrides};

/// Arguments of `nts-rs validate`.
#[derive(Debug, Args)]
pub struct ValidateCommand {
    /// Configuration JSON file (object or list of objects).
    pub config: PathBuf,

    /// Machine-readable JSON output.
    #[arg(long)]
    json: bool,
}

pub(crate) fn execute(command: ValidateCommand) -> Result<u8, String> {
    let path = &command.config;
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;

    let configs = split_configs(&value)?;
    let mut reports: Vec<Value> = Vec::with_capacity(configs.len());
    let mut all_valid = true;

    for (name, config_value) in configs {
        if is_two_d_config(&config_value) {
            all_valid = false;
            reports.push(json!({
                "name": name,
                "valid": false,
                "errors": ["2D configuration validation lands in step 4"],
            }));
            continue;
        }

        match Config::from_value_with(&config_value, RunOverrides::default(), None) {
            Ok(config) => reports.push(json!({
                "name": name,
                "valid": true,
                "errors": [],
                "summary": summary(&config),
            })),
            Err(error) => {
                all_valid = false;
                reports.push(json!({
                    "name": name,
                    "valid": false,
                    "errors": [error.to_string()],
                }));
            }
        }
    }

    if command.json {
        let payload = json!({"valid": all_valid, "configs": reports});
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    } else {
        print_text(&reports);
    }

    Ok(if all_valid { 0 } else { 1 })
}

fn summary(config: &Config) -> Value {
    json!({
        "num_regions": config.num_regions,
        "num_zones": config.num_zones,
        "cells": config.ntc,
        "n": config.n,
        "directions": config.n_half,
        "spatial_scheme": config.method().as_str(),
        "mode": config.mode.as_str(),
        "k_eff": config.k_eff,
        "epsilon": config.epsilon,
        "max_iter": config.max_iter,
        "reflex_izq": config.reflex_izq,
        "reflex_der": config.reflex_der,
        "external_source": external_source(config),
    })
}

fn external_source(config: &Config) -> f64 {
    config
        .q
        .iter()
        .zip(&config.hr)
        .map(|(source, width)| source * width)
        .sum()
}

fn print_text(reports: &[Value]) {
    let multiple = reports.len() > 1;
    for report in reports {
        let name = report["name"].as_str().unwrap_or("config");
        if report["valid"].as_bool().unwrap_or(false) {
            if multiple {
                println!("✓ {name}: configuration is valid");
            } else {
                println!("✓ Configuration is valid");
            }
            if let Some(summary) = report.get("summary") {
                println!(
                    "  regions: {} | zones: {} | cells: {} | S{} ({} per hemisphere)",
                    summary["num_regions"],
                    summary["num_zones"],
                    summary["cells"],
                    summary["n"],
                    summary["directions"]
                );
                println!(
                    "  scheme: {} | mode: {} | k_eff: {}",
                    summary["spatial_scheme"].as_str().unwrap_or("-"),
                    summary["mode"].as_str().unwrap_or("-"),
                    format_optional(summary["k_eff"].as_f64())
                );
                println!(
                    "  boundaries: left={}, right={} | external source: {}",
                    boundary(summary["reflex_izq"].as_bool().unwrap_or(false)),
                    boundary(summary["reflex_der"].as_bool().unwrap_or(false)),
                    format_number(summary["external_source"].as_f64().unwrap_or(0.0))
                );
            }
        } else {
            let errors = report["errors"].as_array().cloned().unwrap_or_default();
            if multiple {
                println!(
                    "✗ {name}: validation failed with {} error(s):",
                    errors.len()
                );
            } else {
                println!("✗ Validation failed with {} error(s):", errors.len());
            }
            for error in errors {
                if let Some(message) = error.as_str() {
                    println!("  - {message}");
                }
            }
        }
    }
}

fn boundary(reflective: bool) -> &'static str {
    if reflective { "reflective" } else { "vacuum" }
}

fn format_number(value: f64) -> String {
    format!("{value}")
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), format_number)
}
