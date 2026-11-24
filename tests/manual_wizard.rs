//! Interactive wizard, ported from `test_dd_1d_manual.py`.

use std::io::Cursor;

use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;
use neutron_transport_dd::scheme::Method;
use neutron_transport_dd::wizard::Wizard;

fn build(
    lines: &[&str],
    forced: Option<Method>,
    epsilon: f64,
) -> Result<Config, neutron_transport_dd::error::ConfigError> {
    let script = format!("{}\n", lines.join("\n"));
    let mut wizard = Wizard::new(Cursor::new(script), Vec::new());
    wizard.build_config(forced, epsilon, 2000)
}

fn build_and_run(lines: &[&str]) -> (Config, neutron_transport_dd::result::RunResult) {
    let config = build(lines, None, 1e-5).expect("wizard");
    let result = Runner::new(config.clone())
        .expect("runner")
        .run()
        .expect("run");
    (config, result)
}

#[test]
fn manual_eigenvalue() {
    let lines = [
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
    ];
    let (config, result) = build_and_run(&lines);
    assert_eq!(result.mode.as_str(), "eigenvalue");
    assert!(result.k_converged.unwrap_or(false));
    assert!((result.k_eff.unwrap() - 1.0266).abs() < 2e-3);
    assert_eq!(config.q, vec![0.0]);
    assert_eq!(config.bound_left, vec![0.0; 4]);
}

#[test]
fn manual_eigenvalue_with_power() {
    let lines = [
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
        "s",
        "10",
        "",
        "",
        "",
        "2.4",
        "n",
        "n",
    ];
    let (config, result) = build_and_run(&lines);
    assert_eq!(result.mode.as_str(), "eigenvalue");
    assert!((result.power_w.unwrap() - 10_000.0).abs() < 1e-6);
    assert_eq!(config.nu, Some(2.4));
}

#[test]
fn manual_power_uses_default_nu() {
    let lines = [
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
        "s",
        "10",
        "",
        "",
        "",
        "",
        "n",
        "n",
    ];
    let (config, result) = build_and_run(&lines);
    assert_eq!(config.nu, Some(2.49));
    assert!((result.power_w.unwrap() - 10_000.0).abs() < 1e-6);
}

#[test]
fn manual_power_with_sigma_f() {
    let lines = [
        "1",
        "1",
        "20",
        "20",
        "1",
        "1.0",
        "0.5",
        "",
        "",
        "s",
        "0.3",
        "eigenvalue",
        "",
        "",
        "",
        "s",
        "10",
        "",
        "",
        "sigma_f",
        "0.3",
        "s",
        "s",
    ];
    let (config, result) = build_and_run(&lines);
    assert_eq!(result.mode.as_str(), "eigenvalue");
    assert!((result.power_w.unwrap() - 10_000.0).abs() < 1e-6);
    assert_eq!(config.sigma_f, Some(vec![0.3]));
    assert_eq!(config.nu, None);
}

#[test]
fn manual_fixed_k_with_source() {
    let lines = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "s", "0.05", "fixed_k", "1.0", "", "", "",
        "n", "1.0", "s", "s",
    ];
    let (_config, result) = build_and_run(&lines);
    assert_eq!(result.mode.as_str(), "fixed_k");
    assert_eq!(result.k_eff, Some(1.0));
    assert!(result.converged);
    assert!((result.k_balance.unwrap() - 0.5).abs() < 1e-4);
}

#[test]
fn manual_driven() {
    let lines = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "s", "0.05", "driven", "", "", "", "n",
        "1.0", "n", "n", "0 0 0 0", "0 0 0 0",
    ];
    let (_config, result) = build_and_run(&lines);
    assert_eq!(result.mode.as_str(), "driven");
    assert_eq!(result.k_eff, Some(1.0));
    assert!(result.converged);
    assert!(result.k_balance.unwrap() < 1.0);
}

#[test]
fn manual_driven_without_source_fails() {
    let lines = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "s", "0.05", "driven", "", "", "", "n",
        "0", "n", "n", "0 0 0 0", "0 0 0 0",
    ];
    assert!(build(&lines, None, 1e-5).is_err());
}

#[test]
fn manual_fixed_source() {
    let lines = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "n", "1", "s", "s",
    ];
    let (_config, result) = build_and_run(&lines);
    assert_eq!(result.mode.as_str(), "fixed_source");
    assert_eq!(result.k_eff, None);
    assert!(result.converged);
}

#[test]
fn manual_invalid_mode_is_rejected() {
    let lines = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "", "s", "0.1", "foo",
    ];
    assert!(build(&lines, None, 1e-5).is_err());
}

#[test]
fn forced_scheme_skips_the_prompt() {
    let lines = [
        "1", "1", "20", "20", "1", "1.0", "0.9", "", "n", "1.0", "s", "s",
    ];
    let config = build(&lines, Some(Method::StepCharacteristic), 1e-5).expect("wizard");
    assert_eq!(config.method(), Method::StepCharacteristic);
    let result = Runner::new(config).expect("runner").run().expect("run");
    assert_eq!(result.spatial_scheme, Method::StepCharacteristic);
}

#[test]
fn placeholders_are_rejected_in_numeric_fields() {
    let lines = [
        "1",
        "1",
        "20",
        "20",
        "1",
        "1.0",
        "0.9",
        "",
        "",
        "s",
        "0.11",
        "eigenvalue",
        "kk",
    ];
    assert!(build(&lines, None, 1e-5).is_err());
}

#[test]
fn summary_payload_is_json_serializable() {
    let lines = [
        "1",
        "1",
        "20",
        "20",
        "1",
        "1.0",
        "0.9",
        "",
        "",
        "s",
        "0.3",
        "eigenvalue",
        "",
        "",
        "",
        "n",
        "s",
        "s",
    ];
    let config = build(&lines, None, 1e-5).expect("wizard");
    let payload = config.as_dict();
    assert_eq!(payload["MODE"], serde_json::json!("eigenvalue"));
    assert_eq!(payload["NU_SIGF"], serde_json::json!([0.3]));
    assert_eq!(payload["N"], serde_json::json!(8));
    serde_json::to_string(&payload).expect("serializable");
}
