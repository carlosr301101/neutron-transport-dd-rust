# Neutron Transport with Domain Decomposition (Rust)

A Rust port of the 1D S_N (discrete ordinates) neutron transport workflow of
[`neutron-transport-PINNs`](https://github.com/carlosr301101/neutron-transport-PINNs).
It solves the one-dimensional transport equation with Diamond Difference,
Step and Step Characteristic spatial schemes, including reflective boundaries
and multiplicative media (fission, `k_eff`, power iteration).

This repository implements **steps 0–3 and 5** of the rewrite: the solver
core, the configuration layer, the runner, the inspection commands, logging,
the configuration generator with placeholder sweeps, the post-processing tools
and the ratatui TUI. The 2D workflow (step 4, intentionally deferred) and the
PINN dataset/packaging step are pending.

## Features

- **S_N sweep** with configurable quadrature (S2, S4, S6, S8).
- **Three spatial schemes**: `DD`, `STEP`, `SC` (selectable or forced).
- **Four physics modes**: `fixed_source`, `fixed_k`, `driven`, `eigenvalue`
  (power iteration for `k_eff`).
- **Power normalization** (`POWER_KW`, `E_F_MEV`, `NU` or `SIGMA_F`).
- **Python-compatible output**: `output_1d_###.json` keeps the exact schema
  of the reference implementation, so `nts-1d-data`, `nts-1d-plot` and
  `nts-1d-excel` keep working.
- **Inspection commands**: `validate`, `list`, `show` and `status`, all with
  `--json`.
- **Parameter sweeps**: `config` writes a template with `kk`/`mm`
  placeholders; `expand` turns ranges into a list of validated configs.
- **Post-processing**: `data` (`datos.json`), `normalize` (target power),
  `excel` (9-sheet workbook) and `plot` (PNG: scalar, angular, k-history, sweeps).
- **TUI** (`nts-rs tui`): config editor, background runs with live log and
  cancellation, results browser, placeholder sweeps, plots and export.
- **Structured logging**: daily global log plus one log per run.
- **Interactive wizard** (`--manual`) with the same question order and
  defaults as the Python CLI.
- **No runtime data files**: quadrature data is embedded in the binary.

## Build

```bash
cargo build --release
```

The binary is `target/release/nts-rs`.

## CLI

Run one configuration or a JSON list:

```bash
nts-rs run --json data/example_eigenvalue.json -o outputs/results/eigen.json
nts-rs run --json data/input.json --verbose
```

Run the interactive wizard:

```bash
nts-rs run --manual
```

Options:

| Flag | Description |
|------|-------------|
| `--json FILE` | One config object or a list of objects |
| `--manual` | Interactive step-by-step configuration |
| `-o, --output PATH` | Output JSON (default: `outputs/results/output_1d_###.json`) |
| `--scheme DD\|STEP\|SC` | Force the spatial scheme, ignoring `SPATIAL_SCHEME` |
| `-e, --epsilon TOL` | Flux convergence tolerance (default `1e-5`) |
| `-m, --max-iter N` | Maximum flux updates (default `2000`) |

Global options accepted by every subcommand:

| Flag | Description |
|------|-------------|
| `-v, --verbose` | Debug logging and per-run progress (repeat for trace) |
| `-q, --quiet` | Only errors |
| `--log-level LEVEL` | Explicit level: `error`, `warn`, `info`, `debug`, `trace` |

The work directory is `NTS_HOME` when set, otherwise the current directory.
Runs write `outputs/results/output_1d_###.json`, auto-numbered.

Exit codes: `0` success, `1` one or more runs failed, `130` cancelled.

### Inspection commands

```bash
nts-rs validate data/example_eigenvalue.json          # check a config (or list)
nts-rs validate configs.json --json                   # machine-readable result
nts-rs list inputs                                    # outputs/inputs
nts-rs list outputs --json                            # solver, runs, failures
nts-rs show outputs/results/output_1d_001.json        # per-run summary
nts-rs status                                         # dirs, counts, 2D solvers
```

- `validate` accepts an object or a list and reports the effective regions,
  zones, cells, scheme, mode and boundaries. 2D configs are reported as
  pending (step 4).
- `list` reports size and modification time; 1D outputs also show the solver,
  number of runs and failures.
- `show` prints convergence, iterations, `k_eff`, `k_balance` and power per
  run; `--json` dumps the raw payload.
- `status` never fails because of missing 2D binaries: it warns. It exits `1`
  only on real I/O errors.

### Configuration generator and sweeps

```bash
# Interactive template; numeric fields accept kk/mm placeholders
nts-rs config                         # -> outputs/inputs/config_1d_001.json

# Expand ranges (one value per step, zipped across placeholders)
nts-rs expand --json outputs/inputs/config_1d_001.json \
    --place-holder kk 0.1 0.5 0.2 \
    --place-holder mm 0.05 0.15 0.05 \
    -o outputs/inputs/inputs_1d.json

nts-rs run --json outputs/inputs/inputs_1d.json
```

- A placeholder is an identifier (`[A-Za-z_][A-Za-z0-9_]*`); `MODE`,
  `SPATIAL_SCHEME` and `ZONE_NAMES` are never treated as placeholders.
- Ranges are inclusive and the last point is clipped to `END` when
  floating-point error pushes it slightly past it.
- Values are rounded to the decimal places of the `STEP`; rounding applies only
  to the injected values (so `TOL_K = 1e-6` survives).
- Different range lengths use the shortest one (zip semantics) and print a
  warning.
- Every expanded configuration is validated; invalid steps are skipped and the
  command exits `1`. The output is always a JSON list.
- Python module output (`.py`) is not supported; use JSON.

### Logging

Every command writes:

- `outputs/logs/nts-rs.log.YYYY-MM-DD` (daily, `trace` level) — rotation is by
  day;
- `outputs/logs/runs/<timestamp>_<run>.log` for `run`, with the effective
  configuration and the result summary.

The console level is `warn` by default; `-v` switches to `debug`, `-vv` to
`trace`, and `-q` to `error`. `--log-level` overrides both.

### Post-processing

```bash
# Scalar fluxes at cell positions -> outputs/results/datos.json
nts-rs data --latest --positions 0 50 100
nts-rs data -r outputs/results/output_1d_001.json --positions 0 100

# Scale an existing output to a target power
nts-rs normalize outputs/results/output_1d_001.json --power-kw 10 \
    [--nu 2.4] [--ef-mev 200] [--area-cm2 1] [--run 0]

# Excel workbook (Metadata, Runs, Zones, Regions, Boundaries,
# ScalarFlux, AngularFlux, Directions, k_history)
nts-rs excel outputs/results/output_1d_001.json [-o book.xlsx]

# PNG plots (1600x900 by default, --width/--height to override)
nts-rs plot -r outputs/results/output_1d_001.json --kind scalar
nts-rs plot -r outputs/results/output_1d_001.json --kind angular
nts-rs plot -r outputs/results/output_1d_001.json --kind k-history
nts-rs plot -i outputs/results/datos.json --kind scs --x SCS
```

- `--run` accepts a 0-based index, a `config_name`, or `all` (except
  `angular`, which requires exactly one run).
- `data` prefers the embedded configuration and falls back to `--inputs`
  (object of name → config or a list).
- `normalize` derives `Sigma_f` from `SIGMA_F` or `NU_SIGF/NU` (default 2.49),
  scales scalar and angular fluxes, and writes `<stem>_normalized.json`.
- `plot` shades material regions and labels them with `ZONE_NAMES`; `scs`
  renders one panel per position. `--show` opens the file with the platform
  viewer.
- Plots use a bundled font (Liberation Sans, SIL OFL) and the bitmap backend,
  so no system font discovery or graphics library is required.

## TUI

```bash
nts-rs tui
```

Built with `ratatui` + `crossterm`. Requires an interactive terminal of at
least 80×24; the terminal is restored on exit and on panic.

| Screen | What it does | Keys |
|--------|--------------|------|
| **Config** | Editable form with collapsible sections and per-zone groups: regions, materials, boundaries, physics, power, `epsilon`/`max_iter` and `ZONE_NAMES`. Placeholders (`kk`, `mm`) are accepted in numeric fields. `IZL`, `N`, scheme and mode open a dropdown. | `↑/↓` move, `←/→` fold/cycle, `Enter` open/toggle, `e` edit as text, `z`/`Z` collapse all, `s` save, `v` validate |
| **Run** | Lists `outputs/inputs`, runs the selected config in a worker thread with a live log. | `r` run, `c` cancel |
| **Results** | Output list and a runs table (`mode`, scheme, convergence, iterations, `k_eff`, `k_balance`). | `↑/↓` select |
| **Placeholders** | Range editor (`name`, `start`, `end`, `step`) and expansion over the selected config. | `a` add, `d` delete, `←/→` column, `Enter` edit, `x` expand |
| **Plots** | Generates scalar, angular, k-history or SCS plots for the selected output. | `k`/`←/→` kind, `g` generate, `o` open |
| **Export** | `datos.json`, Excel workbook and SCS plot. | `d`, `x`, `s`, `o` open |

Configuration, expansion, plotting, Excel and `datos.json` use the same library
code as the CLI, so the outputs are identical. Cancellation is checked inside
the solver between iterations.

### Mouse

Mouse capture is enabled by default and can be toggled with `m` (needed to
select/copy text with the terminal mouse). Left click acts on the matching
widget:

- tabs switch the screen,
- section and zone headers collapse/expand,
- form rows select; a second click opens the editor (or the dropdown for
  `IZL`, `N`, scheme and mode),
- lists and the runs table select; buttons run, generate and export,
- the wheel scrolls the form, lists, log and tables.

Every button also has a keyboard shortcut, so the TUI works without a mouse.

Optional inline plot preview (sixel/kitty/halfblocks):

```bash
cargo build --release --features images
```

## Library usage

```rust
use neutron_transport_dd::config::Config;
use neutron_transport_dd::core::Runner;

let config = Config::from_json_file(
    "data/example_eigenvalue.json",
    1e-5,   // epsilon
    2000,   // max_iter
    None,   // forced scheme
)?;

let result = Runner::new(config)?.run()?;
println!("k_eff = {:?}", result.k_eff);
println!("converged = {}", result.converged);
```

## Configuration

```json
{
  "num_regions": 1,
  "num_zones": 1,
  "NC": [100],
  "HR": [20.0],
  "IZL": [1],
  "SCT": [1.0],
  "SCS": [0.9],
  "NU_SIGF": [0.11],
  "Q": [0.0],
  "N": 8,
  "SPATIAL_SCHEME": "DD",
  "reflex_izq": false,
  "reflex_der": false,
  "bound_left": [0, 0, 0, 0],
  "bound_right": [0, 0, 0, 0],
  "MODE": "eigenvalue",
  "TOL_K": 1e-6,
  "MAX_OUTER": 200,
  "epsilon": 1e-5,
  "max_iter": 2000
}
```

| Key | Type | Description |
|-----|------|-------------|
| `num_regions` | int | Number of spatial regions |
| `num_zones` | int | Number of material zones (1-based) |
| `NC` | int[] | Cells per region |
| `HR` | float[] | Thickness per region \[cm\] |
| `IZL` | int[] | Material zone of each region |
| `SCT`, `SCS` | float[] | Total / scattering cross-section per zone |
| `Q` | float[] | External isotropic source per region |
| `N` | int | Quadrature order: 2, 4, 6 or 8 (even) |
| `SPATIAL_SCHEME` | string | `DD` (default), `STEP` or `SC` |
| `reflex_izq`, `reflex_der` | bool | Reflective boundaries (`0`/`1` also accepted) |
| `bound_left`, `bound_right` | float or float[] | Incoming flux; a scalar expands to `N/2` values |
| `NU_SIGF` | float[] | `nu * Sigma_f` per zone, default zeros |
| `SIGMA_F` | float[]? | `Sigma_f` per zone (alternative to `NU_SIGF` for power) |
| `MODE` | string | `fixed_source`, `fixed_k`, `driven`, `eigenvalue`; inferred when omitted |
| `K_EFF`, `K0` | float? | Imposed / initial `k_eff` |
| `TOL_K`, `MAX_OUTER` | float / int | Power-iteration tolerance and cap |
| `epsilon`, `max_iter` | float / int | Flux tolerance and iteration cap; CLI flags override them when passed |
| `POWER_KW`, `E_F_MEV`, `NU`, `AREA_CM2` | float? | Power normalization |
| `ZONE_NAMES` | string[]? | Optional presentation metadata |

Keys starting with `_` are treated as comments. The effective configuration is
embedded in every output run under `configuration`.

`epsilon` and `max_iter` follow the precedence **CLI flag > JSON > default**
(`1e-5`, `2000`). This makes configurations generated by the Python TUI run
with the same tolerances; `nts-rs run --epsilon ...` still overrides them.
`ZONE_NAMES` is preserved by `run`, `expand`, `normalize` and the TUI.

## Output

`output_1d_###.json`:

```text
{
  "solver": "NTS_DD_1D",
  "spatial_schemes": ["DD"],
  "runs": [ { "config_name", "configuration", "scalar_flux", "angular_flux",
              "miu", "omega", "iteration", "iteration_eigenvalue",
              "converged", "mode", "spatial_scheme", "k_eff", "k_balance",
              "k_history", "k_converged", "outer_iterations", "power_w",
              "power_scale", "power_unnormalized", "source", "timestamp" } ],
  "total": 1,
  "failures": 0,
  "generated_at": "..."
}
```

A failed configuration is recorded as `{"config_name", "error", "timestamp"}`
and does not abort the remaining runs.

Keys are written in the order shown: the run identifier and its
`configuration` first, then the calculation results starting with
`scalar_flux`. This is stable output order (not alphabetical), so the file is
easier to inspect with `jq` or a text editor.

## Numerical parity

The test-suite reproduces the Python reference results exactly (f64):

| Case | Reference |
|------|-----------|
| S2 20 cm slab, DD, `epsilon=1e-5` | iteration counter and fluxes equal to 1e-12 |
| 20 cm vacuum slab, S8, 100 cells, `epsilon=1e-6` | 999 sweeps, `k_eff = 1.0266764379437188` |
| Same, `epsilon=1e-5` | 623 sweeps, `k_eff = 1.0266605744526358` |
| Injected SC scheme, `epsilon=1e-6` | `k_eff = 1.026054` |
| Reflective infinite medium | `k_eff = nu*Sigma_f / Sigma_a` |

## Project layout

```text
src/
  config.rs       JSON schema, validation, per-cell data, as_dict
  quadrature.rs   Embedded S2/S4/S6/S8 Gauss-Legendre data
  scheme.rs       DD / STEP / SC cell kernels and theta
  power.rs        Fission integral and power normalization
  core/runner.rs  Transport sweep, modes, k_eff, balance
  result.rs       RunResult and output payload
  paths.rs        NTS_HOME / outputs/results / auto-numbering
  wizard.rs       Interactive --manual configuration
  placeholder.rs  Detection, replacement, grids and rounding
  logging.rs      fern console + daily file + per-run logs
  post/           model, geometry, data, normalize, excel, plot
  tui/            app, model, services, screens (ratatui)
  cli/            run, config, expand, data, normalize, excel, plot,
                  validate, list, show, status, tui
  main.rs         Entry point (delegates to cli::main)
assets/fonts/     Liberation Sans + license (bundled plot font)
tests/            Pseudocode equivalence, fission modes, schemes, CLI, ...
```

## Testing

```bash
cargo test          # 212 tests
cargo clippy --all-targets --all-features
cargo fmt --all -- --check
```

See [TESTS.md](TESTS.md) for the test inventory and reference values.

## Not ported yet (next steps)

The full rewrite plan lives in [doc/PLAN.md](doc/PLAN.md).

1. 2D workflow (`SimulationConfig`, `input.txt` builder, parallel C binaries) —
   deferred by design.
2. PINN datasets (`nts-rs dataset`) and packaging/CI polish.

Python `.py` configuration modules (`-c inputs.py`) are not supported; use JSON
with `config`/`expand`.

## Physics references

- Lewis & Miller, *Computational Methods of Neutron Transport* (1984)
- Duderstadt & Hamilton, *Nuclear Reactor Analysis* (1976)
- Lathrop, *Remedial Comments on the Computing Time Required for Solving the S_N Equations* (1968)

## License

MIT. The bundled plot font is Liberation Sans, licensed under the SIL Open
Font License (see `assets/fonts/LICENSE-Liberation.txt`).
