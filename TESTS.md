# Suite de Tests - Neutron Diffusion DD

## Descripción General

La aplicación cuenta con una suite completa de tests que cubren:
- **Carga automática de configuración** desde `data/input.json`
- **Validación de estructuras de datos** y vectores
- **Simulación del algoritmo S_N**
- **Convergencia y cálculos numéricos**
- **Condiciones de frontera**
- **Generación de salida**

## Estructura de Tests

### Tests Unitarios (11 pruebas)
Ubicados en: `src/algorithm.rs` (módulo `tests`)

#### 1. `test_config_new_auto_basic`
- Verifica que la configuración se cargue correctamente desde JSON
- Valida: `num_regions`, `num_zones`, `n`

#### 2. `test_weights_directions_s4`
- Verifica que los pesos de cuadratura S4 sean correctos
- Valida que la suma de pesos = 1.0
- Verifica 2 direcciones para S4

#### 3. `test_prelim_calculations_vectors`
- Verifica tamaños correctos de vectores preliminares
- Valida valores no negativos

#### 4. `test_runner_initialization`
- Comprueba que el runner se inicializa correctamente
- Verifica estado inicial: no convergido, iteración 0

#### 5. `test_convergence_check_negative_flux`
- Simula cambio en flujo y verifica que NO converge
- Prueba lógica de criterio de convergencia

#### 6. `test_epsilon_effect_on_convergence`
- Compara convergencia con epsilon flojo vs. ajustado
- Verifica que criterio flojo converja más rápido

#### 7. `test_reflect_boundary_conditions`
- Valida condiciones de frontera reflexivas
- Verifica valores de frontera cargados desde JSON

#### 8. `test_material_zones_mapping`
- Verifica mapeo correcto zona → propiedades
- Valida índices y referencias

#### 9. `test_quadrature_weights_positive`
- Verifica que pesos y cosenos sean válidos
- Valida rangos: ω > 0, μ ∈ [0,1]

#### 10. `test_scalar_flux_non_negative`
- Ejecuta simulación completa
- Verifica que flujo escalar ≥ 0

#### 11. `test_runner_max_iterations_limit`
- Verifica que las iteraciones no excedan `max_iter`
- Usa epsilon muy pequeño para forzar límite

### Tests de Integración (10 pruebas)
Ubicados en: `tests/integration_tests.rs`

#### 1. `test_auto_input_loading`
- Lee `data/input.json` completamente
- Valida NC, HR, IZL, SCT, SCS, Q, N, condiciones frontera
- Compara contra valores esperados

#### 2. `test_cell_and_node_counts`
- Verifica: NTC = 100 (suma de NC)
- Verifica: NTP = 101 (NTC + 1)

#### 3. `test_property_vectors`
- Valida que vectores sigma_t, sigma_s, q_ext tengan 100 elementos
- Verifica valores por región:
  - Región 1: σ_t=1.0, σ_s=0.99
  - Región 2: σ_t=0.6, σ_s=0.4
  - Región 3: σ_t=1.0, σ_s=0.9

#### 4. `test_mesh_spacing`
- Verifica dx en cada región
- Todas las regiones: dx = 0.5 cm

#### 5. `test_runner_simulation`
- Ejecuta simulación completa
- Valida convergencia en < 2000 iteraciones
- Verifica flujo positivo
- Comprueba tiempo < 10 segundos

#### 6. `test_angular_flux_dimensions`
- Verifica dimensiones de matrices de flujo angular
- psi_right, psi_left: [101 x 2]

#### 7. `test_boundary_conditions`
- Valida: reflex_izq=true, reflex_der=false
- Verifica vectores bound_left, bound_right

#### 8. `test_different_epsilon`
- Carga config con epsilon=1e-2
- Carga config con epsilon=1e-7
- Valida valores asignados

#### 9. `test_missing_input_file` (should panic)
- Intenta cargar archivo inexistente
- Verifica que el programa lance excepción

#### 10. `test_output_file_creation`
- Valida creación de archivos `.out`
- Verifica contenido: título, convergencia, flujo

## Cobertura de Flujos

### Flujo Manual Input
No hay tests explícitos (requiere stdin), pero el código está validado a través de tests unitarios.

### Flujo Automático Input
✅ **Completamente cubierto** por:
- `test_auto_input_loading`
- `test_config_new_auto_basic`
- Todos los tests de validación de propiedades

### Flujo de Simulación
✅ **Completamente cubierto** por:
- `test_runner_simulation`
- `test_epsilon_effect_on_convergence`
- `test_scalar_flux_non_negative`
- `test_angular_flux_dimensions`

### Flujo de Salida
✅ **Completamente cubierto** por:
- `test_output_file_creation`

## Ejecutar Tests

### Todos los tests
```bash
cargo test
```

### Solo tests unitarios
```bash
cargo test --lib
```

### Solo tests de integración
```bash
cargo test --test integration_tests
```

### Test específico
```bash
cargo test test_auto_input_loading
```

### Con salida verbose
```bash
cargo test -- --nocapture
```

## Resultados Esperados

```
running 32 tests

Test Summary:
- Unitarios (lib): 11 passed ✅
- Unitarios (bin): 11 passed ✅
- Integración: 10 passed ✅

test result: ok. 32 passed; 0 failed
```

## Notas Importantes

1. **Dependencia JSON**: Los tests de integración requieren `data/input.json`
2. **Directorio actual**: Tests crean archivos `data_*.out` en directorio actual
3. **Rendimiento**: Simulación completa toma ~100-200ms en modo debug
4. **Paralelización**: Tests se ejecutan en paralelo por defecto

## Mejoras Futuras

- [ ] Tests para flujo manual (mock stdin)
- [ ] Benchmarks de rendimiento
- [ ] Property-based testing (quickcheck)
- [ ] Cobertura de líneas (tarpaulin)
- [ ] CI/CD integration (GitHub Actions)
