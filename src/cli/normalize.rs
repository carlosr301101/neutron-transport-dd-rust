//! `nts-rs normalize`: scale an existing output to a target power.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{Value, json};

use crate::cli::write_json;
use crate::paths;
use crate::post::{model, normalize};
use crate::result;

/// Arguments of `nts-rs normalize`.
#[derive(Debug, Args)]
pub struct NormalizeCommand {
    /// Consolidated output (`output_1d_###.json`).
    source: PathBuf,

    /// Output JSON (default: <stem>_normalized.json).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Target power [kW].
    #[arg(long, value_parser = positive)]
    power_kw: f64,

    /// Recoverable energy per fission [MeV] (default: configuration).
    #[arg(long = "ef-mev", alias = "e-f-mev", value_parser = positive)]
    ef_mev: Option<f64>,

    /// Slab cross-sectional area [cm^2] (default: configuration).
    #[arg(long, value_parser = positive)]
    area_cm2: Option<f64>,

    /// Neutrons per fission, used when the config has NU_SIGF but no NU.
    #[arg(long, value_parser = positive)]
    nu: Option<f64>,

    /// Only normalize these runs (name, 0-based index or all).
    #[arg(long, num_args = 1.., value_name = "NAME|INDEX")]
    run: Vec<String>,
}

pub(crate) fn execute(command: NormalizeCommand) -> Result<u8, String> {
    let source = resolve(&command.source);
    let mut payload = model::load(&source)?;

    let runs = payload
        .get_mut("runs")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "expected a consolidated output with runs[]".to_string())?;
    let indices = model::select_runs(runs, &command.run)?;
    if indices.is_empty() {
        return Err("no runs selected".to_string());
    }

    let mut normalized = 0usize;
    for &index in &indices {
        let name = model::run_name(&runs[index], index);
        match normalize::normalize_run(
            &mut runs[index],
            command.power_kw,
            command.ef_mev,
            command.area_cm2,
            command.nu,
        ) {
            Ok((c_n, power)) => {
                normalized += 1;
                println!(
                    "✓ {name}: P~={power:.6e} W, C_N={c_n:.6e}, P={} kW",
                    command.power_kw
                );
            }
            Err(error) => eprintln!("! {name}: {error}"),
        }
    }

    if normalized == 0 {
        eprintln!("error: no run could be normalized");
        return Ok(1);
    }

    let selected: Vec<Value> = indices.iter().map(|&index| runs[index].clone()).collect();
    if let Some(object) = payload.as_object_mut() {
        object.insert("runs".into(), json!(selected));
        object.insert("total".into(), json!(selected.len()));
        object.insert("normalized_at".into(), json!(result::timestamp()));
    }

    let output = command
        .output
        .map(|path| resolve(&path))
        .unwrap_or_else(|| {
            let stem = source
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("output_1d");
            source.with_file_name(format!("{stem}_normalized.json"))
        });
    write_json(&output, &payload)?;
    println!("✓ Normalized output: {}", output.display());

    Ok(0)
}

fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths::work_root().join(path)
    }
}

fn positive(raw: &str) -> Result<f64, String> {
    let value: f64 = raw.parse().map_err(|_| "must be a number".to_string())?;
    if value <= 0.0 {
        return Err("must be > 0".to_string());
    }
    Ok(value)
}
