//! Boundary scalar/vector normalization, ported from `test_boundary_vector.py`.

mod common;

use common::{base, merge, set};
use neutron_transport_dd::config::Config;
use serde_json::{Value, json};

fn fixed_source_base() -> Value {
    merge(
        base(),
        json!({"NU_SIGF": [0.0], "MODE": "fixed_source", "Q": [0.0]}),
    )
}

fn build(value: Value) -> Config {
    Config::from_value(&value, 1e-5, 2000, None).expect("valid config")
}

#[test]
fn scalar_expands_to_n_half() {
    let value = merge(
        fixed_source_base(),
        json!({"bound_left": 0, "bound_right": 1}),
    );
    let config = build(value);
    assert_eq!(config.bound_left, vec![0.0; 4]);
    assert_eq!(config.bound_right, vec![1.0; 4]);
}

#[test]
fn single_element_list_expands() {
    let value = merge(
        fixed_source_base(),
        json!({"bound_left": [0], "bound_right": [1]}),
    );
    let config = build(value);
    assert_eq!(config.bound_left, vec![0.0; 4]);
    assert_eq!(config.bound_right, vec![1.0; 4]);
}

#[test]
fn full_vector_is_kept() {
    let value = merge(
        fixed_source_base(),
        json!({"bound_left": [0, 1, 1, 0], "bound_right": [1, 0, 0, 1]}),
    );
    let config = build(value);
    assert_eq!(config.bound_left, vec![0.0, 1.0, 1.0, 0.0]);
    assert_eq!(config.bound_right, vec![1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn invalid_length_has_a_clear_error() {
    let value = merge(
        fixed_source_base(),
        json!({"bound_left": [0, 1], "bound_right": 0}),
    );
    let error = Config::from_value(&value, 1e-5, 2000, None).expect_err("bad length");
    assert_eq!(
        error.to_string(),
        "bound_left must have 4 elements or one scalar; got 2"
    );
}

#[test]
fn reflective_scalar_is_safe() {
    let value = merge(
        set(fixed_source_base(), "reflex_izq", json!(true)),
        json!({"bound_left": 0, "bound_right": 1}),
    );
    let config = build(value);
    assert_eq!(config.bound_left, vec![0.0; 4]);
    assert_eq!(config.bound_right, vec![1.0; 4]);
}
