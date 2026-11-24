//! Configuration validation rules and JSON round-trip.

mod common;

use common::{base, merge, remove, set};
use neutron_transport_dd::config::{Config, Mode};
use neutron_transport_dd::scheme::Method;
use serde_json::json;

fn invalid(value: &serde_json::Value) -> String {
    Config::from_value(value, 1e-5, 2000, None)
        .expect_err("configuration should be rejected")
        .to_string()
}

#[test]
fn missing_required_field_is_reported() {
    assert_eq!(
        invalid(&remove(base(), "NC")),
        "Missing required field: 'NC'"
    );
}

#[test]
fn dimension_mismatches_are_reported() {
    assert_eq!(
        invalid(&set(base(), "NC", json!([10, 10]))),
        "NC must have 1 elements"
    );
    assert_eq!(
        invalid(&set(base(), "SCT", json!([1.0, 1.0]))),
        "SCT must have 1 elements"
    );
}

#[test]
fn zone_indices_must_be_in_range() {
    assert_eq!(
        invalid(&set(base(), "IZL", json!([2]))),
        "IZL must contain values between 1 and 1"
    );
}

#[test]
fn fission_parameters_are_validated() {
    assert_eq!(
        invalid(&set(base(), "NU_SIGF", json!([-0.1]))),
        "NU_SIGF must be >= 0 in all zones."
    );
    assert_eq!(
        invalid(&set(base(), "SIGMA_F", json!([2.0]))),
        "SIGMA_F cannot exceed SCT."
    );

    let no_fission = merge(base(), json!({"NU_SIGF": [0.0], "MODE": "fixed_source"}));
    assert_eq!(
        invalid(&set(no_fission, "POWER_KW", json!(10.0))),
        "POWER_KW requires NU_SIGF > 0."
    );
}

#[test]
fn zone_names_need_one_entry_per_zone() {
    assert_eq!(
        invalid(&set(base(), "ZONE_NAMES", json!(["a", "b"]))),
        "ZONE_NAMES must have 1 elements"
    );
    assert_eq!(
        invalid(&set(base(), "ZONE_NAMES", json!([" "]))),
        "ZONE_NAMES cannot contain empty names."
    );
}

#[test]
fn unsupported_quadrature_order_is_rejected() {
    let error = invalid(&set(base(), "N", json!(10)));
    assert!(error.contains("No quadrature data for S10"), "{error}");
}

#[test]
fn comment_keys_and_integer_booleans_are_accepted() {
    let value = merge(
        base(),
        json!({
            "_comment": "ignored",
            "reflex_izq": 0,
            "reflex_der": 1
        }),
    );
    let config = Config::from_value(&value, 1e-5, 2000, None).expect("valid");
    assert!(!config.reflex_izq);
    assert!(config.reflex_der);
}

#[test]
fn mode_inference_covers_the_three_branches() {
    let inferred_source = merge(base(), json!({"NU_SIGF": [0.0], "Q": [1.0], "MODE": null}));
    let config = Config::from_value(&inferred_source, 1e-5, 2000, None).expect("source");
    assert_eq!(config.mode, Mode::FixedSource);

    let inferred_eigen = remove(base(), "MODE");
    let config = Config::from_value(&inferred_eigen, 1e-5, 2000, None).expect("eigen");
    assert_eq!(config.mode, Mode::Eigenvalue);

    let inferred_fixed = merge(base(), json!({"MODE": null, "K_EFF": 1.02}));
    let config = Config::from_value(&inferred_fixed, 1e-5, 2000, None).expect("fixed_k");
    assert_eq!(config.mode, Mode::FixedK);
}

#[test]
fn as_dict_round_trip_preserves_the_effective_configuration() {
    let value = merge(
        base(),
        json!({
            "SPATIAL_SCHEME": "SC",
            "ZONE_NAMES": ["Core"],
            "POWER_KW": 10.0,
            "NU": 2.4
        }),
    );
    let config = Config::from_value(&value, 1e-5, 2000, None).expect("config");
    let dict = config.as_dict();
    let restored = Config::from_value(&dict, 1e-5, 2000, None).expect("round trip");
    assert_eq!(restored.as_dict(), dict);
    assert_eq!(dict["SPATIAL_SCHEME"], json!("SC"));
    assert_eq!(dict["ZONE_NAMES"], json!(["Core"]));
    assert_eq!(dict["NU"], json!(2.4));
}

#[test]
fn forced_scheme_sets_the_configuration_scheme() {
    let config = Config::from_value(&base(), 1e-5, 2000, Some(Method::Step)).expect("config");
    assert_eq!(config.method(), Method::Step);
    assert_eq!(config.as_dict()["SPATIAL_SCHEME"], json!("STEP"));
}

#[test]
fn json_file_loader_reports_read_errors() {
    let error = Config::from_json_file("/definitely/not/here.json", 1e-5, 2000, None)
        .expect_err("missing file");
    assert!(error.to_string().contains("failed to read"));
}
