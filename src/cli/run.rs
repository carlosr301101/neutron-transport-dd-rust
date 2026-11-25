//! `nts-rs run`: execute one or more 1D configurations.

use std::path::PathBuf;

use clap::Args;
use serde_json::Value;

use crate::cli::{confirm, split_configs};
use crate::config::{Config, RunOverrides};
use crate::core::Runner;
use crate::logging::RunLog;
use crate::paths;
use crate::result;
use crate::scheme::Method;
use crate::wizard;

/// Arguments of `nts-rs run`.
#[derive(Debug, Args)]
pub struct RunCommand {
    /// JSON file with one configuration object or a list of them.
    #[arg(long, value_name = "FILE", conflicts_with = "manual")]
    json: Option<PathBuf>,

    /// Interactive step-by-step configuration.
    #[arg(long)]
    manual: bool,

    /// Output JSON path (default: auto-numbered outputs/results).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Force the spatial scheme, ignoring SPATIAL_SCHEME.
    #[arg(long, value_parser = parse_scheme)]
    scheme: Option<Method>,

    /// Convergence tolerance for the scalar flux (default: config or 1e-5).
    #[arg(short, long)]
    epsilon: Option<f64>,

    /// Maximum number of flux updates (default: config or 2000).
    #[arg(short, long)]
    max_iter: Option<usize>,
}

fn parse_scheme(raw: &str) -> Result<Method, String> {
    Method::from_name(raw).map_err(|error| error.to_string())
}

pub(crate) fn execute(args: RunCommand, verbose: bool) -> Result<u8, String> {
    let mut runs: Vec<Value> = Vec::new();
    let mut schemes: Vec<String> = Vec::new();
    let mut failures = 0usize;

    if args.manual {
        let epsilon = args.epsilon.unwrap_or(1e-5);
        let max_iter = args.max_iter.unwrap_or(2000);
        let config = wizard::run_interactive(args.scheme, epsilon, max_iter)
            .map_err(|error| error.to_string())?;
        print_summary(&config);
        if !confirm("Run? (y/n) [y]: ", true)? {
            println!("Cancelled.");
            return Ok(0);
        }
        run_one(
            "manual",
            config,
            Some("interactive wizard"),
            verbose,
            &mut runs,
            &mut schemes,
            &mut failures,
        );
    } else if let Some(path) = &args.json {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let configs = split_configs(&value)?;
        let source = path.to_string_lossy().to_string();

        for (name, config_value) in configs {
            match Config::from_value_with(
                &config_value,
                RunOverrides {
                    epsilon: args.epsilon,
                    max_iter: args.max_iter,
                },
                args.scheme,
            ) {
                Ok(config) => run_one(
                    &name,
                    config,
                    Some(source.as_str()),
                    verbose,
                    &mut runs,
                    &mut schemes,
                    &mut failures,
                ),
                Err(error) => {
                    failures += 1;
                    log::warn!("{name}: {error}");
                    eprintln!("✗ {name}: {error}");
                    runs.push(result::error_record(&name, &error.to_string()));
                }
            }
        }
    } else {
        return Err("either --json FILE or --manual is required".to_string());
    }

    schemes.sort();
    schemes.dedup();

    let payload = result::payload(runs, schemes, failures);
    let output = args.output.unwrap_or_else(paths::next_output_path);
    result::write_payload(&output, &payload)
        .map_err(|error| format!("failed to write {}: {error}", output.display()))?;

    let total = payload["total"].as_u64().unwrap_or(0);
    println!("\n✓ Completed {total} run(s). Output: {}", output.display());
    log::info!("completed {total} run(s) into {}", output.display());

    if failures > 0 {
        log::warn!("{failures} run(s) failed");
        eprintln!("Warning: {failures} run(s) failed");
        return Ok(1);
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn run_one(
    name: &str,
    config: Config,
    source: Option<&str>,
    verbose: bool,
    runs: &mut Vec<Value>,
    schemes: &mut Vec<String>,
    failures: &mut usize,
) {
    if verbose {
        println!("Running {name} ...");
    }
    log::info!("run {name} started");

    let configuration = config.as_dict();
    let mut run_log = match RunLog::create(name) {
        Ok(log) => Some(log),
        Err(error) => {
            log::warn!("could not create run log for {name}: {error}");
            None
        }
    };
    if let Some(log) = &mut run_log {
        log.line(format!("run: {name}"));
        log.line(format!("source: {}", source.unwrap_or("-")));
        log.line("configuration:");
        log.line(serde_json::to_string_pretty(&configuration).unwrap_or_default());
    }

    let outcome = Runner::new(config).and_then(|runner| runner.run());

    match outcome {
        Ok(result_value) => {
            if verbose {
                println!(
                    "  mode={} scheme={} iter={} converged={} k_eff={:?}",
                    result_value.mode.as_str(),
                    result_value.spatial_scheme.as_str(),
                    result_value.iteration,
                    result_value.converged,
                    result_value.k_eff
                );
            }
            if let Some(log) = &mut run_log {
                log.line(format!(
                    "result: mode={} scheme={} iteration={} converged={} k_eff={:?} k_balance={:?}",
                    result_value.mode.as_str(),
                    result_value.spatial_scheme.as_str(),
                    result_value.iteration,
                    result_value.converged,
                    result_value.k_eff,
                    result_value.k_balance
                ));
            }
            log::debug!(
                "run {name} converged={} iteration={}",
                result_value.converged,
                result_value.iteration
            );
            schemes.push(result_value.spatial_scheme.as_str().to_string());
            runs.push(result::run_record(
                name,
                &configuration,
                &result_value,
                source,
            ));
        }
        Err(error) => {
            *failures += 1;
            log::error!("run {name} failed: {error}");
            if let Some(log) = &mut run_log {
                log.line(format!("error: {error}"));
            }
            eprintln!("✗ {name}: {error}");
            runs.push(result::error_record(name, &error.to_string()));
        }
    }
}

fn print_summary(config: &Config) {
    println!("\n--- CONFIGURATION SUMMARY ---");
    println!(
        "{} regions, {} zones, S{} ({} directions per hemisphere)",
        config.num_regions, config.num_zones, config.n, config.n_half
    );
    println!("Cells per region: {:?}", config.nc);
    println!("Thicknesses [cm]: {:?}", config.hr);
    println!("Zone distribution: {:?}", config.izl);
    println!("Sigma_T: {:?}", config.sct);
    println!("Sigma_S: {:?}", config.scs);
    println!("nu*Sigma_f: {:?}", config.nu_sigf);
    println!("Sources: {:?}", config.q);
    println!("Spatial scheme: {}", config.spatial_scheme);
    println!(
        "Mode: {} | K_EFF: {:?} | POWER_KW: {:?}",
        config.mode.as_str(),
        config.k_eff,
        config.power_kw
    );
}
