//! Excel export of a consolidated 1D output.

use std::path::Path;

use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, Worksheet, XlsxError};
use serde_json::Value;

use super::{geometry, model};

/// Write a workbook with the reference sheet layout.
///
/// # Errors
///
/// Returns a message when a sheet cannot be written or the file cannot be
/// saved.
pub fn write_workbook(payload: &Value, runs: &[Value], path: &Path) -> Result<(), String> {
    let header = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x1F_4E_78))
        .set_align(FormatAlign::Center);
    let default = Format::new();
    let k_format = Format::new().set_num_format("0.0000000000");
    let flux_format = Format::new().set_num_format("0.000000E+00");
    let mu_format = Format::new().set_num_format("0.000000");
    let x_format = Format::new().set_num_format("0.0000");

    let mut workbook = Workbook::new();

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Metadata").map_err(error_text)?;
        let rows = vec![
            vec![
                Value::String("solver".into()),
                payload.get("solver").cloned().unwrap_or(Value::Null),
            ],
            vec![
                Value::String("generated_at".into()),
                payload.get("generated_at").cloned().unwrap_or(Value::Null),
            ],
            vec![Value::String("total".into()), Value::from(runs.len())],
            vec![
                Value::String("failures".into()),
                payload.get("failures").cloned().unwrap_or(Value::Null),
            ],
            vec![
                Value::String("spatial_schemes".into()),
                Value::String(
                    payload
                        .get("spatial_schemes")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default(),
                ),
            ],
        ];
        write_table(
            sheet,
            &["Field".to_string(), "Value".to_string()],
            &rows,
            &[&default, &default],
            &default,
            &header,
        )?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Runs").map_err(error_text)?;
        let headers = vec![
            "run",
            "mode",
            "spatial_scheme",
            "k_eff",
            "k_balance",
            "k_converged",
            "iteration",
            "iteration_eigenvalue",
            "outer_iterations",
            "converged",
            "power_w",
            "power_scale",
            "power_unnormalized",
            "timestamp",
            "source",
            "error",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let formats: Vec<&Format> = vec![
            &default,
            &default,
            &default,
            &k_format,
            &k_format,
            &default,
            &default,
            &default,
            &default,
            &default,
            &flux_format,
            &flux_format,
            &flux_format,
            &default,
            &default,
            &default,
        ];
        let rows: Vec<Vec<Value>> = runs
            .iter()
            .enumerate()
            .map(|(index, run)| {
                let name = model::run_name(run, index);
                vec![
                    Value::String(name),
                    get(run, "mode"),
                    get(run, "spatial_scheme"),
                    get(run, "k_eff"),
                    get(run, "k_balance"),
                    get(run, "k_converged"),
                    get(run, "iteration"),
                    get(run, "iteration_eigenvalue"),
                    get(run, "outer_iterations"),
                    get(run, "converged"),
                    get(run, "power_w"),
                    get(run, "power_scale"),
                    get(run, "power_unnormalized"),
                    get(run, "timestamp"),
                    get(run, "source"),
                    get(run, "error"),
                ]
            })
            .collect();
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Zones").map_err(error_text)?;
        let headers = vec!["run", "zone", "Sigma_t", "Sigma_s", "nu_Sigma_f", "Sigma_f"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let formats = vec![&default; 6];
        let mut rows = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let name = model::run_name(run, index);
            let configuration = model::configuration(run).cloned().unwrap_or(Value::Null);
            let sct = model::f64_list(&configuration, "SCT");
            let scs = model::f64_list(&configuration, "SCS");
            let nu_sigf = model::f64_list(&configuration, "NU_SIGF");
            let sigma_f = model::f64_list(&configuration, "SIGMA_F");
            let zones = sct
                .len()
                .max(scs.len())
                .max(nu_sigf.len())
                .max(sigma_f.len());
            for zone in 0..zones {
                rows.push(vec![
                    Value::String(name.clone()),
                    Value::from(zone + 1),
                    number_at(&sct, zone),
                    number_at(&scs, zone),
                    number_at(&nu_sigf, zone),
                    number_at(&sigma_f, zone),
                ]);
            }
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Regions").map_err(error_text)?;
        let headers = vec!["run", "region", "NC", "HR_cm", "IZL", "Q"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let formats = vec![&default; 6];
        let mut rows = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let name = model::run_name(run, index);
            let configuration = model::configuration(run).cloned().unwrap_or(Value::Null);
            let nc = model::f64_list(&configuration, "NC");
            let hr = model::f64_list(&configuration, "HR");
            let izl = model::f64_list(&configuration, "IZL");
            let q = model::f64_list(&configuration, "Q");
            let regions = nc.len().max(hr.len()).max(izl.len()).max(q.len());
            for region in 0..regions {
                rows.push(vec![
                    Value::String(name.clone()),
                    Value::from(region + 1),
                    number_at(&nc, region),
                    number_at(&hr, region),
                    number_at(&izl, region),
                    number_at(&q, region),
                ]);
            }
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Boundaries").map_err(error_text)?;
        let headers = vec!["run", "side", "direction", "value"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let formats = vec![&default; 4];
        let mut rows = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let name = model::run_name(run, index);
            let configuration = model::configuration(run).cloned().unwrap_or(Value::Null);
            for (side, key) in [("left", "bound_left"), ("right", "bound_right")] {
                let values = model::f64_list(&configuration, key);
                for (direction, value) in values.iter().enumerate() {
                    rows.push(vec![
                        Value::String(name.clone()),
                        Value::String(side.into()),
                        Value::from(direction + 1),
                        Value::from(*value),
                    ]);
                }
            }
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("ScalarFlux").map_err(error_text)?;
        let mut headers = vec!["cell".to_string()];
        let mut fluxes = Vec::new();
        let mut length = 0usize;
        for (index, run) in runs.iter().enumerate() {
            headers.push(model::run_name(run, index));
            let flux = model::f64_list(run, "scalar_flux");
            length = length.max(flux.len());
            fluxes.push(flux);
        }
        let mut formats: Vec<&Format> = vec![&default];
        formats.extend(std::iter::repeat_n(&flux_format, fluxes.len()));

        let mut rows = Vec::new();
        for cell in 0..length {
            let mut row = vec![Value::from(cell + 1)];
            for flux in &fluxes {
                row.push(number_at(flux, cell));
            }
            rows.push(row);
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("AngularFlux").map_err(error_text)?;
        let headers = vec!["run", "cell", "x_cm", "direction", "mu", "psi"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let formats = vec![
            &default,
            &default,
            &x_format,
            &default,
            &mu_format,
            &flux_format,
        ];
        let mut rows = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let name = model::run_name(run, index);
            let Some(angular) = run.get("angular_flux").and_then(Value::as_array) else {
                continue;
            };
            if angular.is_empty() {
                continue;
            }
            let configuration = model::configuration(run);
            let centers = geometry::centers_or_indices(configuration, angular.len());
            let miu = model::f64_list(run, "miu");
            for (cell, values) in angular.iter().enumerate() {
                let Some(values) = values.as_array() else {
                    continue;
                };
                for (direction, psi) in values.iter().enumerate() {
                    rows.push(vec![
                        Value::String(name.clone()),
                        Value::from(cell + 1),
                        centers.get(cell).map_or(Value::Null, |x| Value::from(*x)),
                        Value::from(direction + 1),
                        number_at(&miu, direction),
                        psi.clone(),
                    ]);
                }
            }
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Directions").map_err(error_text)?;
        let headers = vec!["run", "direction", "mu", "omega"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let formats = vec![&default, &default, &mu_format, &mu_format];
        let mut rows = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let name = model::run_name(run, index);
            let miu = model::f64_list(run, "miu");
            let omega = model::f64_list(run, "omega");
            let directions = miu.len().max(omega.len());
            for direction in 0..directions {
                rows.push(vec![
                    Value::String(name.clone()),
                    Value::from(direction + 1),
                    number_at(&miu, direction),
                    number_at(&omega, direction),
                ]);
            }
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("k_history").map_err(error_text)?;
        let headers = vec!["run", "outer_iteration", "k_eff"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let formats = vec![&default, &default, &k_format];
        let mut rows = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let name = model::run_name(run, index);
            for (iteration, k_eff) in model::f64_list(run, "k_history").iter().enumerate() {
                rows.push(vec![
                    Value::String(name.clone()),
                    Value::from(iteration + 1),
                    Value::from(*k_eff),
                ]);
            }
        }
        write_table(sheet, &headers, &rows, &formats, &default, &header)?;
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    workbook.save(path).map_err(error_text)?;
    Ok(())
}

fn error_text(error: XlsxError) -> String {
    error.to_string()
}

fn get(value: &Value, key: &str) -> Value {
    value.get(key).cloned().unwrap_or(Value::Null)
}

fn number_at(values: &[f64], index: usize) -> Value {
    values
        .get(index)
        .map_or(Value::Null, |value| Value::from(*value))
}

fn write_table(
    sheet: &mut Worksheet,
    headers: &[String],
    rows: &[Vec<Value>],
    formats: &[&Format],
    default: &Format,
    header: &Format,
) -> Result<(), String> {
    for (column, title) in headers.iter().enumerate() {
        sheet
            .write_string_with_format(0, column as u16, title, header)
            .map_err(error_text)?;
    }
    for (row, values) in rows.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let format = formats.get(column).copied().unwrap_or(default);
            write_value(sheet, (row + 1) as u32, column as u16, value, format)?;
        }
    }
    if !rows.is_empty() {
        sheet
            .autofilter(0, 0, rows.len() as u32, (headers.len() - 1) as u16)
            .map_err(error_text)?;
    }
    sheet.set_freeze_panes(1, 0).map_err(error_text)?;
    sheet.autofit();
    Ok(())
}

fn write_value(
    sheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: &Value,
    format: &Format,
) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        Value::Bool(value) => sheet
            .write_boolean_with_format(row, column, *value, format)
            .map(|_| ())
            .map_err(error_text),
        Value::Number(value) => {
            let number = value.as_f64().unwrap_or_default();
            sheet
                .write_number_with_format(row, column, number, format)
                .map(|_| ())
                .map_err(error_text)
        }
        Value::String(value) => sheet
            .write_string_with_format(row, column, value, format)
            .map(|_| ())
            .map_err(error_text),
        other => sheet
            .write_string_with_format(row, column, other.to_string(), format)
            .map(|_| ())
            .map_err(error_text),
    }
}
