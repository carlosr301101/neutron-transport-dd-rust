# Notas sobre el runner usando RUST

## Ideas que tengo que implementar

- Revisar la implementacion del algoritmo para poder usarlo con DD y con STEP.
- Creacion de un nuevo runner que use RUST, y que se pueda usar con el algoritmo de DD y STEP.
- Revisar los benchmarks para hacer las comparaciones con los runners implementados en C.

## Red Neural 1-D

> .[!TIP]
> En este caso hay que hacer algunos ajustes en la red neural, para poder hacer este entrenamiento

### Primer paso

- En este primer paso se quiere entrenar un modelo para predecir el parametro de Corte para los problemas de 1D
- El core utilizado es en Rust para poder hacer todo lo necesario de los calculos de las soluciones
- Luego se pasan esos resultados a unos batch de datos para poder entrenar el modelo.
