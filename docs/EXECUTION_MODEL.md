# Contrato del modelo de ejecución

`cargo-tester` prioriza el aislamiento y el reporte individual. No promete ser
más rápido ni consumir menos recursos que `cargo test`.

## Flujo

1. Compila los targets de test con `cargo test --no-run --message-format=json`.
2. Interroga cada binario con `--list --format=terse`.
3. Ejecuta cada test descubierto en un proceso independiente con `--exact`.
4. Mantiene hasta `execution.jobs` procesos en paralelo; los grupos marcados
   como secuenciales se ejecutan después, uno a uno.

Este contrato permite asociar a cada test su estado, duración y salida, y evita
que el estado global del proceso se comparta entre tests. También implica costes
reales: cada test vuelve a arrancar el binario, la duración incluye ese arranque,
puede aumentar el uso de memoria y el orden no coincide necesariamente con
`cargo test`.

## Límites operativos

- `execution.test-timeout` limita cada proceso de test.
- `execution.discovery-timeout` limita tanto la compilación como cada consulta
  de descubrimiento.
- Al vencer un timeout o recibirse Ctrl-C/SIGTERM, se termina el grupo de
  procesos completo, incluidos los descendientes que sigan asociados a él.
- `execution.max-discovery-output-bytes` acota la salida de compilación y
  descubrimiento. Si stdout no cabe completo, se aborta con un error explícito
  para no ejecutar una lista parcial.
- `execution.max-output-bytes` conserva como máximo esa cantidad por stream y
  proceso de test. stdout y stderr se siguen drenando para que una tubería llena
  no bloquee al hijo; los tests se lanzan con `--nocapture` para evitar una
  segunda acumulación ilimitada dentro de `libtest`, y el reporte marca qué
  stream fue truncado.
- El parser conserva aparte una cola de stdout de 1 KiB para clasificar el
  resultado de `libtest` aunque se trunque la salida visible. Esto no amplía la
  captura publicada ni elimina la dependencia del harness estándar.
- También al salir correctamente el proceso principal se limpia su grupo. La
  terminación y el drenaje tienen cada uno una gracia de hasta un segundo, para
  no esperar indefinidamente por tuberías heredadas.
- Un timeout tiene estado `TIMEOUT`, cuenta por separado de `FAIL` y produce un
  código de salida fallido.

Los límites se aplican en la máquina donde corre la herramienta. Un proceso ya
desvinculado deliberadamente del grupo del test puede quedar fuera del alcance
del terminador; los tests no deben lanzar daemons persistentes.

## Compatibilidad

La versión actual admite binarios que implementen el harness estándar `libtest`
de Rust y su salida de texto. Incluye tests unitarios y de integración, targets
seleccionados por Cargo, workspaces y selecciones `--ignored`/
`--include-ignored`.

No se admiten doctests (`--doc`), benchmarks como protocolo de resultados ni
harness personalizados (`harness = false`). La herramienta devuelve un error
explícito cuando reconoce una selección no soportada; un harness personalizado
que imite exactamente el texto de `libtest` queda fuera del contrato aunque
pueda parecer funcionar.

## Benchmark reproducible

Existe un benchmark end-to-end manual y sin umbral artificial. Calienta ambos
caminos y mide, sobre el mismo fixture de 24 tests triviales, cinco muestras
alternadas de `cargo test --lib` y `cargo-tester --lib`. El resultado usa la
mediana:

```console
cargo test --test e2e benchmark_execution_models -- --ignored --nocapture
```

El resultado imprime tiempos y ratio de esa máquina. No es una garantía de
rendimiento: para una comparación seria se deben repetir muestras en el hardware
y la carga reales del proyecto, observando también memoria máxima. La CI ejecuta
los contratos funcionales, pero no falla por un ratio de tiempo inestable.

El índice fuente recorre cada archivo una sola vez, evita volver a recorrer sus
líneas por cada declaración y aplica un presupuesto global. El benchmark
`cargo test --locked --test features benchmark_source_index_line_lookup -- --ignored --nocapture`
permite observar este coste
de forma independiente; no mide el consumo de memoria de los tests ejecutados.

### Muestra local de la auditoría 1.1.0 (2026-09-13)

En Windows x64 con Rust 1.97.1, sobre el fixture de 24 tests triviales y cinco
muestras: `cargo test` obtuvo una mediana de 54 ms y `cargo-tester` de 910 ms
(ratio 16,71). El aislamiento por test penaliza claramente esta carga.

En el microbenchmark fuente de 2.000 declaraciones, la búsqueda de líneas del
algoritmo anterior tardó 1.014 ms frente a 31 ms del índice completo nuevo.
Es una muestra en build debug, no un benchmark de toda la herramienta ni una
garantía de aceleración. No se ha medido RSS pico en esta auditoría.
