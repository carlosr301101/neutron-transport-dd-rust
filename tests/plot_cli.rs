//! `nts-rs plot`: PNG generation and argument validation.

mod common;

use std::fs;
use std::path::Path;

use common::{base, cli, merge, set};
use serde_json::{Value, json};
use tempfile::TempDir;

fn png_size(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).expect("read png");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "{} is not a PNG",
        path.display()
    );
    assert!(bytes.len() > 1024, "{} looks empty", path.display());
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (width, height)
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

fn run_list(home: &Path) -> std::path::PathBuf {
    let list = home.join("list.json");
    fs::write(
        &list,
        serde_json::to_string_pretty(&json!([
            set(base(), "NC", json!([20])),
            set(base(), "NC", json!([20]))
        ]))
        .unwrap(),
    )
    .expect("write");
    let output = home.join("list_out.json");
    let result = cli(
        home,
        &[
            "run",
            "--json",
            list.to_str().unwrap(),
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

#[test]
fn scalar_plot_has_the_requested_size() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");

    let result = cli(
        home.path(),
        &[
            "plot",
            "-r",
            source.to_str().unwrap(),
            "--kind",
            "scalar",
            "--width",
            "800",
            "--height",
            "500",
        ],
    );
    assert!(result.status.success(), "{result:?}");

    let path = home.path().join("outputs/results/plots/scalar.png");
    assert_eq!(png_size(&path), (800, 500));
}

#[test]
fn every_kind_renders() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");

    for kind in ["scalar", "angular", "k-history"] {
        let result = cli(
            home.path(),
            &["plot", "-r", source.to_str().unwrap(), "--kind", kind],
        );
        assert!(result.status.success(), "{kind}: {result:?}");
        let path = home
            .path()
            .join(format!("outputs/results/plots/{kind}.png"));
        assert_eq!(png_size(&path), (1600, 900));
    }

    let data = cli(
        home.path(),
        &[
            "data",
            "-r",
            source.to_str().unwrap(),
            "--positions",
            "0",
            "19",
        ],
    );
    assert!(data.status.success());
    let result = cli(
        home.path(),
        &[
            "plot",
            "-i",
            home.path()
                .join("outputs/results/datos.json")
                .to_str()
                .unwrap(),
            "--kind",
            "scs",
        ],
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        png_size(&home.path().join("outputs/results/plots/scs.png")),
        (1600, 900)
    );
}

#[test]
fn scalar_can_overlay_selected_runs_and_use_custom_output() {
    let home = TempDir::new().expect("temp");
    let source = run_list(home.path());
    let target = home.path().join("nested/overlay.png");

    let result = cli(
        home.path(),
        &[
            "plot",
            "-r",
            source.to_str().unwrap(),
            "--kind",
            "scalar",
            "--run",
            "0",
            "1",
            "-o",
            target.to_str().unwrap(),
        ],
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(png_size(&target), (1600, 900));
}

#[test]
fn angular_requires_exactly_one_run() {
    let home = TempDir::new().expect("temp");
    let source = run_list(home.path());
    let result = cli(
        home.path(),
        &[
            "plot",
            "-r",
            source.to_str().unwrap(),
            "--kind",
            "angular",
            "--run",
            "all",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("exactly one run"));
}

#[test]
fn k_history_requires_a_history() {
    let home = TempDir::new().expect("temp");
    let value = merge(
        set(base(), "NC", json!([20])),
        json!({"NU_SIGF": [0.0], "MODE": "fixed_source", "Q": [1.0]}),
    );
    let source = run_config(home.path(), &value, "config");
    let result = cli(
        home.path(),
        &[
            "plot",
            "-r",
            source.to_str().unwrap(),
            "--kind",
            "k-history",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("k_eff history"));
}

#[test]
fn scs_needs_input_and_valid_x_axis() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");

    let result = cli(
        home.path(),
        &["plot", "-r", source.to_str().unwrap(), "--kind", "scs"],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("requires -i"));

    let data = cli(
        home.path(),
        &["data", "-r", source.to_str().unwrap(), "--positions", "0"],
    );
    assert!(data.status.success());

    let result = cli(
        home.path(),
        &[
            "plot",
            "-i",
            home.path()
                .join("outputs/results/datos.json")
                .to_str()
                .unwrap(),
            "--kind",
            "scs",
            "--x",
            "NOPE",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid --x"));

    let result = cli(
        home.path(),
        &[
            "plot",
            "-r",
            source.to_str().unwrap(),
            "--kind",
            "scalar",
            "--x",
            "SCS",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("only valid with --kind scs"));
}

#[test]
fn zone_names_flag_is_accepted() {
    let home = TempDir::new().expect("temp");
    let source = run_config(home.path(), &base(), "config");
    let result = cli(
        home.path(),
        &[
            "plot",
            "-r",
            source.to_str().unwrap(),
            "--kind",
            "scalar",
            "--zone-names",
        ],
    );
    assert!(result.status.success(), "{result:?}");
}
