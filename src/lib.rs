//! # neutron-transport-dd
//!
//! 1D S_N (discrete ordinates) neutron transport solver with domain
//! decomposition, ported from the Python reference implementation.
//!
//! The crate exposes the pieces needed to build runs programmatically:
//!
//! * [`config::Config`] - JSON configuration, validation and per-cell data.
//! * [`scheme::Method`] - Diamond Difference, Step and Step Characteristic.
//! * [`core::Runner`] - transport sweep and the four physics modes.
//! * [`result::RunResult`] - fluxes, `k_eff`, balance and power diagnostics.
//!
//! # Example
//!
//! ```
//! use neutron_transport_dd::config::Config;
//! use neutron_transport_dd::core::Runner;
//!
//! let text = r#"{
//!   "num_regions": 1, "num_zones": 1,
//!   "NC": [20], "HR": [20.0], "IZL": [1],
//!   "SCT": [1.0], "SCS": [0.9], "NU_SIGF": [0.11], "Q": [0.0],
//!   "N": 8, "MODE": "eigenvalue",
//!   "bound_left": [0, 0, 0, 0], "bound_right": [0, 0, 0, 0]
//! }"#;
//!
//! let config = Config::from_json_str(text, 1e-5, 2000, None).expect("valid config");
//! let result = Runner::new(config).expect("runner").run().expect("run");
//! assert!(result.k_eff.unwrap() > 1.0);
//! ```

#![warn(missing_docs)]

pub mod cli;
pub mod config;
pub mod core;
pub mod error;
pub mod logging;
pub mod paths;
pub mod placeholder;
pub mod post;
pub mod power;
pub mod quadrature;
pub mod result;
pub mod scheme;
pub mod tui;
pub mod wizard;
