//! `nts-rs` command line interface.
//!
//! Step 0 of the Rust rewrite: the `run` subcommand executes one or more 1D
//! configurations from JSON (or the interactive wizard) and writes the
//! consolidated `output_1d_###.json` expected by the Python post-processing
//! tools.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::Value;

use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;
use neutron_transport_dd::paths;
use neutron_transport_dd::result;
use neutron_transport_dd::scheme::Method;
use neutron_transport_dd::wizard;

/// Neutron transport automation CLI.
#[derive(Debug, Parser)]
#[command(
    name = "nts-rs",
    version,
    about = "1D S_N neutron transport automation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run one configuration, a JSON list, or the interactive wizard.
    Run(RunCommand),
}

/// Arguments of `nts-rs run`.
#[derive(Debug, clap::Args)]
struct RunCommand {
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

    /// Convergence tolerance for the scalar flux.
    #[arg(short, long, default_value_t = 1e-5)]
    epsilon: f64,

    /// Maximum number of flux updates.
    #[arg(short, long, default_value_t = 2000)]
    max_iter: usize,

    /// Print per-run progress.
    #[arg(short, long)]
    verbose: bool,
}

fn parse_scheme(raw: &str) -> Result<Method, String> {
    Method::from_name(raw).map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Command::Run(args) = cli.command;

    match execute_run(args) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("Error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn execute_run(args: RunCommand) -> Result<u8, String> {
    let mut runs: Vec<Value> = Vec::new();
    let mut schemes: Vec<String> = Vec::new();
    let mut failures = 0usize;

    if args.manual {
        let config = wizard::run_interactive(args.scheme, args.epsilon, args.max_iter)
            .map_err(|error| error.to_string())?;
        print_summary(&config);
        if !confirm("Run? (y/n) [y]: ")? {
            println!("Cancelled.");
            return Ok(0);
        }
        run_one(
            "manual",
            config,
            Some("interactive wizard"),
            args.verbose,
            &mut runs,
            &mut schemes,
            &mut failures,
        );
    } else if let Some(path) = &args.json {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let configs = load_json_configs(&value)?;
        let source = path.to_string_lossy().to_string();

        for (name, config_value) in configs {
            match Config::from_value(&config_value, args.epsilon, args.max_iter, args.scheme) {
                Ok(config) => run_one(
                    &name,
                    config,
                    Some(source.as_str()),
                    args.verbose,
                    &mut runs,
                    &mut schemes,
                    &mut failures,
                ),
                Err(error) => {
                    failures += 1;
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
    if failures > 0 {
        eprintln!("Warning: {failures} run(s) failed");
        return Ok(1);
    }
    Ok(0)
}

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

    let configuration = config.as_dict();
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
            eprintln!("✗ {name}: {error}");
            runs.push(result::error_record(name, &error.to_string()));
        }
    }
}

fn load_json_configs(value: &Value) -> Result<Vec<(String, Value)>, String> {
    match value {
        Value::Object(_) => Ok(vec![("json_config_001".to_string(), value.clone())]),
        Value::Array(items) if items.iter().all(Value::is_object) && !items.is_empty() => Ok(items
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("json_config_{:03}", index + 1), item.clone()))
            .collect()),
        _ => Err("JSON must be an object or a list of objects".to_string()),
    }
}

fn confirm(prompt: &str) -> Result<bool, String> {
    print!("{prompt}");
    std::io::stdout()
        .flush()
        .map_err(|error| error.to_string())?;

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let answer = line.trim().to_lowercase();
    if answer.is_empty() {
        return Ok(true);
    }
    Ok(!matches!(answer.as_str(), "n" | "no"))
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
