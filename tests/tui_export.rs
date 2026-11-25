//! TUI phase C: placeholders, plots and export screens.

mod common;

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use common::cli;
use neutron_transport_dd::sweep::SweepRange;
use neutron_transport_dd::tui::app::{App, Tab};
use neutron_transport_dd::tui::services::{config_store, results as results_service};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use tempfile::TempDir;

fn payload() -> Value {
    json!({
        "solver": "NTS_DD_1D",
        "generated_at": "2026-01-01T00:00:00",
        "total": 1,
        "failures": 0,
        "spatial_schemes": ["DD"],
        "runs": [{
            "config_name": "run1",
            "configuration": {
                "NC": [20], "HR": [20.0], "IZL": [1],
                "SCS": [0.9], "NU_SIGF": [0.11], "MODE": "eigenvalue"
            },
            "scalar_flux": (0..20).map(|value| value as f64).collect::<Vec<_>>(),
            "angular_flux": [], "miu": [], "omega": [],
            "iteration": 10, "converged": true,
            "mode": "eigenvalue", "spatial_scheme": "DD",
            "k_eff": 1.02, "k_balance": 1.02,
            "k_history": [1.0, 1.02]
        }]
    })
}

fn write_output(home: &Path) -> std::path::PathBuf {
    let path = home.join("output_1d_001.json");
    fs::write(&path, serde_json::to_string_pretty(&payload()).unwrap()).expect("write");
    path
}

fn app_with_output(home: &Path) -> App {
    write_output(home);
    let mut app = App::default();
    app.results.entries = results_service::list_from(home);
    app
}

fn render(app: &mut App) -> String {
    let backend = TestBackend::new(120, 34);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
}

fn read_list(path: &Path) -> Vec<Value> {
    read_json(path).as_array().cloned().expect("list")
}

fn png_size(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).expect("png");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(bytes.len() > 1024);
    (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    )
}

#[test]
fn placeholder_screen_detects_placeholders() {
    let home = TempDir::new().expect("temp");
    fs::write(
        home.path().join("config_1d_001.json"),
        serde_json::to_string_pretty(&json!({
            "num_regions": 1, "num_zones": 1,
            "NC": [20], "HR": [20.0], "IZL": [1],
            "SCT": [1.0], "SCS": ["kk"], "NU_SIGF": ["mm"],
            "Q": [0.0], "N": 8, "MODE": "eigenvalue",
            "ZONE_NAMES": ["Core"],
            "bound_left": [0, 0], "bound_right": [0, 0]
        }))
        .unwrap(),
    )
    .expect("write");

    let mut app = App::default();
    app.tab = Tab::Placeholders;
    app.saved = config_store::list_from(home.path());
    app.sweeps.config_selected = 0;
    app.scan_placeholders();

    let names: Vec<&str> = app
        .sweeps
        .ranges
        .iter()
        .map(|range| range.name.as_str())
        .collect();
    assert_eq!(names, vec!["kk", "mm"]);
    assert_eq!(app.sweeps.ranges[0].start, "0.1");
    assert_eq!(app.sweeps.ranges[0].end, "0.5");

    let text = render(&mut app);
    assert!(text.contains("Placeholders"));
    assert!(text.contains("kk"));
    assert!(text.contains("Values"));
    assert!(text.contains("Expansion"));
}

#[test]
fn expansion_writes_the_config_list() {
    let home = TempDir::new().expect("temp");
    let config = home.path().join("config_1d_001.json");
    fs::write(
        &config,
        serde_json::to_string_pretty(&json!({
            "num_regions": 1, "num_zones": 1,
            "NC": [20], "HR": [20.0], "IZL": [1],
            "SCT": [1.0], "SCS": ["kk"], "NU_SIGF": [0.11],
            "Q": [0.0], "N": 8, "MODE": "eigenvalue",
            "bound_left": [0, 0, 0, 0], "bound_right": [0, 0, 0, 0]
        }))
        .unwrap(),
    )
    .expect("write");

    let mut app = App::default();
    app.saved = config_store::list_from(home.path());
    app.sweeps.config_selected = 0;
    app.sweeps.ranges = vec![SweepRange {
        name: "kk".into(),
        start: "0.1".into(),
        end: "0.3".into(),
        step: "0.1".into(),
    }];
    let output = home.path().join("inputs_1d.json");
    app.expand_placeholders_to(output.clone());

    let configs: Value = serde_json::from_str(&fs::read_to_string(&output).unwrap()).expect("json");
    let configs = configs.as_array().expect("array");
    assert_eq!(configs.len(), 3);
    assert_eq!(configs[0]["SCS"][0], json!(0.1));
    assert_eq!(configs[2]["SCS"][0], json!(0.3));
    assert!(app.sweeps.message.contains("Generated 3"));
}

#[test]
fn plot_screen_generates_a_png() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_output(home.path());
    app.tab = Tab::Plots;
    app.plots.kind = 0;

    let output = home.path().join("scalar.png");
    app.generate_plot_to(output.clone());
    assert_eq!(png_size(&output), (1600, 900));

    let text = render(&mut app);
    assert!(text.contains("kind: scalar"));
    assert!(text.contains("scalar.png"));
}

#[test]
fn plot_screen_supports_k_history_and_angular() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_output(home.path());
    app.plots.kind = 2;
    app.generate_plot_to(home.path().join("k.png"));
    assert_eq!(png_size(&home.path().join("k.png")), (1600, 900));

    app.plots.kind = 1;
    app.generate_plot_to(home.path().join("angular.png"));
    // The synthetic payload has no angular rows: the plot errors cleanly.
    assert!(app.plots.message.contains("Plot failed"));
}

#[test]
fn export_writes_datos_and_excel() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_output(home.path());

    let datos = home.path().join("datos.json");
    app.export_data_to(datos.clone());
    let value: Value = serde_json::from_str(&fs::read_to_string(&datos).unwrap()).unwrap();
    assert_eq!(value["positions"], json!([0, 19]));
    assert_eq!(value["data"][0]["flux_0"], json!(0.0));
    assert_eq!(value["data"][0]["flux_19"], json!(19.0));

    let book = home.path().join("book.xlsx");
    app.export_excel_to(book.clone());
    assert!(book.exists());
    assert!(fs::metadata(&book).unwrap().len() > 1024);
}

#[test]
fn export_writes_the_scs_plot() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_output(home.path());
    let output = home.path().join("scs.png");
    app.export_scs_to(output.clone());
    assert_eq!(png_size(&output), (1600, 900));
}

#[test]
fn export_screen_renders_actions() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_output(home.path());
    app.tab = Tab::Export;
    let text = render(&mut app);
    assert!(text.contains("datos.json"));
    assert!(text.contains("Excel"));
    assert!(text.contains("SCS"));
}

#[test]
fn tui_requires_an_interactive_terminal() {
    let home = TempDir::new().expect("temp");
    let output = cli(home.path(), &["tui"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
}

fn placeholder_config() -> Value {
    json!({
        "num_regions": 1, "num_zones": 1,
        "NC": [10], "HR": [10.0], "IZL": [1],
        "SCT": [1.0], "SCS": ["kk"], "NU_SIGF": [0.0],
        "Q": ["qq"], "N": 2, "MODE": "fixed_source",
        "epsilon": 1e-3, "max_iter": 500,
        "bound_left": [0], "bound_right": [0]
    })
}

fn app_with_placeholders(home: &Path) -> App {
    fs::write(
        home.join("config_1d_001.json"),
        serde_json::to_string_pretty(&placeholder_config()).unwrap(),
    )
    .expect("write");
    let mut app = App::default();
    app.tab = Tab::Placeholders;
    app.saved = config_store::list_from(home);
    app.sweeps.config_selected = 0;
    app.scan_placeholders();
    app
}

#[test]
fn strict_ranges_expand_in_parallel() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_placeholders(home.path());
    let names: Vec<&str> = app.sweeps.ranges.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["kk", "qq"]);

    app.sweeps.ranges[0].start = "0.1".into();
    app.sweeps.ranges[0].end = "0.5".into();
    app.sweeps.ranges[0].step = "0.2".into();
    app.sweeps.ranges[1].start = "0.5".into();
    app.sweeps.ranges[1].end = "0.7".into();
    app.sweeps.ranges[1].step = "0.1".into();

    let output = home.path().join("inputs.json");
    assert!(app.expand_placeholders_to(output.clone()));
    let configs = read_list(&output);
    assert_eq!(configs.len(), 3);
    assert_eq!(configs[0]["SCS"][0], json!(0.1));
    assert_eq!(configs[0]["Q"][0], json!(0.5));
    assert_eq!(configs[2]["SCS"][0], json!(0.5));
    assert_eq!(configs[2]["Q"][0], json!(0.7));
}

#[test]
fn unequal_ranges_are_rejected() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_placeholders(home.path());
    app.sweeps.ranges[0].start = "0.1".into();
    app.sweeps.ranges[0].end = "0.5".into();
    app.sweeps.ranges[0].step = "0.2".into();
    app.sweeps.ranges[1].start = "0.5".into();
    app.sweeps.ranges[1].end = "0.6".into();
    app.sweeps.ranges[1].step = "0.1".into();

    let output = home.path().join("inputs.json");
    assert!(!app.expand_placeholders_to(output.clone()));
    assert!(!output.exists());
    assert!(app.sweeps.message.contains("same length"));
}

#[test]
fn output_dialog_prefills_the_conventional_path() {
    let mut app = App::default();
    app.open_output_dialog();
    let value = app.save_dialog_value().expect("path");
    assert!(value.ends_with("outputs/inputs/inputs_1d.json"));
}

#[test]
fn batch_runs_every_expanded_configuration() {
    let home = TempDir::new().expect("temp");
    let mut app = app_with_placeholders(home.path());
    app.sweeps.ranges[0].start = "0.1".into();
    app.sweeps.ranges[0].end = "0.3".into();
    app.sweeps.ranges[0].step = "0.1".into();
    app.sweeps.ranges[1].start = "0.5".into();
    app.sweeps.ranges[1].end = "0.7".into();
    app.sweeps.ranges[1].step = "0.1".into();

    let list = home.path().join("inputs.json");
    let output = home.path().join("output_1d_001.json");
    app.run_batch_to(list.clone(), output.clone());
    assert!(app.run.running);

    let deadline = Instant::now() + Duration::from_secs(20);
    while app.run.running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        app.poll_runner();
    }
    assert!(!app.run.running, "batch did not finish");
    assert!(list.exists());

    let payload = read_json(&output);
    assert_eq!(payload["total"], 3);
    assert_eq!(payload["failures"], 0);
    assert_eq!(payload["runs"].as_array().unwrap().len(), 3);
    assert_eq!(payload["runs"][0]["config_name"], "json_config_001");
}
