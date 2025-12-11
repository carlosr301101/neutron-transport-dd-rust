mod algorithm;
use algorithm::{Config, Runner, RunnerResult};
use clap::Parser;
use std::fs::File;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
extern crate nalgebra;
extern crate once_cell;

#[derive(Parser, Debug)]
#[command(name = "neutron-diffusion-dd")]
#[command(about = "Simulador de difusión de neutrones S_N", long_about = None)]
struct Args {
    /// Ruta al archivo input.json para modo automático
    #[arg(short, long, value_name = "FILE")]
    input: Option<String>,

    /// Tolerancia de convergencia (por defecto 1e-5)
    #[arg(short, long, default_value = "0.00001")]
    epsilon: f64,

    /// Número máximo de iteraciones (por defecto 2000)
    #[arg(short, long, default_value = "2000")]
    max_iter: i32,
}

fn main() {
    let args = Args::parse();

    // 1. Ejecutar el cálculo
    let conf = if let Some(input_path) = &args.input {
        // Modo automático: leer desde input.json
        println!("\n📂 Usando archivo de configuración: {}", input_path);
        Config::new_auto(args.epsilon, args.max_iter, input_path)
    } else {
        // Modo manual: solicitar entrada interactiva
        println!("\n⌨️  Modo manual: ingrese los parámetros interactivamente");
        Config::new_manual(args.epsilon, args.max_iter)
    };
    let mut runner = Runner::new(conf);
    let result = runner.run();

    // ----------------------------------------------------
    // 2. Generar el nombre del archivo con timestamp
    // ----------------------------------------------------
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time travel happened")
        .as_secs();
    // Nuevo formato de archivo: data_timestamp.out
    let filename = format!("data_{}.out", now);

    // ----------------------------------------------------
    // 3. Formatear los datos y escribir al archivo
    // ----------------------------------------------------

    // Usamos el DVector::to_string() de nalgebra para una representación amigable.
    let output_content = format!(
        "--- RESULTADOS DE LA SIMULACIÓN DE TRANSPORTE ---\n\n\
        Tiempo de ejecución: {}\n\
        Iteraciones totales: {}\n\
        Converge: {}\n\
        \n--- FLUJO ESCALAR (Scalar Flux) ---\n\
        {}",
        // Asumiendo que RunnerResult tiene un campo 'time' o 'duration'
        result.time.as_millis(), // Reemplaza con el campo de tiempo real si existe
        result.iteration,
        result.converged,
        result.scalar_flux.to_string() // to_string() de nalgebra
                                       // Puedes añadir más datos aquí:
                                       // format!("\n--- MATRIZ PSI ---\n{}", result.psi_matrix.to_string()),
    );

    // Crear y escribir en el archivo
    let mut file =
        File::create(&filename).expect(&format!("Fallo al crear el archivo: {}", filename));

    file.write_all(output_content.as_bytes())
        .expect("Fallo al escribir datos al archivo");

    println!(
        "\n✅ Resultados guardados en el archivo de salida: {}",
        filename
    );
}
