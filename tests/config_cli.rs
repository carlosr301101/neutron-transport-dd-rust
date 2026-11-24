//! `nts-rs config`: interactive generator, ported from
//! `test_generate_1d_config.py`.

mod common;

use std::fs;
use std::path::Path;

use common::cli_stdin;
use serde_json::Value;
use tempfile::TempDir;

fn script(lines: &[&str]) -> String {
    format!("{}\n", lines.join("\n"))
}

fn concrete() -> String {
    script(&[
        "1",
        "1",
        "40",
        "20",
        "1",
        "1.0",
        "0.9",
        "",
        "",
        "s",
        "0.11",
        "eigenvalue",
        "",
        "",
        "",
        "n",
        "n",
        "n",
        "s",
    ])
}

fn placeholders() -> String {
    script(&[
        "1",
        "1",
        "40",
        "20",
        "1",
        "1.0",
        "kk",
        "",
        "",
        "s",
        "mm",
        "eigenvalue",
        "",
        "",
        "",
        "n",
        "n",
        "n",
        "s",
    ])
}

fn geometry() -> String {
    script(&[
        "1",
        "1",
        "cc",
        "hh",
        "1",
        "1.0",
        "0.9",
        "",
        "",
        "s",
        "0.11",
        "eigenvalue",
        "",
        "",
        "",
        "n",
        "n",
        "n",
        "s",
    ])
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read config")).expect("json")
}

#[test]
fn concrete_config_is_saved_and_valid() {
    let home = TempDir::new().expect("temp");
    let output = cli_stdin(home.path(), &["config"], &concrete());
    assert!(output.status.success(), "{output:?}");

    let path = home.path().join("outputs/inputs/config_1d_001.json");
    let value = read(&path);
    assert_eq!(value["MODE"], "eigenvalue");
    assert_eq!(value["SCS"], serde_json::json!([0.9]));
    assert_eq!(value["NU_SIGF"], serde_json::json!([0.11]));
    assert_eq!(value["TOL_K"], serde_json::json!(1e-6));
    assert_eq!(value["N"], serde_json::json!(8));
    assert_eq!(value["NC"], serde_json::json!([40]));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Configuration saved"));
    assert!(stdout.contains("Run: nts-rs run --json"));

    let validate = common::cli(home.path(), &["validate", path.to_str().unwrap()]);
    assert!(validate.status.success(), "{validate:?}");
}

#[test]
fn placeholder_config_is_saved_as_strings() {
    let home = TempDir::new().expect("temp");
    let output = cli_stdin(home.path(), &["config"], &placeholders());
    assert!(output.status.success(), "{output:?}");

    let value = read(&home.path().join("outputs/inputs/config_1d_001.json"));
    assert_eq!(value["SCS"], serde_json::json!(["kk"]));
    assert_eq!(value["NU_SIGF"], serde_json::json!(["mm"]));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Placeholders detected: kk, mm"));
    assert!(stdout.contains("--place-holder kk"));
    assert!(stdout.contains("--place-holder mm"));
}

#[test]
fn geometry_placeholders_are_saved() {
    let home = TempDir::new().expect("temp");
    let output = cli_stdin(home.path(), &["config"], &geometry());
    assert!(output.status.success(), "{output:?}");

    let value = read(&home.path().join("outputs/inputs/config_1d_001.json"));
    assert_eq!(value["NC"], serde_json::json!(["cc"]));
    assert_eq!(value["HR"], serde_json::json!(["hh"]));
    assert_eq!(value["SCS"], serde_json::json!([0.9]));
}

#[test]
fn auto_numbered_output_paths() {
    let home = TempDir::new().expect("temp");
    assert!(
        cli_stdin(home.path(), &["config"], &concrete())
            .status
            .success()
    );
    assert!(
        cli_stdin(home.path(), &["config"], &concrete())
            .status
            .success()
    );

    assert!(
        home.path()
            .join("outputs/inputs/config_1d_001.json")
            .exists()
    );
    assert!(
        home.path()
            .join("outputs/inputs/config_1d_002.json")
            .exists()
    );
}

#[test]
fn invalid_numeric_value_fails_without_writing() {
    let home = TempDir::new().expect("temp");
    let bad = script(&[
        "1",
        "1",
        "40",
        "20",
        "1",
        "1.0",
        "abc def",
        "",
        "s",
        "0.11",
        "eigenvalue",
        "",
        "",
        "",
        "n",
        "n",
        "n",
        "s",
    ]);
    let output = cli_stdin(home.path(), &["config"], &bad);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        !home
            .path()
            .join("outputs/inputs/config_1d_001.json")
            .exists()
    );
}

#[test]
fn declining_the_save_writes_nothing() {
    let home = TempDir::new().expect("temp");
    let decline = script(&[
        "1",
        "1",
        "40",
        "20",
        "1",
        "1.0",
        "0.9",
        "",
        "",
        "s",
        "0.11",
        "eigenvalue",
        "",
        "",
        "",
        "n",
        "n",
        "n",
        "n",
    ]);
    let output = cli_stdin(home.path(), &["config"], &decline);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Cancelled"));
    assert!(
        !home
            .path()
            .join("outputs/inputs/config_1d_001.json")
            .exists()
    );
}

#[test]
fn explicit_output_path_is_respected() {
    let home = TempDir::new().expect("temp");
    let target = home.path().join("nested/config.json");
    let output = cli_stdin(
        home.path(),
        &["config", "-o", target.to_str().unwrap()],
        &concrete(),
    );
    assert!(output.status.success(), "{output:?}");
    assert!(target.exists());
}

#[test]
fn python_output_path_is_rejected() {
    let home = TempDir::new().expect("temp");
    let output = cli_stdin(
        home.path(),
        &[
            "config",
            "-o",
            home.path().join("config.py").to_str().unwrap(),
        ],
        &concrete(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not supported"));
}
