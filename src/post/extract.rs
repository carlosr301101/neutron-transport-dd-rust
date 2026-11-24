//! `datos.json` extraction: scalar flux at selected cell positions.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::model;

/// Result of one extraction.
pub struct Extraction {
    /// Payload written to `datos.json`.
    pub payload: Value,
    /// Per-run problems (reported but not fatal).
    pub errors: Vec<String>,
    /// Number of rows included.
    pub rows: usize,
}

/// Build the `datos.json` payload from a consolidated output.
///
/// # Errors
///
/// Returns a message when the payload has no `runs` array.
pub fn extract(
    payload: &Value,
    source: &str,
    positions: &[usize],
    fallback: Option<&Map<String, Value>>,
) -> Result<Extraction, String> {
    let runs = model::runs(payload)?;
    let mut rows: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut schemes: BTreeSet<String> = BTreeSet::new();

    for (index, run) in runs.iter().enumerate() {
        let name = model::run_name(run, index);
        let configuration =
            model::configuration(run).or_else(|| fallback.and_then(|configs| configs.get(&name)));

        let Some(configuration) = configuration else {
            errors.push(format!(
                "Config not found for {name}: no embedded configuration and not present in inputs"
            ));
            continue;
        };

        let scs = model::f64_list(configuration, "SCS");
        if scs.is_empty() {
            errors.push(format!("Invalid SCS for {name}"));
            continue;
        }

        let flux = model::f64_list(run, "scalar_flux");
        if flux.is_empty() {
            errors.push(format!("Empty scalar_flux for {name}"));
            continue;
        }

        let mut row = Map::new();
        row.insert("config_name".into(), json!(name));
        row.insert("SCS".into(), json!(scs[0]));

        let mut complete = true;
        for position in positions {
            if *position < flux.len() {
                row.insert(format!("flux_{position}"), json!(flux[*position]));
            } else {
                errors.push(format!(
                    "Position {position} out of range for {name} (length {})",
                    flux.len()
                ));
                complete = false;
            }
        }
        if !complete {
            continue;
        }

        let nu_sigf = model::f64_list(configuration, "NU_SIGF");
        if let Some(first) = nu_sigf.first() {
            row.insert("NU_SIGF".into(), json!(first));
        }

        let scheme = model::str_at(run, "spatial_scheme")
            .or_else(|| model::str_at(configuration, "SPATIAL_SCHEME"));
        if let Some(scheme) = scheme {
            row.insert("spatial_scheme".into(), json!(scheme));
            schemes.insert(scheme.to_string());
        }

        for key in ["mode", "k_eff", "k_balance"] {
            if let Some(value) = run.get(key)
                && !value.is_null()
            {
                row.insert(key.to_string(), value.clone());
            }
        }

        rows.push(Value::Object(row));
    }

    let row_count = rows.len();
    let mut payload = json!({
        "source": source,
        "positions": positions,
        "spatial_schemes": schemes.into_iter().collect::<Vec<_>>(),
        "data": rows,
    });
    if !errors.is_empty()
        && let Some(object) = payload.as_object_mut()
    {
        object.insert("errors".into(), json!(errors));
    }

    Ok(Extraction {
        payload,
        errors,
        rows: row_count,
    })
}
