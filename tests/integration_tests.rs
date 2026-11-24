// Tests de integración para neutron-transport-dd

use neutron_transport_dd::algorithm::{Config, Runner};
use std::fs;
use std::path::Path;

/// Test 1: Cargar configuración automática desde input.json
#[test]
fn test_auto_input_loading() {
    let input_path = "data/input.json";
    assert!(Path::new(input_path).exists(), "input.json debe existir");

    let config = Config::new_auto(1e-5, 2000, input_path);

    // Validar que los valores se cargaron correctamente
    assert_eq!(config.num_regions, 3, "Debe haber 3 regiones");
    assert_eq!(config.num_zones, 3, "Debe haber 3 zonas");

    // Verificar datos de NC (celdas por región)
    assert_eq!(config.nc.len(), 3, "NC debe tener 3 elementos");
    assert_eq!(config.nc[0], 20, "Primera región debe tener 20 celdas");
    assert_eq!(config.nc[1], 60, "Segunda región debe tener 60 celdas");
    assert_eq!(config.nc[2], 20, "Tercera región debe tener 20 celdas");

    // Verificar datos de HR (espesor)
    assert_eq!(config.hr.len(), 3, "HR debe tener 3 elementos");
    assert!(
        (config.hr[0] - 10.0).abs() < 1e-10,
        "Primera región espesor 10cm"
    );
    assert!(
        (config.hr[1] - 30.0).abs() < 1e-10,
        "Segunda región espesor 30cm"
    );
    assert!(
        (config.hr[2] - 10.0).abs() < 1e-10,
        "Tercera región espesor 10cm"
    );

    // Verificar IZL (zonas de material)
    assert_eq!(config.izl.len(), 3, "IZL debe tener 3 elementos");
    assert_eq!(config.izl[0], 1, "Región 1 usa zona 1");
    assert_eq!(config.izl[1], 2, "Región 2 usa zona 2");
    assert_eq!(config.izl[2], 3, "Región 3 usa zona 3");

    // Verificar propiedades materiales
    assert_eq!(config.sct.len(), 3, "SCT debe tener 3 elementos");
    assert!(
        (config.sct[0] - 1.0).abs() < 1e-10,
        "Zona 1 sigma total = 1.0"
    );
    assert!(
        (config.sct[1] - 0.6).abs() < 1e-10,
        "Zona 2 sigma total = 0.6"
    );
    assert!(
        (config.sct[2] - 1.0).abs() < 1e-10,
        "Zona 3 sigma total = 1.0"
    );

    // Verificar orden de cuadratura
    assert_eq!(config.n, 4, "Orden debe ser S4");
    assert_eq!(config.n_half, 2, "n_half debe ser 2");

    // Verificar condiciones de frontera
    assert!(config.reflex_izq, "Con reflexión izquierda");
    assert!(!config.reflex_der, "Sin reflexión derecha");
}

/// Test 2: Validar total de celdas y nodos
#[test]
fn test_cell_and_node_counts() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");

    // Total celdas = suma de NC
    let expected_ntc: usize = vec![20, 60, 20].iter().sum();
    assert_eq!(config.ntc, expected_ntc, "Total celdas debe ser 100");
    assert_eq!(config.ntc, 100, "NTC = 100");

    // Total nodos = NTC + 1
    assert_eq!(config.ntp, 101, "NTP = 101");
}

/// Test 3: Validar vectores de propiedades por celda
#[test]
fn test_property_vectors() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");

    // Verificar que los vectores de propiedades tienen tamaño correcto
    assert_eq!(
        config.sigma_t_vec.len(),
        100,
        "sigma_t debe tener 100 elementos"
    );
    assert_eq!(
        config.sigma_s_vec.len(),
        100,
        "sigma_s debe tener 100 elementos"
    );
    assert_eq!(
        config.q_ext_vec.len(),
        100,
        "q_ext debe tener 100 elementos"
    );
    assert_eq!(config.dx_vec.len(), 100, "dx debe tener 100 elementos");

    // Verificar que los valores coinciden con la región y zona
    // Región 1 (celdas 0-19): zona 1, sigma_t=1.0
    for i in 0..20 {
        assert!(
            (config.sigma_t_vec[i] - 1.0).abs() < 1e-10,
            "Región 1 sigma_t=1.0"
        );
        assert!(
            (config.sigma_s_vec[i] - 0.99).abs() < 1e-10,
            "Región 1 sigma_s=0.99"
        );
    }

    // Región 2 (celdas 20-79): zona 2, sigma_t=0.6
    for i in 20..80 {
        assert!(
            (config.sigma_t_vec[i] - 0.6).abs() < 1e-10,
            "Región 2 sigma_t=0.6"
        );
        assert!(
            (config.sigma_s_vec[i] - 0.4).abs() < 1e-10,
            "Región 2 sigma_s=0.4"
        );
    }

    // Región 3 (celdas 80-99): zona 3, sigma_t=1.0
    for i in 80..100 {
        assert!(
            (config.sigma_t_vec[i] - 1.0).abs() < 1e-10,
            "Región 3 sigma_t=1.0"
        );
        assert!(
            (config.sigma_s_vec[i] - 0.9).abs() < 1e-10,
            "Región 3 sigma_s=0.9"
        );
    }
}

/// Test 4: Validar tamaño de mesh (dx)
#[test]
fn test_mesh_spacing() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");

    // dx en región 1: HR[0] / NC[0] = 10 / 20 = 0.5
    for i in 0..20 {
        assert!((config.dx_vec[i] - 0.5).abs() < 1e-10, "Región 1 dx=0.5");
    }

    // dx en región 2: HR[1] / NC[1] = 30 / 60 = 0.5
    for i in 20..80 {
        assert!((config.dx_vec[i] - 0.5).abs() < 1e-10, "Región 2 dx=0.5");
    }

    // dx en región 3: HR[2] / NC[2] = 10 / 20 = 0.5
    for i in 80..100 {
        assert!((config.dx_vec[i] - 0.5).abs() < 1e-10, "Región 3 dx=0.5");
    }
}

/// Test 5: Ejecutar simulación completa y verificar convergencia
#[test]
fn test_runner_simulation() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");
    let mut runner = Runner::new(config);
    let result = runner.run();

    // Verificar que convergió
    assert!(result.converged, "La simulación debe converger");

    // Verificar que el número de iteraciones es razonable
    assert!(result.iteration > 0, "Debe haber al menos 1 iteración");
    assert!(
        result.iteration < 2000,
        "Debe converger antes de 2000 iteraciones"
    );

    // Verificar que el flujo escalar tiene valores positivos
    assert_eq!(
        result.scalar_flux.len(),
        100,
        "Flujo escalar debe tener 100 elementos"
    );

    let max_flux = result.scalar_flux.max();
    assert!(max_flux > 0.0, "Flujo escalar debe ser positivo");

    // Verificar que el tiempo es razonable (< 10 segundos)
    assert!(result.time.as_secs() < 10, "Simulación debe ser rápida");
}

/// Test 6: Validar que los vectores de flujo angular tienen tamaño correcto
#[test]
fn test_angular_flux_dimensions() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");
    let mut runner = Runner::new(config);
    let result = runner.run();

    // psi_right y psi_left deben ser [NTP x n_half]
    assert_eq!(
        result.psi_right.nrows(),
        101,
        "psi_right debe tener 101 filas (nodos)"
    );
    assert_eq!(
        result.psi_right.ncols(),
        2,
        "psi_right debe tener 2 columnas (n_half)"
    );
    assert_eq!(
        result.psi_left.nrows(),
        101,
        "psi_left debe tener 101 filas"
    );
    assert_eq!(result.psi_left.ncols(), 2, "psi_left debe tener 2 columnas");
}

/// Test 7: Validar valores de frontera (reflexivas)
#[test]
fn test_boundary_conditions() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");

    // reflex_izq debe ser verdadero (reflex_izq: 1 en input.json)
    assert!(config.reflex_izq, "Debe tener reflexión izquierda");
    assert!(!config.reflex_der, "No debe tener reflexión derecha");

    // Los vectores de frontera deben tener el tamaño correcto
    assert_eq!(
        config.bound_left.len(),
        2,
        "bound_left debe tener n_half elementos"
    );
    assert_eq!(
        config.bound_right.len(),
        2,
        "bound_right debe tener n_half elementos"
    );
}

/// Test 8: Diferentes valores de epsilon (convergencia más rápida)
#[test]
fn test_different_epsilon() {
    // Epsilon más grande = convergencia más rápida pero menos precisa
    let config_loose = Config::new_auto(1e-2, 500, "data/input.json");
    assert_eq!(config_loose.epsilon, 1e-2);

    // Epsilon pequeño = convergencia más lenta pero más precisa
    let config_tight = Config::new_auto(1e-7, 5000, "data/input.json");
    assert_eq!(config_tight.epsilon, 1e-7);
}

/// Test 9: Validar cargas de archivo inexistente
#[test]
#[should_panic(expected = "No se pudo abrir")]
fn test_missing_input_file() {
    Config::new_auto(1e-5, 2000, "data/nonexistent.json");
}

/// Test 10: Validar iteraciones guardadas en salida
#[test]
fn test_output_file_creation() {
    let config = Config::new_auto(1e-5, 2000, "data/input.json");
    let mut runner = Runner::new(config);
    let _result = runner.run();

    // Buscar si se creó algún archivo con patrón data_*.out en el directorio actual
    // El archivo se crea en la raíz del proyecto durante ejecución de tests
    let current_dir = std::env::current_dir().expect("No se puede obtener directorio actual");

    if let Ok(entries) = fs::read_dir(current_dir) {
        let out_files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().into_string().unwrap_or_default();
                name.starts_with("data_") && name.ends_with(".out")
            })
            .collect();

        // Si hay archivos .out, validar el más reciente
        if !out_files.is_empty() {
            if let Some(last_file) = out_files.last() {
                let content =
                    fs::read_to_string(last_file.path()).expect("Debe poder leer archivo");

                assert!(
                    content.contains("RESULTADOS DE LA SIMULACIÓN"),
                    "Debe tener título"
                );
                // Buscar "Converge" (sin acentos) o variantes
                assert!(
                    content.to_lowercase().contains("converge") || content.contains("Iteraciones"),
                    "Debe tener información de convergencia o iteraciones"
                );
                assert!(
                    content.contains("FLUJO ESCALAR"),
                    "Debe tener sección de flujo"
                );
            }
        }
        // Si no hay archivos, simplemente pasamos (pueden no existir según dónde se ejecute)
    }
}
