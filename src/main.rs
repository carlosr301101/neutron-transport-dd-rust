mod algorithm;
use algorithm::{Runner,RunnerResult,Config};
extern crate nalgebra;
extern crate once_cell;

fn main() {
    // Definición de la precisión de convergencia y el límite de iteraciones
    let epsilon = 1e-5;
    let max_iter = 2000;

    // Configuración y ejecución
    match std::panic::catch_unwind(|| {
        let conf = Config::new_manual(epsilon, max_iter);
        let mut runner = Runner::new(conf);
        runner.run()
    }) {
        Ok(result) => {
            println!("\n--- RESULTADO FINAL (FLUJO ESCALAR) ---");
            println!("Flujo Escalar (por celda): \n{:?}", result.scalar_flux);
            println!("Iteraciones requeridas: {}", result.iteration);
            println!("Convergencia: {}", result.converged);
        }
        Err(_) => {
            eprintln!("\nOcurrió un error en el programa (panic). Revise las entradas o la lógica del código.");
        }
    }
}