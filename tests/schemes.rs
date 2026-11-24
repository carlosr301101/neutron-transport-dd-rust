//! Solver-level scheme tests, ported from `test_spatial_schemes.py`.

mod common;

use common::{assert_close, base, merge, run_value, set};
use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;
use neutron_transport_dd::scheme::{Method, cell_average};
use serde_json::json;

fn dd_regression(epsilon: f64) -> neutron_transport_dd::result::RunResult {
    run_value(&merge(base(), json!({"SPATIAL_SCHEME": "DD"})), epsilon)
}

#[test]
fn default_scheme_is_dd_and_names_are_normalised() {
    let config = Config::from_value(&base(), 1e-5, 2000, None).expect("config");
    assert_eq!(config.method(), Method::Dd);

    for (name, expected) in [
        ("dd", Method::Dd),
        ("Step", Method::Step),
        (" sc ", Method::StepCharacteristic),
        ("STEP_CHARACTERISTIC", Method::StepCharacteristic),
    ] {
        let value = set(base(), "SPATIAL_SCHEME", json!(name));
        let config = Config::from_value(&value, 1e-5, 2000, None).expect("scheme");
        assert_eq!(config.method(), expected);
    }
}

#[test]
fn invalid_scheme_is_rejected() {
    let value = set(base(), "SPATIAL_SCHEME", json!("nope"));
    assert!(Config::from_value(&value, 1e-5, 2000, None).is_err());
}

#[test]
fn dd_regression_benchmark_matches_the_reference_solution() {
    let result = dd_regression(1e-6);
    assert!(result.k_converged.unwrap_or(false));
    assert_eq!(result.iteration, 999);
    assert_eq!(result.iteration_eigenvalue, Some(25));
    assert_eq!(result.outer_iterations, 25);
    assert_close(result.k_eff.unwrap(), 1.0266764379437188, 1e-12);
    assert_close(result.k_balance.unwrap(), 1.0266775219239312, 1e-12);
    assert_close(result.scalar_flux[0], 0.10320820280696363, 1e-12);
    assert_close(result.scalar_flux[50], 1.0, 1e-12);
    assert_close(result.scalar_flux[99], 0.10320820280696363, 1e-12);
    assert_close(
        result.scalar_flux.iter().sum::<f64>(),
        67.70760151718076,
        1e-10,
    );
}

#[test]
fn injected_scheme_wins_over_the_configuration() {
    let config = Config::from_value(&base(), 1e-6, 2000, None).expect("config");
    assert_eq!(config.method(), Method::Dd);
    let result = Runner::with_scheme(config, Method::StepCharacteristic)
        .expect("runner")
        .run()
        .expect("run");
    assert_eq!(result.spatial_scheme, Method::StepCharacteristic);
    assert_close(result.k_eff.unwrap(), 1.026054, 1e-5);
}

#[test]
fn reflective_infinite_medium_holds_for_every_scheme() {
    for scheme in ["DD", "STEP", "SC"] {
        let value = merge(
            base(),
            json!({
                "SPATIAL_SCHEME": scheme,
                "NC": [20],
                "SCT": [1.0],
                "SCS": [0.5],
                "NU_SIGF": [0.3],
                "reflex_izq": true,
                "reflex_der": true
            }),
        );
        let result = run_value(&value, 1e-5);
        assert!(result.k_converged.unwrap_or(false), "{scheme}");
        assert_close(result.k_eff.unwrap(), 0.6, 1e-5);
        assert_close(result.k_balance.unwrap(), 0.6, 1e-5);
    }
}

#[test]
fn benchmark_agrees_across_schemes() {
    let dd = run_value(&set(base(), "SPATIAL_SCHEME", json!("DD")), 1e-5);
    let step = run_value(&set(base(), "SPATIAL_SCHEME", json!("STEP")), 1e-5);
    let sc = run_value(&set(base(), "SPATIAL_SCHEME", json!("SC")), 1e-5);

    for result in [&dd, &step, &sc] {
        assert!(result.k_converged.unwrap_or(false));
        assert!(
            (result.k_eff.unwrap() - result.k_balance.unwrap()).abs() < 1e-4,
            "k_eff vs k_balance"
        );
    }
    assert_close(dd.k_eff.unwrap(), 1.026676, 2e-4);
    assert!((sc.k_eff.unwrap() - dd.k_eff.unwrap()).abs() < 1e-3);
    assert!((step.k_eff.unwrap() - dd.k_eff.unwrap()).abs() < 2e-2);
}

#[test]
fn solver_level_cell_balance_holds_for_every_scheme() {
    for scheme in ["DD", "STEP", "SC"] {
        let value = merge(
            base(),
            json!({
                "SPATIAL_SCHEME": scheme,
                "NC": [40],
                "Q": [0.2],
                "MODE": "fixed_k",
                "K_EFF": 1.4
            }),
        );
        let config = Config::from_value(&value, 1e-8, 2000, None).expect("config");
        let snapshot = config.clone();
        let result = Runner::new(config).expect("runner").run().expect("run");
        assert!(result.converged, "{scheme} did not converge");

        let k_eff = result.k_eff.expect("k_eff");
        let mut worst: f64 = 0.0;
        for k in 0..snapshot.ntc {
            let phi = result.scalar_flux[k];
            let source = snapshot.sigma_s_vec[k] * phi
                + 0.5 * snapshot.q_ext_vec[k]
                + snapshot.nu_sigma_f_vec[k] * phi / k_eff;
            let sigma_t = snapshot.sigma_t_vec[k];
            let dx = snapshot.dx_vec[k];

            for m in 0..snapshot.n_half {
                let mu = snapshot.miu_m[m].abs();
                let a = mu / dx;

                let average_right = cell_average(
                    result.psi_right[k][m],
                    result.psi_right[k + 1][m],
                    sigma_t,
                    dx,
                    mu,
                    snapshot.method(),
                    None,
                )
                .expect("average");
                let average_left = cell_average(
                    result.psi_left[k + 1][m],
                    result.psi_left[k][m],
                    sigma_t,
                    dx,
                    mu,
                    snapshot.method(),
                    None,
                )
                .expect("average");

                worst = worst
                    .max(
                        (a * (result.psi_right[k + 1][m] - result.psi_right[k][m])
                            + sigma_t * average_right
                            - source)
                            .abs(),
                    )
                    .max(
                        (a * (result.psi_left[k][m] - result.psi_left[k + 1][m])
                            + sigma_t * average_left
                            - source)
                            .abs(),
                    );
            }
        }
        assert!(worst < 1e-6, "{scheme} balance residual {worst:.3e}");
    }
}

#[test]
fn fixed_source_vacuum_and_reflective_hold_for_every_scheme() {
    for scheme in ["DD", "STEP", "SC"] {
        let vacuum = run_value(
            &merge(
                base(),
                json!({
                    "SPATIAL_SCHEME": scheme,
                    "NC": [20],
                    "SCS": [0.5],
                    "NU_SIGF": [0.0],
                    "Q": [1.0],
                    "MODE": "fixed_source"
                }),
            ),
            1e-5,
        );
        assert!(vacuum.converged, "{scheme} vacuum");
        assert_eq!(vacuum.k_eff, None);
        assert!(vacuum.scalar_flux.iter().all(|value| value.is_finite()));
        let vacuum_max = vacuum
            .scalar_flux
            .iter()
            .fold(0.0_f64, |acc, value| acc.max(*value));
        assert!(vacuum_max > 0.0);

        let reflective = run_value(
            &merge(
                base(),
                json!({
                    "SPATIAL_SCHEME": scheme,
                    "NC": [20],
                    "SCS": [0.9],
                    "NU_SIGF": [0.0],
                    "Q": [1.0],
                    "MODE": "fixed_source",
                    "reflex_izq": true,
                    "reflex_der": true
                }),
            ),
            1e-5,
        );
        assert!(reflective.converged, "{scheme} reflective");
        assert_eq!(reflective.k_eff, None);

        let n = reflective.scalar_flux.len();
        for i in 0..n {
            let mirrored = reflective.scalar_flux[n - 1 - i];
            let value = reflective.scalar_flux[i];
            assert!(
                (value - mirrored).abs() <= 1e-9 + 1e-6 * mirrored.abs(),
                "{scheme} symmetry at {i}: {value} vs {mirrored}"
            );
        }

        let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
        assert!(mean(&reflective.scalar_flux) > mean(&vacuum.scalar_flux));
    }
}

#[test]
fn scalar_flux_is_the_angular_average() {
    let value = merge(
        base(),
        json!({
            "N": 2,
            "NC": [10],
            "SCS": [0.5],
            "NU_SIGF": [0.0],
            "Q": [1.0],
            "MODE": "fixed_source",
            "bound_left": [1.0],
            "bound_right": [0.0]
        }),
    );
    let config = Config::from_value(&value, 1e-5, 2000, None).expect("config");
    let ntc = config.ntc;
    let n = config.n;
    let result = Runner::new(config).expect("runner").run().expect("run");

    assert_eq!(result.angular_flux.len(), ntc);
    assert!(result.angular_flux.iter().all(|row| row.len() == n));
    assert_eq!(result.miu.len(), n);
    assert_close(result.omega.iter().sum::<f64>(), 2.0, 1e-12);

    for k in 0..ntc {
        let average: f64 = 0.5
            * result
                .omega
                .iter()
                .zip(&result.angular_flux[k])
                .map(|(weight, psi)| weight * psi)
                .sum::<f64>();
        assert_close(result.scalar_flux[k], average, 1e-14);
    }
    assert_close(
        result.scalar_flux[0],
        0.5 * result.angular_flux[0].iter().sum::<f64>(),
        1e-12,
    );
}
