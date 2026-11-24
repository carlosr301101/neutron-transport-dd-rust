//! Logging setup for the command line interface.
//!
//! Two destinations are configured once per process:
//!
//! * console (`stderr`) at the verbosity requested by the user,
//! * a daily file `outputs/logs/nts-rs.log.<YYYY-MM-DD>` at `trace` level.
//!
//! Individual runs additionally get a small log file under
//! `outputs/logs/runs/` with their configuration and result summary.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use log::LevelFilter;

/// Initialise the global logger.
///
/// Returns the log directory that was created (`outputs/logs`).
///
/// # Errors
///
/// Returns an I/O error when the directory or the log file cannot be created,
/// or when a global logger is already installed.
pub fn init(console_level: LevelFilter) -> std::io::Result<PathBuf> {
    let dir = crate::paths::outputs_dir().join("logs");
    std::fs::create_dir_all(&dir)?;

    let console = fern::Dispatch::new()
        .format(format_line)
        .level(console_level)
        .chain(std::io::stderr());

    let file = fern::Dispatch::new()
        .format(format_line)
        .level(LevelFilter::Trace)
        .chain(fern::DateBased::new(dir.join("nts-rs.log"), ".%Y-%m-%d"));

    fern::Dispatch::new()
        .chain(console)
        .chain(file)
        .apply()
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    Ok(dir)
}

fn format_line(
    out: fern::FormatCallback<'_>,
    message: &std::fmt::Arguments<'_>,
    record: &log::Record<'_>,
) {
    out.finish(format_args!(
        "{} {:<5} {}",
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
        record.level(),
        message
    ));
}

/// Per-run log file with the configuration and the result summary.
#[derive(Debug)]
pub struct RunLog {
    path: PathBuf,
    file: File,
}

impl RunLog {
    /// Create `outputs/logs/runs/<timestamp>_<name>.log`.
    ///
    /// # Errors
    ///
    /// Propagates filesystem errors from directory or file creation.
    pub fn create(name: &str) -> std::io::Result<Self> {
        let dir = crate::paths::outputs_dir().join("logs").join("runs");
        std::fs::create_dir_all(&dir)?;

        let stamp = chrono::Local::now().format("%Y%m%dT%H%M%S%.3f");
        let sanitized = sanitize(name);

        for attempt in 0..100 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let path = dir.join(format!("{stamp}_{sanitized}{suffix}.log"));
            match OpenOptions::new().create_new(true).append(true).open(&path) {
                Ok(file) => return Ok(RunLog { path, file }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(std::io::Error::other(
            "could not allocate a unique run log file name",
        ))
    }

    /// Path of the log file.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Append one timestamped line (best effort).
    pub fn line(&mut self, message: impl AsRef<str>) {
        let stamp = chrono::Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(self.file, "[{stamp}] {}", message.as_ref());
        let _ = self.file.flush();
    }
}

fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            out.push(character);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "run".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_safe_characters() {
        assert_eq!(sanitize("json_config_001"), "json_config_001");
        assert_eq!(sanitize("a/b c"), "a_b_c");
        assert_eq!(sanitize(""), "run");
    }
}
