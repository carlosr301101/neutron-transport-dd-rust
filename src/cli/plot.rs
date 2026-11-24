//! `nts-rs plot`: PNG plots of scalar/angular flux, k history and sweeps.

use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};

use crate::paths;
use crate::post::{model, plot};

/// Kind of plot to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PlotKind {
    /// Scalar flux vs x.
    Scalar,
    /// Angular flux per direction vs x.
    Angular,
    /// k_eff history of the power iteration.
    #[value(name = "k-history")]
    KHistory,
    /// Flux at positions vs a sweep parameter (datos.json).
    Scs,
}

impl PlotKind {
    const fn as_str(self) -> &'static str {
        match self {
            PlotKind::Scalar => "scalar",
            PlotKind::Angular => "angular",
            PlotKind::KHistory => "k-history",
            PlotKind::Scs => "scs",
        }
    }
}

/// Arguments of `nts-rs plot`.
#[derive(Debug, Args)]
pub struct PlotCommand {
    /// Consolidated output (`output_1d_###.json`).
    #[arg(short, long, value_name = "FILE")]
    result: Option<PathBuf>,

    /// Sweep data (`datos.json`) for `--kind scs`.
    #[arg(short, long, value_name = "FILE")]
    input: Option<PathBuf>,

    /// Plot kind (default: scalar with -r, scs with -i).
    #[arg(long, value_enum)]
    kind: Option<PlotKind>,

    /// Runs to plot (name, 0-based index or all).
    #[arg(long, num_args = 1.., value_name = "NAME|INDEX")]
    run: Vec<String>,

    /// Sweep parameter for `--kind scs`: SCS, NU_SIGF or k_eff.
    #[arg(long, value_name = "PARAM")]
    x: Option<String>,

    /// Label material regions (uses ZONE_NAMES when present).
    #[arg(long)]
    zone_names: bool,

    /// Output PNG (default: `outputs/results/plots/<kind>.png`).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Open the file with the platform viewer after writing.
    #[arg(long)]
    show: bool,

    /// Canvas width in pixels.
    #[arg(long, default_value_t = 1600)]
    width: u32,

    /// Canvas height in pixels.
    #[arg(long, default_value_t = 900)]
    height: u32,
}

pub(crate) fn execute(command: PlotCommand) -> Result<u8, String> {
    let kind = command.kind.unwrap_or(if command.result.is_some() {
        PlotKind::Scalar
    } else {
        PlotKind::Scs
    });
    if kind != PlotKind::Scs && command.x.is_some() {
        return Err("--x is only valid with --kind scs".to_string());
    }

    let size = plot::PlotSize {
        width: command.width.max(320),
        height: command.height.max(240),
    };
    let output = command
        .output
        .map(|path| resolve(&path))
        .unwrap_or_else(|| {
            paths::results_dir()
                .join("plots")
                .join(format!("{}.png", kind.as_str()))
        });

    match kind {
        PlotKind::Scs => {
            let input = command
                .input
                .as_ref()
                .ok_or_else(|| "--kind scs requires -i/--input (datos.json)".to_string())?;
            let datos = model::load(&resolve(input))?;
            let x_key = x_key(command.x.as_deref())?;
            plot::scs(&datos, x_key, &output, size)?;
        }
        _ => {
            let result = command
                .result
                .as_ref()
                .ok_or_else(|| format!("--kind {} requires -r/--result", kind.as_str()))?;
            let payload = model::load(&resolve(result))?;
            let runs = model::runs(&payload)?;
            let indices = model::select_runs(runs, &command.run)?;
            if indices.is_empty() {
                return Err("no runs selected".to_string());
            }

            match kind {
                PlotKind::Scalar => {
                    plot::scalar(&payload, &indices, &output, size, command.zone_names)?;
                }
                PlotKind::KHistory => {
                    plot::k_history(&payload, &indices, &output, size)?;
                }
                PlotKind::Angular => {
                    if indices.len() != 1 {
                        return Err("--kind angular requires exactly one run".to_string());
                    }
                    plot::angular(&payload, indices[0], &output, size, command.zone_names)?;
                }
                PlotKind::Scs => unreachable!(),
            }
        }
    }

    println!("✓ Plot saved: {}", output.display());
    if command.show {
        open_file(&output);
    }
    Ok(0)
}

fn x_key(raw: Option<&str>) -> Result<&'static str, String> {
    match raw.unwrap_or("SCS") {
        "SCS" | "scs" => Ok("SCS"),
        "NU_SIGF" | "nu_sigf" => Ok("NU_SIGF"),
        "k_eff" | "k-eff" => Ok("k_eff"),
        other => Err(format!("invalid --x {other:?} (use SCS, NU_SIGF or k_eff)")),
    }
}

fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths::work_root().join(path)
    }
}

fn open_file(path: &Path) {
    use std::process::{Command, Stdio};

    let spawned = if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else {
        Command::new("xdg-open")
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };

    if let Err(error) = spawned {
        eprintln!("Warning: could not open {}: {error}", path.display());
    }
}
