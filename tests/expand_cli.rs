//! `nts-rs expand`: placeholder expansion, ported from
//! `test_generate_placeholder_json.py`.

mod common;

use std::fs;
use std::path::Path;

use common::{base, cli, set};
use serde_json::{Value, json};
use tempfile::TempDir;

fn write_template(home: &Path, value: &Value) -> std::path::PathBuf {
    let path = home.join("template.json");
    fs::write(&path, serde_json::to_string_pretty(value).unwrap()).expect("write template");
    path
}

fn read_list(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path).expect("read output");
    let value: Value = serde_json::from_str(&text).expect("json");
    value.as_array().cloned().expect("array")
}

#[test]
fn expansion_matches_the_reference_grid() {
    let home = TempDir::new().expect("temp");
    let template = set(
        set(set(base(), "SCS", json!(["kk"])), "NU_SIGF", json!(["mm"])),
        "TOL_K",
        json!(1e-6),
    );
    let source = write_template(home.path(), &template);
    let output = home.path().join("inputs_1d.json");

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.5",
            "0.2",
            "--place-holder",
            "mm",
            "0.05",
            "0.15",
            "0.05",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stdout).contains("Generated 3 config(s)"));

    let configs = read_list(&output);
    assert_eq!(configs.len(), 3);
    assert_eq!(
        configs
            .iter()
            .map(|cfg| cfg["SCS"][0].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![0.1, 0.3, 0.5]
    );
    assert_eq!(
        configs
            .iter()
            .map(|cfg| cfg["NU_SIGF"][0].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![0.05, 0.1, 0.15]
    );
    assert_eq!(configs[0]["TOL_K"], json!(1e-6));
    assert_eq!(configs[0]["MODE"], json!("eigenvalue"));
}

#[test]
fn geometry_placeholders_become_numbers() {
    let home = TempDir::new().expect("temp");
    let template = set(set(base(), "NC", json!(["cc"])), "HR", json!(["hh"]));
    let source = write_template(home.path(), &template);
    let output = home.path().join("geometry.json");

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "cc",
            "20",
            "40",
            "20",
            "--place-holder",
            "hh",
            "10",
            "20",
            "10",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");

    let configs = read_list(&output);
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0]["NC"][0], json!(20.0));
    assert_eq!(configs[1]["NC"][0], json!(40.0));
    assert_eq!(configs[0]["HR"][0], json!(10.0));
    assert_eq!(configs[1]["HR"][0], json!(20.0));
}

#[test]
fn a_list_of_templates_is_expanded_for_every_step() {
    let home = TempDir::new().expect("temp");
    let template = set(base(), "SCS", json!(["kk"]));
    let source = write_template(home.path(), &json!([template, template]));
    let output = home.path().join("list.json");

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.2",
            "0.1",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(read_list(&output).len(), 4);
}

#[test]
fn missing_placeholder_is_rejected_without_output() {
    let home = TempDir::new().expect("temp");
    let template = set(set(base(), "SCS", json!(["kk"])), "NU_SIGF", json!(["mm"]));
    let source = write_template(home.path(), &template);
    let output = home.path().join("inputs_1d.json");

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.5",
            "0.2",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(!output.exists());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("No configurations generated"));
}

#[test]
fn different_range_lengths_use_zip_semantics() {
    let home = TempDir::new().expect("temp");
    let template = set(set(base(), "SCS", json!(["kk"])), "NU_SIGF", json!(["mm"]));
    let source = write_template(home.path(), &template);
    let output = home.path().join("zip.json");

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.3",
            "0.1",
            "--place-holder",
            "mm",
            "0.05",
            "0.1",
            "0.05",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(read_list(&output).len(), 2);
    assert!(String::from_utf8_lossy(&result.stderr).contains("different lengths"));
}

#[test]
fn invalid_expanded_configs_are_skipped_with_exit_one() {
    let home = TempDir::new().expect("temp");
    let template = set(base(), "SCT", json!(["kk"]));
    let source = write_template(home.path(), &template);
    let output = home.path().join("skipped.json");

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0",
            "1",
            "1",
            "-o",
            output.to_str().unwrap(),
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    let configs = read_list(&output);
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0]["SCT"][0], json!(1.0));
    assert!(String::from_utf8_lossy(&result.stderr).contains("1 config(s) skipped"));
}

#[test]
fn output_defaults_under_nts_home() {
    let home = TempDir::new().expect("temp");
    let template = set(base(), "SCS", json!(["kk"]));
    let source = write_template(home.path(), &template);

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.2",
            "0.1",
        ],
    );
    assert!(result.status.success(), "{result:?}");
    assert!(home.path().join("outputs/inputs/inputs_1d.json").exists());
}

#[test]
fn python_output_and_missing_placeholders_are_rejected() {
    let home = TempDir::new().expect("temp");
    let template = set(base(), "SCS", json!(["kk"]));
    let source = write_template(home.path(), &template);

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.2",
            "0.1",
            "-o",
            home.path().join("inputs.py").to_str().unwrap(),
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("not supported"));

    let result = cli(home.path(), &["expand", "--json", source.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("No placeholders provided"));
}

#[test]
fn invalid_step_is_reported() {
    let home = TempDir::new().expect("temp");
    let template = set(base(), "SCS", json!(["kk"]));
    let source = write_template(home.path(), &template);

    let result = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.5",
            "0",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("Step must be greater than 0"));
}

#[test]
fn expanded_configurations_can_be_run() {
    let home = TempDir::new().expect("temp");
    let template = set(set(base(), "NC", json!([20])), "SCS", json!(["kk"]));
    let source = write_template(home.path(), &template);
    let inputs = home.path().join("inputs.json");
    let results = home.path().join("results.json");

    let expand = cli(
        home.path(),
        &[
            "expand",
            "--json",
            source.to_str().unwrap(),
            "--place-holder",
            "kk",
            "0.1",
            "0.3",
            "0.2",
            "-o",
            inputs.to_str().unwrap(),
        ],
    );
    assert!(expand.status.success(), "{expand:?}");

    let run = cli(
        home.path(),
        &[
            "run",
            "--json",
            inputs.to_str().unwrap(),
            "--epsilon",
            "1e-3",
            "--max-iter",
            "500",
            "-o",
            results.to_str().unwrap(),
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let payload: Value = serde_json::from_str(&fs::read_to_string(&results).unwrap()).unwrap();
    assert_eq!(payload["total"], 2);
    assert_eq!(payload["failures"], 0);
}
