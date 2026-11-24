//! Configuration model for the 1D S_N solver.
//!
//! The JSON schema is the one used by the Python reference implementation:
//! uppercase physical keys (`NC`, `HR`, `SCT`, `NU_SIGF`, ...) and the same
//! validation rules and messages. Every field is validated before the sweep
//! starts, and `as_dict` reproduces the effective configuration written next
//! to each run in the output file.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::error::ConfigError;
use crate::power::DEFAULT_NU;
use crate::quadrature;
use crate::scheme::Method;

/// Physics mode of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No fission; external source and/or incoming flux.
    FixedSource,
    /// Fission with an imposed `k_eff`.
    FixedK,
    /// Physical fission (`k = 1`) driven by an external source.
    Driven,
    /// Homogeneous problem; `k_eff` obtained by power iteration.
    Eigenvalue,
}

impl Mode {
    /// Canonical name written to the output file.
    pub const fn as_str(self) -> &'static str {
        match self {
            Mode::FixedSource => "fixed_source",
            Mode::FixedK => "fixed_k",
            Mode::Driven => "driven",
            Mode::Eigenvalue => "eigenvalue",
        }
    }

    /// Parse a mode name (case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] for unknown modes.
    pub fn from_name(name: &str) -> Result<Self, ConfigError> {
        match name.trim().to_lowercase().as_str() {
            "fixed_source" => Ok(Mode::FixedSource),
            "fixed_k" => Ok(Mode::FixedK),
            "driven" => Ok(Mode::Driven),
            "eigenvalue" => Ok(Mode::Eigenvalue),
            other => Err(ConfigError::invalid(format!(
                "Invalid MODE: {other:?}. Options: [\"driven\", \"eigenvalue\", \"fixed_k\", \"fixed_source\"]"
            ))),
        }
    }
}

/// Fully validated configuration for one 1D run.
#[derive(Debug, Clone)]
pub struct Config {
    /// Convergence tolerance for the scalar flux.
    pub epsilon: f64,
    /// Maximum number of flux updates per solve.
    pub max_iter: usize,
    /// Number of spatial regions.
    pub num_regions: usize,
    /// Number of material zones.
    pub num_zones: usize,
    /// Cells per region.
    pub nc: Vec<usize>,
    /// Thickness of each region \[cm\].
    pub hr: Vec<f64>,
    /// Material zone of each region (1-based).
    pub izl: Vec<usize>,
    /// External source per region.
    pub q: Vec<f64>,
    /// Total cross-section per zone.
    pub sct: Vec<f64>,
    /// Scattering cross-section per zone.
    pub scs: Vec<f64>,

    /// Quadrature order (even, tabulated).
    pub n: usize,
    /// Number of directions per hemisphere.
    pub n_half: usize,
    /// Positive-hemisphere directional cosines (table order).
    pub miu_m: Vec<f64>,
    /// Positive-hemisphere quadrature weights (table order).
    pub omega_m: Vec<f64>,

    /// Reflective left boundary.
    pub reflex_izq: bool,
    /// Reflective right boundary.
    pub reflex_der: bool,
    /// Incoming flux at the left boundary (`N/2` values).
    pub bound_left: Vec<f64>,
    /// Incoming flux at the right boundary (`N/2` values).
    pub bound_right: Vec<f64>,

    /// `nu * Sigma_f` per zone \[cm^-1\].
    pub nu_sigf: Vec<f64>,
    /// `Sigma_f` per zone \[cm^-1\], when provided explicitly.
    pub sigma_f: Option<Vec<f64>>,
    /// Physics mode.
    pub mode: Mode,
    /// Imposed `k_eff` (`fixed_k`, `driven`).
    pub k_eff: Option<f64>,
    /// Tolerance for the outer (eigenvalue) iteration.
    pub tol_k: f64,
    /// Maximum number of outer iterations.
    pub max_outer: usize,
    /// Initial `k_eff` guess for power iteration.
    pub k0: Option<f64>,
    /// Target thermal power \[kW\] (optional normalization).
    pub power_kw: Option<f64>,
    /// Energy released per fission \[MeV\].
    pub e_f_mev: f64,
    /// Neutrons per fission used to derive `Sigma_f` from `NU_SIGF`.
    pub nu: Option<f64>,
    /// Cross-sectional area \[cm^2\].
    pub area_cm2: f64,
    /// Optional presentation metadata.
    pub zone_names: Option<Vec<String>>,
    /// Spatial discretization scheme.
    pub spatial_scheme: Method,

    /// Total number of cells.
    pub ntc: usize,
    /// Total number of nodes (`ntc + 1`).
    pub ntp: usize,
    /// Total cross-section per cell.
    pub sigma_t_vec: Vec<f64>,
    /// Scattering cross-section per cell.
    pub sigma_s_vec: Vec<f64>,
    /// `nu * Sigma_f` per cell.
    pub nu_sigma_f_vec: Vec<f64>,
    /// `Sigma_f` per cell (zero when not provided).
    pub sigma_f_vec: Vec<f64>,
    /// External source per cell.
    pub q_ext_vec: Vec<f64>,
    /// Cell widths.
    pub dx_vec: Vec<f64>,
}

impl Config {
    /// Load and validate a configuration from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the file cannot be read or the contents
    /// are invalid.
    pub fn from_json_file(
        path: impl AsRef<Path>,
        epsilon: f64,
        max_iter: usize,
        forced_scheme: Option<Method>,
    ) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&text, epsilon, max_iter, forced_scheme)
    }

    /// Load and validate a configuration from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the JSON is malformed or invalid.
    pub fn from_json_str(
        text: &str,
        epsilon: f64,
        max_iter: usize,
        forced_scheme: Option<Method>,
    ) -> Result<Self, ConfigError> {
        let value: Value = serde_json::from_str(text)?;
        Self::from_value(&value, epsilon, max_iter, forced_scheme)
    }

    /// Validate a JSON value describing one configuration.
    ///
    /// Keys starting with `_` are treated as comments and ignored.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a field is missing or inconsistent.
    pub fn from_value(
        value: &Value,
        epsilon: f64,
        max_iter: usize,
        forced_scheme: Option<Method>,
    ) -> Result<Self, ConfigError> {
        let cleaned = strip_comment_keys(value);
        let input: ConfigInput = serde_json::from_value(cleaned)?;
        Self::from_input(input, epsilon, max_iter, forced_scheme)
    }

    fn from_input(
        input: ConfigInput,
        epsilon: f64,
        max_iter: usize,
        forced_scheme: Option<Method>,
    ) -> Result<Self, ConfigError> {
        let ConfigInput {
            num_regions,
            num_zones,
            nc,
            hr,
            izl,
            sct,
            scs,
            q,
            n,
            nu_sigf,
            sigma_f,
            mode,
            k_eff,
            tol_k,
            max_outer,
            k0,
            power_kw,
            e_f_mev,
            nu,
            area_cm2,
            zone_names,
            spatial_scheme,
            reflex_izq,
            reflex_der,
            bound_left,
            bound_right,
        } = input;

        // Required physical fields (same order as the Python reference).
        let nc_raw = nc.ok_or_else(|| missing("NC"))?;
        let hr_raw = hr.ok_or_else(|| missing("HR"))?;
        let izl_raw = izl.ok_or_else(|| missing("IZL"))?;
        let sct = sct.ok_or_else(|| missing("SCT"))?;
        let scs = scs.ok_or_else(|| missing("SCS"))?;
        let q = q.ok_or_else(|| missing("Q"))?;
        let n_raw = n.ok_or_else(|| missing("N"))?;

        let num_regions = num_regions.unwrap_or(0);
        if num_regions == 0 {
            return Err(ConfigError::invalid("num_regions must be >= 1."));
        }
        let num_zones = num_zones.unwrap_or(0);
        if num_zones == 0 {
            return Err(ConfigError::invalid("num_zones must be >= 1."));
        }
        if num_zones > num_regions {
            return Err(ConfigError::invalid(
                "Number of zones cannot exceed the number of regions.",
            ));
        }

        let nc = as_int_vec(&nc_raw, "NC", true)?;
        let hr = as_positive_vec(&hr_raw, "HR must contain finite values > 0.")?;
        let izl = as_int_vec(&izl_raw, "IZL", true)?;
        let sct = as_positive_vec(&sct, "SCT must contain finite values > 0.")?;
        let scs = as_non_negative_vec(&scs, "SCS must contain finite values >= 0.")?;
        let q = as_finite_vec(&q, "Q must contain finite values.")?;

        if n_raw % 2 != 0 {
            return Err(ConfigError::invalid(
                "Quadrature order must be an even number.",
            ));
        }
        let n = usize::try_from(n_raw)
            .map_err(|_| ConfigError::invalid("Quadrature order must be positive."))?;
        let n_half = n / 2;
        let (miu_m, omega_m) = quadrature::half_directions(n)?;

        let reflex_izq = reflex_izq.unwrap_or(false);
        let reflex_der = reflex_der.unwrap_or(false);

        let bound_left = boundary_vector(bound_left, n_half, "bound_left", !reflex_izq)?;
        let bound_right = boundary_vector(bound_right, n_half, "bound_right", !reflex_der)?;

        let scheme = match forced_scheme {
            Some(method) => method,
            None => Method::from_name(spatial_scheme.as_deref().unwrap_or("DD"))?,
        };

        let zone_names = match zone_names {
            None => None,
            Some(names) => {
                let names: Vec<String> = names.iter().map(|name| name.trim().to_string()).collect();
                if names.len() != num_zones {
                    return Err(ConfigError::invalid(format!(
                        "ZONE_NAMES must have {num_zones} elements"
                    )));
                }
                if names.iter().any(String::is_empty) {
                    return Err(ConfigError::invalid(
                        "ZONE_NAMES cannot contain empty names.",
                    ));
                }
                Some(names)
            }
        };

        // Dimension checks (same messages as the reference implementation).
        if nc.len() != num_regions {
            return Err(ConfigError::invalid(format!(
                "NC must have {num_regions} elements"
            )));
        }
        if hr.len() != num_regions {
            return Err(ConfigError::invalid(format!(
                "HR must have {num_regions} elements"
            )));
        }
        if izl.len() != num_regions {
            return Err(ConfigError::invalid(format!(
                "IZL must have {num_regions} elements"
            )));
        }
        if sct.len() != num_zones {
            return Err(ConfigError::invalid(format!(
                "SCT must have {num_zones} elements"
            )));
        }
        if scs.len() != num_zones {
            return Err(ConfigError::invalid(format!(
                "SCS must have {num_zones} elements"
            )));
        }
        if q.len() != num_regions {
            return Err(ConfigError::invalid(format!(
                "Q must have {num_regions} elements"
            )));
        }
        if !reflex_izq && bound_left.len() != n_half {
            return Err(ConfigError::invalid(format!(
                "bound_left must have {n_half} elements"
            )));
        }
        if !reflex_der && bound_right.len() != n_half {
            return Err(ConfigError::invalid(format!(
                "bound_right must have {n_half} elements"
            )));
        }
        if izl.iter().any(|zone| *zone < 1 || *zone > num_zones) {
            return Err(ConfigError::invalid(format!(
                "IZL must contain values between 1 and {num_zones}"
            )));
        }

        let mut config = Config {
            epsilon,
            max_iter,
            num_regions,
            num_zones,
            nc,
            hr,
            izl,
            q,
            sct,
            scs,
            n,
            n_half,
            miu_m,
            omega_m,
            reflex_izq,
            reflex_der,
            bound_left,
            bound_right,
            nu_sigf: nu_sigf.unwrap_or_else(|| vec![0.0; num_zones]),
            sigma_f,
            mode: Mode::FixedSource,
            k_eff,
            tol_k: tol_k.unwrap_or(1e-6),
            max_outer: max_outer
                .map(|value| usize::try_from(value).unwrap_or(0))
                .unwrap_or(200),
            k0,
            power_kw,
            e_f_mev: e_f_mev.unwrap_or(200.0),
            nu,
            area_cm2: area_cm2.unwrap_or(1.0),
            zone_names,
            spatial_scheme: scheme,
            ntc: 0,
            ntp: 0,
            sigma_t_vec: Vec::new(),
            sigma_s_vec: Vec::new(),
            nu_sigma_f_vec: Vec::new(),
            sigma_f_vec: Vec::new(),
            q_ext_vec: Vec::new(),
            dx_vec: Vec::new(),
        };

        config.validate_fission_config(mode.as_deref())?;
        config.prelim_calculations();

        Ok(config)
    }

    /// Spatial scheme selected for this run.
    pub const fn method(&self) -> Method {
        self.spatial_scheme
    }

    /// Effective configuration, serialized with the reference key names.
    pub fn as_dict(&self) -> Value {
        let mut map = Map::new();
        map.insert("num_regions".into(), json!(self.num_regions));
        map.insert("num_zones".into(), json!(self.num_zones));
        map.insert("NC".into(), json!(self.nc));
        map.insert("HR".into(), json!(self.hr));
        map.insert("IZL".into(), json!(self.izl));
        map.insert("SCT".into(), json!(self.sct));
        map.insert("SCS".into(), json!(self.scs));
        map.insert("NU_SIGF".into(), json!(self.nu_sigf));
        map.insert("SIGMA_F".into(), json!(self.sigma_f));
        map.insert("Q".into(), json!(self.q));
        map.insert("N".into(), json!(self.n));
        map.insert("SPATIAL_SCHEME".into(), json!(self.spatial_scheme.as_str()));
        map.insert("reflex_izq".into(), json!(self.reflex_izq));
        map.insert("reflex_der".into(), json!(self.reflex_der));
        map.insert("bound_left".into(), json!(self.bound_left));
        map.insert("bound_right".into(), json!(self.bound_right));
        map.insert("MODE".into(), json!(self.mode.as_str()));
        map.insert("K_EFF".into(), json!(self.k_eff));
        map.insert("TOL_K".into(), json!(self.tol_k));
        map.insert("MAX_OUTER".into(), json!(self.max_outer));
        map.insert("K0".into(), json!(self.k0));
        map.insert("POWER_KW".into(), json!(self.power_kw));
        map.insert("E_F_MEV".into(), json!(self.e_f_mev));
        map.insert("NU".into(), json!(self.nu));
        map.insert("AREA_CM2".into(), json!(self.area_cm2));
        if let Some(names) = &self.zone_names {
            map.insert("ZONE_NAMES".into(), json!(names));
        }
        Value::Object(map)
    }

    fn validate_fission_config(&mut self, raw_mode: Option<&str>) -> Result<(), ConfigError> {
        if self.nu_sigf.len() != self.num_zones {
            return Err(ConfigError::invalid(format!(
                "NU_SIGF must have {} elements",
                self.num_zones
            )));
        }
        if self.nu_sigf.iter().any(|value| *value < 0.0) {
            return Err(ConfigError::invalid("NU_SIGF must be >= 0 in all zones."));
        }

        if let Some(sigma_f) = &self.sigma_f {
            if sigma_f.len() != self.num_zones {
                return Err(ConfigError::invalid(format!(
                    "SIGMA_F must have {} elements",
                    self.num_zones
                )));
            }
            if sigma_f.iter().any(|value| *value < 0.0) {
                return Err(ConfigError::invalid("SIGMA_F must be >= 0 in all zones."));
            }
            if sigma_f
                .iter()
                .zip(&self.sct)
                .any(|(fission, total)| fission > total)
            {
                return Err(ConfigError::invalid("SIGMA_F cannot exceed SCT."));
            }
        }

        if let Some(k_eff) = self.k_eff
            && (k_eff.is_nan() || k_eff <= 0.0)
        {
            return Err(ConfigError::invalid("K_EFF must be > 0."));
        }
        if let Some(k0) = self.k0
            && (k0.is_nan() || k0 <= 0.0)
        {
            return Err(ConfigError::invalid("K0 must be > 0."));
        }
        if self.tol_k.is_nan() || self.tol_k <= 0.0 {
            return Err(ConfigError::invalid("TOL_K must be > 0."));
        }
        if self.max_outer == 0 {
            return Err(ConfigError::invalid("MAX_OUTER must be >= 1."));
        }

        let has_fission = self.nu_sigf.iter().any(|value| *value > 0.0);

        if let Some(power_kw) = self.power_kw {
            if power_kw.is_nan() || power_kw <= 0.0 {
                return Err(ConfigError::invalid("POWER_KW must be > 0."));
            }
            if !has_fission {
                return Err(ConfigError::invalid("POWER_KW requires NU_SIGF > 0."));
            }
            if self.e_f_mev.is_nan() || self.e_f_mev <= 0.0 {
                return Err(ConfigError::invalid("E_F_MEV must be > 0."));
            }
            if self.area_cm2.is_nan() || self.area_cm2 <= 0.0 {
                return Err(ConfigError::invalid("AREA_CM2 must be > 0."));
            }
            if let Some(nu) = self.nu
                && (nu.is_nan() || nu <= 0.0)
            {
                return Err(ConfigError::invalid("NU must be > 0."));
            }
            if self.sigma_f.is_none() && self.nu.is_none() {
                self.nu = Some(DEFAULT_NU);
            }
        }

        let mode = match raw_mode {
            None => {
                if !has_fission {
                    Mode::FixedSource
                } else if self.k_eff.is_none() {
                    Mode::Eigenvalue
                } else {
                    Mode::FixedK
                }
            }
            Some(name) => Mode::from_name(name)?,
        };

        let left_external = !self.reflex_izq && self.bound_left.iter().any(|value| *value != 0.0);
        let right_external = !self.reflex_der && self.bound_right.iter().any(|value| *value != 0.0);
        let external_source =
            self.q.iter().any(|value| *value != 0.0) || left_external || right_external;

        match mode {
            Mode::FixedSource => {
                if has_fission {
                    return Err(ConfigError::invalid(
                        "MODE='fixed_source' exige NU_SIGF = 0. Usa MODE='eigenvalue' o MODE='fixed_k'.",
                    ));
                }
                if self.k_eff.is_some() {
                    return Err(ConfigError::invalid(
                        "MODE='fixed_source' does not use K_EFF.",
                    ));
                }
            }
            Mode::Eigenvalue => {
                if !has_fission {
                    return Err(ConfigError::invalid(
                        "MODE='eigenvalue' requires NU_SIGF > 0.",
                    ));
                }
                if self.k_eff.is_some() {
                    return Err(ConfigError::invalid(
                        "MODE='eigenvalue' calcula k_eff; no impongas K_EFF. Usa MODE='fixed_k' si quieres fijarlo.",
                    ));
                }
                if external_source {
                    return Err(ConfigError::invalid(
                        "MODE='eigenvalue' solves the homogeneous problem (Q=0 and zero incoming flux). With a source use MODE='fixed_k' o MODE='driven'.",
                    ));
                }
            }
            Mode::FixedK => {
                if !has_fission {
                    return Err(ConfigError::invalid("MODE='fixed_k' requires NU_SIGF > 0."));
                }
                if self.k_eff.is_none() {
                    return Err(ConfigError::invalid("MODE='fixed_k' requires K_EFF > 0."));
                }
            }
            Mode::Driven => {
                if !has_fission {
                    return Err(ConfigError::invalid("MODE='driven' requires NU_SIGF > 0."));
                }
                match self.k_eff {
                    None => self.k_eff = Some(1.0),
                    Some(k_eff) => {
                        if (k_eff - 1.0).abs() > 1e-12 {
                            return Err(ConfigError::invalid(
                                "MODE='driven' uses the physical k = 1. To scale fission use MODE='fixed_k'.",
                            ));
                        }
                    }
                }
                if !external_source {
                    return Err(ConfigError::invalid(
                        "MODE='driven' requires Q != 0 or non-zero incoming flux.",
                    ));
                }
            }
        }

        self.mode = mode;
        Ok(())
    }

    fn prelim_calculations(&mut self) {
        self.ntc = self.nc.iter().sum();
        self.ntp = self.ntc + 1;

        self.sigma_t_vec = vec![0.0; self.ntc];
        self.sigma_s_vec = vec![0.0; self.ntc];
        self.nu_sigma_f_vec = vec![0.0; self.ntc];
        self.sigma_f_vec = vec![0.0; self.ntc];
        self.q_ext_vec = vec![0.0; self.ntc];
        self.dx_vec = vec![0.0; self.ntc];

        let mut idx = 0;
        for region in 0..self.num_regions {
            let n_cells = self.nc[region];
            let zone = self.izl[region] - 1;
            let dx = self.hr[region] / n_cells as f64;

            for cell in idx..idx + n_cells {
                self.sigma_t_vec[cell] = self.sct[zone];
                self.sigma_s_vec[cell] = self.scs[zone];
                self.nu_sigma_f_vec[cell] = self.nu_sigf[zone];
                if let Some(sigma_f) = &self.sigma_f {
                    self.sigma_f_vec[cell] = sigma_f[zone];
                }
                self.q_ext_vec[cell] = self.q[region];
                self.dx_vec[cell] = dx;
            }
            idx += n_cells;
        }
    }
}

fn missing(field: &str) -> ConfigError {
    ConfigError::invalid(format!("Missing required field: '{field}'"))
}

fn strip_comment_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !key.starts_with('_'))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn as_int_vec(values: &[f64], name: &str, positive: bool) -> Result<Vec<usize>, ConfigError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(ConfigError::invalid(format!(
            "{name} contains non-finite values."
        )));
    }
    if values.iter().any(|value| *value != value.floor()) {
        return Err(ConfigError::invalid(format!(
            "{name} must contain integers."
        )));
    }
    let ints: Vec<usize> = values.iter().map(|value| *value as usize).collect();
    if positive && ints.contains(&0) {
        return Err(ConfigError::invalid(format!(
            "{name} must contain integers > 0."
        )));
    }
    Ok(ints)
}

fn as_finite_vec(values: &[f64], message: &str) -> Result<Vec<f64>, ConfigError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(ConfigError::invalid(message));
    }
    Ok(values.to_vec())
}

fn as_positive_vec(values: &[f64], message: &str) -> Result<Vec<f64>, ConfigError> {
    if values
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(ConfigError::invalid(message));
    }
    Ok(values.to_vec())
}

fn as_non_negative_vec(values: &[f64], message: &str) -> Result<Vec<f64>, ConfigError> {
    if values
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(ConfigError::invalid(message));
    }
    Ok(values.to_vec())
}

fn boundary_vector(
    value: Option<BoundaryValue>,
    n_half: usize,
    name: &str,
    strict: bool,
) -> Result<Vec<f64>, ConfigError> {
    let values = match value {
        None => vec![0.0; n_half],
        Some(BoundaryValue::Scalar(value)) => vec![value; n_half],
        Some(BoundaryValue::Vector(values)) => {
            if values.len() == 1 {
                vec![values[0]; n_half]
            } else {
                values
            }
        }
    };
    if strict && values.len() != n_half {
        return Err(ConfigError::invalid(format!(
            "{name} must have {n_half} elements or one scalar; got {}",
            values.len()
        )));
    }
    Ok(values)
}

/// Raw JSON shape accepted for one configuration.
#[derive(Debug, Default, Deserialize)]
struct ConfigInput {
    num_regions: Option<usize>,
    num_zones: Option<usize>,
    #[serde(rename = "NC")]
    nc: Option<Vec<f64>>,
    #[serde(rename = "HR")]
    hr: Option<Vec<f64>>,
    #[serde(rename = "IZL")]
    izl: Option<Vec<f64>>,
    #[serde(rename = "SCT")]
    sct: Option<Vec<f64>>,
    #[serde(rename = "SCS")]
    scs: Option<Vec<f64>>,
    #[serde(rename = "Q")]
    q: Option<Vec<f64>>,
    #[serde(rename = "N")]
    n: Option<i64>,
    #[serde(rename = "NU_SIGF")]
    nu_sigf: Option<Vec<f64>>,
    #[serde(rename = "SIGMA_F")]
    sigma_f: Option<Vec<f64>>,
    #[serde(rename = "MODE")]
    mode: Option<String>,
    #[serde(rename = "K_EFF")]
    k_eff: Option<f64>,
    #[serde(rename = "TOL_K")]
    tol_k: Option<f64>,
    #[serde(rename = "MAX_OUTER")]
    max_outer: Option<i64>,
    #[serde(rename = "K0")]
    k0: Option<f64>,
    #[serde(rename = "POWER_KW")]
    power_kw: Option<f64>,
    #[serde(rename = "E_F_MEV")]
    e_f_mev: Option<f64>,
    #[serde(rename = "NU")]
    nu: Option<f64>,
    #[serde(rename = "AREA_CM2")]
    area_cm2: Option<f64>,
    #[serde(rename = "ZONE_NAMES")]
    zone_names: Option<Vec<String>>,
    #[serde(rename = "SPATIAL_SCHEME")]
    spatial_scheme: Option<String>,
    #[serde(default, deserialize_with = "de_boolish")]
    reflex_izq: Option<bool>,
    #[serde(default, deserialize_with = "de_boolish")]
    reflex_der: Option<bool>,
    bound_left: Option<BoundaryValue>,
    bound_right: Option<BoundaryValue>,
}

/// Boundary value that may be a scalar, a one-element list or a full vector.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BoundaryValue {
    Scalar(f64),
    Vector(Vec<f64>),
}

/// Booleans are also accepted as integers (`0` / `1`), as in the old inputs.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Boolish {
    Bool(bool),
    Int(i64),
}

fn de_boolish<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Boolish>::deserialize(deserializer)?;
    Ok(value.map(|value| match value {
        Boolish::Bool(flag) => flag,
        Boolish::Int(number) => number != 0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{
        "num_regions": 1,
        "num_zones": 1,
        "NC": [10],
        "HR": [10.0],
        "IZL": [1],
        "SCT": [1.0],
        "SCS": [0.9],
        "Q": [0.0],
        "N": 4,
        "NU_SIGF": [0.0],
        "MODE": "fixed_source",
        "bound_left": [0, 0],
        "bound_right": [0, 0]
    }"#;

    fn base() -> Config {
        Config::from_json_str(BASE, 1e-5, 2000, None).expect("valid base config")
    }

    #[test]
    fn base_config_is_vectorised_per_cell() {
        let config = base();
        assert_eq!(config.ntc, 10);
        assert_eq!(config.ntp, 11);
        assert_eq!(config.dx_vec.len(), 10);
        assert!((config.dx_vec[0] - 1.0).abs() < 1e-15);
        assert_eq!(config.mode, Mode::FixedSource);
    }

    #[test]
    fn scalar_boundaries_expand() {
        let config = Config::from_json_str(
            r#"{"num_regions":1,"num_zones":1,"NC":[10],"HR":[10.0],"IZL":[1],
                "SCT":[1.0],"SCS":[0.9],"Q":[0.0],"N":8,"NU_SIGF":[0.0],
                "MODE":"fixed_source","bound_left":0,"bound_right":1}"#,
            1e-5,
            2000,
            None,
        )
        .expect("valid");
        assert_eq!(config.bound_left, vec![0.0; 4]);
        assert_eq!(config.bound_right, vec![1.0; 4]);
    }

    #[test]
    fn bad_boundary_length_is_rejected() {
        let error = Config::from_json_str(
            r#"{"num_regions":1,"num_zones":1,"NC":[10],"HR":[10.0],"IZL":[1],
                "SCT":[1.0],"SCS":[0.9],"Q":[0.0],"N":8,"NU_SIGF":[0.0],
                "MODE":"fixed_source","bound_left":[0,1],"bound_right":[0,0,0,0]}"#,
            1e-5,
            2000,
            None,
        )
        .expect_err("bad boundary");
        assert_eq!(
            error.to_string(),
            "bound_left must have 4 elements or one scalar; got 2"
        );
    }

    #[test]
    fn comment_keys_are_ignored() {
        let text = BASE.replace("\"num_regions\"", "\"_comment\": 1, \"num_regions\"");
        Config::from_json_str(&text, 1e-5, 2000, None).expect("comments ignored");
    }

    #[test]
    fn integer_booleans_are_accepted() {
        let text = BASE.replace(
            "\"bound_left\": [0, 0]",
            "\"reflex_izq\": 1, \"bound_left\": [0, 0]",
        );
        let config = Config::from_json_str(&text, 1e-5, 2000, None).expect("valid");
        assert!(config.reflex_izq);
    }

    #[test]
    fn odd_quadrature_is_rejected() {
        let text = BASE.replace("\"N\": 4", "\"N\": 3");
        let error = Config::from_json_str(&text, 1e-5, 2000, None).expect_err("odd N");
        assert_eq!(
            error.to_string(),
            "Quadrature order must be an even number."
        );
    }

    #[test]
    fn fission_inference_and_validation() {
        let text = BASE
            .replace("\"NU_SIGF\": [0.0]", "\"NU_SIGF\": [0.11]")
            .replace("\"MODE\": \"fixed_source\"", "\"MODE\": \"eigenvalue\"");
        let config = Config::from_json_str(&text, 1e-5, 2000, None).expect("eigenvalue");
        assert_eq!(config.mode, Mode::Eigenvalue);

        let bad = BASE.replace("\"NU_SIGF\": [0.0]", "\"NU_SIGF\": [0.11]");
        let error =
            Config::from_json_str(&bad, 1e-5, 2000, None).expect_err("fixed_source with fission");
        assert!(error.to_string().contains("NU_SIGF = 0"));
    }

    #[test]
    fn as_dict_round_trip() {
        let config = base();
        let dict = config.as_dict();
        let restored = Config::from_value(&dict, 1e-5, 2000, None).expect("round trip");
        assert_eq!(restored.as_dict(), dict);
    }

    #[test]
    fn forced_scheme_wins() {
        let config =
            Config::from_json_str(BASE, 1e-5, 2000, Some(Method::Step)).expect("forced scheme");
        assert_eq!(config.method(), Method::Step);
        assert_eq!(config.as_dict()["SPATIAL_SCHEME"], json!("STEP"));
    }
}
