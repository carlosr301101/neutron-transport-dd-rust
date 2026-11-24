//! Error types shared by the configuration and solver layers.
//!
//! The library never panics on user input: every fallible path returns one of
//! these errors. The binary maps them to exit codes and messages.

use std::fmt;
use std::path::PathBuf;

/// Errors produced while reading or validating a configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The configuration is syntactically valid but physically inconsistent.
    Invalid(String),
    /// The configuration file could not be read.
    Io {
        /// Path that failed to load.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// The configuration file is not valid JSON for the expected schema.
    Json(serde_json::Error),
}

impl ConfigError {
    /// Build an [`ConfigError::Invalid`] from any displayable value.
    pub fn invalid(message: impl Into<String>) -> Self {
        ConfigError::Invalid(message.into())
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Invalid(message) => f.write_str(message),
            ConfigError::Io { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            ConfigError::Json(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Json(source) => Some(source),
            ConfigError::Invalid(_) => None,
        }
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(source: serde_json::Error) -> Self {
        ConfigError::Json(source)
    }
}

/// Errors produced while running the transport solver.
#[derive(Debug)]
pub enum SolverError {
    /// The configuration was rejected before the sweep started.
    Config(ConfigError),
    /// The run started but could not complete (mode or normalization issues).
    Runtime(String),
}

impl SolverError {
    /// Build a [`SolverError::Runtime`] from any displayable value.
    pub fn runtime(message: impl Into<String>) -> Self {
        SolverError::Runtime(message.into())
    }
}

impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolverError::Config(error) => error.fmt(f),
            SolverError::Runtime(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SolverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SolverError::Config(error) => Some(error),
            SolverError::Runtime(_) => None,
        }
    }
}

impl From<ConfigError> for SolverError {
    fn from(error: ConfigError) -> Self {
        SolverError::Config(error)
    }
}
