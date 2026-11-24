//! Helpers to read consolidated 1D output files (`output_1d_###.json`).

use std::path::Path;

use serde_json::Value;

/// Load a result file as a JSON value.
///
/// # Errors
///
/// Returns a message when the file cannot be read or parsed.
pub fn load(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))
}

/// Reference to the `runs` array.
///
/// # Errors
///
/// Returns a message when the payload is not a consolidated 1D output.
pub fn runs(payload: &Value) -> Result<&Vec<Value>, String> {
    payload
        .get("runs")
        .and_then(Value::as_array)
        .ok_or_else(|| "expected a consolidated output with runs[]".to_string())
}

/// Display name of a run (`config_name` or `run_<index>`).
pub fn run_name(run: &Value, index: usize) -> String {
    run.get("config_name")
        .and_then(Value::as_str)
        .map_or_else(|| format!("run_{index}"), str::to_string)
}

/// Embedded configuration of a run, if any.
pub fn configuration(run: &Value) -> Option<&Value> {
    run.get("configuration").filter(|value| value.is_object())
}

/// Read a list of numbers from an object key (empty when missing).
pub fn f64_list(value: &Value, key: &str) -> Vec<f64> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

/// Read an optional number.
pub fn f64_at(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

/// Read an optional string.
pub fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Resolve `-r/--run` selectors (index, name or `all`) to run indices.
///
/// An empty selector list selects every run. Order is preserved and repeated
/// selectors are ignored.
///
/// # Errors
///
/// Returns a message when a selector matches no run.
pub fn select_runs(runs: &[Value], selectors: &[String]) -> Result<Vec<usize>, String> {
    if selectors.is_empty() || selectors.iter().any(|selector| selector == "all") {
        return Ok((0..runs.len()).collect());
    }

    let mut chosen = Vec::new();
    for selector in selectors {
        let index = if let Ok(index) = selector.parse::<usize>() {
            if index < runs.len() {
                Some(index)
            } else {
                None
            }
        } else {
            runs.iter()
                .position(|run| run.get("config_name").and_then(Value::as_str) == Some(selector))
        };
        let Some(index) = index else {
            return Err(format!("run not found: {selector:?}"));
        };
        if !chosen.contains(&index) {
            chosen.push(index);
        }
    }
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn runs_fixture() -> Vec<Value> {
        vec![
            json!({"config_name": "a"}),
            json!({"config_name": "b"}),
            json!({"config_name": "c"}),
        ]
    }

    #[test]
    fn selectors_cover_index_name_and_all() {
        let runs = runs_fixture();
        assert_eq!(select_runs(&runs, &["1".to_string()]).unwrap(), vec![1]);
        assert_eq!(select_runs(&runs, &["c".to_string()]).unwrap(), vec![2]);
        assert_eq!(
            select_runs(&runs, &["all".to_string()]).unwrap(),
            vec![0, 1, 2]
        );
        assert_eq!(select_runs(&runs, &[]).unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn selectors_reject_unknown_names() {
        let runs = runs_fixture();
        assert!(select_runs(&runs, &["zzz".to_string()]).is_err());
        assert!(select_runs(&runs, &["9".to_string()]).is_err());
    }

    #[test]
    fn lists_and_scalars_are_read() {
        let value = json!({"SCS": [0.9, 0.5], "k_eff": 1.02});
        assert_eq!(f64_list(&value, "SCS"), vec![0.9, 0.5]);
        assert_eq!(f64_at(&value, "k_eff"), Some(1.02));
        assert!(f64_list(&value, "missing").is_empty());
    }
}
