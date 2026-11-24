//! Interactive configuration wizard for the 1D solver.
//!
//! The question order and defaults mirror `--manual` in the Python reference.
//! The wizard is generic over [`BufRead`] / [`Write`] so tests can script the
//! answers without touching the process standard streams.

use std::io::{BufRead, Write};

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::error::ConfigError;
use crate::power::DEFAULT_NU;
use crate::scheme::Method;

/// Line-oriented prompt helper.
pub struct Wizard<R: BufRead, W: Write> {
    reader: R,
    writer: W,
}

impl<R: BufRead, W: Write> Wizard<R, W> {
    /// Create a wizard reading answers from `reader` and writing prompts to
    /// `writer`.
    pub fn new(reader: R, writer: W) -> Self {
        Wizard { reader, writer }
    }

    /// Ask every question and return the validated configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for invalid answers or inconsistent physics.
    pub fn build_config(
        &mut self,
        forced_scheme: Option<Method>,
        epsilon: f64,
        max_iter: usize,
    ) -> Result<Config, ConfigError> {
        let num_regions = self.ask_usize("\n Enter the number of Regions: ", None)?;
        let num_zones = self.ask_usize("\n Enter the number of Zones: ", None)?;
        if num_zones > num_regions {
            return Err(ConfigError::invalid(
                "Number of zones cannot exceed the number of regions.",
            ));
        }

        self.section("--- SPATIAL CONFIGURATION ---")?;
        let mut nc = Vec::with_capacity(num_regions);
        let mut hr = Vec::with_capacity(num_regions);
        let mut izl = Vec::with_capacity(num_regions);
        for region in 0..num_regions {
            nc.push(self.ask_usize(&format!("Cells in Region {} (integer): ", region + 1), None)?);
            hr.push(self.ask_float(
                &format!("Total thickness [cm] Region {}: ", region + 1),
                None,
            )?);
            izl.push(self.ask_usize(
                &format!(
                    "Material Zone ID Region {} (integer, 1-based): ",
                    region + 1
                ),
                None,
            )?);
        }

        self.section("--- MATERIAL CONFIGURATION ---")?;
        let mut sct = Vec::with_capacity(num_zones);
        let mut scs = Vec::with_capacity(num_zones);
        for zone in 0..num_zones {
            sct.push(self.ask_float(&format!("Sigma_Total Zone {}: ", zone + 1), None)?);
            scs.push(self.ask_float(&format!("Sigma_Scattering Zone {}: ", zone + 1), None)?);
        }

        self.section("--- QUADRATURE CONFIGURATION ---")?;
        let n = self.ask_usize("Quadrature order (2, 4, 6, 8) [8]: ", Some(8))?;
        if n % 2 != 0 {
            return Err(ConfigError::invalid("Quadrature order must be even."));
        }

        let scheme = match forced_scheme {
            Some(method) => method,
            None => {
                let name = self.ask_text("Spatial scheme (DD / STEP / SC) [DD]: ", "DD")?;
                Method::from_name(name)?
            }
        };

        let mut config_map = Map::new();
        config_map.insert("num_regions".into(), json!(num_regions));
        config_map.insert("num_zones".into(), json!(num_zones));
        config_map.insert("NC".into(), json!(nc));
        config_map.insert("HR".into(), json!(hr));
        config_map.insert("IZL".into(), json!(izl));
        config_map.insert("SCT".into(), json!(sct));
        config_map.insert("SCS".into(), json!(scs));
        config_map.insert("N".into(), json!(n));
        config_map.insert("SPATIAL_SCHEME".into(), json!(scheme.as_str()));

        // --- Multiplicative medium ---
        let mut mode = String::from("fixed_source");
        let mut nu_sigf = vec![0.0; num_zones];
        let mut sigma_f: Option<Vec<f64>> = None;
        let mut k_eff: Option<f64> = None;
        let mut tol_k = 1e-6;
        let mut max_outer = 200usize;
        let mut k0: Option<f64> = None;
        let mut power_kw: Option<f64> = None;
        let mut e_f_mev = 200.0;
        let mut nu: Option<f64> = None;
        let mut area_cm2 = 1.0;

        let fission = self.ask_bool("Multiplicative medium (fission)? (y/n) [n]: ", false)?;
        if fission {
            for (zone, slot) in nu_sigf.iter_mut().enumerate() {
                *slot = self.ask_float(&format!("nu*Sigma_f Zone {} [cm^-1]: ", zone + 1), None)?;
            }

            mode = self
                .ask_text(
                    "Mode (eigenvalue / fixed_k / driven) [eigenvalue]: ",
                    "eigenvalue",
                )?
                .to_lowercase();
            if !matches!(mode.as_str(), "eigenvalue" | "fixed_k" | "driven") {
                return Err(ConfigError::invalid(format!("Invalid mode: {mode}")));
            }
            match mode.as_str() {
                "fixed_k" => k_eff = Some(self.ask_float("k_eff imposed: ", None)?),
                "driven" => k_eff = Some(1.0),
                _ => k_eff = None,
            }

            tol_k = self.ask_float("TOL_K [1e-6]: ", Some(1e-6))?;
            max_outer = self.ask_usize("MAX_OUTER [200]: ", Some(200))?;
            let k0_text = self.ask_raw("K0 (empty = automatic): ")?;
            if !k0_text.is_empty() {
                k0 =
                    Some(k0_text.parse().map_err(|_| {
                        ConfigError::invalid(format!("Invalid value: {k0_text:?}"))
                    })?);
            }

            if self.ask_bool("Normalize to a power? (y/n) [n]: ", false)? {
                power_kw = Some(self.ask_float("Power [kW]: ", None)?);
                e_f_mev = self.ask_float("E_f [MeV/fission] [200]: ", Some(200.0))?;
                area_cm2 = self.ask_float("Cross-sectional area [cm^2] [1]: ", Some(1.0))?;
                let conversion = self.ask_text("Fission conversion: nu or sigma_f [nu]: ", "nu")?;
                match conversion.to_lowercase().as_str() {
                    "nu" => {
                        nu = Some(self.ask_float(
                            &format!("nu (neutrons/fission, U-238 default) [{DEFAULT_NU}]: "),
                            Some(DEFAULT_NU),
                        )?);
                    }
                    "sigma_f" => {
                        let mut values = Vec::with_capacity(num_zones);
                        for zone in 0..num_zones {
                            values.push(self.ask_float(
                                &format!("Sigma_f Zone {} [cm^-1]: ", zone + 1),
                                None,
                            )?);
                        }
                        sigma_f = Some(values);
                    }
                    _ => {
                        return Err(ConfigError::invalid(
                            "Conversion must be 'nu' or 'sigma_f'.",
                        ));
                    }
                }
            }
        }
        config_map.insert("NU_SIGF".into(), json!(nu_sigf));
        config_map.insert("MODE".into(), json!(mode));
        config_map.insert("K_EFF".into(), json!(k_eff));
        config_map.insert("TOL_K".into(), json!(tol_k));
        config_map.insert("MAX_OUTER".into(), json!(max_outer));
        config_map.insert("K0".into(), json!(k0));
        config_map.insert("POWER_KW".into(), json!(power_kw));
        config_map.insert("E_F_MEV".into(), json!(e_f_mev));
        config_map.insert("NU".into(), json!(nu));
        config_map.insert("AREA_CM2".into(), json!(area_cm2));
        config_map.insert(
            "SIGMA_F".into(),
            match sigma_f {
                Some(values) => json!(values),
                None => Value::Null,
            },
        );

        // --- External source ---
        let q = if mode == "eigenvalue" {
            vec![0.0; num_regions]
        } else {
            self.section("--- EXTERNAL SOURCE ---")?;
            let mut values = Vec::with_capacity(num_regions);
            for region in 0..num_regions {
                values.push(self.ask_float(
                    &format!("Source (Q) Region {} [0]: ", region + 1),
                    Some(0.0),
                )?);
            }
            values
        };
        config_map.insert("Q".into(), json!(q));

        // --- Boundary conditions ---
        self.section("--- BOUNDARY CONDITIONS ---")?;
        let reflex_izq = self.ask_bool("Reflective left? (y/n) [n]: ", false)?;
        let reflex_der = self.ask_bool("Reflective right? (y/n) [n]: ", false)?;
        let n_half = n / 2;

        let bound_left = if reflex_izq || mode == "eigenvalue" {
            vec![0.0; n_half]
        } else {
            self.ask_boundary("izquierda", n_half)?
        };
        let bound_right = if reflex_der || mode == "eigenvalue" {
            vec![0.0; n_half]
        } else {
            self.ask_boundary("derecha", n_half)?
        };

        config_map.insert("reflex_izq".into(), json!(reflex_izq));
        config_map.insert("reflex_der".into(), json!(reflex_der));
        config_map.insert("bound_left".into(), json!(bound_left));
        config_map.insert("bound_right".into(), json!(bound_right));

        Config::from_value(&Value::Object(config_map), epsilon, max_iter, forced_scheme)
    }

    fn section(&mut self, title: &str) -> Result<(), ConfigError> {
        writeln!(self.writer, "\n{title}").map_err(io_error)
    }

    fn ask_raw(&mut self, prompt: &str) -> Result<String, ConfigError> {
        write!(self.writer, "{prompt}").map_err(io_error)?;
        self.writer.flush().map_err(io_error)?;
        let mut line = String::new();
        self.reader.read_line(&mut line).map_err(io_error)?;
        Ok(line.trim().to_string())
    }

    fn ask_usize(&mut self, prompt: &str, default: Option<usize>) -> Result<usize, ConfigError> {
        let text = self.ask_raw(prompt)?;
        if text.is_empty() {
            return default
                .ok_or_else(|| ConfigError::invalid(format!("Value required: {prompt}")));
        }
        text.parse()
            .map_err(|_| ConfigError::invalid(format!("Invalid value: {text:?}")))
    }

    fn ask_float(&mut self, prompt: &str, default: Option<f64>) -> Result<f64, ConfigError> {
        let text = self.ask_raw(prompt)?;
        if text.is_empty() {
            return default
                .ok_or_else(|| ConfigError::invalid(format!("Value required: {prompt}")));
        }
        text.parse()
            .map_err(|_| ConfigError::invalid(format!("Invalid value: {text:?}")))
    }

    fn ask_text(&mut self, prompt: &str, default: &str) -> Result<String, ConfigError> {
        let text = self.ask_raw(prompt)?;
        Ok(if text.is_empty() {
            default.to_string()
        } else {
            text
        })
    }

    fn ask_bool(&mut self, prompt: &str, default: bool) -> Result<bool, ConfigError> {
        let text = self.ask_raw(prompt)?.to_lowercase();
        if text.is_empty() {
            return Ok(default);
        }
        Ok(matches!(text.as_str(), "y" | "s" | "yes" | "si"))
    }

    fn ask_boundary(&mut self, side: &str, n_half: usize) -> Result<Vec<f64>, ConfigError> {
        let default = vec!["0"; n_half].join(" ");
        let prompt = format!("Incoming flux {side} ({n_half} values) [{default}]: ");
        let text = self.ask_raw(&prompt)?;
        if text.is_empty() {
            return Ok(vec![0.0; n_half]);
        }
        let tokens: Vec<String> = text
            .replace(',', " ")
            .split_whitespace()
            .map(str::to_string)
            .collect();
        if tokens.len() != n_half {
            return Err(ConfigError::invalid(format!(
                "Expected {n_half} values for boundary {side}."
            )));
        }
        tokens
            .iter()
            .map(|token| {
                token
                    .parse()
                    .map_err(|_| ConfigError::invalid(format!("Invalid value: {token:?}")))
            })
            .collect()
    }
}

fn io_error(error: std::io::Error) -> ConfigError {
    ConfigError::invalid(format!("failed to read input: {error}"))
}

/// Run the wizard on the process standard streams.
///
/// # Errors
///
/// Returns [`ConfigError`] for invalid answers or inconsistent physics.
pub fn run_interactive(
    forced_scheme: Option<Method>,
    epsilon: f64,
    max_iter: usize,
) -> Result<Config, ConfigError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut wizard = Wizard::new(stdin.lock(), stdout);
    wizard.build_config(forced_scheme, epsilon, max_iter)
}
