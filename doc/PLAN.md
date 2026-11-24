# Plan de reescritura a Rust

Port del workflow 1D de
[`neutron-transport-PINNs`](https://github.com/carlosr301101/neutron-transport-PINNs)
(rama `main`, versión 3.10.0) hacia este crate: `neutron-transport-dd`, binario
`nts-rs`.

Objetivo: reproducir la física y el formato de salida de Python con paridad
numérica verificable, sustituyendo dependencias pesadas (`numpy`, `pandas`,
`pydantic`, `textual`) por una implementación nativa en Rust.

## Decisiones de diseño

| Decisión | Elección |
|----------|----------|
| Alcance inicial | Librería + subcomando `run` (el resto de comandos en pasos posteriores) |
| Binario | `nts-rs` (el paquete sigue siendo `neutron-transport-dd`) |
| Salida | Idéntica a `output_1d_###.json` para que `nts-1d-data`, `nts-1d-plot` y `nts-1d-excel` sigan funcionando |
| Modo manual | Incluido desde el paso 0 (`--manual`) |
| Dependencias | `serde`, `serde_json`, `clap`, `chrono`; `tempfile` solo en tests |
| Álgebra | `Vec<f64>` y slices planos; sin `nalgebra` |
| Cuadratura | Tabla S2/S4/S6/S8 embebida, sin archivo en runtime |
| Despacho de esquemas | `enum Method` con `match` (sin `dyn`) |
| Errores | `ConfigError` / `SolverError` con `Display` manual, sin `panic!` |
| Impresión | El core es silencioso; la CLI imprime |

## Paso 0 — Configuración, runner y CORE (completado)

### Mapeo Python → Rust

| Python (`main`) | Rust (`src/`) |
|-----------------|---------------|
| `solvers/runners/cuadraturas.py` | `quadrature.rs` |
| `solvers/runners/scheme.py` + `NTS_{DD,STEP,SC}_1D.py` | `scheme.rs` |
| `solvers/runners/config.py` | `config.rs` |
| `solvers/runners/runner.py` | `core/runner.rs` |
| `solvers/runners/power.py` | `power.rs` |
| `cli/commands.py::cmd_run_1d` + `_serialize_1d_run` | `result.rs` + `main.rs` |
| `utils/paths.py` (parte 1D) | `paths.rs` |
| `config.py` (modo manual) | `wizard.rs` |

Eliminados: `src/algorithm.rs`, `tests/integration_tests.rs`,
`data/cuadraturas.json`, dependencias `nalgebra` y `once_cell`.

### Fases

- [x] **1. Esqueleto** — `error.rs`, `lib.rs`, `[[bin]] nts-rs`.
- [x] **2. `quadrature.rs`** — S2/S4/S6/S8 verbatim; N impar o no tabulado → error
      (sin fallback silencioso a S2).
- [x] **3. `scheme.rs`** — DD/STEP/SC, alias (`dd`, `Step`, ` sc `,
      `STEP_CHARACTERISTIC`, `STEP-CHARACTERISTIC`), `theta` con serie para
      α < 1e-3, `|μ|` normalizado, error con μ = 0.
- [x] **4. `config.rs`** — claves JSON de Python, comentarios `_`, booleanos
      `0/1`, fronteras escalar/lista/vector, validación en el mismo orden y con
      los mismos mensajes, inferencia de `MODE`, `as_dict()`.
- [x] **5. `core/runner.rs`** — barrido en una pasada, 4 modos
      (`fixed_source`, `fixed_k`, `driven`, `eigenvalue`), `k_history`,
      `k_balance`, `inner_tol = max(epsilon, TOL_K)`, normalización de potencia.
- [x] **6. `result.rs`, `paths.rs`, `main.rs`** — payload compatible, `NTS_HOME`,
      numeración automática, `--json`, `--manual`, `--scheme`, `--epsilon`,
      `--max-iter`, `--verbose`; códigos de salida 0/1/130.
- [x] **7. `wizard.rs`** — mismas preguntas y defaults que `--manual` de Python.
- [x] **8. Datos de ejemplo** — `data/input.json` (legado), 
      `data/example_eigenvalue.json`, `data/example_driven.json`.
- [x] **9. Tests y documentación** — ver matriz abajo; README y TESTS.md.

### Convenciones que no deben cambiar

- `scalar_flux` es el **promedio angular** (`0.5 * Σ ω_m ψ_m`, con Σω = 1 por
  hemisferio). No es la integral total.
- Fuente isotrópica: `S = Σs·φ + 0.5·Q + νΣf·φ/k`.
- Convergencia: `max |φ_new − φ_old| / |φ_old|`, con `|φ_old| ≤ 1e-13`
  usando diferencia absoluta.
- Sin fixup de flujo negativo (Python no lo tiene).
- Orden reflectivo: derecha y luego izquierda, igual que Python.
- `iteration` se incrementa una vez por actualización de flujo, dentro del
  chequeo de convergencia.

### Matriz de tests (`tests/`)

| Archivo | Origen Python | Casos |
|---------|---------------|-------|
| `dd_pseudocode.rs` | `test_dd_pseudocode*.py` | DD literal S2, 5/10/20 celdas, mismo `iteration`, flujos a 1e-12 |
| `fission_modes.rs` | `test_dd_1d_fission.py` | k_inf=0.6, benchmark 1.02668, fixed_k, driven, `NU`=2.49, potencia 10 kW, contadores, regresión, fronteras mixtas, inferencia, 4 errores |
| `schemes.rs` | `test_spatial_schemes.py` | alias, rechazo, regresión DD exacta, SC inyectado 1.026054, k=0.6 en 3 esquemas, benchmark, balance de celda, vacío/reflectivo, φ = promedio angular |
| `convergence.rs` | `test_convergence.py` | denominador φ viejo, φ negativo, φ=0, ε más fino → más sweeps |
| `boundary_vector.rs` | `test_boundary_vector.py` | escalar, lista de 1, vector, longitud inválida, reflectivo |
| `config_validation.rs` | `config.py` + `test_dd_1d_json.py` | faltantes, dimensiones, IZL, NU_SIGF, SIGMA_F, POWER_KW, ZONE_NAMES, N no soportado, `_`, 0/1, inferencia, round-trip |
| `manual_wizard.rs` | `test_dd_1d_manual.py` | eigenvalue, potencia (NU y SIGMA_F), fixed_k, driven (con/sin fuente), fixed_source, modo inválido, esquema forzado, placeholder rechazado |
| `output_json.rs` | `cmd_run_1d` | esquema completo, numeración, fallo parcial, `--scheme`, `--output`, `--manual` por stdin, error sin fuente |

Unitarios (30) en `quadrature`, `scheme`, `config`, `power`, `paths`.
Doc-test: ejemplo de eigenvalue del README.

### Criterios de aceptación (verificados)

- [x] `cargo fmt --all -- --check`.
- [x] `cargo clippy --all-targets --all-features` sin warnings.
- [x] `cargo test`: 97/97 (30 unit + 66 integración + 1 doc-test).
- [x] `cargo doc --no-deps` sin warnings.
- [x] `cargo build --release` → `target/release/nts-rs`.
- [x] Benchmark S8 100 celdas, ε=1e-6: 999 sweeps, `k_eff = 1.026676437943719`,
      `k_balance = 1.0266775219239306`, `Σφ = 67.70760151718075`.
- [x] Benchmark con ε=1e-5: 623 sweeps.
- [x] `data/input.json` legado sigue corriendo (`fixed_source`).
- [x] Cero referencias a "diffusion" y sin `algorithm.rs`.

## Pasos siguientes

### Paso 1 — CLI completa

- `nts-rs validate <config>` con resumen legible.
- `nts-rs list inputs|outputs`, `show <result>`, `status`.
- Logging a archivo (`outputs/logs/`), nivel `--verbose`.
- Sustituir el soporte de `-c inputs.py` por JSON (o un conversor `py → json`).

### Paso 2 — Generación de configuraciones

- Placeholders `kk`/`mm`: `nts-rs generate --json config.json
  --place-holder kk 0.1 0.95 0.05`.
- `RawConfig` con `NumOrName` para aceptar nombres simbólicos.
- `nts-rs config` (wizard con placeholders) y escritura de
  `outputs/inputs/config_1d_###.json` / `inputs_1d.json`.

### Paso 3 — Post-proceso

- `nts-rs data` (equivalente a `nts-1d-data`) → `datos.json`.
- `nts-rs normalize` (`POWER_KW` sobre un output existente).
- `nts-rs excel` (crate `rust_xlsxwriter`).
- `nts-rs plot` (crate `plotters`): scalar, angular, `k_history`, SCS.

### Paso 4 — Workflow 2D

- `SimulationConfig` equivalente a Pydantic (`N`, `NZ`, `zones`, `NR_X`,
  `XDOM`, `NR_Y`, `YDOM`, `ZMAP`, `QMAP`, `BC`, `TOL`).
- `input_builder` para `input_###.txt`.
- Runner paralelo de los binarios C (`NTS_DD`, `NTS_LD`, `NTS_STEP`,
  `NTS_RM_CN`, `NTS_RM_LLN`) con pool configurable.

### Paso 5 — TUI y PINN

- TUI con `ratatui` (config, runs, resultados, plots, export).
- Integración con el entrenamiento PINN y generación de datasets.

## Riesgos y notas

- **Paridad de iteraciones**: cualquier cambio en el orden reflectivo o en el
  momento de `iteration += 1` rompe `dd_pseudocode`. Es el test guardián.
- **Tolerancias**: `inner_tol = max(epsilon, TOL_K)`; los valores de referencia
  dependen de ello.
- **`-c inputs.py`** no es portable; se documenta como limitación hasta el
  paso 1/2.
- **crates.io**: el crate `neutron-diffusion-dd` 0.1.0 ya está publicado; este
  nombre se publicará como crate nuevo cuando se etiquete `vX.Y.Z`.
- **Repositorio GitHub**: pendiente renombrar el remoto a
  `neutron-transport-dd-rust` para que `git push` funcione.
