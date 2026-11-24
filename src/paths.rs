//! Work-directory and output-path helpers.
//!
//! Mirrors the path part of the Python `utils/paths.py`: the work directory is
//! `NTS_HOME` when set, otherwise the current directory, and results live in
//! `outputs/results/output_1d_###.json`.

use std::path::PathBuf;

/// Root of the user work directory.
pub fn work_root() -> PathBuf {
    match std::env::var_os("NTS_HOME") {
        Some(raw) => {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            }
        }
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// `outputs/` inside the work directory.
pub fn outputs_dir() -> PathBuf {
    work_root().join("outputs")
}

/// `outputs/results/` inside the work directory.
pub fn results_dir() -> PathBuf {
    outputs_dir().join("results")
}

/// Next free `output_1d_###.json` path.
pub fn next_output_path() -> PathBuf {
    let dir = results_dir();
    let mut last = 0usize;

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with("output_1d_") || !name.ends_with(".json") {
                continue;
            }
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            if let Some(index) = stem.rsplit('_').next().and_then(|part| part.parse().ok()) {
                last = last.max(index);
            }
        }
    }

    dir.join(format!("output_1d_{:03}.json", last + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_output_path_is_zero_padded() {
        let path = next_output_path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("output_1d_"));
        assert_eq!(name.len(), "output_1d_001.json".len());
    }
}
