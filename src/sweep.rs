//! Shared placeholder sweep expansion used by the CLI and the TUI.
//!
//! The semantics match `core/placeholders.py`: inclusive grids, rounding to the
//! decimal places of the step, zip across placeholders (shortest wins) and
//! validation of every expanded configuration.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::config::{Config, RunOverrides};
use crate::placeholder::{
    build_values, contains_placeholders, decimal_places, replace_placeholders, round_to,
};

/// One placeholder range as typed by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepRange {
    /// Placeholder name (`kk`, `mm`, ...).
    pub name: String,
    /// Range start (text).
    pub start: String,
    /// Range end (text).
    pub end: String,
    /// Range step (text; its decimals drive the rounding).
    pub step: String,
}

impl SweepRange {
    /// Default range used when adding a row in the TUI.
    pub fn new(name: impl Into<String>) -> Self {
        SweepRange {
            name: name.into(),
            start: "0.1".into(),
            end: "0.5".into(),
            step: "0.1".into(),
        }
    }
}

/// Result of an expansion.
#[derive(Debug)]
pub struct Expansion {
    /// Valid expanded configurations.
    pub configs: Vec<Value>,
    /// Per-step problems (the configs that could not be produced).
    pub errors: Vec<String>,
    /// Number of zip steps attempted.
    pub steps: usize,
    /// Longest grid (used to warn about zip truncation).
    pub max_len: usize,
}

/// Expand placeholder ranges over one or more base configurations.
///
/// # Errors
///
/// Returns a message when no range is given, a value cannot be parsed, or a
/// grid is invalid.
pub fn expand(bases: &[(String, Value)], ranges: &[SweepRange]) -> Result<Expansion, String> {
    if ranges.is_empty() {
        return Err("No placeholders provided".to_string());
    }

    let mut rounding_digits = 0usize;
    let mut grids: Vec<(String, Vec<f64>)> = Vec::with_capacity(ranges.len());
    for range in ranges {
        let start = parse(&range.name, &range.start)?;
        let end = parse(&range.name, &range.end)?;
        let step = parse(&range.name, &range.step)?;
        rounding_digits = rounding_digits.max(decimal_places(&range.step));
        let values = build_values(start, end, step)
            .map_err(|error| format!("invalid placeholder '{}': {error}", range.name))?;
        grids.push((range.name.clone(), values));
    }

    let steps = grids
        .iter()
        .map(|(_, values)| values.len())
        .min()
        .unwrap_or(0);
    let max_len = grids
        .iter()
        .map(|(_, values)| values.len())
        .max()
        .unwrap_or(0);
    let names: BTreeSet<String> = grids.iter().map(|(name, _)| name.clone()).collect();

    let mut configs = Vec::new();
    let mut errors = Vec::new();

    for index in 0..steps {
        let replacements: BTreeMap<String, f64> = grids
            .iter()
            .map(|(name, values)| (name.clone(), round_to(values[index], rounding_digits)))
            .collect();

        for (base_name, base) in bases {
            let expanded = replace_placeholders(base, &replacements);
            if contains_placeholders(&expanded, &names) {
                errors.push(format!(
                    "Step {} ({base_name}): unresolved placeholders",
                    index + 1
                ));
                continue;
            }
            if crate::cli::is_two_d_config(&expanded) {
                errors.push(format!(
                    "Step {} ({base_name}): 2D expansion lands in step 4",
                    index + 1
                ));
                continue;
            }
            match Config::from_value_with(&expanded, RunOverrides::default(), None) {
                Ok(_) => configs.push(expanded),
                Err(error) => errors.push(format!("Step {} ({base_name}): {error}", index + 1)),
            }
        }
    }

    Ok(Expansion {
        configs,
        errors,
        steps,
        max_len,
    })
}

fn parse(name: &str, raw: &str) -> Result<f64, String> {
    raw.trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid placeholder '{name}': invalid value {raw:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "num_regions": 1, "num_zones": 1,
            "NC": [20], "HR": [20.0], "IZL": [1],
            "SCT": [1.0], "SCS": ["kk"], "NU_SIGF": [0.11],
            "Q": [0.0], "N": 8, "MODE": "eigenvalue",
            "bound_left": [0, 0, 0, 0], "bound_right": [0, 0, 0, 0]
        })
    }

    #[test]
    fn expansion_uses_the_reference_grid() {
        let result = expand(
            &[("base".to_string(), base())],
            &[SweepRange {
                name: "kk".into(),
                start: "0.1".into(),
                end: "0.5".into(),
                step: "0.2".into(),
            }],
        )
        .expect("expand");
        assert_eq!(result.configs.len(), 3);
        assert_eq!(result.steps, 3);
        assert!(result.errors.is_empty());
        let values: Vec<f64> = result
            .configs
            .iter()
            .map(|config| config["SCS"][0].as_f64().unwrap())
            .collect();
        assert_eq!(values, vec![0.1, 0.3, 0.5]);
    }

    #[test]
    fn invalid_ranges_are_reported() {
        let error = expand(
            &[("base".to_string(), base())],
            &[SweepRange {
                name: "kk".into(),
                start: "1".into(),
                end: "0".into(),
                step: "0.1".into(),
            }],
        )
        .expect_err("invalid grid");
        assert!(error.contains("kk"));
    }

    #[test]
    fn missing_placeholder_leaves_an_error() {
        let result = expand(
            &[("base".to_string(), base())],
            &[SweepRange {
                name: "mm".into(),
                start: "0.1".into(),
                end: "0.2".into(),
                step: "0.1".into(),
            }],
        )
        .expect("expand");
        assert!(result.configs.is_empty());
        assert!(!result.errors.is_empty());
    }
}
