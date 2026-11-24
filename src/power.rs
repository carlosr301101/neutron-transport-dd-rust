//! Power normalization helpers for the 1D S_N solver.

use crate::error::SolverError;

/// 1 MeV expressed in joules.
pub const MEV_TO_J: f64 = 1.602_176_634e-13;

/// Default neutrons per fission (U-238 fast fission).
///
/// Only used to convert `NU_SIGF` into `SIGMA_F` for power normalization.
pub const DEFAULT_NU: f64 = 2.49;

/// Physical fission integral: `2 * sum(sigma_f * phi * dx)`.
///
/// `phi` is the angular average, so the reaction-rate integral carries the
/// factor 2 (the full N weights sum to 2).
pub fn fission_integral(scalar_flux: &[f64], sigma_f: &[f64], dx: &[f64]) -> f64 {
    debug_assert_eq!(scalar_flux.len(), sigma_f.len());
    debug_assert_eq!(scalar_flux.len(), dx.len());
    2.0 * scalar_flux
        .iter()
        .zip(sigma_f)
        .zip(dx)
        .map(|((phi, sigma), width)| sigma * phi * width)
        .sum::<f64>()
}

/// Unnormalized thermal power \[W\] for the given flux distribution.
pub fn power_watts(
    scalar_flux: &[f64],
    sigma_f: &[f64],
    dx: &[f64],
    ef_mev: f64,
    area_cm2: f64,
) -> f64 {
    area_cm2 * ef_mev * MEV_TO_J * fission_integral(scalar_flux, sigma_f, dx)
}

/// `C_N = target / unnormalized power`.
///
/// # Errors
///
/// Returns [`SolverError::Runtime`] when the fission integral is not positive.
pub fn scale_factor(
    scalar_flux: &[f64],
    sigma_f: &[f64],
    dx: &[f64],
    ef_mev: f64,
    area_cm2: f64,
    target_w: f64,
) -> Result<f64, SolverError> {
    let power = power_watts(scalar_flux, sigma_f, dx, ef_mev, area_cm2);
    if power <= 0.0 {
        return Err(SolverError::runtime(
            "Cannot normalize power: fission integral is zero.",
        ));
    }
    Ok(target_w / power)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fission_integral_carries_the_factor_two() {
        let integral = fission_integral(&[2.0, 2.0], &[0.5, 0.5], &[1.0, 1.0]);
        assert!((integral - 4.0).abs() < 1e-15);
    }

    #[test]
    fn zero_production_cannot_be_normalized() {
        let error =
            scale_factor(&[0.0], &[1.0], &[1.0], 200.0, 1.0, 10_000.0).expect_err("zero fission");
        assert!(error.to_string().contains("fission integral is zero"));
    }
}
