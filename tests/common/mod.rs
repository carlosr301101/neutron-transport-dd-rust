//! Shared helpers for the integration tests.

#![allow(dead_code)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;
use neutron_transport_dd::result::RunResult;
use neutron_transport_dd::scheme::Method;
use serde_json::{Value, json};

/// Path to the compiled `nts-rs` binary.
pub fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_nts-rs")
}

/// Run the CLI with `NTS_HOME` pointing at `home`.
pub fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("NTS_HOME", home)
        .current_dir(home)
        .output()
        .expect("spawn nts-rs")
}

/// Run the CLI feeding `script` through stdin.
pub fn cli_stdin(home: &Path, args: &[&str], script: &str) -> Output {
    use std::io::Write;

    let mut child = Command::new(binary())
        .args(args)
        .env("NTS_HOME", home)
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nts-rs");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write script");
    child.wait_with_output().expect("wait")
}

/// Benchmark configuration used by the reference test-suite.
pub fn base() -> Value {
    json!({
        "num_regions": 1,
        "num_zones": 1,
        "NC": [100],
        "HR": [20.0],
        "IZL": [1],
        "SCT": [1.0],
        "SCS": [0.9],
        "NU_SIGF": [0.11],
        "Q": [0.0],
        "N": 8,
        "SPATIAL_SCHEME": "DD",
        "reflex_izq": false,
        "reflex_der": false,
        "bound_left": [0, 0, 0, 0],
        "bound_right": [0, 0, 0, 0],
        "MODE": "eigenvalue"
    })
}

/// Overwrite one key.
pub fn set(mut value: Value, key: &str, new: Value) -> Value {
    value
        .as_object_mut()
        .expect("object config")
        .insert(key.to_string(), new);
    value
}

/// Remove one key.
pub fn remove(mut value: Value, key: &str) -> Value {
    value.as_object_mut().expect("object config").remove(key);
    value
}

/// Build and run a configuration with the default tolerance.
pub fn run_value(value: &Value, epsilon: f64) -> RunResult {
    run_with_scheme(value, epsilon, None)
}

/// Build and run a configuration, optionally forcing the scheme.
pub fn run_with_scheme(value: &Value, epsilon: f64, scheme: Option<Method>) -> RunResult {
    let config = Config::from_value(value, epsilon, 2000, scheme).expect("configuration");
    let runner = match scheme {
        Some(method) => Runner::with_scheme(config, method),
        None => Runner::new(config),
    };
    runner.expect("runner").run().expect("run")
}

/// Build and run a raw JSON object with a JSON fragment of overrides.
pub fn merge(value: Value, patch: Value) -> Value {
    let mut base = value.as_object().expect("object").clone();
    for (key, item) in patch.as_object().expect("object patch") {
        base.insert(key.clone(), item.clone());
    }
    Value::Object(base)
}

pub fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

/// Fast benchmark configuration serialized as pretty JSON (for the CLI tests).
pub fn run_config_json() -> String {
    let value = set(base(), "NC", json!([20]));
    serde_json::to_string_pretty(&value).expect("serialize config")
}

/// JSON list with one valid and one invalid configuration.
pub fn json_patch_valid_list() -> String {
    let valid = set(base(), "NC", json!([20]));
    let invalid = remove(valid.clone(), "SCT");
    serde_json::to_string_pretty(&json!([valid, invalid])).expect("serialize list")
}
