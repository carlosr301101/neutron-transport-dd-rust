# Test suite - Neutron Transport DD

260 tests (72 unit, 187 integration, 1 doc-test) covering configuration,
spatial schemes, the transport sweep, the four physics modes, the interactive
wizard, the CLI inspection commands, logging, placeholder expansion,
post-processing (data, normalize, Excel, plots), cancellation, the TUI (config
form, mouse, accordion, dropdowns, plots/export), Python-TUI configuration
compatibility and the output format.

Run everything with:

```bash
cargo test
```

Useful variants:

```bash
cargo test -- --nocapture          # show prints
cargo test --test schemes          # only one target
cargo clippy --all-targets --all-features
cargo fmt --all -- --check
```

## Unit tests (inside `src/`)

| Module | Cases |
|--------|-------|
| `quadrature` | N/2 table sizes, weights sum to 1, directions in (0,1) and decreasing, odd/untabulated orders rejected |
| `scheme` | name aliases and canonical round-trip, invalid names, theta limits/series/monotonicity, cell balance for DD/STEP/SC, SC→DD thin limit, SC→STEP thick limit, `mu` sign, zero `mu`, no positivity clipping, `prepare` shape |
| `config` | vectorised base config, scalar boundary expansion, bad length, comment keys, integer booleans, odd quadrature, fission inference/validation, `as_dict` round-trip, forced scheme |
| `power` | fission integral factor 2, zero-production normalization error |
| `paths` | zero-padded output name |
| `placeholder` | identifier detection, enum-key skipping, replacement, unresolved checks, inclusive/clipped grids, decimal places, rounding |
| `logging` | run-name sanitisation |
| `post::model` | run selectors (index/name/all), list and scalar access |
| `post::geometry` | centres/edges from NC/HR/IZL, invalid meshes, index fallback |
| `post::normalize` | scaling of scalar/angular fluxes, missing fission data |
| `sweep` | reference grid, invalid ranges, unresolved placeholder |
| `tui::model` | form conversion/validation, placeholders, resizing, quadrature, invalid values |
| `tui::app` | tab cycling, geometry bar, row collapsing/grouping, default selection, dropdown targets |
| `cli` | object/list splitting, 2D detection, verbosity resolution |
| doc-test | README-style eigenvalue example |

## Integration tests (`tests/`)

### `dd_pseudocode.rs` - equivalence with the literal DD algorithm

S2, 20 cm slab, `Sigma_t=1`, `Sigma_s=0.97`, unit incoming flux on the left.
The literal pseudoalgorithm (right sweep, left sweep, relative change) is
reimplemented in the test and compared with `Runner`:

- same iteration counter (exact),
- scalar flux, angular averages and interface fluxes within `1e-12`,
- 5, 10 and 20 cells,
- mode inference when `MODE` is omitted.

### `fission_modes.rs` - multiplicative media

| Test | Expectation |
|------|-------------|
| reflective infinite medium | `k_eff = k_inf = 0.6` (±1e-6) |
| 20 cm vacuum slab, S8, 100 cells | `k_eff = 1.02668` (±2e-4), `k_eff ≈ k_balance` |
| `fixed_k` at the eigenvalue | `k_balance ≈ k*` (±1e-3) |
| driven reflective slab (`k_inf=0.5`) | `k_eff = 1`, `k_balance = 0.5` (±1e-4) |
| `POWER_KW` without `NU` | `NU = 2.49` (U-238), power = 10 kW |
| sweep counters | `iteration_eigenvalue == outer_iterations`; `None` for fixed source |
| power normalization | 10 kW, scale × unnormalized = 10 kW |
| fixed-source regression | no `k_eff`, converged |
| mixed boundaries without source | zero flux |
| mode inference | `NU_SIGF>0`, no `K_EFF`, no `MODE` → eigenvalue |
| invalid combinations | 4 cases rejected |

### `schemes.rs` - spatial schemes at solver level

- default `DD`, alias normalisation, invalid scheme rejected,
- **DD regression**: `epsilon=1e-6` → 999 sweeps, 25 outer iterations,
  `k_eff = 1.0266764379437188`, `k_balance = 1.0266775219239312`,
  `phi[0] = 0.10320820280696363`, `sum(phi) = 67.70760151718076`,
- injected SC scheme with `epsilon=1e-6` → `k_eff = 1.026054` (±1e-5),
- reflective infinite medium `k=0.6` for DD/STEP/SC,
- benchmark agreement: DD vs SC within `1e-3`, DD vs STEP within `2e-2`,
  `k_eff` vs `k_balance` within `1e-4`,
- solver-level cell balance for every scheme (`fixed_k`, residual < 1e-6),
- fixed-source vacuum/reflective cases, symmetry of the reflective slab,
- `scalar_flux = 0.5 * sum(omega * angular_flux)`.

### `convergence.rs` - flux criterion

- old-flux denominator, `|1 - new/old|`,
- negative flux, zero-flux absolute fallback,
- tighter `epsilon` needs more sweeps,
- identical fluxes give zero change.

### `boundary_vector.rs`

Scalar expansion, one-element list expansion, full vector kept, invalid length
message (`bound_left must have 4 elements or one scalar; got 2`), reflective
scalar safety.

### `config_validation.rs`

Missing required field, dimension mismatches, zone index range, negative
`NU_SIGF`, `SIGMA_F > SCT`, `POWER_KW` without fission, `ZONE_NAMES` length and
empty names, unsupported quadrature order, comment keys, integer booleans,
mode inference branches, `as_dict` round-trip, forced scheme, file read errors.

### `manual_wizard.rs`

Scripted answers for: eigenvalue, power with default `NU`, power with
`SIGMA_F`, `fixed_k` with source, driven with/without source, fixed source,
invalid mode, forced scheme (skips the prompt), placeholder rejection in
numeric fields, JSON-serializable summary.

### `cli_basic.rs` - inspection commands and logging

- `validate` valid/invalid configs, list input and `--json` payload.
- 2D configs reported as pending (step 4).
- `list inputs/outputs` text and JSON (solver, runs, failures).
- `show` summary and raw `--json` dump; unknown files rejected.
- `status` counts, `NTS_SOLVERS_DIR` warning, missing binaries do not fail.
- daily log and per-run log files written with configuration and result.
- `-q` suppresses progress, `-v` prints it; `--log-level` accepted.
- `run --manual` through stdin with global flags.
- unsupported JSON shapes rejected.

### `config_cli.rs` - `nts-rs config`

- concrete eigenvalue config saved and accepted by `validate`,
- `kk`/`mm` placeholders preserved as strings plus expansion hint,
- geometry placeholders (`cc`/`hh`),
- auto-numbered `config_1d_001.json` / `_002`,
- invalid numeric value fails without writing,
- declining the save writes nothing,
- explicit `-o` path,
- `.py` output rejected.

### `expand_cli.rs` - `nts-rs expand`

- reference grid: `SCS = [0.1, 0.3, 0.5]`, `NU_SIGF = [0.05, 0.1, 0.15]`,
  `TOL_K` and `MODE` preserved,
- geometry placeholders become numbers,
- a list of templates expands for every step,
- missing placeholder -> exit 1 and no output file,
- different range lengths use zip semantics and warn,
- invalid expanded configs are skipped while valid ones are written (exit 1),
- default output `outputs/inputs/inputs_1d.json`,
- `.py` output and missing placeholders rejected,
- invalid step (`0`) reported,
- expanded list runs end to end.

### `data_cli.rs` - scalar-flux extraction

- reference `datos.json` schema with `positions`, `spatial_schemes` and rows,
- flux values match `scalar_flux[position]`,
- `--latest` picks the last `output_1d_*.json`,
- out-of-range positions are reported and produce no row,
- fallback `--inputs` when the configuration is not embedded,
- positional and `-r` forms agree; custom output path; missing source errors.

### `normalize_cli.rs` - target power

- scalar and angular fluxes scaled by `C_N`, `power_w = target`,
- default `NU = 2.49` and `--nu` override,
- `SIGMA_F` takes precedence over `NU_SIGF`,
- run selection reduces `runs`/`total`,
- runs without fission cannot be normalized (exit 1),
- unknown run and broken mesh errors; non-positive power rejected by clap.

### `excel_cli.rs` - workbook

- nine sheets with the reference names and row counts,
- scalar-flux values round-trip,
- run selection limits the workbook,
- custom output path and unknown run error.

### `plot_cli.rs` - PNG plots

- scalar plot honours `--width/--height`,
- scalar, angular and k-history render 1600x900 PNGs (signature + IHDR),
- `scs` renders from `datos.json`,
- overlay of selected runs and custom output,
- angular requires exactly one run,
- fixed-source runs have no k-history,
- `scs` requires `-i`, invalid `--x` and `--x` outside `scs` are rejected,
- `--zone-names` accepted.

### `output_json.rs` - end-to-end CLI

- reference schema (`solver`, `spatial_schemes`, `runs`, `total`, `failures`,
  `generated_at`) and all run keys,
- auto-numbering `output_1d_001.json`, `output_1d_002.json`,
- failed config recorded without aborting the list (exit code 1),
- forced `--scheme SC`,
- explicit `--output`,
- `--manual` through stdin,
- missing `--json`/`--manual` error,
- JSON list of objects,
- key order: `configuration` before `scalar_flux`, with `scalar_flux` as the
  first calculation result.

### `cancellation.rs` - runner cancellation

- preset flag aborts immediately with `cancelled`,
- clear flag does not change the result,
- mid-run cancellation from another thread stops between iterations.

### `tui_app.rs` - config screen (phase A)

- config and small-terminal rendering,
- editing the region count adds rows and revalidates,
- arrow cycling of the quadrature order,
- save writes a loadable configuration,
- invalid configurations are not saved,
- `s` opens the save-as dialog with the default path and `Esc` cancels,
- the Configs panel and placeholder hint render; saved configs load (object and list item),
- the edited path is saved and loads back; non-JSON paths are rejected,
- the save dialog is 76 columns wide,
- cursor editing: Home + insert in the middle, Left + Backspace, Home + Delete,
- the caret is the terminal cursor and does not split a number,
- the geometry panel keeps the bar but no duplicated `total:` line,
- the geometry bar uses one colour per material zone,
- placeholders reported as a template,
- tabs render each screen, flag toggles without opening the editor.

### `tui_run.rs` - run and results screens (phase B)

- background run writes `output_1d` payload,
- cancellation of a background run,
- config/results services list and load files,
- run screen lists configurations,
- app runs a config to an explicit output and loads it,
- results screen renders a payload,
- starting without configs reports an error.

### `python_tui_compat.rs` - configurations from the Python TUI

- loads the sample fixture with `epsilon`, `max_iter`, `ZONE_NAMES` and
  one-element boundary vectors,
- effective tolerances from JSON, boundary expansion to N/2,
- `ZONE_NAMES`/tolerances survive `as_dict` round-trip,
- the sample configuration runs (50 cells, eigenvalue),
- `validate` accepts it and `run` embeds the metadata,
- explicit `--epsilon`/`--max-iter` flags override the JSON,
- `expand` preserves names and tolerances.

### `tui_mouse.rs` - mouse, accordion and dropdowns

- tab clicks, section and zone header clicks,
- row selection and double-click editor,
- scheme dropdown (click) and mode dropdown (keyboard),
- `z`/`Z` collapse-all shortcuts,
- wheel over the form and the run log,
- `m` toggles mouse capture,
- plot-kind button click.

### `tui_export.rs` - placeholders, plots and export (phase C)

- placeholder detection excludes zone names and preloads ranges,
- strict parallel expansion with equal ranges; unequal ranges are rejected,
- expansion writes `inputs_1d.json` with the reference grid,
- the output dialog defaults to `outputs/inputs/inputs_1d.json`,
- batch execution runs every generated config (`output_1d_###.json`),
- plot screen generates PNGs (scalar, k-history; angular errors cleanly),
- `datos.json` positions/values and Excel workbook,
- SCS export,
- export screen rendering,
- `tui` refuses to start without a terminal.

## Reference environment

Tests are deterministic (no random inputs). Benchmarks use the Python
reference values, so a regression in the sweep, the convergence criterion or
the `k_eff` iteration fails with a precise tolerance.
