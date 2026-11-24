//! Cancellation support in the transport runner.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use common::{base, set};
use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;
use serde_json::json;

fn eigenvalue_config(cells: usize, epsilon: f64) -> Config {
    let value = set(base(), "NC", json!([cells]));
    Config::from_value(&value, epsilon, 2000, None).expect("config")
}

#[test]
fn a_preset_cancellation_aborts_the_run() {
    let flag = Arc::new(AtomicBool::new(true));
    let error = Runner::new(eigenvalue_config(100, 1e-5))
        .expect("runner")
        .with_cancel(flag)
        .run()
        .expect_err("must be cancelled");
    assert!(error.to_string().contains("cancelled"), "{error}");
}

#[test]
fn a_clear_flag_does_not_affect_the_run() {
    let flag = Arc::new(AtomicBool::new(false));
    let result = Runner::new(eigenvalue_config(20, 1e-3))
        .expect("runner")
        .with_cancel(flag)
        .run()
        .expect("run");
    assert!(result.converged);
}

#[test]
fn cancellation_arrives_between_iterations() {
    let flag = Arc::new(AtomicBool::new(false));
    let worker_flag = Arc::clone(&flag);
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        worker_flag.store(true, Ordering::Relaxed);
    });

    let error = Runner::new(eigenvalue_config(400, 1e-12))
        .expect("runner")
        .with_cancel(flag)
        .run()
        .expect_err("must be cancelled");
    writer.join().expect("writer");
    assert!(error.to_string().contains("cancelled"), "{error}");
}
