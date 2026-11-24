//! Scheme-agnostic S_N transport engine for the 1D slab.
//!
//! The engine mirrors the Python reference (`solvers/runners/runner.py`):
//!
//! * `fixed_source`: no fission, source iteration over scattering.
//! * `fixed_k`: fission with an imposed `k_eff`.
//! * `driven`: physical fission (`k = 1`) with an external source.
//! * `eigenvalue`: homogeneous problem, `k_eff` from power iteration.
//!
//! Conventions that must not drift from the reference:
//!
//! * `scalar_flux` is the **angular average** (`0.5 * sum_m omega_m psi_m`,
//!   with the positive weights summing to 1).
//! * The isotropic source enters the sweep as `Sigma_s * phi + 0.5 * Q`.
//! * Convergence is `max |phi_new - phi_old| / |phi_old| < epsilon`, with the
//!   zero-flux cells falling back to the absolute difference.

use crate::config::{Config, Mode};
use crate::error::SolverError;
use crate::power;
use crate::result::RunResult;
use crate::scheme::Method;

/// Iterative S_N solver for one configuration.
#[derive(Debug)]
pub struct Runner {
    config: Config,
    method: Method,
    theta: Option<Vec<Vec<f64>>>,

    // Angular fluxes at the nodes: (ntp, n_half).
    psi_right: Vec<Vec<f64>>,
    psi_left: Vec<Vec<f64>>,
    // Cell-averaged angular flux: (ntc, n), columns ordered by ascending mu.
    angular_flux: Vec<Vec<f64>>,

    miu: Vec<f64>,
    omega: Vec<f64>,
    right_col: Vec<usize>,
    left_col: Vec<usize>,

    scalar_flux: Vec<f64>,
    total_source: Vec<f64>,
    fission_source: Vec<f64>,

    converged: bool,
    iteration: usize,
    outer_iterations: usize,
    iteration_eigenvalue: Option<usize>,

    k_eff: Option<f64>,
    k_history: Vec<f64>,
    k_converged: Option<bool>,
    k_balance: Option<f64>,

    power_w: Option<f64>,
    power_scale: f64,
    power_unnormalized: Option<f64>,
}

impl Runner {
    /// Build a runner for a validated configuration.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError::Config`] if the scheme weights cannot be built.
    pub fn new(config: Config) -> Result<Self, SolverError> {
        let method = config.method();
        Self::with_scheme(config, method)
    }

    /// Build a runner overriding the spatial scheme (used by tests and the
    /// `--scheme` flag). The configuration keeps its own `SPATIAL_SCHEME`.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError::Config`] if the scheme weights cannot be built.
    pub fn with_scheme(config: Config, method: Method) -> Result<Self, SolverError> {
        let theta = method.prepare(
            &config.sigma_t_vec,
            &config.dx_vec,
            &config.miu_m,
            config.ntc,
        )?;

        let ntp = config.ntp;
        let ntc = config.ntc;
        let n_half = config.n_half;
        let n = config.n;

        let mut psi_right = vec![vec![0.0; n_half]; ntp];
        let mut psi_left = vec![vec![0.0; n_half]; ntp];
        if !config.reflex_izq {
            psi_right[0][..n_half].copy_from_slice(&config.bound_left[..n_half]);
        }
        if !config.reflex_der {
            psi_left[ntp - 1][..n_half].copy_from_slice(&config.bound_right[..n_half]);
        }

        // Full angular set ordered by ascending mu, as in the reference.
        let mut order: Vec<usize> = (0..n_half).collect();
        order.sort_by(|&a, &b| {
            config.miu_m[a]
                .partial_cmp(&config.miu_m[b])
                .expect("quadrature data is finite")
        });

        let mut miu = Vec::with_capacity(n);
        let mut omega = Vec::with_capacity(n);
        for &index in order.iter().rev() {
            miu.push(-config.miu_m[index]);
            omega.push(config.omega_m[index]);
        }
        for &index in order.iter() {
            miu.push(config.miu_m[index]);
            omega.push(config.omega_m[index]);
        }

        let mut rank = vec![0usize; n_half];
        for (position, &index) in order.iter().enumerate() {
            rank[index] = position;
        }
        let right_col: Vec<usize> = (0..n_half).map(|m| n_half + rank[m]).collect();
        let left_col: Vec<usize> = (0..n_half).map(|m| n_half - 1 - rank[m]).collect();

        let mut scalar_flux = vec![0.0; ntc];
        let needs_guess = config.mode == Mode::Eigenvalue
            || (matches!(config.mode, Mode::FixedK | Mode::Driven)
                && !has_external_source(&config, &psi_right, &psi_left));
        if needs_guess {
            scalar_flux.iter_mut().for_each(|value| *value = 1.0);
        }

        Ok(Runner {
            config,
            method,
            theta,
            psi_right,
            psi_left,
            angular_flux: vec![vec![0.0; n]; ntc],
            miu,
            omega,
            right_col,
            left_col,
            scalar_flux,
            total_source: vec![0.0; ntc],
            fission_source: vec![0.0; ntc],
            converged: false,
            iteration: 0,
            outer_iterations: 0,
            iteration_eigenvalue: None,
            k_eff: None,
            k_history: Vec::new(),
            k_converged: None,
            k_balance: None,
            power_w: None,
            power_scale: 1.0,
            power_unnormalized: None,
        })
    }

    /// Run the configured mode and return the result.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError::Runtime`] when a mode cannot produce a solution
    /// (for example, `driven` without source or zero fission production).
    pub fn run(mut self) -> Result<RunResult, SolverError> {
        match self.config.mode {
            Mode::Eigenvalue => self.eigenvalue_iteration()?,
            Mode::FixedK | Mode::Driven => self.fixed_k_iteration()?,
            Mode::FixedSource => self.fixed_source_iteration(),
        }
        self.apply_power_normalization()?;
        Ok(self.build_result())
    }

    // -----------------------------------------------------------------
    // Sweep and convergence
    // -----------------------------------------------------------------

    /// One transport sweep. Returns the scalar flux before the update.
    fn sweep(&mut self) -> Vec<f64> {
        let ntc = self.config.ntc;
        let n_half = self.config.n_half;
        let method = self.method;

        for k in 0..ntc {
            // S = Sigma_s * phi + 0.5 * Q + fission source.
            self.total_source[k] = self.config.sigma_s_vec[k] * self.scalar_flux[k]
                + 0.5 * self.config.q_ext_vec[k]
                + self.fission_source[k];
        }

        let old_flux = self.scalar_flux.clone();
        self.scalar_flux.iter_mut().for_each(|value| *value = 0.0);
        for row in &mut self.angular_flux {
            row.iter_mut().for_each(|value| *value = 0.0);
        }

        // --- Rightgoing sweep (mu > 0) ---
        for k in 0..ntc {
            let dx = self.config.dx_vec[k];
            let sigma_t = self.config.sigma_t_vec[k];
            let source = self.total_source[k];
            let mut phi_k = 0.0;

            for m in 0..n_half {
                let theta = self.theta.as_ref().map(|rows| rows[k][m]);
                let mu = self.config.miu_m[m];
                let psi_in = self.psi_right[k][m];
                let psi_out = method.out(psi_in, source, sigma_t, dx, mu, theta);
                self.psi_right[k + 1][m] = psi_out;

                let average = method.average(psi_in, psi_out, sigma_t, dx, mu, theta);
                self.angular_flux[k][self.right_col[m]] = average;
                phi_k += self.config.omega_m[m] * average;
            }
            self.scalar_flux[k] = phi_k;
        }

        // --- Leftgoing sweep (mu < 0) ---
        for k in (0..ntc).rev() {
            let dx = self.config.dx_vec[k];
            let sigma_t = self.config.sigma_t_vec[k];
            let source = self.total_source[k];
            let mut phi_k = self.scalar_flux[k];

            for m in 0..n_half {
                let theta = self.theta.as_ref().map(|rows| rows[k][m]);
                let mu = self.config.miu_m[m].abs();
                let psi_in = self.psi_left[k + 1][m];
                let psi_out = method.out(psi_in, source, sigma_t, dx, mu, theta);
                self.psi_left[k][m] = psi_out;

                let average = method.average(psi_in, psi_out, sigma_t, dx, mu, theta);
                self.angular_flux[k][self.left_col[m]] = average;
                phi_k += self.config.omega_m[m] * average;
            }
            self.scalar_flux[k] = 0.5 * phi_k;
        }

        old_flux
    }

    /// `max_i |1 - new_i / old_i|`, with an absolute fallback for zero flux.
    ///
    /// Exposed as a diagnostic helper; the solver uses it internally for the
    /// convergence criterion.
    pub fn relative_flux_change(old_flux: &[f64], new_flux: &[f64]) -> f64 {
        let mut maximum = 0.0_f64;
        for (old, new) in old_flux.iter().zip(new_flux) {
            let denominator = if old.abs() > 1e-13 { old.abs() } else { 1.0 };
            let change = (new - old).abs() / denominator;
            if change > maximum {
                maximum = change;
            }
        }
        maximum
    }

    /// Increment the iteration counter and test the flux criterion.
    fn check_convergence(&mut self, old_flux: &[f64], tol: f64) -> bool {
        self.iteration += 1;
        Self::relative_flux_change(old_flux, &self.scalar_flux) < tol
    }

    // -----------------------------------------------------------------
    // Boundary updates
    // -----------------------------------------------------------------

    fn update_reflective_boundaries_right(&mut self) {
        let last = self.config.ntp - 1;
        self.psi_left[last].copy_from_slice(&self.psi_right[last]);
    }

    fn update_reflective_boundaries_left(&mut self) {
        self.psi_right[0].copy_from_slice(&self.psi_left[0]);
    }

    // -----------------------------------------------------------------
    // Iteration strategies
    // -----------------------------------------------------------------

    fn fixed_source_iteration(&mut self) {
        self.converged = false;
        let tol = self.config.epsilon;
        while self.iteration < self.config.max_iter {
            if self.config.reflex_der {
                self.update_reflective_boundaries_right();
            }
            if self.config.reflex_izq {
                self.update_reflective_boundaries_left();
            }
            let old_flux = self.sweep();
            if self.check_convergence(&old_flux, tol) {
                self.converged = true;
                break;
            }
        }
    }

    fn inner_solve(&mut self) -> bool {
        self.converged = false;
        let mut inner = 0;
        let inner_tol = self.config.epsilon.max(self.config.tol_k);
        while inner < self.config.max_iter {
            inner += 1;
            if self.config.reflex_der {
                self.update_reflective_boundaries_right();
            }
            if self.config.reflex_izq {
                self.update_reflective_boundaries_left();
            }
            let old_flux = self.sweep();
            if self.check_convergence(&old_flux, inner_tol) {
                self.converged = true;
                break;
            }
        }
        self.converged
    }

    fn fixed_k_iteration(&mut self) -> Result<(), SolverError> {
        let k_eff = self
            .config
            .k_eff
            .ok_or_else(|| SolverError::runtime("MODE='fixed_k' requires K_EFF > 0."))?;
        let has_source = self.has_external_source();
        if self.config.mode == Mode::Driven && !has_source {
            return Err(SolverError::runtime(
                "MODE='driven' requires Q != 0 or non-zero incoming flux.",
            ));
        }

        self.k_eff = Some(k_eff);
        self.converged = false;
        let tol = self.config.epsilon;

        while self.iteration < self.config.max_iter {
            if self.config.reflex_der {
                self.update_reflective_boundaries_right();
            }
            if self.config.reflex_izq {
                self.update_reflective_boundaries_left();
            }
            for k in 0..self.config.ntc {
                self.fission_source[k] =
                    self.config.nu_sigma_f_vec[k] * self.scalar_flux[k] / k_eff;
            }
            let old_flux = self.sweep();
            if !has_source {
                let scale = self
                    .scalar_flux
                    .iter()
                    .fold(0.0_f64, |acc, value| acc.max(value.abs()));
                if scale > 0.0 {
                    self.scale_state(1.0 / scale);
                }
            }
            if self.check_convergence(&old_flux, tol) {
                self.converged = true;
                break;
            }
        }

        self.k_converged = Some(self.converged);
        self.k_balance = self.balance_k();
        Ok(())
    }

    fn eigenvalue_iteration(&mut self) -> Result<(), SolverError> {
        let mut k_eff = match self.config.k0 {
            Some(k0) => k0,
            None => self.estimate_k0(),
        };
        if k_eff <= 0.0 {
            return Err(SolverError::runtime(
                "K0 must be > 0 to start power iteration.",
            ));
        }
        self.k_eff = Some(k_eff);

        self.scalar_flux.iter_mut().for_each(|value| *value = 1.0);
        let mut production_old = self.fission_production();
        if production_old <= 0.0 {
            return Err(SolverError::runtime(
                "NU_SIGF must be positive in at least one zone for the eigenvalue.",
            ));
        }

        self.k_history.clear();
        self.k_converged = Some(false);
        let tol_k = self.config.tol_k;

        for outer in 1..=self.config.max_outer {
            self.outer_iterations = outer;
            for k in 0..self.config.ntc {
                self.fission_source[k] =
                    self.config.nu_sigma_f_vec[k] * self.scalar_flux[k] / k_eff;
            }

            let inner_ok = self.inner_solve();

            let production_new = self.fission_production();
            if production_new <= 0.0 {
                return Err(SolverError::runtime(
                    "Zero fission production during power iteration.",
                ));
            }
            let k_new = k_eff * production_new / production_old;

            let scale = self
                .scalar_flux
                .iter()
                .fold(0.0_f64, |acc, value| acc.max(value.abs()));
            if scale > 0.0 {
                self.scale_state(1.0 / scale);
            }
            production_old = self.fission_production();

            self.k_history.push(k_new);
            let dk = (k_new - k_eff).abs() / k_new.abs();
            k_eff = k_new;
            self.k_eff = Some(k_eff);

            if !inner_ok {
                break;
            }
            if dk < tol_k {
                self.k_converged = Some(true);
                break;
            }
        }

        self.iteration_eigenvalue = Some(self.outer_iterations);
        self.converged = self.k_converged == Some(true);
        self.k_balance = self.balance_k();
        Ok(())
    }

    // -----------------------------------------------------------------
    // Diagnostics and normalization
    // -----------------------------------------------------------------

    fn fission_production(&self) -> f64 {
        self.config
            .nu_sigma_f_vec
            .iter()
            .zip(&self.scalar_flux)
            .zip(&self.config.dx_vec)
            .map(|((nu_sigma_f, phi), dx)| nu_sigma_f * phi * dx)
            .sum()
    }

    fn estimate_k0(&self) -> f64 {
        let production: f64 = self
            .config
            .nu_sigma_f_vec
            .iter()
            .zip(&self.config.dx_vec)
            .map(|(nu_sigma_f, dx)| nu_sigma_f * dx)
            .sum();
        let absorption: f64 = self
            .config
            .sigma_t_vec
            .iter()
            .zip(&self.config.sigma_s_vec)
            .zip(&self.config.dx_vec)
            .map(|((total, scatter), dx)| (total - scatter) * dx)
            .sum();
        if production > 0.0 && absorption > 0.0 {
            production / absorption
        } else {
            1.0
        }
    }

    fn net_leakage(&self) -> f64 {
        let n_half = self.config.n_half;
        let omega = &self.config.omega_m;
        let mu = &self.config.miu_m;
        let last = self.config.ntp - 1;

        let mut left_out = 0.0;
        let mut left_in = 0.0;
        let mut right_out = 0.0;
        let mut right_in = 0.0;
        for m in 0..n_half {
            left_out += omega[m] * mu[m].abs() * self.psi_left[0][m];
            left_in += omega[m] * mu[m] * self.psi_right[0][m];
            right_out += omega[m] * mu[m] * self.psi_right[last][m];
            right_in += omega[m] * mu[m].abs() * self.psi_left[last][m];
        }
        (left_out - left_in) + (right_out - right_in)
    }

    fn balance_k(&self) -> Option<f64> {
        if !self.config.nu_sigma_f_vec.iter().any(|value| *value > 0.0) {
            return None;
        }
        let production = 2.0 * self.fission_production();
        let absorption: f64 = 2.0
            * self
                .config
                .sigma_t_vec
                .iter()
                .zip(&self.config.sigma_s_vec)
                .zip(&self.scalar_flux)
                .zip(&self.config.dx_vec)
                .map(|(((total, scatter), phi), dx)| (total - scatter) * phi * dx)
                .sum::<f64>();
        let denominator = absorption + self.net_leakage();
        if denominator <= 0.0 || production <= 0.0 {
            None
        } else {
            Some(production / denominator)
        }
    }

    fn has_external_source(&self) -> bool {
        has_external_source(&self.config, &self.psi_right, &self.psi_left)
    }

    fn scale_state(&mut self, factor: f64) {
        for value in &mut self.scalar_flux {
            *value *= factor;
        }
        for row in &mut self.angular_flux {
            for value in row {
                *value *= factor;
            }
        }
        for row in &mut self.psi_right {
            for value in row {
                *value *= factor;
            }
        }
        for row in &mut self.psi_left {
            for value in row {
                *value *= factor;
            }
        }
    }

    fn apply_power_normalization(&mut self) -> Result<(), SolverError> {
        let Some(power_kw) = self.config.power_kw else {
            return Ok(());
        };

        let sigma_f: Vec<f64> = if self.config.sigma_f_vec.iter().any(|value| *value > 0.0) {
            self.config.sigma_f_vec.clone()
        } else if let Some(nu) = self.config.nu {
            if nu.is_nan() || nu <= 0.0 {
                return Err(SolverError::runtime("NU must be > 0."));
            }
            self.config
                .nu_sigma_f_vec
                .iter()
                .map(|value| value / nu)
                .collect()
        } else {
            return Err(SolverError::runtime(
                "POWER_KW requires SIGMA_F per zone or NU (nu_bar).",
            ));
        };

        let unnormalized = power::power_watts(
            &self.scalar_flux,
            &sigma_f,
            &self.config.dx_vec,
            self.config.e_f_mev,
            self.config.area_cm2,
        );
        self.power_unnormalized = Some(unnormalized);
        if unnormalized <= 0.0 {
            return Err(SolverError::runtime(
                "Cannot normalize power: fission integral is zero.",
            ));
        }

        let target_w = power_kw * 1e3;
        self.power_scale = target_w / unnormalized;
        self.scale_state(self.power_scale);
        self.power_w = Some(target_w);
        Ok(())
    }

    fn build_result(self) -> RunResult {
        RunResult {
            scalar_flux: self.scalar_flux,
            angular_flux: self.angular_flux,
            miu: self.miu,
            omega: self.omega,
            psi_right: self.psi_right,
            psi_left: self.psi_left,
            iteration: self.iteration,
            iteration_eigenvalue: self.iteration_eigenvalue,
            converged: self.converged,
            mode: self.config.mode,
            spatial_scheme: self.method,
            k_eff: self.k_eff,
            k_balance: self.k_balance,
            k_history: self.k_history,
            k_converged: self.k_converged,
            outer_iterations: self.outer_iterations,
            power_w: self.power_w,
            power_scale: self.power_scale,
            power_unnormalized: self.power_unnormalized,
        }
    }
}

/// Whether the problem has a source (external `Q` or incoming flux).
fn has_external_source(config: &Config, psi_right: &[Vec<f64>], psi_left: &[Vec<f64>]) -> bool {
    let left_external = !config.reflex_izq
        && (config.bound_left.iter().any(|value| *value != 0.0)
            || psi_right[0].iter().any(|value| *value != 0.0));
    let right_external = !config.reflex_der
        && (config.bound_right.iter().any(|value| *value != 0.0)
            || psi_left[config.ntp - 1].iter().any(|value| *value != 0.0));
    config.q_ext_vec.iter().any(|value| *value != 0.0) || left_external || right_external
}
