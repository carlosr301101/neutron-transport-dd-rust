//! Spatial discretization schemes for the 1D S_N solver.
//!
//! Three cell-level kernels are provided, matching the Python reference:
//!
//! * `DD`   - Diamond Difference: second order, arithmetic cell average.
//! * `STEP` - Step (implicit upwind): first order, average = outgoing face.
//! * `SC`   - Step Characteristic: second order, characteristic weighting.

use std::fmt;

use crate::error::ConfigError;

/// Spatial discretization scheme used in the cell sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Diamond Difference.
    Dd,
    /// Step (implicit upwind).
    Step,
    /// Step Characteristic.
    StepCharacteristic,
}

impl Method {
    /// Parse a scheme name, accepting the aliases used by the Python CLI.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] for unknown names.
    pub fn from_name(name: impl AsRef<str>) -> Result<Self, ConfigError> {
        let text = name.as_ref().trim().to_uppercase();
        match text.as_str() {
            "DD" => Ok(Method::Dd),
            "STEP" => Ok(Method::Step),
            "SC" | "STEP_CHARACTERISTIC" | "STEP-CHARACTERISTIC" => Ok(Method::StepCharacteristic),
            _ => Err(ConfigError::invalid(format!(
                "Invalid SPATIAL_SCHEME: {name:?}. Options: [\"DD\", \"SC\", \"STEP\"]",
                name = name.as_ref()
            ))),
        }
    }

    /// Canonical name written to configuration and output files.
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Dd => "DD",
            Method::Step => "STEP",
            Method::StepCharacteristic => "SC",
        }
    }

    /// Per-cell optical weights required by the scheme, if any.
    ///
    /// Only Step Characteristic needs data: `theta[k][m]`, shape
    /// `(ntc, n_half)`. The other schemes return `None`.
    ///
    /// # Errors
    ///
    /// Propagates an invalid optical thickness from [`theta_step_characteristic`].
    pub fn prepare(
        self,
        sigma_t: &[f64],
        dx: &[f64],
        miu: &[f64],
        ntc: usize,
    ) -> Result<Option<Vec<Vec<f64>>>, ConfigError> {
        if self != Method::StepCharacteristic {
            return Ok(None);
        }

        let mut theta = Vec::with_capacity(ntc);
        for k in 0..ntc {
            let mut row = Vec::with_capacity(miu.len());
            for &mu in miu {
                row.push(theta_step_characteristic(sigma_t[k] * dx[k] / mu)?);
            }
            theta.push(row);
        }
        Ok(Some(theta))
    }

    /// Outgoing angular flux of one cell. `mu` must be positive.
    pub(crate) fn out(
        self,
        psi_in: f64,
        source: f64,
        sigma_t: f64,
        dx: f64,
        mu: f64,
        theta: Option<f64>,
    ) -> f64 {
        let a = mu / dx;
        match self {
            Method::Dd => ((a - 0.5 * sigma_t) * psi_in + source) / (a + 0.5 * sigma_t),
            Method::Step => (a * psi_in + source) / (a + sigma_t),
            Method::StepCharacteristic => {
                let theta = theta.unwrap_or_else(|| theta_unchecked(sigma_t * dx / mu));
                ((a - 0.5 * sigma_t * (1.0 - theta)) * psi_in + source)
                    / (a + 0.5 * sigma_t * (1.0 + theta))
            }
        }
    }

    /// Cell-averaged angular flux. `mu` must be positive.
    pub(crate) fn average(
        self,
        psi_in: f64,
        psi_out: f64,
        sigma_t: f64,
        dx: f64,
        mu: f64,
        theta: Option<f64>,
    ) -> f64 {
        match self {
            Method::Dd => 0.5 * (psi_in + psi_out),
            Method::Step => psi_out,
            Method::StepCharacteristic => {
                let theta = theta.unwrap_or_else(|| theta_unchecked(sigma_t * dx / mu));
                0.5 * (1.0 + theta) * psi_out + 0.5 * (1.0 - theta) * psi_in
            }
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Characteristic weighting `theta = coth(alpha/2) - 2/alpha`.
///
/// `alpha` is the cell optical thickness `Sigma_t * dx / |mu|`. The series
/// expansion avoids catastrophic cancellation as `alpha -> 0`.
///
/// # Errors
///
/// Returns [`ConfigError::Invalid`] for negative or non-finite (NaN) optical
/// thickness. Positive infinity maps to `1.0`.
pub fn theta_step_characteristic(alpha: f64) -> Result<f64, ConfigError> {
    if alpha < 0.0 {
        return Err(ConfigError::invalid(format!(
            "Optical thickness must be >= 0, got {alpha}."
        )));
    }
    if !alpha.is_finite() {
        if alpha.is_infinite() {
            return Ok(1.0);
        }
        return Err(ConfigError::invalid(format!(
            "Optical thickness must be finite, got {alpha}."
        )));
    }
    Ok(theta_unchecked(alpha))
}

/// Validated-free version of [`theta_step_characteristic`].
fn theta_unchecked(alpha: f64) -> f64 {
    if alpha == 0.0 {
        return 0.0;
    }
    if alpha < 1.0e-3 {
        let a2 = alpha * alpha;
        return alpha / 6.0 - alpha * a2 / 360.0 + alpha * a2 * a2 / 15120.0;
    }
    1.0 / (0.5 * alpha).tanh() - 2.0 / alpha
}

/// Outgoing angular flux for the selected scheme.
///
/// `mu` is the direction cosine magnitude; a negative value is normalised to
/// `|mu|`.
///
/// # Errors
///
/// Returns [`ConfigError::Invalid`] when `mu` is zero.
pub fn cell_out(
    psi_in: f64,
    source: f64,
    sigma_t: f64,
    dx: f64,
    mu: f64,
    method: Method,
    theta: Option<f64>,
) -> Result<f64, ConfigError> {
    let mu = mu.abs();
    if mu == 0.0 {
        return Err(ConfigError::invalid("|mu| must be > 0 to sweep a cell."));
    }
    Ok(method.out(psi_in, source, sigma_t, dx, mu, theta))
}

/// Cell-averaged angular flux for the selected scheme.
///
/// # Errors
///
/// Returns [`ConfigError::Invalid`] when `mu` is zero.
pub fn cell_average(
    psi_in: f64,
    psi_out: f64,
    sigma_t: f64,
    dx: f64,
    mu: f64,
    method: Method,
    theta: Option<f64>,
) -> Result<f64, ConfigError> {
    let mu = mu.abs();
    if mu == 0.0 {
        return Err(ConfigError::invalid("|mu| must be > 0 to average a cell."));
    }
    Ok(method.average(psi_in, psi_out, sigma_t, dx, mu, theta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalised() {
        assert_eq!(Method::from_name("dd").unwrap(), Method::Dd);
        assert_eq!(Method::from_name("Step").unwrap(), Method::Step);
        assert_eq!(
            Method::from_name(" sc ").unwrap(),
            Method::StepCharacteristic
        );
        assert_eq!(
            Method::from_name("STEP_CHARACTERISTIC").unwrap(),
            Method::StepCharacteristic
        );
        assert_eq!(
            Method::from_name("STEP-CHARACTERISTIC").unwrap(),
            Method::StepCharacteristic
        );
    }

    #[test]
    fn invalid_name_is_rejected() {
        assert!(Method::from_name("nope").is_err());
    }

    #[test]
    fn canonical_names_round_trip() {
        for method in [Method::Dd, Method::Step, Method::StepCharacteristic] {
            assert_eq!(Method::from_name(method.as_str()).unwrap(), method);
        }
    }

    #[test]
    fn theta_limits() {
        assert_eq!(theta_step_characteristic(0.0).unwrap(), 0.0);
        assert_eq!(theta_step_characteristic(f64::INFINITY).unwrap(), 1.0);
        assert!(theta_step_characteristic(-1e-9).is_err());
        assert!(theta_step_characteristic(f64::NEG_INFINITY).is_err());
        assert!(theta_step_characteristic(f64::NAN).is_err());
    }

    #[test]
    fn theta_series_matches_closed_form_at_threshold() {
        for alpha in [9e-4, 1e-3, 5e-3, 0.1, 1.0] {
            let alpha: f64 = alpha;
            let exact = 1.0 / (0.5 * alpha).tanh() - 2.0 / alpha;
            let tolerance = 50.0 * f64::EPSILON * (2.0 / alpha) + 1e-12;
            let got = theta_step_characteristic(alpha).unwrap();
            assert!(
                (got - exact).abs() <= tolerance,
                "alpha={alpha}: got {got}, exact {exact}"
            );
        }
    }

    #[test]
    fn theta_is_monotone_and_below_one() {
        let mut previous = -1.0;
        let mut alpha = 1e-8;
        while alpha < 1e4 {
            let value = theta_step_characteristic(alpha).unwrap();
            assert!((0.0..1.0).contains(&value));
            assert!(value > previous, "theta must increase with alpha");
            previous = value;
            alpha *= 1.5;
        }
    }

    #[test]
    fn cell_balance_holds_for_every_scheme() {
        for method in [Method::Dd, Method::Step, Method::StepCharacteristic] {
            for (psi_in, source, sigma_t, dx, mu) in [
                (0.7, 0.3, 1.0, 0.2, 0.5),
                (1.0, 0.0, 2.0, 0.05, 0.9),
                (0.2, 1.5, 0.4, 1.0, 0.1),
                (3.0, 0.1, 5.0, 0.01, 0.05),
            ] {
                let out = cell_out(psi_in, source, sigma_t, dx, mu, method, None).unwrap();
                let avg = cell_average(psi_in, out, sigma_t, dx, mu, method, None).unwrap();
                let residual = (mu / dx) * (out - psi_in) + sigma_t * avg - source;
                assert!(
                    residual.abs() < 1e-9,
                    "{method} residual {residual} for {psi_in},{source},{sigma_t},{dx},{mu}"
                );
            }
        }
    }

    #[test]
    fn sc_reduces_to_dd_for_thin_cells() {
        let (sigma_t, dx, mu) = (1.0, 1e-9, 1.0);
        let dd_out = cell_out(0.7, 0.3, sigma_t, dx, mu, Method::Dd, None).unwrap();
        let sc_out = cell_out(0.7, 0.3, sigma_t, dx, mu, Method::StepCharacteristic, None).unwrap();
        assert!((sc_out - dd_out).abs() < 1e-9);

        let dd_avg = cell_average(0.7, dd_out, sigma_t, dx, mu, Method::Dd, None).unwrap();
        let sc_avg = cell_average(
            0.7,
            sc_out,
            sigma_t,
            dx,
            mu,
            Method::StepCharacteristic,
            None,
        )
        .unwrap();
        assert!((sc_avg - dd_avg).abs() < 1e-9);
    }

    #[test]
    fn sc_average_tends_to_step_for_thick_cells() {
        let (sigma_t, dx, mu) = (1e6, 1.0, 1.0);
        let step_out = cell_out(1.0, 0.0, sigma_t, dx, mu, Method::Step, None).unwrap();
        let sc_avg = cell_average(
            1.0,
            step_out,
            sigma_t,
            dx,
            mu,
            Method::StepCharacteristic,
            None,
        )
        .unwrap();
        assert!((sc_avg - step_out).abs() < 1e-5);
    }

    #[test]
    fn mu_sign_is_normalised() {
        for method in [Method::Dd, Method::Step, Method::StepCharacteristic] {
            let positive = cell_out(1.0, 0.5, 1.0, 0.2, 0.5, method, None).unwrap();
            let negative = cell_out(1.0, 0.5, 1.0, 0.2, -0.5, method, None).unwrap();
            assert!((positive - negative).abs() < 1e-15);
        }
    }

    #[test]
    fn zero_mu_is_rejected() {
        assert!(cell_out(1.0, 0.5, 1.0, 0.2, 0.0, Method::Dd, None).is_err());
        assert!(cell_average(1.0, 0.9, 1.0, 0.2, 0.0, Method::Dd, None).is_err());
    }

    #[test]
    fn no_artificial_positivity_clipping() {
        let (sigma_t, dx, mu) = (1.0, 1.0, 0.02);
        let dd = cell_out(1.0, 0.0, sigma_t, dx, mu, Method::Dd, None).unwrap();
        let step = cell_out(1.0, 0.0, sigma_t, dx, mu, Method::Step, None).unwrap();
        assert!(dd < 0.0);
        assert!(step > 0.0);
    }

    #[test]
    fn prepare_only_for_step_characteristic() {
        let (sigma_t, dx, miu) = ([1.0, 2.0], [0.5, 0.25], [0.9, 0.3]);
        assert!(
            Method::Dd
                .prepare(&sigma_t, &dx, &miu, 2)
                .unwrap()
                .is_none()
        );
        assert!(
            Method::Step
                .prepare(&sigma_t, &dx, &miu, 2)
                .unwrap()
                .is_none()
        );
        let theta = Method::StepCharacteristic
            .prepare(&sigma_t, &dx, &miu, 2)
            .unwrap()
            .expect("SC weights");
        assert_eq!(theta.len(), 2);
        assert_eq!(theta[0].len(), 2);
        assert!((theta[0][0] - theta_step_characteristic(1.0 * 0.5 / 0.9).unwrap()).abs() < 1e-15);
    }
}
