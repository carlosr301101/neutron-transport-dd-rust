# Changelog

All notable changes to this project are documented in this file.

## [0.2.0] - 2026-11-25

First public release of the Rust rewrite. `v0.1.0` was already taken by the
earlier Python-era tag, so the crate starts at 0.2.0 to keep the tag, the
GitHub release and the crates.io version in sync.

### Added

- 1D S_N transport solver (S2/S4/S6/S8) with DD, STEP and Step
  Characteristic sweep schemes.
- Physics modes `fixed_source`, `fixed_k`, `driven` and `eigenvalue`
  (power iteration) with `k_eff` and `k_balance`.
- Power normalization (`POWER_KW`, `E_F_MEV`, `NU`/`SIGMA_F`).
- JSON configuration compatible with the Python reference, including
  `ZONE_NAMES`, `epsilon`, `max_iter` and one-element boundary vectors.
- `nts-rs run`, `validate`, `list`, `show` and `status` with `--json`.
- Parameter sweeps: `config` and `expand` with placeholders (`kk`, `mm`).
- Post-processing: `data` (`datos.json`), `normalize`, `excel` and `plot`
  (scalar, angular, k-history and SCS PNGs with bundled fonts).
- Interactive ratatui TUI: editable config with collapsible sections and
  dropdowns, saved-config loader (including list items), background runs
  with live log and cancellation, placeholder detection with strict equal
  ranges, batch execution, plots, Excel and `datos.json` export.
- Structured logging with a daily global log and one log per run.
- Test suite: 265 tests with numerical parity against the Python reference.

### Notes

- `v0.1.0` corresponds to the previous Python implementation; this crate starts
  at 0.2.0 so the git tag, the GitHub release and the crates.io version match.
