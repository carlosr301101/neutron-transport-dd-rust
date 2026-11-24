//! PNG plots rendered with `plotters`.
//!
//! A bundled font (Liberation Sans) is registered once per process so the
//! output does not depend on system font discovery.

use std::path::Path;
use std::sync::OnceLock;

use plotters::coord::Shift;
use plotters::element::Rectangle;
use plotters::prelude::*;
use plotters::style::{FontStyle, register_font};
use serde_json::Value;

use super::{geometry, model};

/// Pixel size of a plot.
#[derive(Debug, Clone, Copy)]
pub struct PlotSize {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
}

impl Default for PlotSize {
    fn default() -> Self {
        PlotSize {
            width: 1600,
            height: 900,
        }
    }
}

type DrawError = Box<dyn std::error::Error>;
type Canvas<'a> = DrawingArea<BitMapBackend<'a>, Shift>;

type Series = (Vec<(f64, f64)>, String, RGBColor);

const PALETTE: [RGBColor; 6] = [
    RGBColor(8, 48, 107),
    RGBColor(203, 24, 29),
    RGBColor(33, 102, 172),
    RGBColor(178, 24, 43),
    RGBColor(49, 163, 84),
    RGBColor(140, 81, 10),
];

fn register_plot_font() -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            register_font(
                "sans-serif",
                FontStyle::Normal,
                include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf"),
            )
            .map_err(|_| "could not register the bundled plot font".to_string())
        })
        .clone()
}

fn render(
    path: &Path,
    size: PlotSize,
    draw: impl for<'a> FnOnce(&Canvas<'a>) -> Result<(), DrawError>,
) -> Result<(), String> {
    register_plot_font()?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    let root = BitMapBackend::new(path, (size.width, size.height)).into_drawing_area();
    root.fill(&WHITE).map_err(|error| error.to_string())?;
    draw(&root).map_err(|error| error.to_string())?;
    root.present().map_err(|error| error.to_string())?;
    Ok(())
}

/// Scalar flux versus x, one line per selected run.
///
/// # Errors
///
/// Returns a message when the output has no runs or the mesh does not match.
pub fn scalar(
    payload: &Value,
    indices: &[usize],
    path: &Path,
    size: PlotSize,
    zone_labels: bool,
) -> Result<(), String> {
    let runs = model::runs(payload)?;
    let selected: Vec<&Value> = indices.iter().map(|&index| &runs[index]).collect();
    if selected.is_empty() {
        return Err("no runs selected".to_string());
    }

    let mut series: Vec<Series> = Vec::new();
    for (order, run) in selected.iter().enumerate() {
        let flux = model::f64_list(run, "scalar_flux");
        if flux.is_empty() {
            return Err(format!(
                "{}: empty scalar_flux",
                model::run_name(run, indices[order])
            ));
        }
        let centers = cell_centers(run, flux.len())?;
        let points: Vec<(f64, f64)> = centers.iter().copied().zip(flux.iter().copied()).collect();
        let label = run_label(run, indices[order]);
        series.push((points, label, PALETTE[order % PALETTE.len()]));
    }

    let x_max = series
        .iter()
        .map(|(points, _, _)| points.last().map_or(0.5, |(x, _)| *x + 0.5))
        .fold(0.5, f64::max);

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for (points, _, _) in &series {
        for (_, value) in points {
            y_min = y_min.min(*value);
            y_max = y_max.max(*value);
        }
    }
    let (y0, y1) = scalar_y_range(y_min, y_max);

    let shading = model::configuration(selected[0]).and_then(geometry::Geometry::from_config);
    let title = if series.len() == 1 {
        let run = selected[0];
        let mut title = format!("Scalar flux — {}", series[0].1);
        if let Some(k_eff) = model::f64_at(run, "k_eff") {
            title.push_str(&format!(" (k_eff = {k_eff:.6})"));
        }
        title
    } else {
        "Scalar flux vs x".to_string()
    };

    render(path, size, |root| {
        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 28))
            .margin(24)
            .x_label_area_size(56)
            .y_label_area_size(80)
            .build_cartesian_2d(0.0_f64..x_max, y0..y1)?;

        chart
            .configure_mesh()
            .x_desc("x [cm]")
            .y_desc("Scalar flux")
            .light_line_style(RGBColor(225, 225, 225))
            .draw()?;

        draw_regions(&mut chart, shading.as_ref(), y0, y1, zone_labels)?;

        for (points, label, color) in &series {
            if points.is_empty() {
                continue;
            }
            let style = color.stroke_width(2);
            chart
                .draw_series(LineSeries::new(points.clone(), style))?
                .label(label.clone())
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 24, y)], color.stroke_width(2))
                });
            if points.len() <= 400 {
                chart.draw_series(
                    points
                        .iter()
                        .map(|point| Circle::new(*point, 3, color.filled())),
                )?;
            }
        }

        if series.len() > 1 {
            chart
                .configure_series_labels()
                .position(SeriesLabelPosition::UpperRight)
                .background_style(WHITE.mix(0.85))
                .border_style(RGBColor(160, 160, 160))
                .draw()?;
        }
        Ok(())
    })
}

/// Angular flux versus x, one line per direction (single run).
///
/// # Errors
///
/// Returns a message when the run has no angular flux or the mesh does not
/// match.
pub fn angular(
    payload: &Value,
    index: usize,
    path: &Path,
    size: PlotSize,
    zone_labels: bool,
) -> Result<(), String> {
    let runs = model::runs(payload)?;
    let run = runs
        .get(index)
        .ok_or_else(|| format!("run not found: {index}"))?;
    let angular = run
        .get("angular_flux")
        .and_then(Value::as_array)
        .ok_or_else(|| "run has no angular_flux".to_string())?;
    if angular.is_empty() {
        return Err("run has no angular_flux".to_string());
    }

    let centers = cell_centers(run, angular.len())?;
    let miu = model::f64_list(run, "miu");
    let directions = angular
        .iter()
        .filter_map(Value::as_array)
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    if directions == 0 {
        return Err("run has no angular_flux".to_string());
    }

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for row in angular {
        for value in row.as_array().into_iter().flatten() {
            if let Some(value) = value.as_f64() {
                y_min = y_min.min(value);
                y_max = y_max.max(value);
            }
        }
    }
    let (y0, y1) = symmetric_range(y_min, y_max);

    let shading = model::configuration(run).and_then(geometry::Geometry::from_config);
    let x_max = centers.last().copied().unwrap_or(0.0) + 0.5;
    let label = run_label(run, index);

    render(path, size, |root| {
        let mut chart = ChartBuilder::on(root)
            .caption(format!("Angular flux — {label}"), ("sans-serif", 28))
            .margin(24)
            .x_label_area_size(56)
            .y_label_area_size(80)
            .build_cartesian_2d(0.0_f64..x_max, y0..y1)?;

        chart
            .configure_mesh()
            .x_desc("x [cm]")
            .y_desc("Angular flux ψ(x, μ)")
            .light_line_style(RGBColor(225, 225, 225))
            .draw()?;

        draw_regions(&mut chart, shading.as_ref(), y0, y1, zone_labels)?;

        for direction in 0..directions {
            let mut points = Vec::with_capacity(centers.len());
            for (cell, row) in angular.iter().enumerate() {
                let value = row
                    .as_array()
                    .and_then(|values| values.get(direction))
                    .and_then(Value::as_f64);
                if let Some(value) = value {
                    points.push((centers[cell], value));
                }
            }
            if points.is_empty() {
                continue;
            }
            let color = direction_color(direction, directions);
            let mu = miu.get(direction).copied();
            let legend = mu.map_or_else(
                || format!("direction {}", direction + 1),
                |mu| format!("μ={mu:+.6}"),
            );
            chart
                .draw_series(LineSeries::new(points, color.stroke_width(2)))?
                .label(legend)
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 24, y)], color.stroke_width(2))
                });
        }

        chart
            .configure_series_labels()
            .position(SeriesLabelPosition::UpperRight)
            .background_style(WHITE.mix(0.85))
            .border_style(RGBColor(160, 160, 160))
            .draw()?;
        Ok(())
    })
}

/// `k_eff` history of the power iteration.
///
/// # Errors
///
/// Returns a message when no selected run has a history.
pub fn k_history(
    payload: &Value,
    indices: &[usize],
    path: &Path,
    size: PlotSize,
) -> Result<(), String> {
    let runs = model::runs(payload)?;
    let mut series: Vec<Series> = Vec::new();
    for (order, &index) in indices.iter().enumerate() {
        let run = &runs[index];
        let history = model::f64_list(run, "k_history");
        if history.is_empty() {
            continue;
        }
        let points = history
            .iter()
            .enumerate()
            .map(|(iteration, value)| (iteration as f64 + 1.0, *value))
            .collect();
        series.push((
            points,
            run_label(run, index),
            PALETTE[order % PALETTE.len()],
        ));
    }
    if series.is_empty() {
        return Err("no selected run has a k_eff history".to_string());
    }

    let x_max = series
        .iter()
        .map(|(points, _, _)| points.last().map_or(1.0, |(x, _)| *x))
        .fold(1.0, f64::max);
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for (points, _, _) in &series {
        for (_, value) in points {
            y_min = y_min.min(*value);
            y_max = y_max.max(*value);
        }
    }
    let (y0, y1) = padded_range(y_min, y_max, false);

    render(path, size, |root| {
        let mut chart = ChartBuilder::on(root)
            .caption("k_eff history", ("sans-serif", 28))
            .margin(24)
            .x_label_area_size(56)
            .y_label_area_size(80)
            .build_cartesian_2d(0.0_f64..x_max, y0..y1)?;

        chart
            .configure_mesh()
            .x_desc("outer iteration")
            .y_desc("k_eff")
            .light_line_style(RGBColor(225, 225, 225))
            .draw()?;

        for (points, label, color) in &series {
            chart
                .draw_series(LineSeries::new(points.clone(), color.stroke_width(2)))?
                .label(label.clone())
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 24, y)], color.stroke_width(2))
                });
            chart.draw_series(
                points
                    .iter()
                    .map(|point| Circle::new(*point, 3, color.filled())),
            )?;
        }

        if series.len() == 1
            && let Some(final_k) = series[0].0.last().map(|(_, value)| *value)
            && final_k >= y0
            && final_k <= y1
        {
            chart.draw_series(std::iter::once(PathElement::new(
                vec![(0.0, final_k), (x_max, final_k)],
                RGBColor(120, 120, 120).stroke_width(1),
            )))?;
            chart.draw_series(std::iter::once(Text::new(
                format!("k_eff = {final_k:.8}"),
                (x_max * 0.62, final_k + (y1 - y0) * 0.03),
                ("sans-serif", 16).into_font().color(&RGBColor(60, 60, 60)),
            )))?;
        }

        if series.len() > 1 {
            chart
                .configure_series_labels()
                .position(SeriesLabelPosition::UpperRight)
                .background_style(WHITE.mix(0.85))
                .border_style(RGBColor(160, 160, 160))
                .draw()?;
        }
        Ok(())
    })
}

/// Scalar flux at fixed positions versus a sweep parameter (`datos.json`).
///
/// # Errors
///
/// Returns a message when the payload has no data or no positions.
pub fn scs(datos: &Value, x_key: &str, path: &Path, size: PlotSize) -> Result<(), String> {
    let rows = datos
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "datos.json has no data[]".to_string())?;
    let positions: Vec<usize> = datos
        .get("positions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_u64)
                .map(|v| v as usize)
                .collect()
        })
        .unwrap_or_default();
    if rows.is_empty() || positions.is_empty() {
        return Err("datos.json has no rows/positions".to_string());
    }

    let cols = positions.len().min(3);
    let rows_count = positions.len().div_ceil(cols);
    let key = x_key.to_string();

    render(path, size, |root| {
        let areas = root.split_evenly((rows_count, cols));
        for (panel, position) in positions.iter().enumerate() {
            let area = &areas[panel];
            let flux_key = format!("flux_{position}");
            let mut points: Vec<(f64, f64)> = Vec::new();
            for row in rows {
                let x = row.get(&key).and_then(Value::as_f64);
                let y = row.get(&flux_key).and_then(Value::as_f64);
                if let (Some(x), Some(y)) = (x, y) {
                    points.push((x, y));
                }
            }
            points.sort_by(|a, b| a.0.total_cmp(&b.0));

            let (x0, x1) = if points.is_empty() {
                (0.0, 1.0)
            } else {
                padded_range(
                    points.iter().map(|(x, _)| *x).fold(f64::INFINITY, f64::min),
                    points
                        .iter()
                        .map(|(x, _)| *x)
                        .fold(f64::NEG_INFINITY, f64::max),
                    false,
                )
            };
            let (y0, y1) = if points.is_empty() {
                (0.0, 1.0)
            } else {
                padded_range(
                    points.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min),
                    points
                        .iter()
                        .map(|(_, y)| *y)
                        .fold(f64::NEG_INFINITY, f64::max),
                    true,
                )
            };

            let mut chart = ChartBuilder::on(area)
                .caption(format!("position {position}"), ("sans-serif", 20))
                .margin(12)
                .x_label_area_size(40)
                .y_label_area_size(56)
                .build_cartesian_2d(x0..x1, y0..y1)?;
            chart
                .configure_mesh()
                .x_desc(key.clone())
                .y_desc("flux")
                .light_line_style(RGBColor(230, 230, 230))
                .draw()?;
            if points.is_empty() {
                chart.draw_series(std::iter::once(Text::new(
                    "no data",
                    (x0 + (x1 - x0) * 0.4, y0 + (y1 - y0) * 0.5),
                    ("sans-serif", 16)
                        .into_font()
                        .color(&RGBColor(120, 120, 120)),
                )))?;
            } else {
                let color = PALETTE[panel % PALETTE.len()];
                chart.draw_series(LineSeries::new(points.clone(), color.stroke_width(2)))?;
                chart.draw_series(
                    points
                        .iter()
                        .map(|point| Circle::new(*point, 3, color.filled())),
                )?;
            }
        }
        Ok(())
    })
}

fn draw_regions<DB, X, Y>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<X, Y>>,
    geometry: Option<&geometry::Geometry>,
    y0: f64,
    y1: f64,
    labels: bool,
) -> Result<(), DrawError>
where
    DB: DrawingBackend,
    DB::ErrorType: std::error::Error + Send + Sync + 'static,
    X: Ranged<ValueType = f64>,
    Y: Ranged<ValueType = f64>,
{
    let Some(geometry) = geometry else {
        return Ok(());
    };
    let shading = RGBColor(31, 78, 120);
    for region in 0..geometry.region_zones.len() {
        let xa = geometry.edges[region];
        let xb = geometry.edges[region + 1];
        let alpha = if region % 2 == 0 { 0.07 } else { 0.03 };
        chart.draw_series(std::iter::once(Rectangle::new(
            [(xa, y0), (xb, y1)],
            shading.mix(alpha).filled(),
        )))?;
        if labels || geometry.zone_names.is_some() {
            chart.draw_series(std::iter::once(Text::new(
                geometry.zone_label(region),
                ((xa + xb) * 0.5, y1 - (y1 - y0) * 0.06),
                ("sans-serif", 14).into_font().color(&RGBColor(90, 90, 90)),
            )))?;
        }
    }
    for edge in geometry
        .edges
        .iter()
        .skip(1)
        .take(geometry.region_zones.len().saturating_sub(1))
    {
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(*edge, y0), (*edge, y1)],
            RGBColor(170, 170, 170),
        )))?;
    }
    Ok(())
}

fn cell_centers(run: &Value, count: usize) -> Result<Vec<f64>, String> {
    let Some(configuration) = model::configuration(run) else {
        return Ok((0..count).map(|cell| cell as f64 + 0.5).collect());
    };
    match geometry::Geometry::from_config(configuration) {
        Some(geometry) if geometry.cells() == count => Ok(geometry.centers),
        Some(geometry) => Err(format!(
            "scalar_flux ({count}) does not match NC ({})",
            geometry.cells()
        )),
        None => Ok((0..count).map(|cell| cell as f64 + 0.5).collect()),
    }
}

fn run_label(run: &Value, index: usize) -> String {
    model::run_name(run, index)
}

fn scalar_y_range(min: f64, max: f64) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    let low = min.min(0.0);
    let span = (max - min).max(f64::MIN_POSITIVE);
    (low, max + 0.18 * span)
}

fn symmetric_range(min: f64, max: f64) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() {
        return (-1.0, 1.0);
    }
    let bound = min.abs().max(max.abs()).max(f64::MIN_POSITIVE);
    (-bound * 1.05, bound * 1.05)
}

fn padded_range(min: f64, max: f64, include_zero: bool) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    let mut low = if include_zero { min.min(0.0) } else { min };
    let mut high = max;
    if (high - low).abs() < f64::EPSILON {
        let pad = low.abs().max(1.0) * 0.05;
        low -= pad;
        high += pad;
    } else {
        let pad = (high - low) * 0.05;
        low -= pad;
        high += pad;
        if include_zero {
            low = low.min(0.0);
        }
    }
    (low, high)
}

fn direction_color(index: usize, total: usize) -> RGBColor {
    if total <= 1 {
        return RGBColor(8, 48, 107);
    }
    let t = index as f64 / (total - 1) as f64;
    let mix = |start: f64, end: f64| (start + (end - start) * t).round() as u8;
    RGBColor(mix(33.0, 203.0), mix(102.0, 24.0), mix(172.0, 29.0))
}
