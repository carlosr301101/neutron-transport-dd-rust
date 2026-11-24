//! `nts-rs config`: interactive configuration generator with placeholders.

use std::path::PathBuf;

use clap::Args;

use crate::cli::{confirm, write_json};
use crate::paths;
use crate::placeholder::detect_placeholders;
use crate::wizard::Wizard;

/// Arguments of `nts-rs config`.
#[derive(Debug, Args)]
pub struct ConfigCommand {
    /// Output JSON path (default: auto-numbered outputs/inputs/config_1d_###.json).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
}

pub(crate) fn execute(command: ConfigCommand) -> Result<u8, String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut wizard = Wizard::with_placeholders(stdin.lock(), stdout);
    let value = wizard
        .build_value(None)
        .map_err(|error| error.to_string())?;
    // Release the stdin lock before asking the confirmation question.
    drop(wizard);

    println!("\n--- CONFIGURATION SUMMARY ---");
    println!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    );

    if !confirm("Save configuration? (y/n) [y]: ", true)? {
        println!("Cancelled.");
        return Ok(0);
    }

    let output = command.output.unwrap_or_else(paths::next_input_path);
    if output.extension().and_then(|extension| extension.to_str()) == Some("py") {
        return Err("Python module output is not supported; use a .json path".to_string());
    }
    write_json(&output, &value)?;
    println!("✓ Configuration saved: {}", output.display());

    let names = detect_placeholders(&value);
    if names.is_empty() {
        println!("Run: nts-rs run --json {}", output.display());
    } else {
        println!(
            "Placeholders detected: {}",
            names.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        println!("Expand with:");
        let mut hint = format!("  nts-rs expand --json {}", output.display());
        for name in &names {
            hint.push_str(&format!(" --place-holder {name} START END STEP"));
        }
        hint.push_str(" -o outputs/inputs/inputs_1d.json");
        println!("{hint}");
    }

    Ok(0)
}
