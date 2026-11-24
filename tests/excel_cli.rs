//! `nts-rs excel`: workbook layout and read-back checks.

mod common;

use std::fs;
use std::path::Path;

use calamine::{Data, DataType, Reader, open_workbook_auto};
use common::{base, cli};
use serde_json::{Value, json};
use tempfile::TempDir;

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

#[test]
fn workbook_contains_the_reference_sheets() {
    let home = TempDir::new().expect("temp");
    let source = run_config(
        home.path(),
        &common::set(base(), "NC", json!([20])),
        "config",
    );

    let output = cli(home.path(), &["excel", source.to_str().unwrap()]);
    assert!(output.status.success(), "{output:?}");

    let path = home.path().join("out.xlsx");
    assert!(path.exists());
    let mut workbook = open_workbook_auto(&path).expect("workbook");
    let names = workbook.sheet_names().to_vec();
    for sheet in [
        "Metadata",
        "Runs",
        "Zones",
        "Regions",
        "Boundaries",
        "ScalarFlux",
        "AngularFlux",
        "Directions",
        "k_history",
    ] {
        assert!(names.iter().any(|name| name == sheet), "missing {sheet}");
    }

    let metadata = workbook.worksheet_range("Metadata").expect("metadata");
    assert_eq!(metadata.height(), 6);
    assert_eq!(
        metadata.rows().nth(1).unwrap()[1],
        Data::String("NTS_DD_1D".into())
    );

    let runs = workbook.worksheet_range("Runs").expect("runs");
    assert_eq!(runs.height(), 2);

    let scalar = workbook.worksheet_range("ScalarFlux").expect("scalar");
    assert_eq!(scalar.height(), 21); // header + 20 cells
    assert_eq!(scalar.width(), 2); // cell + one run

    let regions = workbook.worksheet_range("Regions").expect("regions");
    assert_eq!(regions.height(), 2);

    let boundaries = workbook.worksheet_range("Boundaries").expect("boundaries");
    assert_eq!(boundaries.height(), 9); // 4 left + 4 right + header

    let directions = workbook.worksheet_range("Directions").expect("directions");
    assert_eq!(directions.height(), 9); // N=8 + header

    let history = workbook.worksheet_range("k_history").expect("history");
    assert!(history.height() > 1);
}

#[test]
fn scalar_flux_values_round_trip() {
    let home = TempDir::new().expect("temp");
    let source = run_config(
        home.path(),
        &common::set(base(), "NC", json!([20])),
        "config",
    );
    let payload: Value = serde_json::from_str(&fs::read_to_string(&source).unwrap()).unwrap();
    let first = payload["runs"][0]["scalar_flux"][0].as_f64().unwrap();

    let output = cli(home.path(), &["excel", source.to_str().unwrap()]);
    assert!(output.status.success());

    let mut workbook = open_workbook_auto(home.path().join("out.xlsx")).expect("workbook");
    let scalar = workbook.worksheet_range("ScalarFlux").expect("scalar");
    let row = scalar.rows().nth(1).unwrap();
    let value = row[1].as_f64().expect("number");
    assert!((value - first).abs() <= first.abs() * 1e-12);
    assert_eq!(row[0], Data::Float(1.0));
}

#[test]
fn run_selection_limits_the_sheets() {
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
    assert!(run.status.success());

    let output = cli(
        home.path(),
        &["excel", source.to_str().unwrap(), "--run", "0"],
    );
    assert!(output.status.success(), "{output:?}");

    let mut workbook = open_workbook_auto(home.path().join("out.xlsx")).expect("workbook");
    let runs = workbook.worksheet_range("Runs").expect("runs");
    assert_eq!(runs.height(), 2);
    let scalar = workbook.worksheet_range("ScalarFlux").expect("scalar");
    assert_eq!(scalar.width(), 2);
    let metadata = workbook.worksheet_range("Metadata").expect("metadata");
    let total = metadata.rows().nth(3).unwrap()[1].as_f64().unwrap();
    assert_eq!(total, 1.0);
}

#[test]
fn explicit_output_and_unknown_run() {
    let home = TempDir::new().expect("temp");
    let source = run_config(
        home.path(),
        &common::set(base(), "NC", json!([20])),
        "config",
    );
    let target = home.path().join("nested/book.xlsx");

    let output = cli(
        home.path(),
        &[
            "excel",
            source.to_str().unwrap(),
            "-o",
            target.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    assert!(target.exists());

    let output = cli(
        home.path(),
        &["excel", source.to_str().unwrap(), "--run", "zzz"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("run not found"));
}
