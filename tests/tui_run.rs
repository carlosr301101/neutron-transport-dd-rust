//! TUI phase B: background runs, cancellation and the results screen.

mod common;

use std::fs;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::{base, merge, set};
use neutron_transport_dd::config::Config;
use neutron_transport_dd::tui::app::{App, Tab};
use neutron_transport_dd::tui::services::{
    config_store, results as results_service,
    runner::{self, RunEvent},
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use tempfile::TempDir;

fn fixed_source_value() -> Value {
    merge(
        set(base(), "NC", json!([20])),
        json!({"NU_SIGF": [0.0], "MODE": "fixed_source", "Q": [1.0]}),
    )
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

#[test]
fn background_run_writes_the_output() {
    let home = TempDir::new().expect("temp");
    let value = fixed_source_value();
    let config = Config::from_value(&value, 1e-3, 500, None).expect("config");
    let output = home.path().join("out.json");

    let handle = runner::spawn(
        "test".into(),
        value,
        config,
        None,
        output.clone(),
        Some("unit".into()),
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut done = false;
    while Instant::now() < deadline {
        match handle.receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(RunEvent::Done { result, .. }) => {
                assert!(result.converged);
                done = true;
                break;
            }
            Ok(RunEvent::Failed(error)) => panic!("run failed: {error}"),
            Ok(RunEvent::Log(_)) => {}
            Err(_) => {}
        }
    }
    assert!(done, "run did not finish");
    let payload: Value = serde_json::from_str(&fs::read_to_string(&output).unwrap()).expect("json");
    assert_eq!(payload["total"], 1);
    assert_eq!(payload["runs"][0]["mode"], "fixed_source");
}

#[test]
fn cancellation_stops_a_background_run() {
    let home = TempDir::new().expect("temp");
    let value = set(base(), "NC", json!([400]));
    let config = Config::from_value(&value, 1e-12, 2000, None).expect("config");

    let handle = runner::spawn(
        "cancel".into(),
        value,
        config,
        None,
        home.path().join("out.json"),
        None,
    );
    handle.cancel.store(true, Ordering::Relaxed);

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut cancelled = false;
    while Instant::now() < deadline {
        match handle.receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(RunEvent::Failed(error)) => {
                assert!(error.contains("cancelled"), "{error}");
                cancelled = true;
                break;
            }
            Ok(RunEvent::Done { .. }) => panic!("run finished before cancellation"),
            Ok(RunEvent::Log(_)) => {}
            Err(_) => {}
        }
    }
    assert!(cancelled, "cancellation was not observed");
}

#[test]
fn config_store_lists_json_files() {
    let home = TempDir::new().expect("temp");
    fs::write(home.path().join("config_1d_002.json"), "{}").unwrap();
    fs::write(home.path().join("config_1d_001.json"), "{}").unwrap();
    fs::write(home.path().join("notes.txt"), "ignored").unwrap();

    let entries = config_store::list_from(home.path());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "config_1d_001.json");
    assert_eq!(entries[1].name, "config_1d_002.json");
}

#[test]
fn results_service_lists_and_loads_outputs() {
    let home = TempDir::new().expect("temp");
    let payload = json!({
        "solver": "NTS_DD_1D",
        "runs": [{"config_name": "a"}],
        "total": 1,
        "failures": 0
    });
    fs::write(
        home.path().join("output_1d_001.json"),
        serde_json::to_string(&payload).unwrap(),
    )
    .unwrap();
    fs::write(home.path().join("output_1d_002.json"), "{}").unwrap();

    let entries = results_service::list_from(home.path());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].solver, "NTS_DD_1D");
    assert_eq!(entries[0].total, 1);

    let loaded = results_service::load(&entries[0].path).expect("load");
    assert_eq!(loaded["runs"][0]["config_name"], "a");
}

#[test]
fn run_screen_renders_configurations() {
    let home = TempDir::new().expect("temp");
    let path = home.path().join("config_1d_001.json");
    fs::write(&path, serde_json::to_string(&fixed_source_value()).unwrap()).unwrap();

    let mut app = App::default();
    app.run.entries = config_store::list_from(home.path());
    app.tab = Tab::Run;
    let text = render(&mut app);
    assert!(text.contains("Configs (1)"));
    assert!(text.contains("config_1d_001.json"));
    assert!(text.contains("idle"));
}

#[test]
fn app_runs_a_config_and_loads_the_result() {
    let home = TempDir::new().expect("temp");
    let config_path = home.path().join("config_1d_001.json");
    fs::write(
        &config_path,
        serde_json::to_string(&fixed_source_value()).unwrap(),
    )
    .unwrap();

    let mut app = App::default();
    app.run.entries = config_store::list_from(home.path());
    let output = home.path().join("out.json");
    app.start_run_with_output(output.clone());
    assert!(app.run.running);

    let deadline = Instant::now() + Duration::from_secs(10);
    while app.run.running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        app.poll_runner();
    }
    assert!(!app.run.running, "run did not finish");
    assert!(output.exists());

    app.load_results_from(&output).expect("load");
    app.tab = Tab::Results;
    let text = render(&mut app);
    assert!(text.contains("config_1d_001.json"));
    assert!(text.contains("fixed_source"));
    assert!(text.contains("Runs"));
}

#[test]
fn results_screen_renders_a_loaded_payload() {
    let home = TempDir::new().expect("temp");
    let path = home.path().join("payload.json");
    fs::write(
        &path,
        serde_json::to_string(&json!({
            "solver": "NTS_DD_1D",
            "runs": [{
                "config_name": "json_config_001",
                "mode": "eigenvalue",
                "spatial_scheme": "DD",
                "converged": true,
                "iteration": 10,
                "k_eff": 1.0266,
                "k_balance": 1.0267
            }],
            "total": 1,
            "failures": 0
        }))
        .unwrap(),
    )
    .unwrap();

    let mut app = App::default();
    app.load_results_from(&path).expect("load");
    app.tab = Tab::Results;
    let text = render(&mut app);
    assert!(text.contains("json_config_001"));
    assert!(text.contains("eigenvalue"));
    assert!(text.contains("1.026600"));
}

#[test]
fn starting_without_configs_reports_an_error() {
    let home = TempDir::new().expect("temp");
    let mut app = App::default();
    app.run.entries = config_store::list_from(home.path());
    app.start_run_with_output(home.path().join("out.json"));
    assert!(!app.run.running);
    assert!(app.status.contains("No configuration selected"));
}
