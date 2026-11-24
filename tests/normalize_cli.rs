//! `nts-rs normalize`: target-power scaling of an existing output.

mod common;

use std::fs;
use std::path::Path;

use common::{base, cli, merge};
use neutron_transport_dd::power::power_watts;
use serde_json::{Value, json};
use tempfile::TempDir;

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
}

fn run_config(home: &Path, value: &Value, name: &str) -> std::path::PathBuf {
    let config = home.join(format!("{name}.json"));
    fs::write(&config, serde_json::to_string_pretty(value).unwrap()).expect("write");
    let output = home.join("out.json");
    let result = cli(
        home,
        &[
            "run",
            "--json",
            config.to_str().unwrap(),
            "--epsilon",
            "1e-3",
            "--max-iter",
            "500",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");
    output
}

fn expected_power(flux: &[f64], sigma_f: f64, dx: f64) -> f64 {
    let sigma = vec![sigma_f; flux.len()];
    let widths = vec![dx; flux.len()];
    power_watts(flux, &sigma, &widths, 200.0, 1.0)
}

#[test]
fn normalization_scales_scalar_and_angular_fluxes() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");
    let original = read_json(&source);
    let flux = original["runs"][0]["scalar_flux"]
        .as_array()
        .unwrap()
        .clone();
    let angular = original["runs"][0]["angular_flux"]
        .as_array()
        .unwrap()
        .clone();

    let result = cli(
        home.path(),
        &["normalize", source.to_str().unwrap(), "--power-kw", "10"],
    );
    assert!(result.status.success(), "{result:?}");

    let output = source.with_file_name("out_normalized.json");
    let payload = read_json(&output);
    let run = &payload["runs"][0];
    assert!((run["power_w"].as_f64().unwrap() - 10_000.0).abs() < 1e-9);
    let scale = run["power_scale"].as_f64().unwrap();
    assert!((scale * run["power_unnormalized"].as_f64().unwrap() - 10_000.0).abs() < 1e-6);

    let scaled = run["scalar_flux"].as_array().unwrap();
    for (index, before) in flux.iter().enumerate() {
        let expected = before.as_f64().unwrap() * scale;
        assert!(
            (scaled[index].as_f64().unwrap() - expected).abs() <= expected.abs() * 1e-12 + 1e-9
        );
    }
    let scaled_angular = run["angular_flux"].as_array().unwrap();
    for (cell, row) in angular.iter().enumerate() {
        for (direction, before) in row.as_array().unwrap().iter().enumerate() {
            let expected = before.as_f64().unwrap() * scale;
            let got = scaled_angular[cell].as_array().unwrap()[direction]
                .as_f64()
                .unwrap();
            assert!((got - expected).abs() <= expected.abs() * 1e-12 + 1e-9);
        }
    }
    assert!(payload["normalized_at"].is_string());
}

#[test]
fn default_nu_is_used_when_missing() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");
    let flux: Vec<f64> = read_json(&source)["runs"][0]["scalar_flux"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();

    let result = cli(
        home.path(),
        &["normalize", source.to_str().unwrap(), "--power-kw", "10"],
    );
    assert!(result.status.success(), "{result:?}");

    let payload = read_json(&source.with_file_name("out_normalized.json"));
    let power = payload["runs"][0]["power_unnormalized"].as_f64().unwrap();
    let expected = expected_power(&flux, 0.11 / 2.49, 0.2);
    assert!((power - expected).abs() <= expected.abs() * 1e-12);

    let result = cli(
        home.path(),
        &[
            "normalize",
            source.to_str().unwrap(),
            "--power-kw",
            "10",
            "--nu",
            "2.4",
            "-o",
            source.with_file_name("nu24.json").to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");
    let payload = read_json(&source.with_file_name("nu24.json"));
    let power = payload["runs"][0]["power_unnormalized"].as_f64().unwrap();
    let expected = expected_power(&flux, 0.11 / 2.4, 0.2);
    assert!((power - expected).abs() <= expected.abs() * 1e-12);
}

#[test]
fn explicit_sigma_f_takes_precedence() {
    let home = TempDir::new().expect("temp");
    let value = merge(base(), json!({"SIGMA_F": [0.05]}));
    let source = run_config(home.path(), &value, "config");
    let flux: Vec<f64> = read_json(&source)["runs"][0]["scalar_flux"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();

    let result = cli(
        home.path(),
        &["normalize", source.to_str().unwrap(), "--power-kw", "10"],
    );
    assert!(result.status.success(), "{result:?}");

    let payload = read_json(&source.with_file_name("out_normalized.json"));
    let power = payload["runs"][0]["power_unnormalized"].as_f64().unwrap();
    let expected = expected_power(&flux, 0.05, 0.2);
    assert!((power - expected).abs() <= expected.abs() * 1e-12);
}

#[test]
fn run_selection_keeps_only_the_selected_runs() {
    let home = TempDir::new().expect("temp");
    let list = home.path().join("list.json");
    fs::write(
        &list,
        serde_json::to_string_pretty(&json!([base(), base()])).unwrap(),
    )
    .expect("write");
    let source = home.path().join("out.json");
    let run = cli(
        home.path(),
        &[
            "run",
            "--json",
            list.to_str().unwrap(),
            "--epsilon",
            "1e-3",
            "--max-iter",
            "500",
            "-o",
            source.to_str().unwrap(),
        ],
    );
    assert!(run.status.success(), "{run:?}");

    let result = cli(
        home.path(),
        &[
            "normalize",
            source.to_str().unwrap(),
            "--power-kw",
            "10",
            "--run",
            "1",
        ],
    );
    assert!(result.status.success(), "{result:?}");

    let payload = read_json(&source.with_file_name("out_normalized.json"));
    assert_eq!(payload["total"], 1);
    let runs = payload["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["config_name"], "json_config_002");
    assert!((runs[0]["power_w"].as_f64().unwrap() - 10_000.0).abs() < 1e-9);
}

#[test]
fn runs_without_fission_cannot_be_normalized() {
    let home = TempDir::new().expect("temp");
    let value = merge(
        base(),
        json!({"NU_SIGF": [0.0], "MODE": "fixed_source", "Q": [1.0]}),
    );
    let source = run_config(home.path(), &value, "config");

    let result = cli(
        home.path(),
        &["normalize", source.to_str().unwrap(), "--power-kw", "10"],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("no run could be normalized"));
    assert!(!source.with_file_name("out_normalized.json").exists());
}

#[test]
fn unknown_run_and_broken_mesh_are_reported() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");

    let result = cli(
        home.path(),
        &[
            "normalize",
            source.to_str().unwrap(),
            "--power-kw",
            "10",
            "--run",
            "zzz",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("run not found"));

    let mut payload = read_json(&source);
    let configuration = payload["runs"][0]["configuration"]
        .as_object_mut()
        .expect("configuration");
    configuration.remove("NC");
    let broken = home.path().join("broken.json");
    fs::write(&broken, serde_json::to_string_pretty(&payload).unwrap()).expect("write");

    let result = cli(
        home.path(),
        &["normalize", broken.to_str().unwrap(), "--power-kw", "10"],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("NC and HR are required"));
}

#[test]
fn non_positive_power_is_rejected_by_clap() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");
    let result = cli(
        home.path(),
        &["normalize", source.to_str().unwrap(), "--power-kw", "0"],
    );
    assert_eq!(result.status.code(), Some(2));
}
