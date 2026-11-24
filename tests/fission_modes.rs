//! Multiplicative-media (fission) modes, ported from `test_dd_1d_fission.py`.

mod common;

use common::{assert_close, base, merge, run_value, set};
use neutron_transport_dd::config::Config;
use neutron_transport_dd::power::MEV_TO_J;
use serde_json::json;

#[test]
fn reflective_infinite_medium_has_k_inf() {
    let value = merge(
        base(),
        json!({
            "NC": [20],
            "SCT": [1.0],
            "SCS": [0.5],
            "NU_SIGF": [0.3],
            "reflex_izq": true,
            "reflex_der": true
        }),
    );
    let result = run_value(&value, 1e-5);
    assert!(result.k_converged.unwrap_or(false));
    assert_close(result.k_eff.unwrap(), 0.6, 1e-6);
    assert_close(result.k_balance.unwrap(), 0.6, 1e-6);
}

#[test]
fn twenty_centimetre_vacuum_slab_benchmark() {
    let result = run_value(&base(), 1e-5);
    assert!(result.k_converged.unwrap_or(false));
    assert_close(result.k_eff.unwrap(), 1.02668, 2e-4);
    assert_close(result.k_eff.unwrap(), result.k_balance.unwrap(), 1e-4);
}

#[test]
fn fixed_k_at_the_eigenvalue_is_stationary() {
    let critical_value = set(base(), "NC", json!([40]));
    let critical = run_value(&critical_value, 1e-5);
    let k_star = critical.k_eff.expect("critical k_eff");

    let value = merge(
        set(base(), "NC", json!([40])),
        json!({"MODE": "fixed_k", "K_EFF": k_star}),
    );
    let result = run_value(&value, 1e-5);
    assert!(result.converged);
    assert_close(result.k_balance.unwrap(), k_star, 1e-3);
}

#[test]
fn driven_subcritical_reflective_slab_balances_at_k_inf() {
    let value = merge(
        set(base(), "NC", json!([20])),
        json!({
            "SCS": [0.9],
            "NU_SIGF": [0.05],
            "Q": [1.0],
            "MODE": "driven",
            "reflex_izq": true,
            "reflex_der": true
        }),
    );
    let result = run_value(&value, 1e-5);
    assert!(result.converged);
    assert_close(result.k_eff.unwrap(), 1.0, 1e-12);
    assert_close(result.k_balance.unwrap(), 0.5, 1e-4);
}

#[test]
fn power_without_nu_uses_the_u238_default() {
    let value = merge(
        set(base(), "NC", json!([20])),
        json!({"POWER_KW": 10.0, "E_F_MEV": 200.0}),
    );
    let config = Config::from_value(&value, 1e-5, 2000, None).expect("config");
    assert_eq!(config.nu, Some(2.49));
    let result = run_value(&value, 1e-5);
    assert_close(result.power_w.unwrap(), 10_000.0, 1e-6);
}

#[test]
fn sweep_counters_are_consistent() {
    let eigen = run_value(&base(), 1e-5);
    assert!(eigen.iteration > 0);
    assert_eq!(eigen.iteration_eigenvalue, Some(eigen.outer_iterations));

    let fixed = run_value(
        &merge(
            base(),
            json!({"NC": [20], "MODE": "fixed_source", "NU_SIGF": [0.0], "Q": [1.0]}),
        ),
        1e-5,
    );
    assert!(fixed.iteration > 0);
    assert_eq!(fixed.iteration_eigenvalue, None);
}

#[test]
fn power_normalization_scales_the_flux() {
    let value = merge(
        base(),
        json!({"POWER_KW": 10.0, "E_F_MEV": 200.0, "NU": 2.4, "AREA_CM2": 1.0}),
    );
    let result = run_value(&value, 1e-5);
    assert_close(result.power_w.unwrap(), 10_000.0, 1e-6);

    let sigma_f = 0.11 / 2.4;
    let dx = 20.0 / 100.0;
    let recomputed = 2.0
        * 200.0
        * MEV_TO_J
        * result
            .scalar_flux
            .iter()
            .map(|phi| sigma_f * phi * dx)
            .sum::<f64>();
    assert!(
        (recomputed - 10_000.0).abs() < 1e-3,
        "recomputed power {recomputed}"
    );
    assert!(result.power_unnormalized.unwrap() > 0.0);
    assert_close(
        result.power_scale * result.power_unnormalized.unwrap(),
        10_000.0,
        1e-6,
    );
}

#[test]
fn fixed_source_regression_without_fission() {
    let value = merge(
        set(base(), "NC", json!([20])),
        json!({
            "NU_SIGF": [0.0],
            "MODE": "fixed_source",
            "Q": [1.0],
            "reflex_izq": true,
            "reflex_der": true
        }),
    );
    let result = run_value(&value, 1e-5);
    assert!(result.converged);
    assert_eq!(result.k_eff, None);
    assert_eq!(result.mode.as_str(), "fixed_source");
}

#[test]
fn mixed_boundaries_do_not_inject_flux() {
    let value = merge(
        set(base(), "NC", json!([20])),
        json!({
            "NU_SIGF": [0.0],
            "MODE": "fixed_source",
            "Q": [0.0],
            "reflex_izq": true,
            "reflex_der": false
        }),
    );
    let result = run_value(&value, 1e-5);
    assert!(result.converged);
    let maximum = result
        .scalar_flux
        .iter()
        .fold(0.0_f64, |acc, value| acc.max(value.abs()));
    assert_eq!(maximum, 0.0);
}

#[test]
fn mode_is_inferred_when_omitted() {
    let value = common::remove(base(), "MODE");
    let result = run_value(&value, 1e-5);
    assert_eq!(result.mode.as_str(), "eigenvalue");
    assert!(result.k_converged.unwrap_or(false));
}

#[test]
fn invalid_mode_combinations_are_rejected() {
    let cases = [
        merge(base(), json!({"MODE": "eigenvalue", "Q": [1.0]})),
        merge(base(), json!({"MODE": "fixed_k"})),
        merge(base(), json!({"MODE": "fixed_source"})),
        common::remove(
            merge(base(), json!({"MODE": "driven", "Q": [0.0]})),
            "K_EFF",
        ),
    ];
    for value in cases {
        assert!(
            Config::from_value(&value, 1e-5, 2000, None).is_err(),
            "configuration should be rejected: {value}"
        );
    }
}
