//! End-to-end CLI and output-file schema tests.
//!
//! The output must stay compatible with the Python post-processing tools
//! (`nts-1d-data`, `nts-1d-plot`, `nts-1d-excel`).

mod common;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{base, json_patch_valid_list, run_config_json};
use serde_json::{Value, json};
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_nts-rs")
}

fn run_cli(home: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("NTS_HOME", home)
        .current_dir(home)
        .output()
        .expect("spawn nts-rs")
}

fn run_cli_with_stdin(home: &Path, args: &[&str], script: &str) -> Output {
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

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read output")).expect("valid JSON")
}

#[test]
fn single_configuration_writes_the_reference_schema() {
    let home = TempDir::new().expect("temp dir");
    let config_path = home.path().join("config.json");
    fs::write(&config_path, run_config_json()).expect("write config");

    let output = run_cli(
        home.path(),
        &["run", "--json", config_path.to_str().unwrap()],
    );
    assert!(output.status.success(), "{:?}", output);

    let results = home.path().join("outputs/results/output_1d_001.json");
    assert!(results.exists(), "auto-numbered output missing");

    let payload = read_json(&results);
    assert_eq!(payload["solver"], "NTS_DD_1D");
    assert_eq!(payload["total"], 1);
    assert_eq!(payload["failures"], 0);
    assert_eq!(payload["spatial_schemes"], json!(["DD"]));

    let run = &payload["runs"][0];
    for key in [
        "config_name",
        "configuration",
        "scalar_flux",
        "angular_flux",
        "miu",
        "omega",
        "iteration",
        "iteration_eigenvalue",
        "converged",
        "mode",
        "spatial_scheme",
        "k_eff",
        "k_balance",
        "k_history",
        "k_converged",
        "outer_iterations",
        "power_w",
        "power_scale",
        "power_unnormalized",
        "source",
        "timestamp",
    ] {
        assert!(run.get(key).is_some(), "missing key {key}");
    }

    assert_eq!(run["config_name"], "json_config_001");
    assert_eq!(run["configuration"]["NU_SIGF"], json!([0.11]));
    assert_eq!(run["mode"], "eigenvalue");
    assert_eq!(run["spatial_scheme"], "DD");
    assert!(run["k_eff"].as_f64().unwrap() > 1.0);
    assert_eq!(run["source"], config_path.to_string_lossy().as_ref());
    let timestamp = run["timestamp"].as_str().expect("timestamp");
    assert_eq!(timestamp.len(), 19);
    assert!(timestamp.contains('T'));
}

#[test]
fn outputs_are_auto_numbered() {
    let home = TempDir::new().expect("temp dir");
    let config_path = home.path().join("config.json");
    fs::write(&config_path, run_config_json()).expect("write config");
    let args = ["run", "--json", config_path.to_str().unwrap()];

    assert!(run_cli(home.path(), &args).status.success());
    assert!(run_cli(home.path(), &args).status.success());

    assert!(
        home.path()
            .join("outputs/results/output_1d_001.json")
            .exists()
    );
    assert!(
        home.path()
            .join("outputs/results/output_1d_002.json")
            .exists()
    );
}

#[test]
fn failing_configuration_is_recorded_without_aborting_the_list() {
    let home = TempDir::new().expect("temp dir");
    let config_path = home.path().join("configs.json");
    fs::write(&config_path, json_patch_valid_list()).expect("write configs");

    let output = run_cli(
        home.path(),
        &["run", "--json", config_path.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1), "failures must exit with 1");

    let payload = read_json(&home.path().join("outputs/results/output_1d_001.json"));
    assert_eq!(payload["total"], 2);
    assert_eq!(payload["failures"], 1);
    assert_eq!(payload["runs"][0]["config_name"], "json_config_001");
    assert!(payload["runs"][1]["error"].is_string());
    assert!(payload["runs"][1].get("scalar_flux").is_none());
}

#[test]
fn forced_scheme_is_applied_to_the_output() {
    let home = TempDir::new().expect("temp dir");
    let config_path = home.path().join("config.json");
    fs::write(&config_path, run_config_json()).expect("write config");

    let output = run_cli(
        home.path(),
        &[
            "run",
            "--json",
            config_path.to_str().unwrap(),
            "--scheme",
            "SC",
        ],
    );
    assert!(output.status.success(), "{:?}", output);

    let payload = read_json(&home.path().join("outputs/results/output_1d_001.json"));
    assert_eq!(payload["runs"][0]["spatial_scheme"], "SC");
    assert_eq!(payload["runs"][0]["configuration"]["SPATIAL_SCHEME"], "SC");
}

#[test]
fn explicit_output_path_is_respected() {
    let home = TempDir::new().expect("temp dir");
    let config_path = home.path().join("config.json");
    fs::write(&config_path, run_config_json()).expect("write config");
    let target = home.path().join("nested/result.json");

    let output = run_cli(
        home.path(),
        &[
            "run",
            "--json",
            config_path.to_str().unwrap(),
            "--output",
            target.to_str().unwrap(),
        ],
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(target.exists());
}

#[test]
fn manual_mode_runs_the_wizard_and_writes_the_output() {
    let home = TempDir::new().expect("temp dir");
    let script = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "n", "1.0", "s", "s", "y",
    ]
    .join("\n")
        + "\n";

    let output = run_cli_with_stdin(home.path(), &["run", "--manual"], &script);
    assert!(output.status.success(), "{:?}", output);

    let payload = read_json(&home.path().join("outputs/results/output_1d_001.json"));
    assert_eq!(payload["total"], 1);
    let run = &payload["runs"][0];
    assert_eq!(run["config_name"], "manual");
    assert_eq!(run["mode"], "fixed_source");
    assert_eq!(run["source"], "interactive wizard");
}

#[test]
fn missing_source_flag_is_an_error() {
    let home = TempDir::new().expect("temp dir");
    let output = run_cli(home.path(), &["run"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("either --json FILE or --manual"));
}

#[test]
fn json_list_of_objects_is_accepted() {
    let home = TempDir::new().expect("temp dir");
    let config_path = home.path().join("list.json");
    let value = json!([base(), base()]);
    fs::write(&config_path, serde_json::to_string_pretty(&value).unwrap()).expect("write list");

    let output = run_cli(
        home.path(),
        &["run", "--json", config_path.to_str().unwrap()],
    );
    assert!(output.status.success(), "{:?}", output);

    let payload = read_json(&home.path().join("outputs/results/output_1d_001.json"));
    assert_eq!(payload["total"], 2);
    assert_eq!(payload["runs"][0]["config_name"], "json_config_001");
    assert_eq!(payload["runs"][1]["config_name"], "json_config_002");
}
