//! Editable configuration model for the TUI.
//!
//! Values are kept as text so the user can type either numbers or placeholder
//! names (`kk`, `mm`). `to_value` converts back to the JSON schema and
//! `validate` runs the same rules as the CLI.

use serde_json::{Map, Value, json};

use crate::config::{Config, RunOverrides};
use crate::placeholder::detect_placeholders;

/// Quadrature orders offered by the editor.
pub const QUADRATURE: [usize; 4] = [2, 4, 6, 8];
/// Spatial schemes offered by the editor.
pub const SCHEMES: [&str; 3] = ["DD", "STEP", "SC"];
/// Physics modes offered by the editor.
pub const MODES: [&str; 4] = ["fixed_source", "fixed_k", "driven", "eigenvalue"];

/// Identifies one editable row of the configuration form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Number of spatial regions.
    NumRegions,
    /// Number of material zones.
    NumZones,
    /// Cells in one region.
    RegionCells(usize),
    /// Thickness of one region.
    RegionThickness(usize),
    /// Material zone of one region.
    RegionZone(usize),
    /// External source of one region.
    RegionSource(usize),
    /// Total cross-section of one zone.
    ZoneSigmaT(usize),
    /// Scattering cross-section of one zone.
    ZoneSigmaS(usize),
    /// nu*Sigma_f of one zone.
    ZoneNuSigf(usize),
    /// Sigma_f of one zone.
    ZoneSigmaF(usize),
    /// Quadrature order.
    Quadrature,
    /// Spatial scheme.
    Scheme,
    /// Reflective left boundary.
    ReflexLeft,
    /// Reflective right boundary.
    ReflexRight,
    /// Incoming left boundary values.
    BoundLeft,
    /// Incoming right boundary values.
    BoundRight,
    /// Physics mode.
    Mode,
    /// Imposed k_eff.
    KEff,
    /// Outer tolerance.
    TolK,
    /// Outer iteration cap.
    MaxOuter,
    /// Initial k_eff guess.
    K0,
    /// Target power.
    PowerKw,
    /// Energy per fission.
    EfMev,
    /// Neutrons per fission.
    Nu,
    /// Cross-sectional area.
    AreaCm2,
    /// Scalar-flux convergence tolerance.
    Epsilon,
    /// Maximum number of flux updates.
    MaxIter,
    /// Optional zone names.
    ZoneNames,
}

/// One row of the form.
#[derive(Debug, Clone)]
pub struct Field {
    /// Section shown in the first column.
    pub section: &'static str,
    /// Parameter name.
    pub label: String,
    /// Current text value.
    pub value: String,
    /// Kind of the row.
    pub kind: FieldKind,
}

/// Result of validating the model.
#[derive(Debug, Clone)]
pub enum Validation {
    /// The configuration is complete and satisfies the solver rules.
    Valid(Box<Config>),
    /// The configuration still contains placeholder names.
    Template(Vec<String>),
    /// The configuration is invalid; the string is the first error.
    Invalid(String),
}

/// Configuration being edited in the TUI.
#[derive(Debug, Clone)]
pub struct ConfigModel {
    /// Number of regions.
    pub num_regions: usize,
    /// Number of zones.
    pub num_zones: usize,
    /// Cells per region.
    pub nc: Vec<String>,
    /// Region thickness \[cm\].
    pub hr: Vec<String>,
    /// Material zone per region (1-based).
    pub izl: Vec<String>,
    /// `Sigma_t` per zone.
    pub sct: Vec<String>,
    /// `Sigma_s` per zone.
    pub scs: Vec<String>,
    /// `nu*Sigma_f` per zone.
    pub nu_sigf: Vec<String>,
    /// `Sigma_f` per zone (may be empty).
    pub sigma_f: Vec<String>,
    /// External source per region.
    pub q: Vec<String>,
    /// Quadrature order.
    pub n: usize,
    /// Spatial scheme index.
    pub scheme: usize,
    /// Reflective left boundary.
    pub reflex_izq: bool,
    /// Reflective right boundary.
    pub reflex_der: bool,
    /// Incoming flux at the left boundary.
    pub bound_left: Vec<String>,
    /// Incoming flux at the right boundary.
    pub bound_right: Vec<String>,
    /// Physics mode index.
    pub mode: usize,
    /// Imposed `k_eff`.
    pub k_eff: String,
    /// Outer tolerance.
    pub tol_k: String,
    /// Outer iteration cap.
    pub max_outer: String,
    /// Initial `k_eff` guess.
    pub k0: String,
    /// Target power \[kW\].
    pub power_kw: String,
    /// Energy per fission \[MeV\].
    pub ef_mev: String,
    /// Neutrons per fission.
    pub nu: String,
    /// Cross-sectional area \[cm²\].
    pub area_cm2: String,
    /// Scalar-flux tolerance.
    pub epsilon: String,
    /// Maximum number of flux updates.
    pub max_iter: String,
    /// Optional zone names.
    pub zone_names: Vec<String>,
}

impl Default for ConfigModel {
    fn default() -> Self {
        ConfigModel {
            num_regions: 1,
            num_zones: 1,
            nc: vec!["20".into()],
            hr: vec!["20".into()],
            izl: vec!["1".into()],
            sct: vec!["1.0".into()],
            scs: vec!["0.9".into()],
            nu_sigf: vec!["0.0".into()],
            sigma_f: vec![String::new()],
            q: vec!["0.0".into()],
            n: 8,
            scheme: 0,
            reflex_izq: false,
            reflex_der: false,
            bound_left: vec!["0".into(); 4],
            bound_right: vec!["0".into(); 4],
            mode: 0,
            k_eff: String::new(),
            tol_k: "1e-6".into(),
            max_outer: "200".into(),
            k0: String::new(),
            power_kw: String::new(),
            ef_mev: "200".into(),
            nu: String::new(),
            area_cm2: "1".into(),
            epsilon: "1e-5".into(),
            max_iter: "2000".into(),
            zone_names: Vec::new(),
        }
    }
}

impl ConfigModel {
    /// Build the current form rows.
    pub fn fields(&self) -> Vec<Field> {
        let mut fields = Vec::new();
        fields.push(Field {
            section: "Geometry",
            label: "Regions".into(),
            value: self.num_regions.to_string(),
            kind: FieldKind::NumRegions,
        });
        fields.push(Field {
            section: "Geometry",
            label: "Zones".into(),
            value: self.num_zones.to_string(),
            kind: FieldKind::NumZones,
        });
        for region in 0..self.num_regions {
            fields.push(Field {
                section: "Geometry",
                label: format!("NC region {}", region + 1),
                value: get(&self.nc, region),
                kind: FieldKind::RegionCells(region),
            });
            fields.push(Field {
                section: "Geometry",
                label: format!("HR region {} [cm]", region + 1),
                value: get(&self.hr, region),
                kind: FieldKind::RegionThickness(region),
            });
            fields.push(Field {
                section: "Geometry",
                label: format!("IZL region {}", region + 1),
                value: get(&self.izl, region),
                kind: FieldKind::RegionZone(region),
            });
            fields.push(Field {
                section: "Source",
                label: format!("Q region {}", region + 1),
                value: get(&self.q, region),
                kind: FieldKind::RegionSource(region),
            });
        }
        for zone in 0..self.num_zones {
            fields.push(Field {
                section: "Materials",
                label: format!("SCT zone {}", zone + 1),
                value: get(&self.sct, zone),
                kind: FieldKind::ZoneSigmaT(zone),
            });
            fields.push(Field {
                section: "Materials",
                label: format!("SCS zone {}", zone + 1),
                value: get(&self.scs, zone),
                kind: FieldKind::ZoneSigmaS(zone),
            });
            fields.push(Field {
                section: "Materials",
                label: format!("NU_SIGF zone {}", zone + 1),
                value: get(&self.nu_sigf, zone),
                kind: FieldKind::ZoneNuSigf(zone),
            });
            fields.push(Field {
                section: "Materials",
                label: format!("SIGMA_F zone {}", zone + 1),
                value: get(&self.sigma_f, zone),
                kind: FieldKind::ZoneSigmaF(zone),
            });
        }
        fields.push(Field {
            section: "Quadrature",
            label: "N".into(),
            value: self.n.to_string(),
            kind: FieldKind::Quadrature,
        });
        fields.push(Field {
            section: "Quadrature",
            label: "Scheme".into(),
            value: SCHEMES[self.scheme].into(),
            kind: FieldKind::Scheme,
        });
        fields.push(Field {
            section: "Boundaries",
            label: "Reflective left".into(),
            value: yes_no(self.reflex_izq),
            kind: FieldKind::ReflexLeft,
        });
        fields.push(Field {
            section: "Boundaries",
            label: "Reflective right".into(),
            value: yes_no(self.reflex_der),
            kind: FieldKind::ReflexRight,
        });
        fields.push(Field {
            section: "Boundaries",
            label: "Incoming left".into(),
            value: self.bound_left.join(" "),
            kind: FieldKind::BoundLeft,
        });
        fields.push(Field {
            section: "Boundaries",
            label: "Incoming right".into(),
            value: self.bound_right.join(" "),
            kind: FieldKind::BoundRight,
        });
        fields.push(Field {
            section: "Physics",
            label: "MODE".into(),
            value: MODES[self.mode].into(),
            kind: FieldKind::Mode,
        });
        fields.push(Field {
            section: "Physics",
            label: "K_EFF".into(),
            value: self.k_eff.clone(),
            kind: FieldKind::KEff,
        });
        fields.push(Field {
            section: "Physics",
            label: "TOL_K".into(),
            value: self.tol_k.clone(),
            kind: FieldKind::TolK,
        });
        fields.push(Field {
            section: "Physics",
            label: "MAX_OUTER".into(),
            value: self.max_outer.clone(),
            kind: FieldKind::MaxOuter,
        });
        fields.push(Field {
            section: "Physics",
            label: "K0".into(),
            value: self.k0.clone(),
            kind: FieldKind::K0,
        });
        fields.push(Field {
            section: "Power",
            label: "POWER_KW".into(),
            value: self.power_kw.clone(),
            kind: FieldKind::PowerKw,
        });
        fields.push(Field {
            section: "Power",
            label: "E_F_MEV".into(),
            value: self.ef_mev.clone(),
            kind: FieldKind::EfMev,
        });
        fields.push(Field {
            section: "Power",
            label: "NU".into(),
            value: self.nu.clone(),
            kind: FieldKind::Nu,
        });
        fields.push(Field {
            section: "Power",
            label: "AREA_CM2".into(),
            value: self.area_cm2.clone(),
            kind: FieldKind::AreaCm2,
        });
        fields.push(Field {
            section: "Convergence",
            label: "epsilon".into(),
            value: self.epsilon.clone(),
            kind: FieldKind::Epsilon,
        });
        fields.push(Field {
            section: "Convergence",
            label: "max_iter".into(),
            value: self.max_iter.clone(),
            kind: FieldKind::MaxIter,
        });
        fields.push(Field {
            section: "Metadata",
            label: "ZONE_NAMES".into(),
            value: self.zone_names.join(", "),
            kind: FieldKind::ZoneNames,
        });
        fields
    }

    /// Apply a textual value to one row.
    pub fn set_field(&mut self, kind: FieldKind, text: &str) {
        let text = text.trim();
        match kind {
            FieldKind::NumRegions => {
                if let Ok(value) = text.parse::<usize>() {
                    self.resize_regions(value.max(1));
                }
            }
            FieldKind::NumZones => {
                if let Ok(value) = text.parse::<usize>() {
                    self.resize_zones(value.max(1));
                }
            }
            FieldKind::RegionCells(index) => put(&mut self.nc, index, text),
            FieldKind::RegionThickness(index) => put(&mut self.hr, index, text),
            FieldKind::RegionZone(index) => put(&mut self.izl, index, text),
            FieldKind::RegionSource(index) => put(&mut self.q, index, text),
            FieldKind::ZoneSigmaT(index) => put(&mut self.sct, index, text),
            FieldKind::ZoneSigmaS(index) => put(&mut self.scs, index, text),
            FieldKind::ZoneNuSigf(index) => put(&mut self.nu_sigf, index, text),
            FieldKind::ZoneSigmaF(index) => put(&mut self.sigma_f, index, text),
            FieldKind::Quadrature => {
                if let Ok(value) = text.parse::<usize>()
                    && QUADRATURE.contains(&value)
                {
                    self.n = value;
                    self.resize_boundaries();
                }
            }
            FieldKind::Scheme => {
                if let Some(index) = SCHEMES
                    .iter()
                    .position(|scheme| scheme.eq_ignore_ascii_case(text))
                {
                    self.scheme = index;
                }
            }
            FieldKind::Mode => {
                if let Some(index) = MODES
                    .iter()
                    .position(|mode| mode.eq_ignore_ascii_case(text))
                {
                    self.mode = index;
                }
            }
            FieldKind::ReflexLeft => self.reflex_izq = truthy(text),
            FieldKind::ReflexRight => self.reflex_der = truthy(text),
            FieldKind::BoundLeft => self.bound_left = tokens(text),
            FieldKind::BoundRight => self.bound_right = tokens(text),
            FieldKind::KEff => self.k_eff = text.to_string(),
            FieldKind::TolK => self.tol_k = text.to_string(),
            FieldKind::MaxOuter => self.max_outer = text.to_string(),
            FieldKind::K0 => self.k0 = text.to_string(),
            FieldKind::PowerKw => self.power_kw = text.to_string(),
            FieldKind::EfMev => self.ef_mev = text.to_string(),
            FieldKind::Nu => self.nu = text.to_string(),
            FieldKind::AreaCm2 => self.area_cm2 = text.to_string(),
            FieldKind::Epsilon => self.epsilon = text.to_string(),
            FieldKind::MaxIter => self.max_iter = text.to_string(),
            FieldKind::ZoneNames => {
                self.zone_names = text
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect();
            }
        }
    }

    /// Toggle or cycle a row without opening the editor.
    pub fn cycle_field(&mut self, kind: FieldKind, delta: i32) {
        match kind {
            FieldKind::Quadrature => {
                let index = (self.quadrature_index() as i32 + delta)
                    .rem_euclid(QUADRATURE.len() as i32) as usize;
                self.n = QUADRATURE[index];
                self.resize_boundaries();
            }
            FieldKind::Scheme => {
                self.scheme =
                    (self.scheme as i32 + delta).rem_euclid(SCHEMES.len() as i32) as usize;
            }
            FieldKind::Mode => {
                self.mode = (self.mode as i32 + delta).rem_euclid(MODES.len() as i32) as usize;
            }
            FieldKind::NumRegions => {
                let next = (self.num_regions as i32 + delta).max(1) as usize;
                self.resize_regions(next);
            }
            FieldKind::NumZones => {
                let next = (self.num_zones as i32 + delta).max(1) as usize;
                self.resize_zones(next);
            }
            FieldKind::ReflexLeft => self.reflex_izq = !self.reflex_izq,
            FieldKind::ReflexRight => self.reflex_der = !self.reflex_der,
            _ => {}
        }
    }

    /// Convert the form to the configuration JSON schema.
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("num_regions".into(), json!(self.num_regions));
        map.insert("num_zones".into(), json!(self.num_zones));
        map.insert("NC".into(), numbers(&self.nc));
        map.insert("HR".into(), numbers(&self.hr));
        map.insert("IZL".into(), numbers(&self.izl));
        map.insert("SCT".into(), numbers(&self.sct));
        map.insert("SCS".into(), numbers(&self.scs));
        map.insert("NU_SIGF".into(), numbers(&self.nu_sigf));
        if self.sigma_f.iter().any(|value| !value.trim().is_empty()) {
            map.insert("SIGMA_F".into(), numbers(&self.sigma_f));
        }
        map.insert("Q".into(), numbers(&self.q));
        map.insert("N".into(), json!(self.n));
        map.insert("SPATIAL_SCHEME".into(), json!(SCHEMES[self.scheme]));
        map.insert("reflex_izq".into(), json!(self.reflex_izq));
        map.insert("reflex_der".into(), json!(self.reflex_der));
        map.insert("bound_left".into(), numbers(&self.bound_left));
        map.insert("bound_right".into(), numbers(&self.bound_right));
        map.insert("MODE".into(), json!(MODES[self.mode]));
        insert_optional(&mut map, "K_EFF", &self.k_eff);
        insert_optional(&mut map, "TOL_K", &self.tol_k);
        insert_optional(&mut map, "MAX_OUTER", &self.max_outer);
        insert_optional(&mut map, "K0", &self.k0);
        insert_optional(&mut map, "POWER_KW", &self.power_kw);
        insert_optional(&mut map, "E_F_MEV", &self.ef_mev);
        insert_optional(&mut map, "NU", &self.nu);
        insert_optional(&mut map, "AREA_CM2", &self.area_cm2);
        insert_optional(&mut map, "epsilon", &self.epsilon);
        insert_optional(&mut map, "max_iter", &self.max_iter);
        if !self.zone_names.is_empty() {
            map.insert("ZONE_NAMES".into(), json!(self.zone_names));
        }
        Value::Object(map)
    }

    /// Validate the form with the solver rules.
    pub fn validate(&self) -> Validation {
        let value = self.to_value();
        let placeholders = detect_placeholders(&value);
        if !placeholders.is_empty() {
            return Validation::Template(placeholders.into_iter().collect());
        }
        match Config::from_value_with(&value, RunOverrides::default(), None) {
            Ok(config) => Validation::Valid(Box::new(config)),
            Err(error) => Validation::Invalid(error.to_string()),
        }
    }

    /// Build the form from a configuration object.
    ///
    /// Tolerant by design: placeholders are kept as text, `null` optionals are
    /// left blank and missing arrays are padded with defaults.
    ///
    /// # Errors
    ///
    /// Returns a message when the value is not a JSON object.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "configuration must be a JSON object".to_string())?;

        let nc = text_list(object.get("NC"));
        let hr = text_list(object.get("HR"));
        let izl = text_list(object.get("IZL"));
        let q = text_list(object.get("Q"));
        let sct = text_list(object.get("SCT"));
        let scs = text_list(object.get("SCS"));
        let nu_sigf = text_list(object.get("NU_SIGF"));
        let sigma_f = text_list(object.get("SIGMA_F"));
        let names = text_list(object.get("ZONE_NAMES"));

        let regions = object
            .get("num_regions")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| nc.len().max(hr.len()).max(izl.len()).max(q.len()).max(1))
            .max(1);
        let zones = object
            .get("num_zones")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| {
                sct.len()
                    .max(scs.len())
                    .max(nu_sigf.len())
                    .max(sigma_f.len())
                    .max(names.len())
                    .max(1)
            })
            .max(1);

        let mut model = ConfigModel {
            num_regions: regions,
            num_zones: zones,
            nc: fit(nc, regions, "10"),
            hr: fit(hr, regions, "10"),
            izl: fit(izl, regions, "1"),
            q: fit(q, regions, "0.0"),
            sct: fit(sct, zones, "1.0"),
            scs: fit(scs, zones, "0.9"),
            nu_sigf: fit(nu_sigf, zones, "0.0"),
            sigma_f: fit(sigma_f, zones, ""),
            ..ConfigModel::default()
        };

        if !names.is_empty() {
            let mut names = fit(names, zones, "");
            for (index, name) in names.iter_mut().enumerate() {
                if name.is_empty() {
                    *name = format!("Zone {}", index + 1);
                }
            }
            model.zone_names = names;
        }

        if let Some(n) = object.get("N").and_then(Value::as_u64) {
            let n = n as usize;
            if QUADRATURE.contains(&n) {
                model.n = n;
            }
        }
        if let Some(name) = object.get("SPATIAL_SCHEME").and_then(Value::as_str)
            && let Some(index) = SCHEMES
                .iter()
                .position(|scheme| scheme.eq_ignore_ascii_case(name))
        {
            model.scheme = index;
        }
        if let Some(name) = object.get("MODE").and_then(Value::as_str)
            && let Some(index) = MODES
                .iter()
                .position(|mode| mode.eq_ignore_ascii_case(name))
        {
            model.mode = index;
        }

        model.reflex_izq = boolish(object.get("reflex_izq")).unwrap_or(false);
        model.reflex_der = boolish(object.get("reflex_der")).unwrap_or(false);

        let half = model.n / 2;
        let left = text_list(object.get("bound_left"));
        let right = text_list(object.get("bound_right"));
        model.bound_left = if left.is_empty() {
            vec!["0".into(); half]
        } else {
            left
        };
        model.bound_right = if right.is_empty() {
            vec!["0".into(); half]
        } else {
            right
        };

        model.k_eff = optional_text(object.get("K_EFF"));
        model.k0 = optional_text(object.get("K0"));
        model.power_kw = optional_text(object.get("POWER_KW"));
        model.nu = optional_text(object.get("NU"));
        model.tol_k = default_text(object.get("TOL_K"), "1e-6");
        model.max_outer = default_text(object.get("MAX_OUTER"), "200");
        model.ef_mev = default_text(object.get("E_F_MEV"), "200");
        model.area_cm2 = default_text(object.get("AREA_CM2"), "1");
        model.epsilon = default_text(object.get("epsilon"), "1e-5");
        model.max_iter = default_text(object.get("max_iter"), "2000");

        Ok(model)
    }

    /// Total slab thickness \[cm\] from the current text values.
    pub fn total_thickness(&self) -> f64 {
        self.hr
            .iter()
            .filter_map(|value| value.trim().parse::<f64>().ok())
            .filter(|value| *value > 0.0)
            .sum()
    }

    fn resize_regions(&mut self, regions: usize) {
        resize(&mut self.nc, regions, "10");
        resize(&mut self.hr, regions, "10");
        resize(&mut self.izl, regions, "1");
        resize(&mut self.q, regions, "0.0");
        self.num_regions = regions;
        if self.num_zones > regions {
            self.resize_zones(regions);
        }
    }

    fn resize_zones(&mut self, zones: usize) {
        resize(&mut self.sct, zones, "1.0");
        resize(&mut self.scs, zones, "0.9");
        resize(&mut self.nu_sigf, zones, "0.0");
        resize(&mut self.sigma_f, zones, "");
        let had_names = self.zone_names.iter().any(|name| !name.is_empty());
        resize(&mut self.zone_names, zones, "");
        if had_names {
            for (index, name) in self.zone_names.iter_mut().enumerate() {
                if name.is_empty() {
                    *name = format!("Zone {}", index + 1);
                }
            }
        } else {
            self.zone_names.clear();
        }
        self.num_zones = zones;
    }

    fn quadrature_index(&self) -> usize {
        QUADRATURE
            .iter()
            .position(|order| *order == self.n)
            .unwrap_or(QUADRATURE.len() - 1)
    }

    fn resize_boundaries(&mut self) {
        let half = self.n / 2;
        resize(&mut self.bound_left, half, "0");
        resize(&mut self.bound_right, half, "0");
    }
}

fn text_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().map(value_text).collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(other) => vec![value_text(other)],
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => String::new(),
    }
}

fn optional_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Null) | None => String::new(),
        Some(other) => value_text(other),
    }
}

fn default_text(value: Option<&Value>, default: &str) -> String {
    let text = optional_text(value);
    if text.is_empty() {
        default.to_string()
    } else {
        text
    }
}

fn boolish(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(flag)) => Some(*flag),
        Some(Value::Number(number)) => Some(number.as_i64().unwrap_or(0) != 0),
        Some(Value::String(text)) => Some(matches!(
            text.to_ascii_lowercase().as_str(),
            "true" | "yes" | "y" | "1"
        )),
        _ => None,
    }
}

fn fit(mut values: Vec<String>, len: usize, default: &str) -> Vec<String> {
    values.resize(len, default.to_string());
    values
}

fn get(values: &[String], index: usize) -> String {
    values.get(index).cloned().unwrap_or_default()
}

fn put(values: &mut Vec<String>, index: usize, text: &str) {
    if index >= values.len() {
        values.resize(index + 1, String::new());
    }
    values[index] = text.to_string();
}

fn resize(values: &mut Vec<String>, len: usize, default: &str) {
    values.resize(len, default.to_string());
}

fn tokens(text: &str) -> Vec<String> {
    text.replace(',', " ")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn yes_no(value: bool) -> String {
    if value { "yes".into() } else { "no".into() }
}

fn truthy(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "yes" | "y" | "true" | "1"
    )
}

fn numbers(values: &[String]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| {
                let text = value.trim();
                if text.is_empty() {
                    Value::Null
                } else {
                    number_value(text)
                }
            })
            .collect(),
    )
}

fn number_value(text: &str) -> Value {
    match text.parse::<f64>() {
        Ok(number) if number.fract() == 0.0 && number.abs() <= 9.0e15 => {
            json!(number as i64)
        }
        Ok(number) => json!(number),
        Err(_) => json!(text),
    }
}

fn insert_optional(map: &mut Map<String, Value>, key: &str, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    map.insert(key.to_string(), number_value(trimmed));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value_reads_placeholders_zones_and_nulls() {
        let value = json!({
            "num_regions": 2, "num_zones": 2,
            "NC": [10, 20], "HR": [5.0, 10.0], "IZL": [1, 2],
            "SCT": [1.0, 2.0], "SCS": ["kk", "mm"],
            "NU_SIGF": [0.0, 0.1], "SIGMA_F": null, "Q": [0, 0],
            "N": 4, "SPATIAL_SCHEME": "SC", "MODE": "eigenvalue",
            "ZONE_NAMES": ["A", "B"], "K_EFF": null,
            "epsilon": 1e-6, "max_iter": 500,
            "reflex_der": true, "bound_left": 0.0, "bound_right": [0.0]
        });
        let model = ConfigModel::from_value(&value).expect("model");
        assert_eq!(model.scs, vec!["kk", "mm"]);
        assert_eq!(model.zone_names, vec!["A", "B"]);
        assert_eq!(model.scheme, 2);
        assert_eq!(model.mode, 3);
        assert!((model.epsilon.parse::<f64>().unwrap() - 1e-6).abs() < 1e-18);
        assert_eq!(model.max_iter, "500");
        assert_eq!(model.bound_left, vec!["0.0"]);
        assert!(model.reflex_der);
        assert!(model.k_eff.is_empty());
    }

    #[test]
    fn default_model_is_valid() {
        let model = ConfigModel::default();
        assert!(matches!(model.validate(), Validation::Valid(_)));
        let value = model.to_value();
        assert_eq!(value["N"], 8);
        assert_eq!(value["MODE"], "fixed_source");
    }

    #[test]
    fn placeholders_make_a_template() {
        let mut model = ConfigModel::default();
        model.scs[0] = "kk".into();
        assert!(matches!(model.validate(), Validation::Template(names) if names == vec!["kk"]));
    }

    #[test]
    fn editing_counts_resizes_vectors() {
        let mut model = ConfigModel::default();
        model.set_field(FieldKind::NumRegions, "2");
        model.set_field(FieldKind::NumZones, "2");
        assert_eq!(model.nc.len(), 2);
        assert_eq!(model.sct.len(), 2);
        assert_eq!(model.q.len(), 2);

        model.set_field(FieldKind::NumRegions, "1");
        assert_eq!(model.num_zones, 1);
        assert_eq!(model.sct.len(), 1);
    }

    #[test]
    fn quadrature_resizes_boundaries() {
        let mut model = ConfigModel::default();
        model.set_field(FieldKind::Quadrature, "2");
        assert_eq!(model.bound_left.len(), 1);
        model.cycle_field(FieldKind::Quadrature, 1);
        assert_eq!(model.bound_left.len(), 2);
    }

    #[test]
    fn invalid_values_are_reported() {
        let mut model = ConfigModel::default();
        model.set_field(FieldKind::ZoneSigmaT(0), "0");
        assert!(matches!(model.validate(), Validation::Invalid(_)));
    }
}
