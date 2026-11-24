//! `nts-rs` command line interface.
//!
//! Global options (`-v`, `-q`, `--log-level`) are handled here; each
//! subcommand lives in its own module and returns an exit code.

pub mod config;
pub mod data;
pub mod expand;
pub mod list;
pub mod normalize;
pub mod run;
pub mod show;
pub mod status;
pub mod validate;

use std::process::ExitCode;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use log::LevelFilter;
use serde_json::Value;

use crate::logging;

/// Options accepted by every subcommand.
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Increase verbosity (repeat for trace).
    #[arg(
        short = 'v',
        long,
        global = true,
        action = ArgAction::Count,
        conflicts_with = "quiet"
    )]
    verbose: u8,

    /// Only print errors.
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    /// Explicit log level (overrides -v/-q).
    #[arg(long, global = true, value_enum, value_name = "LEVEL")]
    log_level: Option<LogLevel>,
}

/// Log levels accepted by `--log-level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    /// Only errors.
    Error,
    /// Errors and warnings (default).
    Warn,
    /// Progress information.
    Info,
    /// Diagnostic detail.
    Debug,
    /// Everything, including the file log.
    Trace,
}

impl LogLevel {
    /// Convert to the `log` crate filter.
    pub const fn filter(self) -> LevelFilter {
        match self {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
        }
    }
}

impl GlobalArgs {
    /// Console level derived from `--log-level`, `-q` and `-v`.
    pub fn console_level(&self) -> LevelFilter {
        if let Some(level) = self.log_level {
            return level.filter();
        }
        if self.quiet {
            return LevelFilter::Error;
        }
        match self.verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        }
    }

    /// Whether progress information should be printed.
    pub fn is_verbose(&self) -> bool {
        self.console_level() >= LevelFilter::Info
    }
}

/// Neutron transport automation CLI.
#[derive(Debug, Parser)]
#[command(
    name = "nts-rs",
    version,
    about = "1D S_N neutron transport automation"
)]
pub struct Cli {
    #[command(flatten)]
    global: GlobalArgs,

    #[command(subcommand)]
    command: Command,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run one configuration, a JSON list, or the interactive wizard.
    Run(run::RunCommand),
    /// Generate a configuration JSON, optionally with placeholders.
    Config(config::ConfigCommand),
    /// Expand placeholders into a list of configurations.
    Expand(expand::ExpandCommand),
    /// Extract scalar fluxes at positions into datos.json.
    Data(data::DataCommand),
    /// Normalize an existing output to a target power.
    Normalize(normalize::NormalizeCommand),
    /// Validate one configuration or a list of them.
    Validate(validate::ValidateCommand),
    /// List generated inputs or results.
    List(list::ListCommand),
    /// Summarise a result file.
    Show(show::ShowCommand),
    /// Report directories, file counts and solver availability.
    Status(status::StatusCommand),
}

/// Entry point used by `src/main.rs`.
pub fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(error) = logging::init(cli.global.console_level()) {
        eprintln!("Error: failed to initialise logging: {error}");
        return ExitCode::FAILURE;
    }
    log::debug!("command: {:?}", cli.command);

    let verbose = cli.global.is_verbose();
    let outcome = match cli.command {
        Command::Run(command) => run::execute(command, verbose),
        Command::Config(command) => config::execute(command),
        Command::Expand(command) => expand::execute(command),
        Command::Data(command) => data::execute(command),
        Command::Normalize(command) => normalize::execute(command),
        Command::Validate(command) => validate::execute(command),
        Command::List(command) => list::execute(command),
        Command::Show(command) => show::execute(command),
        Command::Status(command) => status::execute(command),
    };

    match outcome {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            log::error!("{message}");
            eprintln!("Error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Split a JSON object or list of objects into named configurations.
pub(crate) fn split_configs(value: &Value) -> Result<Vec<(String, Value)>, String> {
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

/// Detect a 2D configuration (validated in step 4).
pub(crate) fn is_two_d_config(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    ["NZ", "zones", "NR_X", "XDOM", "ZMAP", "QMAP", "BC", "TOL"]
        .iter()
        .any(|key| object.contains_key(*key))
}

/// Ask a yes/no question on `stdin`; an empty answer yields `default_yes`.
pub(crate) fn confirm(prompt: &str, default_yes: bool) -> Result<bool, String> {
    use std::io::Write;

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
        return Ok(default_yes);
    }
    Ok(!matches!(answer.as_str(), "n" | "no"))
}

/// Write pretty JSON, creating parent directories.
pub(crate) fn write_json(path: &std::path::Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    text.push('\n');
    std::fs::write(path, text)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn two_d_config_is_detected() {
        assert!(is_two_d_config(&json!({"NZ": 1, "N": 4})));
        assert!(!is_two_d_config(&json!({"num_zones": 1, "N": 4})));
    }

    #[test]
    fn split_accepts_object_and_list() {
        let single = split_configs(&json!({"a": 1})).expect("object");
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].0, "json_config_001");

        let list = split_configs(&json!([{"a": 1}, {"b": 2}])).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].0, "json_config_002");
    }

    #[test]
    fn split_rejects_other_shapes() {
        assert!(split_configs(&json!([])).is_err());
        assert!(split_configs(&json!([1, 2])).is_err());
        assert!(split_configs(&json!("text")).is_err());
    }

    #[test]
    fn verbosity_resolution() {
        let base = GlobalArgs {
            verbose: 0,
            quiet: false,
            log_level: None,
        };
        assert_eq!(base.console_level(), LevelFilter::Warn);
        assert!(!base.is_verbose());

        let debug = GlobalArgs {
            verbose: 1,
            ..base.clone()
        };
        assert_eq!(debug.console_level(), LevelFilter::Debug);
        assert!(debug.is_verbose());

        let trace = GlobalArgs {
            verbose: 2,
            ..base.clone()
        };
        assert_eq!(trace.console_level(), LevelFilter::Trace);

        let quiet = GlobalArgs {
            quiet: true,
            ..base.clone()
        };
        assert_eq!(quiet.console_level(), LevelFilter::Error);

        let explicit = GlobalArgs {
            log_level: Some(LogLevel::Info),
            ..base
        };
        assert_eq!(explicit.console_level(), LevelFilter::Info);
    }
}
