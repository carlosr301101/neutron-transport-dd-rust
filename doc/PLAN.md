# Plan de reescritura a Rust

Port del workflow 1D y 2D de
[`neutron-transport-PINNs`](https://github.com/carlosr301101/neutron-transport-PINNs)
(rama `main`, versión 3.10.0) hacia este crate: `neutron-transport-dd`, binario
`nts-rs`.

Objetivo: reproducir la física y los formatos de salida de Python con paridad
numérica verificable, sustituyendo `numpy`, `pandas`, `pydantic` y `textual`
por una implementación nativa en Rust.

## Índice

- [Estado y decisiones](#estado-y-decisiones)
- [Contrato de paridad](#contrato-de-paridad)
- [Mapa del sistema Python](#mapa-del-sistema-python)
- [Paso 0 — Configuración, runner y CORE (completado)](#paso-0--configuración-runner-y-core-completado)
- [Paso 1 — CLI base, validación y logging](#paso-1--cli-base-validación-y-logging)
- [Paso 2 — Placeholders, sweeps y generador de configs](#paso-2--placeholders-sweeps-y-generador-de-configs)
- [Paso 3 — Post-proceso: data, normalize, Excel y plots](#paso-3--post-proceso-data-normalize-excel-y-plots)
- [Paso 4 — Workflow 2D: input.txt y binarios C](#paso-4--workflow-2d-inputtxt-y-binarios-c)
- [Paso 5 — TUI](#paso-5--tui)
- [Paso 6 — Datasets PINN, empaquetado y CI](#paso-6--datasets-pinn-empaquetado-y-ci)
- [Orden, dependencias y esfuerzo](#orden-dependencias-y-esfuerzo)
- [Riesgos y notas](#riesgos-y-notas)

## Estado y decisiones

| Decisión | Elección |
|----------|----------|
| Alcance del paso 0 | Librería + subcomando `run` |
| Binario | `nts-rs` (paquete `neutron-transport-dd`) |
| Salida 1D | Idéntica a `output_1d_###.json` (compatible con las tools Python) |
| Modo manual | Incluido desde el paso 0 |
| Dependencias base | `serde`, `serde_json`, `clap`, `chrono`; `tempfile` (tests) |
| Álgebra | `Vec<f64>` y slices; sin `nalgebra` |
| Cuadratura | S2/S4/S6/S8 embebida |
| Errores | `ConfigError` / `SolverError`, sin `panic!` en producción |
| Impresión | Core silencioso; la CLI imprime |
| Configs Python (`.py`) | No soportadas; todo pasa por JSON |
| Paso 4 (2D) | Aparcado por decisión del usuario; se retomará al cerrar la reescritura |

## Contrato de paridad

Estas convenciones están fijadas por los tests y **no deben cambiar** en pasos
posteriores:

- `scalar_flux` es el **promedio angular**: `0.5 * Σ ω_m ψ_m`, con `Σω = 1`
  por hemisferio (los N pesos completos suman 2).
- Fuente isotrópica en el barrido: `S = Σs·φ + 0.5·Q + νΣf·φ/k`.
- Convergencia: `max |φ_new − φ_old| / |φ_old|`; si `|φ_old| ≤ 1e-13` se usa la
  diferencia absoluta.
- Sin fixup de flujo negativo (Python no lo aplica).
- Actualización reflectiva: derecha y luego izquierda.
- `iteration` se incrementa una vez por actualización de flujo, dentro del
  chequeo de convergencia.
- Tolerancia interna: `max(epsilon, TOL_K)`.
- Iteración de potencia: `k_new = k_old · P_new / P_old`, escalado por
  `max|φ|`, `dk = |k_new − k_old| / |k_new|`.
- `k_balance = P / (A + L)` con fuga neta en ambas fronteras.
- Las tasas de reacción llevan factor 2 (φ es promedio angular).
- Cualquier divergencia en estos puntos la detecta `tests/dd_pseudocode.rs`
  (contador de iteraciones exacto) y el benchmark de `tests/schemes.rs`
  (`k_eff = 1.0266764379437188`).

## Mapa del sistema Python

| Comando Python | Comando Rust (objetivo) | Estado |
|----------------|--------------------------|--------|
| `nts tui` | `nts-rs tui` | hecho |
| `nts status` | `nts-rs status` | hecho (1D; 2D en 4) |
| `nts validate` | `nts-rs validate` | hecho |
| `nts generate` (2D `input.txt`) | `nts-rs generate` | paso 4 |
| `nts-generate` / `nts generate --place-holder` | `nts-rs expand` | hecho |
| `nts-1d-config` | `nts-rs config` | hecho |
| `nts run` (binarios 2D) | `nts-rs run2d` | paso 4 |
| `nts run-1d` / `nts-1d-run*` | `nts-rs run` | hecho |
| `nts-1d-data` | `nts-rs data` | hecho |
| `nts-1d-normalize` | `nts-rs normalize` | hecho |
| `nts-1d-excel` | `nts-rs excel` | hecho |
| `nts-1d-plot` | `nts-rs plot` | hecho |
| `nts plot` (heatmap 2D) | `nts-rs plot2d` | paso 4 |
| `nts list` / `nts show` | `nts-rs list` / `nts-rs show` | hecho (1D; 2D en 4) |
| `python -m cli.run_tests` | `cargo test` | hecho |

Nota: el `run` actual es 1D. En el paso 4, el runner de binarios C se expone
como `run2d` para no romper scripts existentes del workflow 1D.

---

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

### Entregables

- [x] `error.rs`: `ConfigError` / `SolverError` con `Display` y `source()`.
- [x] `quadrature.rs`: S2/S4/S6/S8 verbatim; N impar o no tabulado → error.
- [x] `scheme.rs`: DD/STEP/SC, alias de nombres, `theta` con serie (α < 1e-3),
      `|μ|` normalizado, error con μ = 0, balance de celda.
- [x] `config.rs`: claves JSON de Python, comentarios `_`, booleanos `0/1`,
      fronteras escalar/lista/vector, validación en el mismo orden y con los
      mismos mensajes, inferencia de `MODE`, vectores por celda, `as_dict()`.
- [x] `core/runner.rs`: barrido en una pasada, `fixed_source`, `fixed_k`,
      `driven`, `eigenvalue`, `k_history`, `k_balance`, normalización de
      potencia, `iteration_eigenvalue`.
- [x] `result.rs`: `RunResult` + `runs[]` y payload de `output_1d_###.json`.
- [x] `paths.rs`: `NTS_HOME`, `outputs/results`, numeración `###`.
- [x] `wizard.rs`: `--manual` con el mismo orden de preguntas.
- [x] `main.rs`: `run --json|--manual`, `--scheme`, `--epsilon`,
      `--max-iter`, `--output`, `--verbose`; códigos 0/1/130.
- [x] `data/`: `input.json` legado, `example_eigenvalue.json`,
      `example_driven.json`.

### Tests (97)

Unitarios (30) en `quadrature`, `scheme`, `config`, `power`, `paths`.
Integración (66): `dd_pseudocode`, `fission_modes`, `schemes`,
`convergence`, `boundary_vector`, `config_validation`, `manual_wizard`,
`output_json`. Doc-test (1): ejemplo de eigenvalue del README.

### Verificación

- `cargo fmt --check`, `cargo clippy` sin warnings, 97/97 tests.
- Benchmark S8, 100 celdas, ε=1e-6: 999 sweeps,
  `k_eff = 1.026676437943719`, `k_balance = 1.0266775219239306`,
  `Σφ = 67.70760151718075`.
- ε=1e-5: 623 sweeps, `k_eff = 1.0266605744526358`.
- SC inyectado: `k_eff = 1.026054`.

---

## Paso 1 — CLI base, validación y logging (completado)

**Objetivo:** dejar de tener un CLI de un solo comando y añadir las utilidades
de inspección y diagnóstico, además de logging a archivo.

**Prerrequisitos:** paso 0.

### Módulos

```text
src/
  cli/
    mod.rs        # Cli, Command, GlobalArgs y dispatch
    run.rs        # lógica del run 1D
    validate.rs
    list.rs
    show.rs
    status.rs
  logging.rs      # fern: consola + archivo diario + log por run
```

`main.rs` es un envoltorio de tres líneas que llama a `cli::main`.

### Comandos y opciones globales

```text
nts-rs [-v|-q] [--log-level LEVEL] <COMMAND>

  run       --json FILE | --manual [-o PATH] [--scheme S] [-e TOL] [-m N]
  validate  <config.json> [--json]
  list      inputs|outputs [--json]
  show      <output.json> [--json]
  status    [--json]
```

- **`validate`**: objeto o lista JSON. Resumen con regiones, zonas, celdas,
  S_N, esquema, modo, `k_eff`, fronteras y fuente externa (`Σ Q_r·HR_r`).
  Errores con el mensaje exacto del paso 0. Configs 2D se reportan como
  pendientes del paso 4. Exit 0/1.
- **`list inputs`**: `outputs/inputs/*.json` y `*.txt`; **`list outputs`**:
  `outputs/results/output_*.json`, con solver, runs y fallos para 1D.
- **`show`**: cabecera (solver, fecha, total, fallos, esquemas) y una línea por
  run con modo, esquema, convergencia, iteraciones (outer), `k_eff`,
  `k_balance` y potencia. Runs con `error` se muestran como `ERROR: ...`.
  `--json` vuelca el payload original sin transformar.
- **`status`**: raíz de trabajo, presencia de directorios, conteo de configs,
  resultados 1D/2D y logs; si `NTS_SOLVERS_DIR` está definido, lista los cinco
  binarios C indicando cuáles faltan. Nunca falla por binarios ausentes; solo
  devuelve error ante fallos reales de E/S.

### Logging

- Crate `fern` (`date-based`) con dos destinos:
  `outputs/logs/nts-rs.log.YYYY-MM-DD` (nivel `trace`) y consola (`stderr`).
- Nivel de consola: `warn` por defecto; `-v` → `debug`; `-vv` → `trace`;
  `-q` → `error`; `--log-level` tiene prioridad.
- Log por run en `outputs/logs/runs/<timestamp>_<config_name>.log` con la
  configuración efectiva y el resumen del resultado (o el error).
- Formato: `YYYY-MM-DDTHH:MM:SS LEVEL mensaje`. El core sigue silencioso.

### Tests (`tests/cli_basic.rs`, 11 casos)

- `validate` válido/ inválido y `--json`; config 2D pendiente.
- `list inputs/outputs` en texto y JSON (solver, runs, fallos).
- `show` con payload generado por `run`, volcado JSON y archivo desconocido.
- `status` con y sin `NTS_SOLVERS_DIR`; binarios ausentes no fallan.
- Logs diario y por run creados y con contenido.
- `-q` silencia el progreso, `-v` lo muestra, `--log-level` aceptado.
- `run --manual` por stdin combinado con flags globales.
- Formas JSON no soportadas rechazadas.

### Criterios de aceptación (verificados)

- [x] `nts-rs --help` lista los nuevos subcomandos.
- [x] Ningún comando escribe fuera de `NTS_HOME`.
- [x] `--json` en `validate`/`list`/`show`/`status`.
- [x] Logs en `outputs/logs/` y `outputs/logs/runs/`.
- [x] Suite total: 113 tests verdes, `clippy` y `fmt` limpios, `cargo doc`
      sin warnings.
- [x] El comportamiento de `run` del paso 0 no cambia (`-v` sigue mostrando
      el progreso).

### Esfuerzo: S/M.

---

## Paso 2 — Placeholders, sweeps y generador de configs (completado)

**Objetivo:** reproducir `nts-1d-config` y `nts-generate` (expansión de
parámetros) sin Python.

**Prerrequisitos:** paso 1.

### Módulos implementados

```text
src/
  placeholder.rs   # detección, reemplazo, grillas, redondeo y decimales
  wizard.rs        # build_value() devuelve el JSON crudo; con placeholders
  cli/config.rs    # wizard con placeholders -> config_1d_###.json
  cli/expand.rs    # expansión a lista JSON validada
  paths.rs         # next_input_path()
```

No se necesitó un `RawConfig`/`NumOrName`: el template se mantiene como
`serde_json::Value`, de modo que los nombres conviven con números sin tipar
artificialmente. El wizard produce ese `Value` y `expand` lo sustituye.

### Reglas heredadas de `core/placeholders.py`

- Un placeholder es un identificador `[A-Za-z_][A-Za-z0-9_]*`.
- Se ignoran los campos de texto/enum: `SPATIAL_SCHEME`, `MODE`, `ZONE_NAMES`.
- `detect_placeholders` recorre listas y objetos.
- `replace_placeholders` sustituye solo los nombres conocidos y respeta los
  campos de texto.
- `build_values(start, end, step)`: grilla inclusiva; el último punto se
  recorta a `end` si el error de coma flotante lo pasa por menos de `1e-12`;
  `step <= 0` y `end < start` → error.
- Redondeo: se redondean **solo** los valores inyectados, a los decimales del
  `STEP` de mayor precisión entre todos los placeholders (`decimal_places`
  respeta notación exponencial).
- Semántica de combinación: **zip**, no producto cartesiano. Se usa la longitud
  mínima y se avisa si las longitudes difieren.
- Los enteros (`IZL`, `N`, `MAX_OUTER`) y los parámetros de ejecución
  (`TOL_K`, `K_EFF`, `POWER_KW`, `E_F_MEV`, `AREA_CM2`, `NU`, `K0`) no aceptan
  placeholders, igual que en Python.

### Comandos

```text
nts-rs config [-o PATH]
nts-rs expand --json TEMPLATE \
              --place-holder NAME START END STEP \
              [--place-holder ...] \
              [-o outputs/inputs/inputs_1d.json]
```

- **`config`**: wizard con placeholders en NC, HR, SCT, SCS, `NU_SIGF`,
  `SIGMA_F`, Q y fronteras. Muestra el JSON, pide confirmación
  (`Save configuration?`) y escribe
  `outputs/inputs/config_1d_###.json` (siguiente libre). Imprime
  `Placeholders detected: ...` y el comando de expansión, o `Run: ...` si no
  hay placeholders. Rechaza salida `.py`.
- **`expand`**: lee un objeto o lista JSON, expande con zip, valida cada
  configuración con `Config::from_value`, escribe una lista JSON y devuelve
  exit 1 si alguna se omitió. Sin resultados no escribe archivo. Rechaza
  salida `.py`.
- Salida por defecto: `outputs/inputs/inputs_1d.json`.
- Los configs 2D se reportan como pendientes del paso 4.

### Formatos

Config con placeholders:

```json
{
  "NC": [20], "HR": [20.0], "IZL": [1],
  "SCT": [1.0], "SCS": ["kk"], "NU_SIGF": ["mm"],
  "Q": [0.0], "N": 8, "MODE": "eigenvalue", "...": "..."
}
```

Salida de `expand` (lista):

```json
[
  { "NC": [20], "SCS": [0.1], "NU_SIGF": [10.0], "...": "..." },
  { "NC": [20], "SCS": [0.15], "NU_SIGF": [11.0], "...": "..." }
]
```

### Tests (18 nuevos, 140 totales)

Unitarios en `placeholder.rs` (8): identificadores, claves enum, reemplazo,
placeholders sin resolver, grilla inclusiva/recortada, decimales, redondeo.

`tests/config_cli.rs` (8): config concreta + `validate`, placeholders
`kk`/`mm`, geometría `cc`/`hh`, numeración automática, valor inválido sin
archivo, cancelar guardado, `-o` explícito, `.py` rechazado.

`tests/expand_cli.rs` (10): grilla de referencia
(`SCS = [0.1, 0.3, 0.5]`, `NU_SIGF = [0.05, 0.1, 0.15]`, `TOL_K`/`MODE`
preservados), geometría, lista de templates, placeholder faltante sin salida,
longitudes distintas (zip + warning), configs inválidas omitidas con exit 1,
salida por defecto, `.py`/sin placeholders rechazados, `STEP = 0` reportado,
lista expandida ejecutada de punta a punta.

### Criterios de aceptación (verificados)

- [x] Una config con `kk`/`mm` se expande y corre de punta a punta.
- [x] Grilla y redondeo coinciden con la referencia de Python.
- [x] `NTS_HOME` respetado en `config` y `expand`.
- [x] `validate` acepta el JSON generado por `config`.
- [x] Sin placeholders → error; configs inválidas → skip + exit 1.
- [x] Suite total: 140 tests, `clippy`, `fmt` y `cargo doc` limpios.

### Esfuerzo: M.

---

## Paso 3 — Post-proceso: data, normalize, Excel y plots (completado)

**Objetivo:** cubrir `nts-1d-data`, `nts-1d-normalize`, `nts-1d-excel` y
`nts-1d-plot`.

**Prerrequisitos:** pasos 0 y 1.

### Módulos implementados

```text
src/
  post/
    model.rs      # acceso a output_1d y selectores de runs
    geometry.rs   # dx, centros de celda, bordes y zonas
    extract.rs    # datos.json
    normalize.rs  # potencia sobre un output existente
    excel.rs      # workbook (rust_xlsxwriter)
    plot.rs       # PNG (plotters)
  cli/{data,normalize,excel,plot}.rs
assets/fonts/LiberationSans-Regular.ttf (+ licencia OFL)
```

Dependencias: `plotters` (backend bitmap + `ab_glyph`, sin `font-kit`),
`rust_xlsxwriter`; `calamine` solo en tests.

### `nts-rs data`

```text
nts-rs data [SOURCE] [-r FILE | --latest] [--positions 0 100 ...] \
            [--inputs FILE] [-o outputs/results/datos.json]
```

- Usa la `configuration` embebida o `--inputs` (objeto nombre→config o lista
  con nombres `json_config_###`).
- Fila por run: `config_name`, `SCS` (zona 1), `flux_<pos>`, `NU_SIGF`,
  `spatial_scheme`, `mode`, `k_eff`, `k_balance`.
- Posición fuera de rango: error de ese run, sin abortar el resto.
- Payload: `{source, positions, spatial_schemes, data[, errors]}`.
- Salida por defecto `outputs/results/datos.json`; posiciones por defecto
  `0 100` (como Python).

### `nts-rs normalize`

```text
nts-rs normalize SOURCE --power-kw 10 [--ef-mev 200] [--area-cm2 1] \
                        [--nu 2.4] [--run 0|NAME|all] [-o PATH]
```

- Reconstruye `dx` desde `NC`/`HR` y `Σf` por celda desde `SIGMA_F` o
  `NU_SIGF/NU` (por defecto 2.49).
- Escala `scalar_flux` y `angular_flux` por `C_N = P_target/P_unnorm`, actualiza
  `power_unnormalized`, `power_scale` y `power_w`.
- Escribe `<stem>_normalized.json` con `normalized_at`, `runs` seleccionados y
  `total`; errores por run se reportan y, si ninguno se normaliza, exit 1.
- `--ef-mev` acepta el alias `--e-f-mev`.

### `nts-rs excel`

```text
nts-rs excel SOURCE [-o PATH] [--run ...]
```

Hojas idénticas a Python: `Metadata`, `Runs`, `Zones`, `Regions`,
`Boundaries`, `ScalarFlux`, `AngularFlux`, `Directions`, `k_history`.
Cabeceras con estilo, paneles congelados, autofiltro, anchos automáticos y
formatos numéricos (`0.000000E+00` para flujos, `0.0000000000` para `k`).
Salida por defecto `<stem>.xlsx`.

### `nts-rs plot`

```text
nts-rs plot -r OUTPUT --kind scalar|angular|k-history [--run ...] \
            [--zone-names] [-o PATH] [--show] [--width W] [--height H]
nts-rs plot -i datos.json --kind scs [--x SCS|NU_SIGF|k_eff]
```

- **backend**: `plotters` `BitMapBackend`, PNG RGB 1600×900 por defecto.
- **fuente**: Liberation Sans embebida y registrada con `ab_glyph`; sin
  `font-kit`, freetype ni fontconfig del sistema.
- `scalar`: φ(x), regiones sombreadas, separadores, etiquetas de `ZONE_NAMES`,
  varias series con leyenda; eje y desde 0 con 18 % de margen superior.
- `angular`: un solo run, una línea por dirección con gradiente azul→rojo y
  leyenda `μ=…`.
- `k-history`: línea + puntos, línea horizontal y anotación del `k_eff` final;
  error claro si el run es `fixed_source`.
- `scs`: un panel por posición (hasta 3 columnas) desde `datos.json`.
- `--run` acepta índice, nombre o `all` (no en `angular`).
- `--show` abre el PNG con `xdg-open`/`open`/`cmd start`; si falla, avisa.
- Salida por defecto `outputs/results/plots/<kind>.png`.

### Tests (25 nuevos, 174 totales)

- `tests/data_cli.rs` (7): esquema, valores, `--latest`, posiciones inválidas,
  fallback `--inputs`, equivalencia posicional/`-r`, salida y errores.
- `tests/normalize_cli.rs` (7): escalado de ambos flujos, `NU` por defecto y
  override, `SIGMA_F` prioritario, selección de runs, sin fisión, malla rota,
  potencia no positiva.
- `tests/excel_cli.rs` (4): nueve hojas, valores que sobreviven al viaje,
  selección de runs, salida y run inexistente.
- `tests/plot_cli.rs` (7): tamaño, los cuatro tipos (firma PNG + IHDR),
  superposición, `angular` con un run, `k-history` sin historia, validaciones
  de `scs`/`--x`, `--zone-names`.

### Criterios de aceptación (verificados)

- [x] `data` produce `datos.json` y `plot --kind scs` lo consume.
- [x] `normalize` deja `power_w = target` y escala ambos flujos.
- [x] Excel y PNG se generan sin dependencias de sistema (fuente incluida).
- [x] Salidas por defecto y `--run` compatibles con la referencia.
- [x] Suite total: 174 tests; `fmt`, `clippy` y `cargo doc` limpios.

### Esfuerzo: M/L.

---

## Paso 4 — Workflow 2D: input.txt y binarios C

**Objetivo:** portar `core/config.py`, `core/validator.py`,
`core/input_builder.py`, `execution/runner.py` y `execution/parallel.py`.

**Prerrequisitos:** paso 1 (CLI, paths, logging).

### Módulos

```text
src/
  two_d/
    config.rs        # SimulationConfig + validación
    input_builder.rs # input_###.txt
    execution.rs     # subprocess de un binario C
    parallel.rs      # pool de ejecución
    results.rs       # parseo de output_###.json (MFLUX)
  cli/generate.rs
  cli/run2d.rs
  cli/plot2d.rs
```

### `SimulationConfig`

Campos y reglas equivalentes a Pydantic:

| Campo | Tipo | Regla |
|-------|------|-------|
| `N` | int | par, > 0 |
| `NZ` | int | > 0 |
| `zones` | lista | longitud `NZ`; `sigma_t > 0`; `0 ≤ sigma_s < sigma_t` |
| `NR_X`, `NR_Y` | int | > 0 |
| `XDOM`, `YDOM` | lista | longitud `NR_X`/`NR_Y`; `length > 0`, `nodes > 0` |
| `ZMAP` | matriz | `NR_Y × NR_X`; valores en `1..=NZ` |
| `QMAP` | matriz | `NR_Y × NR_X`; valores ≥ 0 |
| `BC` | [f64;4] | cada valor 0.0 o 1.0 |
| `TOL` | f64 | `1e-7 ≤ TOL ≤ 1e-2` |

Claves que empiezan con `_` se ignoran. Resumen imprimible para `validate`.

### `InputBuilder`

Orden exacto de líneas del formato `input.txt`:

1. `N`
2. `NZ`
3. `NZ` líneas `sigma_t sigma_s`
4. `NR_X`
5. `NR_X` líneas `length nodes`
6. `NR_Y`
7. `NR_Y` líneas `length nodes`
8. `NR_Y` filas de `ZMAP`
9. `NR_Y` filas de `QMAP`
10. `BC` en una línea (`left right bottom top`)
11. `TOL`

Funciones: `build() -> String`, `preview(max_lines)`, `save(path)` con
creación de directorios y numeración `outputs/inputs/input_###.txt`.

### Ejecución

- `run_solver(exe, input, output, timeout)`:
  - resuelve el binario con `NTS_SOLVERS_DIR` (o ruta absoluta),
  - verifica que input existe,
  - ejecuta `NTS_XX input.txt` con stdout redirigido a `output_###.json`,
  - captura stderr, mide duración, devuelve `SolverResult { success, exit_code,
    duration, stderr, error }`.
- `run_parallel(tasks, nproc, timeout)`:
  - pool de hilos (rayon o `std::thread::scope` + canal),
  - resumen: total, éxitos, fallos, duración media,
  - sin estado compartido mutable.
- Comandos:
  - `nts-rs generate <config.json> [-p|--preview] [-o PATH]`
  - `nts-rs run2d -s NTS_DD -i input_001.txt ... -p 4`
  - `nts-rs list outputs` reconoce ambos formatos.
  - `nts-rs show` detecta 2D y muestra `STATUS/ITER/CPU`.
  - `nts-rs plot2d -r output_###.json` → heatmap de `MFLUX`.

### Tests

- `tests/two_d_config.rs`: todas las reglas de validación y matriz de errores.
- `tests/input_builder.rs`: golden file comparado línea a línea con
  `solvers/doc_solvers/NTS_DD_EXAMPLE_INPUT.json` (y los otros 4 ejemplos) →
  `input_###.txt` esperado.
- `tests/two_d_run.rs`: binario falso (script que copia un JSON fijo) →
  `SolverResult` correcto, timeout, código de salida distinto de 0.
- `tests/two_d_parallel.rs`: 8 tareas con un falso solver, `nproc=4`, conteo
  de éxitos/fallos y orden de resultados.
- `status` reporta binarios ausentes sin fallar.

### Criterios de aceptación

- `generate` produce archivos idénticos (salvo numeración) a los del repo
  Python para los 5 `NTS_*_EXAMPLE_INPUT.json`.
- `run2d` ejecuta los binarios reales del repo Python y deja
  `output_###.json` válidos.
- El pool no lanza más de `nproc` procesos a la vez.

### Esfuerzo: M/L.

---

## Paso 5 — TUI (completado)

**Objetivo:** portar `tui/` (Textual) a `ratatui` + `crossterm`, reutilizando
la librería existente.

**Prerrequisitos:** pasos 0–3.

### Stack y entrada

- `ratatui` 0.30 + `crossterm` 0.29 (backend por defecto).
- `nts-rs tui`; exige stdout interactivo y mínimo 80×24.
- `ratatui::init` / `ratatui::restore` restauran el terminal en salida y ante
  panic.
- Preview inline opcional con la feature `images`:
  `ratatui-image` 11 sin `chafa` (sixel/kitty/halfblocks) + `image` (PNG).

### Módulos implementados

```text
src/tui/
  mod.rs        # run(): comprobación de TTY, init/restore
  app.rs        # estado, teclas, render de las 6 pantallas
  model.rs      # formulario editable y validación
  services/
    config_store.rs  # listar/cargar outputs/inputs
    results.rs       # listar/cargar outputs/results
    runner.rs        # hilo + canal de eventos + cancelación
```

Las pantallas y tablas se renderizan desde `app.rs` (no se separaron en
`screens/` y `widgets/` para evitar duplicación); la lógica de negocio vive en
`services/` y en los módulos de librería ya existentes.

### Fases

- **Fase A (commit propio):** terminal, pestañas, formulario de configuración
  (regiones, zonas, materiales, fronteras, física, potencia), barra de
  geometría, validación con placeholders, guardado en
  `outputs/inputs/config_1d_###.json`.
- **Fase B (commit propio):** hook de cancelación en `Runner`
  (`Arc<AtomicBool>` comprobado entre iteraciones), servicio de ejecución en
  hilo con `mpsc`, pantalla Run (lista de configs, log en vivo, cancelar) y
  pantalla Results (lista de outputs + tabla de runs).
- **Fase C (commit propio):** editor de rangos de placeholders y expansión,
  pantalla Plots (scalar, angular, k-history, scs), pantalla Export
  (`datos.json`, Excel, SCS) y preview opcional.

### Teclas

| Pantalla | Acciones |
|----------|----------|
| Config | `↑/↓` mover, `←/→` ciclar, `Enter` editar/toggle, `s` guardar, `v` validar |
| Run | `↑/↓` seleccionar, `r` ejecutar, `c` cancelar |
| Results | `↑/↓` seleccionar output |
| Placeholders | `a` añadir, `d` borrar, `←/→` columna, `Enter` editar, `x` expandir |
| Plots | `k`/`←/→` tipo, `g` generar, `o` abrir |
| Export | `d` datos, `x` Excel, `s` SCS, `o` abrir |
| Global | `Tab`/`Shift+Tab` cambiar pantalla, `q` salir |

### Tests (38 nuevos, 212 totales)

- Unitarios: `sweep` (3), `tui::model` (5), `tui::app` (2).
- `tests/cancellation.rs` (3): cancelación predefinida, flag limpio y
  cancelación entre iteraciones.
- `tests/tui_app.rs` (9): render, terminal pequeño, edición, guardado,
  validación, placeholders, pestañas, toggles.
- `tests/tui_run.rs` (8): ejecución en segundo plano, cancelación, servicios,
  pantallas Run/Results, arranque sin configs.
- `tests/tui_export.rs` (8): rangos, expansión, PNGs, `datos.json`, Excel, SCS,
  render de Export y rechazo sin TTY.

### Criterios de aceptación (verificados)

- [x] Ciclo Config → Save → Run → Results → Plot → Export sin salir de la TUI.
- [x] Cancelación real dentro del solver.
- [x] Salidas idénticas a las del CLI (misma librería).
- [x] `cargo clippy --all-targets` limpio y `--features images` compila.
- [x] Suite total: 212 tests.

### Iteración 2 — jerarquía y ratón

- Acordeón por sección y por zona (`z`/`Z`, clic en cabeceras).
- Dropdowns para `IZL`, `N`, esquema y modo (teclado o clic).
- Clics en pestañas, filas, cabeceras, listas, tabla, botones y popups.
- Rueda para formulario, listas, log y tablas.
- Captura de ratón con `m` para poder copiar texto.
- Guardado con diálogo de ruta editable (`s`), con default
  `outputs/inputs/config_1d_###.json`.
- 11 tests nuevos en `tests/tui_mouse.rs` (227 totales).

### Compatibilidad con configuraciones del TUI de Python

- `epsilon` y `max_iter` del JSON se respetan (precedencia flag CLI > JSON >
  default); `Config::as_dict()` los incluye.
- Fronteras de un elemento se expanden siempre a `N/2`; en reflectivas se
  normalizan a ceros si la longitud no coincide.
- `ZONE_NAMES` se conserva en `run`, `expand`, `normalize`, la TUI y el
  round-trip `as_dict`.
- Fixture de referencia en `tests/fixtures/python_tui_config.json` y 7 tests en
  `tests/python_tui_compat.rs` (236 totales).

### Esfuerzo: L.

---

## Paso 6 — Datasets PINN, empaquetado y CI

**Objetivo:** cerrar el ciclo hacia el entrenamiento PINN y dejar el proyecto
listo para distribución.

**Prerrequisitos:** pasos 2–4.

### Dataset

```text
nts-rs dataset --json base.json \
               --place-holder kk 0.1 0.95 0.05 \
               --place-holder mm 10 20 1 \
               --positions 0 50 100 \
               [-o outputs/datasets/dataset_###.json] \
               [--format json|csv|parquet]
```

- Pipeline: `expand` → `run` → `data` (en memoria) → normalización opcional →
  escritura.
- Esquema por fila: parámetros (`SCS`, `NU_SIGF`), `mode`, `k_eff`,
  `k_balance`, `flux_<pos>`, `spatial_scheme`, `N`, y metadatos del run.
- Manifiesto reproducible: semilla/versión, comandos, contadores y hash de la
  config base.
- `--split train/val/test` opcional con proporciones y semilla.

### Interoperabilidad con Python

- Fase A (recomendada): frontera por archivos JSON/CSV; el código Python de
  entrenamiento no cambia.
- Fase B (opcional): bindings con `pyo3` (feature `python`) para llamar al
  solver desde Python sin subprocess.

### Empaquetado y CI

- `Cargo.toml`: metadatos finales, `rust-version`, `keywords`, `categories`.
- CI: `fmt`, `clippy --all-targets --all-features`, `test`, `doc`, build
  `--release`; cache de `~/.cargo` y `target`.
- Release: binarios Linux/macOS/Windows; el workflow de publicación valida
  que `vX.Y.Z` coincide con `Cargo.toml`.
- Licencia/MIT y `README` con instalación (`cargo install`, binarios).
- `CHANGELOG.md` por paso.

### Tests

- `dataset` genera N filas = runs × posiciones, columnas completas.
- Semilla fija → mismo split.
- Formato CSV/Parquet legible (con `csv`/`parquet` dev-deps si se habilitan).
- Pipeline completo con 2–3 configs en CI.

### Criterios de aceptación

- Un solo comando produce un dataset listo para entrenar.
- CI verde en Linux/macOS/Windows (o matriz mínima Linux).
- `cargo install --path .` deja `nts-rs` funcional.

### Esfuerzo: M.

---

## Orden, dependencias y esfuerzo

| Paso | Depende de | Esfuerzo | Riesgo principal |
|------|------------|----------|------------------|
| 0 | — | L (hecho) | paridad numérica |
| 1 | 0 | S/M (hecho) | logging sin ensuciar el core |
| 2 | 1 | M (hecho) | semántica zip/redondeo |
| 3 | 0, 1 | M/L (hecho) | plots sin display, Excel |
| 4 | 1 | M/L | binarios externos, paralelismo |
| 5 | 0–3 | L (hecho) | alcance de la TUI |
| 6 | 2–4 | M | formatos de dataset |

Sugerencia de ejecución: **1 → 2 → 3 → 4 → 5 → 6**, con un commit por paso y
un `CHANGELOG` por release. Cada paso debe terminar con tests, `fmt` y `clippy`
limpios, y sin romper los 97 tests del paso 0.

## Riesgos y notas

- **Paridad de iteraciones**: cualquier cambio en el orden reflectivo o en el
  momento de `iteration += 1` rompe `dd_pseudocode`. Es el test guardián.
- **Tolerancias**: `inner_tol = max(epsilon, TOL_K)`; los `k_eff` de referencia
  dependen de ello.
- **`.py` configs**: no se ejecutan; toda entrada es JSON. Documentar el
  reemplazo en `--help` y en el README.
- **Binarios 2D**: no se compilan desde Rust; se resuelven por
  `NTS_SOLVERS_DIR`. `status` debe avisar sin fallar.
- **Plots sin display**: `--show` no es portable; por defecto solo se escribe
  el archivo.
- **crates.io**: `neutron-diffusion-dd` 0.1.0 ya está publicado; este nombre
  será un crate nuevo al etiquetar `vX.Y.Z`.
- **GitHub**: pendiente renombrar el remoto a `neutron-transport-dd-rust`.
- **Rendimiento**: los `Runner` por run son independientes; el paralelismo de
  sweeps 1D puede hacerse con `rayon` sobre configs, pero no dentro de un mismo
  barrido (dependencia secuencial). Medir antes de introducirlo.
