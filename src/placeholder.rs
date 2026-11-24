//! Placeholder detection and expansion.
//!
//! Port of `core/placeholders.py`: a placeholder is a symbolic name (`kk`,
//! `mm`, ...) stored wherever a number is expected. `nts-rs expand` turns one
//! template plus ranges into a list of concrete configurations.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// Keys whose string values are never considered placeholders.
pub const TEXT_KEYS: [&str; 3] = ["SPATIAL_SCHEME", "MODE", "ZONE_NAMES"];

/// True when `value` looks like a symbolic placeholder name.
pub fn is_placeholder(value: &str) -> bool {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Collect every placeholder name in a configuration.
///
/// Strings under [`TEXT_KEYS`] are ignored. The function descends into lists
/// and dictionaries, mirroring the reference implementation.
pub fn detect_placeholders(value: &Value) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    visit(value, None, &mut found);
    found
}

fn visit(value: &Value, key: Option<&str>, found: &mut BTreeSet<String>) {
    if let Some(key) = key
        && TEXT_KEYS.contains(&key)
    {
        return;
    }
    match value {
        Value::Object(map) => {
            for (child_key, child) in map {
                visit(child, Some(child_key), found);
            }
        }
        Value::Array(items) => {
            for item in items {
                visit(item, key, found);
            }
        }
        Value::String(text) if is_placeholder(text) => {
            found.insert(text.clone());
        }
        _ => {}
    }
}

/// Replace placeholder names recursively, leaving enum-like keys untouched.
pub fn replace_placeholders(value: &Value, replacements: &BTreeMap<String, f64>) -> Value {
    replace(value, replacements, None)
}

fn replace(value: &Value, replacements: &BTreeMap<String, f64>, key: Option<&str>) -> Value {
    if let Some(key) = key
        && TEXT_KEYS.contains(&key)
    {
        return value.clone();
    }
    match value {
        Value::Object(map) => {
            let mut result = Map::with_capacity(map.len());
            for (child_key, child) in map {
                result.insert(
                    child_key.clone(),
                    replace(child, replacements, Some(child_key)),
                );
            }
            Value::Object(result)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| replace(item, replacements, key))
                .collect(),
        ),
        Value::String(text) => match replacements.get(text) {
            Some(number) => json!(number),
            None => value.clone(),
        },
        _ => value.clone(),
    }
}

/// True when any of `names` remains in `value`.
pub fn contains_placeholders(value: &Value, names: &BTreeSet<String>) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            !TEXT_KEYS.contains(&key.as_str()) && contains_placeholders(child, names)
        }),
        Value::Array(items) => items.iter().any(|item| contains_placeholders(item, names)),
        Value::String(text) => names.contains(text),
        _ => false,
    }
}

/// Inclusive arithmetic grid from `start` to `end`.
///
/// The last point is clipped to `end` when floating-point error pushes it
/// slightly past it.
///
/// # Errors
///
/// Returns a message when `step <= 0` or `end < start`.
pub fn build_values(start: f64, end: f64, step: f64) -> Result<Vec<f64>, String> {
    if step <= 0.0 {
        return Err("Step must be greater than 0".to_string());
    }
    if end < start {
        return Err("End must be greater than or equal to start".to_string());
    }

    let mut values = Vec::new();
    let epsilon = 1e-12;
    let mut index = 0u64;
    loop {
        let mut value = start + step * index as f64;
        if value > end + epsilon {
            break;
        }
        if value > end {
            value = end;
        }
        values.push(value);
        index += 1;
    }

    if values.is_empty() {
        values.push(start);
    }
    Ok(values)
}

/// Number of decimal places in a numeric string (`0.05` -> 2, `1e-2` -> 2).
pub fn decimal_places(raw: &str) -> usize {
    let text = raw.trim();
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(index) => {
            let exponent = text[index + 1..].parse::<i32>().unwrap_or(0);
            (&text[..index], exponent)
        }
        None => (text, 0),
    };
    let decimals = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as i32);
    (decimals - exponent).max(0) as usize
}

/// Round to `digits` decimal places.
pub fn round_to(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse::<f64>().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_names_are_identifiers() {
        assert!(is_placeholder("kk"));
        assert!(is_placeholder("_x1"));
        assert!(!is_placeholder("1e-3"));
        assert!(!is_placeholder("abc def"));
        assert!(!is_placeholder(""));
    }

    #[test]
    fn detection_skips_enum_keys_and_descends() {
        let value = json!({
            "SCS": ["kk"],
            "MODE": "eigenvalue",
            "SPATIAL_SCHEME": "mm",
            "ZONE_NAMES": ["nn"],
            "nested": {"Q": ["qq"], "list": [{"HR": ["hh"]}]}
        });
        let found = detect_placeholders(&value);
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            vec!["hh", "kk", "qq"]
        );
    }

    #[test]
    fn replacement_preserves_text_keys_and_unknown_strings() {
        let value = json!({
            "SCS": ["kk"],
            "MODE": "kk",
            "ZONE_NAMES": ["kk"],
            "note": "keep",
            "nested": [{"Q": ["qq"]}]
        });
        let replacements = BTreeMap::from([("kk".to_string(), 0.5), ("qq".to_string(), 1.0)]);
        let replaced = replace_placeholders(&value, &replacements);
        assert_eq!(replaced["SCS"], json!([0.5]));
        assert_eq!(replaced["MODE"], json!("kk"));
        assert_eq!(replaced["ZONE_NAMES"], json!(["kk"]));
        assert_eq!(replaced["note"], json!("keep"));
        assert_eq!(replaced["nested"][0]["Q"], json!([1.0]));
    }

    #[test]
    fn unresolved_placeholders_are_detected() {
        let names = BTreeSet::from(["kk".to_string()]);
        assert!(contains_placeholders(&json!({"SCS": ["kk"]}), &names));
        assert!(!contains_placeholders(&json!({"SCS": [0.5]}), &names));
        assert!(!contains_placeholders(&json!({"MODE": "kk"}), &names));
    }

    #[test]
    fn grid_is_inclusive_and_clipped() {
        let grid = build_values(0.1, 0.5, 0.2).unwrap();
        assert_eq!(grid.len(), 3);
        assert!((grid[0] - 0.1).abs() < 1e-15);
        assert!((grid[1] - 0.3).abs() < 1e-15);
        assert!((grid[2] - 0.5).abs() < 1e-15);

        let clipped = build_values(0.0, 0.3, 0.1).unwrap();
        assert_eq!(clipped.len(), 4);
        assert!((clipped[3] - 0.3).abs() < 1e-15);
    }

    #[test]
    fn invalid_grid_is_rejected() {
        assert!(build_values(0.0, 1.0, 0.0).is_err());
        assert!(build_values(1.0, 0.0, 0.1).is_err());
    }

    #[test]
    fn decimal_places_follows_the_text() {
        assert_eq!(decimal_places("0.05"), 2);
        assert_eq!(decimal_places("1"), 0);
        assert_eq!(decimal_places("1.50"), 2);
        assert_eq!(decimal_places("1e-2"), 2);
        assert_eq!(decimal_places("2.5E-3"), 4);
        assert_eq!(decimal_places("abc"), 0);
    }

    #[test]
    fn rounding_uses_the_requested_digits() {
        assert_eq!(round_to(0.300_000_000_000_000_04, 2), 0.3);
        assert_eq!(round_to(20.0, 0), 20.0);
        assert_eq!(round_to(1.2345, 3), 1.234);
    }
}
