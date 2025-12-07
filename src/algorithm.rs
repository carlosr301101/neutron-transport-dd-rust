use nalgebra::{DMatrix, DVector};
use once_cell::sync::Lazy;
use std::f64; // Para float64 (f64)
use std::io::{self, Write}; // Para input manual y flush
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use std::fs::File;
use std::io::Read;

// --- Placeholder para datos de cuadratura ---
// En el código Python, `DATA` viene de un módulo externo.
// Para este ejemplo, solo incluimos los datos de S4.
// mu_m: Cosinos direccionales (sólo positivos).
// omega_m: Pesos de cuadratura.
#[derive(Debug, Serialize, Deserialize)]
pub struct QuadratureDataFile {
    pub N: Vec<i32>,
    pub m: Vec<i32>,
    pub mu_m: Vec<f64>,
    pub omega_m: Vec<f64>,
}

const QUADRATURE_FILE_PATH: &str = "data/cuadraturas.json";

pub static ALL_QUADRATURE_DATA: Lazy<QuadratureDataFile> = Lazy::new(|| {
    // 1. Abrir el archivo
    let mut file = File::open(QUADRATURE_FILE_PATH).expect(&format!(
        "No se pudo abrir el archivo de cuadratura: {}",
        QUADRATURE_FILE_PATH
    ));

    // 2. Leer el contenido a una cadena
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Fallo al leer el archivo a una cadena.");

    // 3. Deserializar la cadena JSON
    serde_json::from_str(&contents)
        .expect("Fallo al deserializar los datos de cuadratura JSON. Verifique el formato.")
});

// =================================================================
// Paso A: Configuración
// =================================================================

/// Configuración de la geometría, materiales y orden de cuadratura.
#[derive(Debug, Clone)]
pub struct Config {
    pub epsilon: f64,
    pub max_iter: i32,
    pub num_regions: usize,
    pub num_zones: usize,

    // Geometría y materiales por región
    pub nc: DVector<usize>,  // Celdas por región
    pub hr: DVector<f64>,    // Espesor por región [cm]
    pub izl: DVector<usize>, // Zona de material por región (1-based)
    pub q: DVector<f64>,     // Fuente externa por región

    // Propiedades de material por zona
    pub sct: DVector<f64>, // Sigma Total por zona
    pub scs: DVector<f64>, // Sigma Scattering por zona

    // Cuadratura
    pub n: i32,
    pub n_half: usize,
    pub miu_m: DVector<f64>,
    pub omega_m: DVector<f64>,

    // Condiciones de frontera
    pub reflex_izq: bool,
    pub reflex_der: bool,
    pub bound_left: DVector<f64>,
    pub bound_right: DVector<f64>,

    // Propiedades vectorizadas a nivel de celda (cálculos preliminares)
    pub ntc: usize, // Total celdas
    pub ntp: usize, // Total nodos
    pub sigma_t_vec: DVector<f64>,
    pub sigma_s_vec: DVector<f64>,
    pub q_ext_vec: DVector<f64>,
    pub dx_vec: DVector<f64>,
}

impl Config {
    // Implementación simplificada del constructor manual
    pub fn new_manual(epsilon: f64, max_iter: i32) -> Self {
        println!("\n--- CONFIGURACIÓN MANUAL ---");

        let num_regions = Self::read_input::<usize>("Introduzca el número de Regiones: ");
        let num_zones = Self::read_input::<usize>("Introduzca el número de Zonas: ");

        let mut conf = Config {
            epsilon,
            max_iter,
            num_regions,
            num_zones,
            nc: DVector::zeros(num_regions),
            hr: DVector::zeros(num_regions),
            izl: DVector::zeros(num_regions),
            q: DVector::zeros(num_regions),
            sct: DVector::zeros(num_zones),
            scs: DVector::zeros(num_zones),

            // Inicialización placeholder para campos de cuadratura/cálculos preliminares
            n: 0,
            n_half: 0,
            miu_m: DVector::zeros(0),
            omega_m: DVector::zeros(0),

            reflex_izq: false,
            reflex_der: false,
            bound_left: DVector::zeros(0),
            bound_right: DVector::zeros(0),

            ntc: 0,
            ntp: 0,
            sigma_t_vec: DVector::zeros(0),
            sigma_s_vec: DVector::zeros(0),
            q_ext_vec: DVector::zeros(0),
            dx_vec: DVector::zeros(0),
        };

        conf.manual_input();
        conf.weights_directions();
        conf.prelim_calculations();

        println!("\nConfiguración completa: {:?}", conf);
        conf
    }

    // Función auxiliar para leer entrada de usuario
    fn read_input<T: std::str::FromStr>(prompt: &str) -> T {
        loop {
            print!("{}", prompt);
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .expect("Fallo al leer la línea");

            match input.trim().parse() {
                Ok(val) => return val,
                Err(_) => println!("Entrada inválida. Intente de nuevo."),
            }
        }
    }

    fn manual_input(&mut self) {
        println!("\n--- CONFIGURACIÓN ESPACIAL ---");

        // Corrección 1: nc (usize)
        let nc_vec: Vec<usize> = (0..self.num_regions)
            .map(|i| Self::read_input(&format!("Celdas en Región {}: ", i + 1)))
            .collect();
        self.nc = DVector::from_vec(nc_vec);

        // Corrección 2: hr (f64)
        let hr_vec: Vec<f64> = (0..self.num_regions)
            .map(|i| Self::read_input(&format!("Espesor total [cm] Región {}: ", i + 1)))
            .collect();
        self.hr = DVector::from_vec(hr_vec);

        // Corrección 3: izl (usize)
        let izl_vec: Vec<usize> = (0..self.num_regions)
            .map(|i| {
                loop {
                    let zone: usize = Self::read_input(&format!(
                        "ID Zona Material Región {} (1-based, 1-{}): ",
                        i + 1,
                        self.num_zones
                    ));
                    if zone >= 1 && zone <= self.num_zones {
                        return zone;
                    }
                    println!(
                        "ID de zona inválido. Debe estar entre 1 y {}.",
                        self.num_zones
                    );
                }
            })
            .collect();
        self.izl = DVector::from_vec(izl_vec);

        println!("\n--- CONFIGURACIÓN MATERIALES ---");

        // Corrección 4: sct (f64)
        let sct_vec: Vec<f64> = (0..self.num_zones)
            .map(|i| Self::read_input(&format!("Sigma_Total Zona {}: ", i + 1)))
            .collect();
        self.sct = DVector::from_vec(sct_vec);

        // Corrección 5: scs (f64)
        let scs_vec: Vec<f64> = (0..self.num_zones)
            .map(|i| Self::read_input(&format!("Sigma_Scattering Zona {}: ", i + 1)))
            .collect();
        self.scs = DVector::from_vec(scs_vec);

        // Corrección 6: q (f64)
        let q_vec: Vec<f64> = (0..self.num_regions)
            .map(|i| Self::read_input(&format!("Fuente (Q) Región {}: ", i + 1)))
            .collect();
        self.q = DVector::from_vec(q_vec);

        println!("\n--- CONFIGURACIÓN CUADRATURA ---");
        self.n = loop {
            let n_input: i32 = Self::read_input("Orden de cuadratura S_N (ej. 2, 4, 8) -> ");
            if n_input % 2 == 0 {
                break n_input;
            }
            println!("El orden de la cuadratura debe ser un número par.");
        };
        self.n_half = (self.n / 2) as usize;

        // --- Condiciones de frontera (Manual) ---
        let reflex_izq_str: String =
            Self::read_input("¿Condiciones reflexivas por la Izquierda? (s/n): ");
        self.reflex_izq = reflex_izq_str.trim().to_lowercase() == "s";
        let reflex_der_str: String =
            Self::read_input("¿Condiciones reflexivas por la Derecha? (s/n): ");
        self.reflex_der = reflex_der_str.trim().to_lowercase() == "s";

        if !self.reflex_izq {
            println!("Ingrese flujo incidente en IZQUIERDA (mu > 0):");
            // Corrección 7: bound_left (f64)
            let bound_left_vec: Vec<f64> = (0..self.n_half)
                .map(|i| Self::read_input(&format!("  Dirección {}: ", i + 1)))
                .collect();
            self.bound_left = DVector::from_vec(bound_left_vec);
        } else {
            self.bound_left = DVector::from_element(self.n_half, 0.0); // Correcto (no usa collect)
        }

        if !self.reflex_der {
            println!("Ingrese flujo incidente en DERECHA (mu < 0):");
            // Corrección 8: bound_right (f64)
            let bound_right_vec: Vec<f64> = (0..self.n_half)
                .map(|i| Self::read_input(&format!("  Dirección {}: ", i + 1)))
                .collect();
            self.bound_right = DVector::from_vec(bound_right_vec);
        } else {
            self.bound_right = DVector::from_element(self.n_half, 0.0); // Correcto (no usa collect)
        }
    }

    fn weights_directions(&mut self) {
        // Acceder a los datos estáticos que ya están deserializados
        let all_data = &*ALL_QUADRATURE_DATA;

        let n_required = self.n;
        let mut miu_filtered = Vec::new();
        let mut omega_filtered = Vec::new();
        let mut found = false;

        // Iterar y filtrar los datos para el orden N requerido
        for (i, &n_val) in all_data.N.iter().enumerate() {
            if n_val == n_required {
                miu_filtered.push(all_data.mu_m[i]);
                omega_filtered.push(all_data.omega_m[i]);
                found = true;
            }
        }

        if !found {
            // Manejo de error si el orden N no existe en el JSON
            eprintln!(
                "Advertencia: No hay datos para S{}. Se usará S2 por defecto.",
                n_required
            );

            // Revertir a un S2 simple (o forzar un error si es crítico)
            self.n = 2;
            self.n_half = 1;
            self.miu_m = DVector::from_vec(vec![0.577350269189626]);
            self.omega_m = DVector::from_vec(vec![1.0]);
        } else {
            self.n_half = miu_filtered.len();

            // Convertir Vec<f64> a DVector<f64>
            self.miu_m = DVector::from_vec(miu_filtered);
            self.omega_m = DVector::from_vec(omega_filtered);
        }
    }

    fn prelim_calculations(&mut self) {
        self.ntc = self.nc.iter().sum(); // Total celdas
        self.ntp = self.ntc + 1; // Total nodos

        // Inicializar vectores de celda
        self.sigma_t_vec = DVector::zeros(self.ntc);
        self.sigma_s_vec = DVector::zeros(self.ntc);
        self.q_ext_vec = DVector::zeros(self.ntc);
        self.dx_vec = DVector::zeros(self.ntc);

        let mut current_idx = 0;
        for r in 0..self.num_regions {
            let n_cells = self.nc[r];
            let zone_idx = self.izl[r] - 1; // Ajuste a base-0
            let hr_region = self.hr[r];
            let dx = hr_region / n_cells as f64;

            // Rellenar las secciones del vector para esta región
            let end_idx = current_idx + n_cells;

            self.sigma_t_vec
                .view_mut((current_idx, 0), (n_cells, 1))
                .fill(self.sct[zone_idx]);
            self.sigma_s_vec
                .view_mut((current_idx, 0), (n_cells, 1))
                .fill(self.scs[zone_idx]);
            self.q_ext_vec
                .view_mut((current_idx, 0), (n_cells, 1))
                .fill(self.q[r]);
            self.dx_vec
                .view_mut((current_idx, 0), (n_cells, 1))
                .fill(dx);

            current_idx = end_idx;
        }
    }
}

// =================================================================
// Paso B: Runner (Motor de Cálculo)
// =================================================================

/// Motor de cálculo que ejecuta el algoritmo $S_N$ por barrido (Sweep).
pub struct Runner {
    config: Config,
    converged: bool,
    iteration: i32,

    // Flujos Angulares [Nodos x Direcciones] -> DMatrix (NTP x N_HALF)
    psi_right: DMatrix<f64>, // mu > 0
    psi_left: DMatrix<f64>,  // mu < 0

    // Flujo Escalar [Celdas] -> DVector (NTC)
    scalar_flux: DVector<f64>,

    // Fuente Total [Celdas] (Scattering + Externa)
    total_source: DVector<f64>,
}

impl Runner {
    pub fn new(config: Config) -> Self {
        let ntp = config.ntp;
        let ntc = config.ntc;
        let n_half = config.n_half;

        // Inicializar flujos angulares con ceros
        let mut psi_right = DMatrix::zeros(ntp, n_half);
        let mut psi_left = DMatrix::zeros(ntp, n_half);

        // Aplicar condiciones de frontera iniciales
        if !config.reflex_izq {
            psi_right
                .row_mut(0)
                .copy_from(&config.bound_left.transpose());
        }
        if !config.reflex_der {
            psi_left
                .row_mut(ntp - 1)
                .copy_from(&config.bound_right.transpose());
        }

        Runner {
            config,
            converged: false,
            iteration: 0,
            psi_right,
            psi_left,
            scalar_flux: DVector::zeros(ntc),
            total_source: DVector::zeros(ntc),
        }
    }

    fn update_reflective_boundaries(&mut self) {
        // Izquierda (x=0): Lo que sale hacia la izquierda (PSI_LEFT[0]) entra hacia la derecha
        if self.config.reflex_izq {
            self.psi_right.row_mut(0).copy_from(&self.psi_left.row(0));
        }

        // Derecha (x=L): Lo que sale hacia la derecha (PSI_RIGHT[NTP-1]) entra hacia la izquierda
        if self.config.reflex_der {
            self.psi_left
                .row_mut(self.config.ntp - 1)
                .copy_from(&self.psi_right.row(self.config.ntp - 1));
        }
    }

    fn sweep(&mut self) {
        // 1. Pre-cálculo de la Fuente Total
        // Q_total = 0.5 * (Q_ext + Sigma_S * Phi)
        self.total_source = 0.5
            * (&self.config.q_ext_vec + self.config.sigma_s_vec.component_mul(&self.scalar_flux));

        // --- BARRIDO HACIA LA DERECHA (mu > 0) ---
        for k in 0..self.config.ntc {
            let dx = self.config.dx_vec[k];
            let st = self.config.sigma_t_vec[k];
            let source = self.total_source[k];

            for m in 0..self.config.n_half {
                let mu = self.config.miu_m[m];
                let psi_in = self.psi_right[(k, m)]; // Nodo k (entrada)

                let alpha = mu / dx;
                let num = (alpha - 0.5 * st) * psi_in + source;
                let den = alpha + 0.5 * st;

                let mut psi_out = num / den;

                // Fixup de Flujo Negativo (step-wise)
                if psi_out < 0.0 {
                    // Revertir a esquema de paso completo (paso cero)
                    psi_out = (source + alpha * psi_in) / (alpha + st);
                }

                self.psi_right[(k + 1, m)] = psi_out; // Nodo k+1 (salida)
            }
        }

        // --- BARRIDO HACIA LA IZQUIERDA (mu < 0) ---
        for k in (0..self.config.ntc).rev() {
            // k desde NTC-1 hasta 0
            let dx = self.config.dx_vec[k];
            let st = self.config.sigma_t_vec[k];
            let source = self.total_source[k];

            for m in 0..self.config.n_half {
                // Usamos valor positivo para la fórmula simétrica
                let mu = self.config.miu_m[m].abs();
                let psi_in = self.psi_left[(k + 1, m)]; // Nodo k+1 (entrada desde la derecha)

                let alpha = mu / dx;
                let num = (alpha - 0.5 * st) * psi_in + source;
                let den = alpha + 0.5 * st;

                let mut psi_out = num / den;

                // Fixup de Flujo Negativo (step-wise)
                if psi_out < 0.0 {
                    psi_out = (source + alpha * psi_in) / (alpha + st);
                }

                self.psi_left[(k, m)] = psi_out; // Nodo k (salida)
            }
        }
    }

    fn calculo_flujo(&mut self) -> DVector<f64> {
        let old_flux = self.scalar_flux.clone();

        self.scalar_flux.fill(0.0);

        for k in 0..self.config.ntc {
            let mut suma = 0.0;
            for m in 0..self.config.n_half {
                let w = self.config.omega_m[m];

                // Flujo promedio en la celda (Diamond Difference Average)
                // psi_avg = 0.5 * (psi_in + psi_out)

                // Dirección derecha (mu > 0)
                let psi_avg_der = 0.5 * (self.psi_right[(k, m)] + self.psi_right[(k + 1, m)]);
                // Dirección izquierda (mu < 0)
                let psi_avg_izq = 0.5 * (self.psi_left[(k, m)] + self.psi_left[(k + 1, m)]);

                // Integración sobre el ángulo sólido (suma de las dos direcciones)
                suma += w * (psi_avg_der + psi_avg_izq);
            }
            self.scalar_flux[k] = suma;
        }

        old_flux
    }

    fn check_convergence(&self, old_flux: &DVector<f64>) -> bool {
        // |actual - old| / actual
        let diff = (&self.scalar_flux - old_flux).abs();

        // Manejo seguro de división por cero (análogo a np.where)
        let denom: DVector<f64> = self
            .scalar_flux
            .map(|val| if val.abs() > 1e-13 { val.abs() } else { 1.0 });

        let rel_error = (diff.component_div(&denom)).max();

        if self.iteration % 100 == 0 || self.iteration == 1 {
            println!("Iter {}: Error Max = {:.2e}", self.iteration, rel_error);
        }

        rel_error < self.config.epsilon
    }

    pub fn run(&mut self) -> RunnerResult {
        println!("\n--- INICIANDO CÁLCULO S{} ---", self.config.n);
        let start = Instant::now();

        while self.iteration < self.config.max_iter {
            self.iteration += 1;

            // 1. Actualizar fronteras si es reflexiva
            self.update_reflective_boundaries();

            // 2. Barrido (Transport Sweep)
            self.sweep();

            // 3. Calcular flujo escalar y obtener el anterior
            let old_flux = self.calculo_flujo();

            // 4. Chequear convergencia
            if self.check_convergence(&old_flux) {
                println!("\nCONVERGENCIA ALCANZADA en iteración {}.", self.iteration);
                self.converged = true;
                break;
            }
        }

        let duration = start.elapsed();
        if !self.converged {
            println!("\nADVERTENCIA: Máximo de iteraciones alcanzado sin convergencia.");
        }
        println!("Tiempo de cálculo: {:.2?}", duration);

        RunnerResult {
            scalar_flux: self.scalar_flux.clone(),
            iteration: self.iteration,
            psi_right: self.psi_right.clone(),
            psi_left: self.psi_left.clone(),
            converged: self.converged,
            time: duration, // Aquí se podría crear una estructura para guardar a archivo,
                            // similar al DataFrame/Excel en Python
        }
    }
}

// Estructura para agrupar los resultados
pub struct RunnerResult {
    pub scalar_flux: DVector<f64>,
    pub iteration: i32,
    pub time: Duration,
    pub psi_right: DMatrix<f64>,
    pub psi_left: DMatrix<f64>,
    pub converged: bool,
}
