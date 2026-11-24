//! Power normalization of an existing consolidated output.

use serde_json::Value;

use crate::power::{DEFAULT_NU, power_watts};

use super::model;

/// Normalize one run in place to `power_kw`.
///
/// Returns `(C_N, P_unnormalized)`.
///
/// # Errors
///
/// Returns a message when the configuration is incomplete, the mesh does not
/// match the flux or the fission integral is zero.
pub fn normalize_run(
    run: &mut Value,
    power_kw: f64,
    ef_mev: Option<f64>,
    area_cm2: Option<f64>,
    nu_override: Option<f64>,
) -> Result<(f64, f64), String> {
    let configuration = model::configuration(run)
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    let scalar = model::f64_list(run, "scalar_flux");
    if scalar.is_empty() {
        return Err("run has no scalar_flux".to_string());
    }

    let dx = dx_from_config(&configuration)?;
    if dx.len() != scalar.len() {
        return Err(format!(
            "NC/HR define {} cells but scalar_flux has {}.",
            dx.len(),
            scalar.len()
        ));
    }

    let sigma_f = sigma_f_per_cell(&configuration, nu_override)?;
    if sigma_f.len() != scalar.len() {
        return Err("SIGMA_F/NU_SIGF do not match the mesh.".to_string());
    }

    let ef = ef_mev
        .or_else(|| model::f64_at(&configuration, "E_F_MEV"))
        .unwrap_or(200.0);
    let area = area_cm2
        .or_else(|| model::f64_at(&configuration, "AREA_CM2"))
        .unwrap_or(1.0);
    if ef <= 0.0 || area <= 0.0 {
        return Err("E_F_MEV and AREA_CM2 must be positive.".to_string());
    }

    let power_unnormalized = power_watts(&scalar, &sigma_f, &dx, ef, area);
    if power_unnormalized <= 0.0 {
        return Err("fission integral is zero".to_string());
    }

    let c_n = power_kw * 1e3 / power_unnormalized;
    scale_list(run, "scalar_flux", c_n);
    scale_matrix(run, "angular_flux", c_n);

    if let Some(object) = run.as_object_mut() {
        object.insert(
            "power_unnormalized".into(),
            serde_json::json!(power_unnormalized),
        );
        object.insert("power_scale".into(), serde_json::json!(c_n));
        object.insert("power_w".into(), serde_json::json!(power_kw * 1e3));
    }

    Ok((c_n, power_unnormalized))
}

fn dx_from_config(configuration: &Value) -> Result<Vec<f64>, String> {
    let nc = model::f64_list(configuration, "NC");
    let hr = model::f64_list(configuration, "HR");
    if nc.is_empty() || nc.len() != hr.len() {
        return Err("NC and HR are required to build the mesh.".to_string());
    }

    let mut dx = Vec::new();
    for (cells, width) in nc.iter().zip(&hr) {
        if *cells < 1.0 || cells.fract() != 0.0 || *width <= 0.0 {
            return Err("NC and HR must be positive.".to_string());
        }
        let count = *cells as usize;
        dx.extend(std::iter::repeat_n(*width / count as f64, count));
    }
    Ok(dx)
}

fn per_cell(values: &[f64], configuration: &Value, name: &str) -> Result<Vec<f64>, String> {
    let nc = model::f64_list(configuration, "NC");
    let izl = model::f64_list(configuration, "IZL");
    if nc.is_empty() || nc.len() != izl.len() {
        return Err("NC and IZL are required to expand zone data.".to_string());
    }
    if values.is_empty() {
        return Err(format!("{name} is required."));
    }

    let mut out = Vec::new();
    for (cells, zone) in nc.iter().zip(&izl) {
        let index = *zone as isize - 1;
        if index < 0 || index as usize >= values.len() {
            return Err(format!("{name} has no value for zone {zone}."));
        }
        out.extend(std::iter::repeat_n(values[index as usize], *cells as usize));
    }
    Ok(out)
}

fn sigma_f_per_cell(configuration: &Value, nu_override: Option<f64>) -> Result<Vec<f64>, String> {
    let sigma_f = model::f64_list(configuration, "SIGMA_F");
    if !sigma_f.is_empty() {
        return per_cell(&sigma_f, configuration, "SIGMA_F");
    }

    let nu = nu_override
        .or_else(|| model::f64_at(configuration, "NU"))
        .unwrap_or(DEFAULT_NU);
    if nu <= 0.0 {
        return Err("NU must be > 0.".to_string());
    }

    let nu_sigf = model::f64_list(configuration, "NU_SIGF");
    if nu_sigf.is_empty() {
        return Err("NU_SIGF is required when SIGMA_F is missing.".to_string());
    }
    Ok(per_cell(&nu_sigf, configuration, "NU_SIGF")?
        .into_iter()
        .map(|value| value / nu)
        .collect())
}

fn scale_list(run: &mut Value, key: &str, factor: f64) {
    if let Some(values) = run.get_mut(key).and_then(Value::as_array_mut) {
        for value in values {
            if let Some(number) = value.as_f64() {
                *value = serde_json::json!(number * factor);
            }
        }
    }
}

fn scale_matrix(run: &mut Value, key: &str, factor: f64) {
    if let Some(rows) = run.get_mut(key).and_then(Value::as_array_mut) {
        for row in rows {
            if let Some(values) = row.as_array_mut() {
                for value in values {
                    if let Some(number) = value.as_f64() {
                        *value = serde_json::json!(number * factor);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run_fixture() -> Value {
        json!({
            "config_name": "a",
            "configuration": {
                "NC": [2], "HR": [2.0], "IZL": [1],
                "NU_SIGF": [0.1], "SIGMA_F": null,
                "E_F_MEV": 200.0, "AREA_CM2": 1.0
            },
            "scalar_flux": [1.0, 2.0],
            "angular_flux": [[1.0, 1.0], [2.0, 2.0]]
        })
    }

    #[test]
    fn normalization_scales_fluxes_and_reports_power() {
        let mut run = run_fixture();
        let (c_n, power) = normalize_run(&mut run, 10.0, None, None, None).expect("normalize");
        assert!(power > 0.0);
        assert!((run["power_w"].as_f64().unwrap() - 10_000.0).abs() < 1e-9);
        assert!((run["power_scale"].as_f64().unwrap() - c_n).abs() < 1e-15);
        assert!((run["power_unnormalized"].as_f64().unwrap() - power).abs() < 1e-15);
        assert!((run["scalar_flux"][0].as_f64().unwrap() - c_n).abs() < 1e-12);
        assert!((run["angular_flux"][1][0].as_f64().unwrap() - 2.0 * c_n).abs() < 1e-12);
    }

    #[test]
    fn missing_fission_data_is_rejected() {
        let mut run = run_fixture();
        run["configuration"]["NU_SIGF"] = json!([]);
        assert!(normalize_run(&mut run, 10.0, None, None, None).is_err());
    }
}
