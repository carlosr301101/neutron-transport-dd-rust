//! Scalar-flux convergence criterion, ported from `test_convergence.py`.

mod common;

use common::{base, merge, run_value};
use neutron_transport_dd::core::Runner;
use serde_json::json;

#[test]
fn uses_the_old_flux_as_denominator() {
    // |1 - 2/1| = 1.0 (a new-flux denominator would give 0.5).
    let change = Runner::relative_flux_change(&[1.0, 1.0], &[2.0, 2.0]);
    assert!((change - 1.0).abs() < 1e-15);
}

#[test]
fn matches_one_minus_ratio() {
    let change = Runner::relative_flux_change(&[1.0, 2.0], &[1.0 + 9e-6, 2.0 + 1.8e-5]);
    assert!((change - 9e-6).abs() < 1e-12);
}

#[test]
fn negative_flux_uses_the_absolute_value() {
    let change = Runner::relative_flux_change(&[-1.0], &[-1.0 - 1e-5]);
    assert!((change - 1e-5).abs() < 1e-12);
}

#[test]
fn zero_old_flux_falls_back_to_the_absolute_difference() {
    assert!((Runner::relative_flux_change(&[0.0, 1.0], &[1e-6, 1.0]) - 1e-6).abs() < 1e-15);
    assert_eq!(Runner::relative_flux_change(&[0.0, 1.0], &[0.0, 1.0]), 0.0);
}

#[test]
fn tighter_tolerance_needs_more_sweeps() {
    let value = merge(
        base(),
        json!({"NC": [20], "NU_SIGF": [0.0], "MODE": "fixed_source", "Q": [1.0]}),
    );
    let loose = run_value(&value, 1e-3);
    let tight = run_value(&value, 1e-7);
    assert!(loose.converged);
    assert!(tight.converged);
    assert!(
        tight.iteration > loose.iteration,
        "tight {} vs loose {}",
        tight.iteration,
        loose.iteration
    );
}

#[test]
fn relative_change_is_zero_for_identical_fluxes() {
    assert_eq!(
        Runner::relative_flux_change(&[1.0, -2.0, 3.0], &[1.0, -2.0, 3.0]),
        0.0
    );
}
