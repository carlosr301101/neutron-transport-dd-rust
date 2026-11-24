//! Cross-command CLI tests: validate, list, show, status and logging.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{base, run_config_json};
use serde_json::{Value, json};
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_nts-rs")
}

fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("NTS_HOME", home)
        .current_dir(home)
        .output()
        .expect("spawn nts-rs")
}

fn cli_stdin(home: &Path, args: &[&str], script: &str) -> Output {
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

fn write_config(home: &Path, name: &str, value: &Value) -> std::path::PathBuf {
    let path = home.join(name);
    fs::write(&path, serde_json::to_string_pretty(value).unwrap()).expect("write config");
    path
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
}

#[test]
fn validate_reports_valid_and_invalid_configs() {
    let home = TempDir::new().expect("temp");
    let good = write_config(home.path(), "good.json", &base());
    let output = cli(home.path(), &["validate", good.to_str().unwrap()]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Configuration is valid"));
    assert!(stdout.contains("regions: 1"));

    let bad_value = common::remove(base(), "SCT");
    let bad = write_config(home.path(), "bad.json", &bad_value);
    let output = cli(home.path(), &["validate", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Missing required field: 'SCT'"));
}

#[test]
fn validate_json_output_is_machine_readable() {
    let home = TempDir::new().expect("temp");
    let list = json!([base(), common::remove(base(), "SCT")]);
    let path = write_config(home.path(), "list.json", &list);

    let output = cli(home.path(), &["validate", path.to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(payload["valid"], false);
    assert_eq!(payload["configs"].as_array().unwrap().len(), 2);
    assert_eq!(payload["configs"][0]["valid"], true);
    assert_eq!(payload["configs"][1]["valid"], false);
}

#[test]
fn validate_marks_2d_configs_as_pending() {
    let home = TempDir::new().expect("temp");
    let path = write_config(
        home.path(),
        "two_d.json",
        &json!({"N": 4, "NZ": 1, "zones": [{"sigma_t": 1.0, "sigma_s": 0.5}]}),
    );
    let output = cli(home.path(), &["validate", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("lands in step 4"));
}

#[test]
fn list_inputs_and_outputs_including_json() {
    let home = TempDir::new().expect("temp");
    let inputs = home.path().join("outputs/inputs");
    fs::create_dir_all(&inputs).unwrap();
    fs::write(inputs.join("config_1d_001.json"), run_config_json()).unwrap();

    let output = cli(home.path(), &["list", "inputs"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("INPUTS (1)"));
    assert!(text.contains("config_1d_001.json"));

    let config_path = inputs.join("config_1d_001.json");
    let run = cli(
        home.path(),
        &[
            "run",
            "--json",
            config_path.to_str().unwrap(),
            "-o",
            home.path()
                .join("outputs/results/output_1d_001.json")
                .to_str()
                .unwrap(),
        ],
    );
    assert!(run.status.success(), "{run:?}");

    let output = cli(home.path(), &["list", "outputs", "--json"]);
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(payload["kind"], "outputs");
    assert_eq!(payload["entries"][0]["solver"], "NTS_DD_1D");
    assert_eq!(payload["entries"][0]["runs"], 1);
}

#[test]
fn show_summarises_a_run_and_dumps_json() {
    let home = TempDir::new().expect("temp");
    let config_path = write_config(home.path(), "config.json", &base());
    let result_path = home.path().join("result.json");
    let run = cli(
        home.path(),
        &[
            "run",
            "--json",
            config_path.to_str().unwrap(),
            "-o",
            result_path.to_str().unwrap(),
        ],
    );
    assert!(run.status.success(), "{run:?}");

    let output = cli(home.path(), &["show", result_path.to_str().unwrap()]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Solver:    NTS_DD_1D"));
    assert!(text.contains("k_eff="));
    assert!(text.contains("converged"));

    let output = cli(
        home.path(),
        &["show", result_path.to_str().unwrap(), "--json"],
    );
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(payload["runs"].as_array().unwrap().len(), 1);
}

#[test]
fn show_rejects_unknown_files() {
    let home = TempDir::new().expect("temp");
    let path = write_config(home.path(), "other.json", &json!({"hello": "world"}));
    let output = cli(home.path(), &["show", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized result file"));
}

#[test]
fn status_reports_counts_and_missing_solvers() {
    let home = TempDir::new().expect("temp");
    let output = cli(home.path(), &["status"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Work root:"));
    assert!(text.contains("NTS_SOLVERS_DIR not set"));

    let output = cli(home.path(), &["status", "--json"]);
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(payload["counts"]["configs"], 0);
    assert_eq!(
        payload["solvers_2d"]["missing"].as_array().unwrap().len(),
        0
    );

    let output = Command::new(binary())
        .args(["status", "--json"])
        .env("NTS_HOME", home.path())
        .env("NTS_SOLVERS_DIR", home.path().join("solvers"))
        .current_dir(home.path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(
        payload["solvers_2d"]["missing"].as_array().unwrap().len(),
        5
    );
}

#[test]
fn run_writes_global_and_per_run_logs() {
    let home = TempDir::new().expect("temp");
    let config_path = write_config(home.path(), "config.json", &base());
    let output = cli(
        home.path(),
        &[
            "run",
            "--json",
            config_path.to_str().unwrap(),
            "-o",
            home.path().join("result.json").to_str().unwrap(),
        ],
    );
    assert!(output.status.success());

    let logs = home.path().join("outputs/logs");
    let global: Vec<_> = fs::read_dir(&logs)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nts-rs.log"))
        })
        .collect();
    assert_eq!(global.len(), 1, "daily log file");
    let text = fs::read_to_string(&global[0]).expect("read log");
    assert!(text.contains("completed 1 run(s)"));

    let run_logs: Vec<_> = fs::read_dir(logs.join("runs"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert_eq!(run_logs.len(), 1);
    let text = fs::read_to_string(&run_logs[0]).expect("read run log");
    assert!(text.contains("configuration:"));
    assert!(text.contains("result:"));
}

#[test]
fn global_verbosity_controls_progress() {
    let home = TempDir::new().expect("temp");
    let config_path = write_config(home.path(), "config.json", &base());
    let config = config_path.to_str().unwrap();
    let result_path = home.path().join("result.json");
    let result = result_path.to_str().unwrap();

    let quiet = cli(home.path(), &["-q", "run", "--json", config, "-o", result]);
    assert!(quiet.status.success());
    let quiet_text = String::from_utf8_lossy(&quiet.stdout);
    assert!(!quiet_text.contains("Running"));

    let verbose = cli(home.path(), &["-v", "run", "--json", config, "-o", result]);
    assert!(verbose.status.success());
    let verbose_text = String::from_utf8_lossy(&verbose.stdout);
    assert!(verbose_text.contains("Running json_config_001"));
}

#[test]
fn manual_run_through_stdin_still_works_with_global_flags() {
    let home = TempDir::new().expect("temp");
    let script = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "n", "1.0", "s", "s", "y",
    ]
    .join("\n")
        + "\n";

    let output = cli_stdin(
        home.path(),
        &["--log-level", "warn", "run", "--manual"],
        &script,
    );
    assert!(output.status.success(), "{output:?}");
    let payload = read_json(&home.path().join("outputs/results/output_1d_001.json"));
    assert_eq!(payload["runs"][0]["mode"], "fixed_source");
}

#[test]
fn unsupported_json_shape_is_rejected() {
    let home = TempDir::new().expect("temp");
    let path = write_config(home.path(), "bad.json", &json!([1, 2, 3]));
    let output = cli(home.path(), &["validate", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("JSON must be an object"));
}
