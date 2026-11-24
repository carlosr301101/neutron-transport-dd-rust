//! Gauss-Legendre quadrature data for the S_N method.
//!
//! The table is the same one used by the Python reference implementation
//! (`cuadraturas.py`) and is embedded in the binary so the library does not
//! depend on the current working directory.
//!
//! Only the positive hemisphere is stored: the full angular set is obtained by
//! mirroring `mu` (and the weights) to the negative side.

use crate::error::ConfigError;

/// Quadrature orders with tabulated data.
pub const SUPPORTED_ORDERS: [usize; 4] = [2, 4, 6, 8];

const S2_MU: [f64; 1] = [0.577_350_269_189_626];
const S2_OMEGA: [f64; 1] = [1.0];

const S4_MU: [f64; 2] = [0.861_136_311_594_053, 0.339_981_043_584_856];
const S4_OMEGA: [f64; 2] = [0.347_854_845_137_454, 0.652_145_154_862_546];

const S6_MU: [f64; 3] = [
    0.932_469_514_203_152,
    0.661_209_386_466_264_5,
    0.238_619_186_083_196_9,
];
const S6_OMEGA: [f64; 3] = [
    0.171_324_492_379_170_4,
    0.360_761_573_048_138_4,
    0.467_913_934_572_691,
];

const S8_MU: [f64; 4] = [
    0.960_289_856_497_536,
    0.796_666_477_413_627,
    0.525_532_409_916_329,
    0.183_434_642_495_65,
];
const S8_OMEGA: [f64; 4] = [
    0.101_228_536_290_376,
    0.222_381_034_453_374,
    0.313_706_645_877_887,
    0.362_683_783_378_362,
];

/// Return the positive-hemisphere directions and weights for order `n`.
///
/// Directions are returned in the table order (descending `mu`), matching the
/// Python reference so boundary vectors keep the same meaning.
///
/// # Errors
///
/// Returns [`ConfigError::Invalid`] when `n` is odd or has no tabulated data.
pub fn half_directions(n: usize) -> Result<(Vec<f64>, Vec<f64>), ConfigError> {
    if !n.is_multiple_of(2) {
        return Err(ConfigError::invalid(
            "Quadrature order must be an even number.",
        ));
    }

    let (mu, omega): (&[f64], &[f64]) = match n {
        2 => (&S2_MU, &S2_OMEGA),
        4 => (&S4_MU, &S4_OMEGA),
        6 => (&S6_MU, &S6_OMEGA),
        8 => (&S8_MU, &S8_OMEGA),
        _ => {
            return Err(ConfigError::invalid(format!(
                "No quadrature data for S{n}. Supported orders: {SUPPORTED_ORDERS:?}."
            )));
        }
    };

    Ok((mu.to_vec(), omega.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_table_has_n_over_two_entries() {
        for n in SUPPORTED_ORDERS {
            let (mu, omega) = half_directions(n).expect("supported order");
            assert_eq!(mu.len(), n / 2);
            assert_eq!(omega.len(), n / 2);
        }
    }

    #[test]
    fn weights_sum_to_one_per_hemisphere() {
        for n in SUPPORTED_ORDERS {
            let (_, omega) = half_directions(n).expect("supported order");
            let sum: f64 = omega.iter().sum();
            assert!((sum - 1.0).abs() < 1e-14, "S{n} weights sum to {sum}");
        }
    }

    #[test]
    fn directions_are_strictly_decreasing_inside_the_unit_interval() {
        for n in SUPPORTED_ORDERS {
            let (mu, _) = half_directions(n).expect("supported order");
            assert!(mu.iter().all(|value| *value > 0.0 && *value < 1.0));
            assert!(mu.windows(2).all(|pair| pair[0] > pair[1]));
        }
    }

    #[test]
    fn odd_order_is_rejected() {
        let error = half_directions(3).expect_err("odd order");
        assert_eq!(
            error.to_string(),
            "Quadrature order must be an even number."
        );
    }

    #[test]
    fn unsupported_order_is_rejected() {
        let error = half_directions(10).expect_err("untabulated order");
        assert!(error.to_string().contains("No quadrature data for S10"));
    }
}
