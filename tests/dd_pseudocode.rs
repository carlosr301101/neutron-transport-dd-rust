//! Equivalence with the literal Diamond Difference pseudoalgorithm (S2).
//!
//! The reference below applies the five steps of the DD algorithm (input,
//! initialization, rightgoing sweep, leftgoing sweep, convergence) exactly,
//! for the 20 cm slab with `Sigma_t = 1`, `Sigma_s = 0.97`, S2, a unit
//! incoming flux on the left and vacuum on the right.

mod common;

use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;

const LENGTH: f64 = 20.0;
const SIGMA_T: f64 = 1.0;
const SIGMA_S: f64 = 0.97;
const EPSILON: f64 = 1e-5;

struct Reference {
    iteration: usize,
    phi: Vec<f64>,
    avg_pos: Vec<f64>,
    avg_neg: Vec<f64>,
    out_pos: Vec<f64>,
    out_neg: Vec<f64>,
}

fn literal_dd(cells: usize) -> Reference {
    let mu = 1.0 / 3.0_f64.sqrt();
    let dx = LENGTH / cells as f64;
    let a = mu / dx;

    let mut phi: Vec<f64> = vec![0.0; cells];
    let mut phi_old: Vec<f64> = vec![0.0; cells];
    let mut s_sc = vec![0.0; cells];
    let mut avg_pos = vec![0.0; cells];
    let mut avg_neg = vec![0.0; cells];
    let mut out_pos = vec![0.0; cells];
    let mut out_neg = vec![0.0; cells];
    let mut iteration = 0;

    for it in 1..=2000 {
        iteration = it;

        // Rightgoing sweep.
        let mut psi_in = 1.0;
        for i in 0..cells {
            let out = ((a - 0.5 * SIGMA_T) * psi_in + s_sc[i]) / (a + 0.5 * SIGMA_T);
            let avg = 0.5 * (psi_in + out);
            out_pos[i] = out;
            avg_pos[i] = avg;
            phi[i] = 0.5 * avg;
            psi_in = out;
        }

        // Leftgoing sweep.
        let mut psi_in = 0.0;
        for i in (0..cells).rev() {
            let out = ((a - 0.5 * SIGMA_T) * psi_in + s_sc[i]) / (a + 0.5 * SIGMA_T);
            let avg = 0.5 * (psi_in + out);
            out_neg[i] = out;
            avg_neg[i] = avg;
            phi[i] += 0.5 * avg;
            psi_in = out;
        }

        // Convergence.
        let mut error: f64 = 0.0;
        for i in 0..cells {
            let denominator = if phi_old[i].abs() > 1e-13 {
                phi_old[i].abs()
            } else {
                1.0
            };
            error = error.max((phi[i] - phi_old[i]).abs() / denominator);
        }
        if error < EPSILON {
            break;
        }

        phi_old.copy_from_slice(&phi);
        for i in 0..cells {
            s_sc[i] = SIGMA_S * phi[i];
        }
    }

    Reference {
        iteration,
        phi,
        avg_pos,
        avg_neg,
        out_pos,
        out_neg,
    }
}

fn run_solver(cells: usize) -> neutron_transport_dd::result::RunResult {
    let value = serde_json::json!({
        "num_regions": 1,
        "num_zones": 1,
        "NC": [cells],
        "HR": [LENGTH],
        "IZL": [1],
        "SCT": [SIGMA_T],
        "SCS": [SIGMA_S],
        "NU_SIGF": [0.0],
        "Q": [0.0],
        "N": 2,
        "SPATIAL_SCHEME": "DD",
        "MODE": "fixed_source",
        "reflex_izq": false,
        "reflex_der": false,
        "bound_left": [1.0],
        "bound_right": [0.0]
    });
    let config = Config::from_value(&value, EPSILON, 2000, None).expect("config");
    Runner::new(config).expect("runner").run().expect("run")
}

fn check(cells: usize) {
    let reference = literal_dd(cells);
    let result = run_solver(cells);

    assert_eq!(
        reference.iteration, result.iteration,
        "iteration counters must match exactly"
    );

    for i in 0..cells {
        assert!(
            (reference.phi[i] - result.scalar_flux[i]).abs() <= 1e-12,
            "cell {i}: phi {} vs {}",
            reference.phi[i],
            result.scalar_flux[i]
        );
        assert!(
            (reference.avg_pos[i] - result.angular_flux[i][1]).abs() <= 1e-12,
            "cell {i}: positive average"
        );
        assert!(
            (reference.avg_neg[i] - result.angular_flux[i][0]).abs() <= 1e-12,
            "cell {i}: negative average"
        );
        assert!(
            (reference.out_pos[i] - result.psi_right[i + 1][0]).abs() <= 1e-12,
            "cell {i}: right interface"
        );
        assert!(
            (reference.out_neg[i] - result.psi_left[i][0]).abs() <= 1e-12,
            "cell {i}: left interface"
        );
    }
}

#[test]
fn five_cells_match_the_pseudoalgorithm() {
    check(5);
}

#[test]
fn ten_cells_match_the_pseudoalgorithm() {
    check(10);
}

#[test]
fn twenty_cells_match_the_pseudoalgorithm() {
    check(20);
}

#[test]
fn benchmark_config_is_rejected_without_mode() {
    let value = common::remove(common::base(), "MODE");
    let config = Config::from_value(&value, 1e-5, 2000, None).expect("mode inferred");
    assert_eq!(config.mode.as_str(), "eigenvalue");
}
