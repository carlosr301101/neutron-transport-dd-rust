//! Run results and the consolidated output file (`output_1d_###.json`).
//!
//! The JSON shape is intentionally identical to the Python reference so that
//! `nts-1d-data`, `nts-1d-plot` and `nts-1d-excel` keep working unchanged.

use std::path::Path;

use serde_json::{Value, json};

use crate::config::Mode;
use crate::scheme::Method;

/// Outcome of one solver run.
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Scalar flux (angular average) per cell.
    pub scalar_flux: Vec<f64>,
    /// Angular flux per cell and direction (`ntc x n`, ascending `mu`).
    pub angular_flux: Vec<Vec<f64>>,
    /// Full direction cosines (ascending).
    pub miu: Vec<f64>,
    /// Full quadrature weights (ascending, sum to 2).
    pub omega: Vec<f64>,
    /// Angular flux at nodes travelling to the right.
    pub psi_right: Vec<Vec<f64>>,
    /// Angular flux at nodes travelling to the left.
    pub psi_left: Vec<Vec<f64>>,
    /// Number of completed flux updates.
    pub iteration: usize,
    /// Number of outer (eigenvalue) iterations, when applicable.
    pub iteration_eigenvalue: Option<usize>,
    /// Whether the flux update converged.
    pub converged: bool,
    /// Physics mode of the run.
    pub mode: Mode,
    /// Spatial scheme used by the sweep.
    pub spatial_scheme: Method,
    /// Effective multiplication factor.
    pub k_eff: Option<f64>,
    /// `k` estimated from the global balance `P/(A+L)`.
    pub k_balance: Option<f64>,
    /// `k_eff` history of the power iteration.
    pub k_history: Vec<f64>,
    /// Whether `k_eff` converged.
    pub k_converged: Option<bool>,
    /// Number of outer iterations performed.
    pub outer_iterations: usize,
    /// Normalized thermal power \[W\].
    pub power_w: Option<f64>,
    /// Scale factor applied by the power normalization.
    pub power_scale: f64,
    /// Power before normalization \[W\].
    pub power_unnormalized: Option<f64>,
}

/// Current local timestamp in the format used by the reference output.
pub fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Build one entry of the `runs` array.
pub fn run_record(
    config_name: &str,
    configuration: &Value,
    result: &RunResult,
    source: Option<&str>,
) -> Value {
    json!({
        "config_name": config_name,
        "configuration": configuration,
        "scalar_flux": result.scalar_flux,
        "angular_flux": result.angular_flux,
        "miu": result.miu,
        "omega": result.omega,
        "iteration": result.iteration,
        "iteration_eigenvalue": result.iteration_eigenvalue,
        "converged": result.converged,
        "mode": result.mode.as_str(),
        "spatial_scheme": result.spatial_scheme.as_str(),
        "k_eff": result.k_eff,
        "k_balance": result.k_balance,
        "k_history": result.k_history,
        "k_converged": result.k_converged,
        "outer_iterations": result.outer_iterations,
        "power_w": result.power_w,
        "power_scale": result.power_scale,
        "power_unnormalized": result.power_unnormalized,
        "source": source,
        "timestamp": timestamp(),
    })
}

/// Build one `runs` entry describing a failed configuration or run.
pub fn error_record(config_name: &str, message: &str) -> Value {
    json!({
        "config_name": config_name,
        "error": message,
        "timestamp": timestamp(),
    })
}

/// Build the top-level payload written to `output_1d_###.json`.
pub fn payload(runs: Vec<Value>, schemes: Vec<String>, failures: usize) -> Value {
    let total = runs.len();
    json!({
        "solver": "NTS_DD_1D",
        "spatial_schemes": schemes,
        "runs": runs,
        "total": total,
        "failures": failures,
        "generated_at": timestamp(),
    })
}

/// Write a payload as pretty JSON, creating parent directories.
///
/// # Errors
///
/// Propagates filesystem errors (missing permissions, etc.).
pub fn write_payload(path: &Path, payload: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(payload).unwrap_or_default();
    text.push('\n');
    std::fs::write(path, text)
}
