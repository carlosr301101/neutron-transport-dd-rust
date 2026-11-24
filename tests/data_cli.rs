//! `nts-rs data`: scalar-flux extraction into `datos.json`.

mod common;

use std::fs;
use std::path::Path;

use common::{cli, run_config_json};
use serde_json::{Value, json};
use tempfile::TempDir;

fn run_once(home: &Path) -> std::path::PathBuf {
    let config = home.join("config.json");
    fs::write(&config, run_config_json()).expect("write config");
    let output = cli(home, &["run", "--json", config.to_str().unwrap()]);
    assert!(output.status.success(), "{output:?}");
    home.join("outputs/results/output_1d_001.json")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
}

#[test]
fn extraction_writes_the_reference_schema() {
    let home = TempDir::new().expect("temp");
    let result = run_once(home.path());

    let output = cli(
        home.path(),
        &[
            "data",
            "-r",
            result.to_str().unwrap(),
            "--positions",
            "0",
            "10",
            "19",
        ],
    );
    assert!(output.status.success(), "{output:?}");

    let datos = read_json(&home.path().join("outputs/results/datos.json"));
    assert_eq!(datos["positions"], json!([0, 10, 19]));
    assert_eq!(datos["spatial_schemes"], json!(["DD"]));
    assert_eq!(datos["source"], result.to_string_lossy().as_ref());

    let row = &datos["data"][0];
    assert_eq!(row["config_name"], "json_config_001");
    assert_eq!(row["SCS"], json!(0.9));
    assert_eq!(row["NU_SIGF"], json!(0.11));
    assert_eq!(row["spatial_scheme"], "DD");
    assert_eq!(row["mode"], "eigenvalue");

    let payload = read_json(&result);
    let flux = payload["runs"][0]["scalar_flux"].as_array().unwrap();
    assert_eq!(row["flux_0"], flux[0]);
    assert_eq!(row["flux_10"], flux[10]);
    assert_eq!(row["flux_19"], flux[19]);
}

#[test]
fn latest_uses_the_last_output() {
    let home = TempDir::new().expect("temp");
    run_once(home.path());

    let config = home.path().join("config.json");
    let second = cli(home.path(), &["run", "--json", config.to_str().unwrap()]);
    assert!(second.status.success());
    let latest = home.path().join("outputs/results/output_1d_002.json");
    assert!(latest.exists());

    let output = cli(home.path(), &["data", "--latest", "--positions", "0"]);
    assert!(output.status.success(), "{output:?}");

    let datos = read_json(&home.path().join("outputs/results/datos.json"));
    assert_eq!(datos["source"], latest.to_string_lossy().as_ref());
}

#[test]
fn out_of_range_positions_are_reported_without_rows() {
    let home = TempDir::new().expect("temp");
    let result = run_once(home.path());

    let output = cli(
        home.path(),
        &[
            "data",
            "-r",
            result.to_str().unwrap(),
            "--positions",
            "0",
            "999",
        ],
    );
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("skipped"));

    let datos = read_json(&home.path().join("outputs/results/datos.json"));
    assert_eq!(datos["data"], json!([]));
    let errors = datos["errors"].as_array().expect("errors");
    assert!(errors[0].as_str().unwrap().contains("out of range"));
}

#[test]
fn fallback_inputs_supply_the_configuration() {
    let home = TempDir::new().expect("temp");
    let result = run_once(home.path());

    let mut payload = read_json(&result);
    let run = payload["runs"][0].as_object_mut().expect("run");
    let configuration = run.remove("configuration").expect("configuration");
    let stripped = home.path().join("no_config.json");
    fs::write(&stripped, serde_json::to_string_pretty(&payload).unwrap()).expect("write");

    let inputs = home.path().join("inputs.json");
    fs::write(
        &inputs,
        serde_json::to_string_pretty(&json!({"json_config_001": configuration})).unwrap(),
    )
    .expect("write inputs");

    let output = cli(
        home.path(),
        &[
            "data",
            "-r",
            stripped.to_str().unwrap(),
            "--inputs",
            inputs.to_str().unwrap(),
            "--positions",
            "0",
        ],
    );
    assert!(output.status.success(), "{output:?}");

    let datos = read_json(&home.path().join("outputs/results/datos.json"));
    assert_eq!(datos["data"].as_array().unwrap().len(), 1);
    assert_eq!(datos["data"][0]["SCS"], json!(0.9));
}

#[test]
fn positional_and_option_forms_are_equivalent() {
    let home = TempDir::new().expect("temp");
    let result = run_once(home.path());

    let positional = cli(
        home.path(),
        &[
            "data",
            result.to_str().unwrap(),
            "--positions",
            "5",
            "-o",
            home.path().join("a.json").to_str().unwrap(),
        ],
    );
    assert!(positional.status.success());

    let option = cli(
        home.path(),
        &[
            "data",
            "-r",
            result.to_str().unwrap(),
            "--positions",
            "5",
            "-o",
            home.path().join("b.json").to_str().unwrap(),
        ],
    );
    assert!(option.status.success());

    assert_eq!(
        read_json(&home.path().join("a.json")),
        read_json(&home.path().join("b.json"))
    );
}

#[test]
fn missing_source_and_latest_without_outputs_are_errors() {
    let home = TempDir::new().expect("temp");
    let output = cli(home.path(), &["data"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--latest"));

    let output = cli(home.path(), &["data", "--latest"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no output_1d"));
}

#[test]
fn custom_output_path_is_respected() {
    let home = TempDir::new().expect("temp");
    let result = run_once(home.path());
    let target = home.path().join("nested/datos.json");

    let output = cli(
        home.path(),
        &[
            "data",
            "-r",
            result.to_str().unwrap(),
            "--positions",
            "0",
            "-o",
            target.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    assert!(target.exists());
}
