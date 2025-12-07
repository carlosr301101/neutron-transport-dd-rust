mod algorithm;
use algorithm::{Runner,RunnerResult,Config};
use std::fs::File;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
extern crate nalgebra;
extern crate once_cell;

fn main() {
    // 1. Ejecutar el cálculo
    // Esto asume que tienes una función new_manual, o la carga manual de datos.
    let conf = Config::new_manual(1e-5, 2000); 
    let mut runner = Runner::new(conf);
    let result = runner.run();
    
    // ----------------------------------------------------
    // 2. Generar el nombre del archivo con timestamp
    // ----------------------------------------------------
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time travel happened").as_secs();
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
        \n--- FLUJO ESCALAR (Scalar Flux) ---\n\
        {}",
        // Asumiendo que RunnerResult tiene un campo 'time' o 'duration'
        result.time.as_millis(), // Reemplaza con el campo de tiempo real si existe
        result.iteration, 
        result.scalar_flux.to_string() // to_string() de nalgebra
        // Puedes añadir más datos aquí:
        // format!("\n--- MATRIZ PSI ---\n{}", result.psi_matrix.to_string()), 
    );

    // Crear y escribir en el archivo
    let mut file = File::create(&filename)
        .expect(&format!("Fallo al crear el archivo: {}", filename));
    
    file.write_all(output_content.as_bytes())
        .expect("Fallo al escribir datos al archivo");

    println!("\n✅ Resultados guardados en el archivo de salida: {}", filename);
}