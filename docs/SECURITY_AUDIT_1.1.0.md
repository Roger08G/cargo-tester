# Auditoría de seguridad y calidad — 1.1.0

Fecha: 2026-09-13. Alcance: código Rust, configuración, descubrimiento y ejecución
de procesos, reportes, persistencia, dependencias, documentación y distribución.
La revisión combina inspección de código, revisión independiente y pruebas de
regresión; no constituye certificación ni garantiza ausencia de vulnerabilidades.

## Hallazgos corregidos

| Prioridad | Problema | Corrección y evidencia reproducible |
| --- | --- | --- |
| Alta | Un descendiente mantenía tuberías abiertas tras salir el test principal. | Limpieza del grupo también al completar, terminación y drenaje acotados; fixture de descendiente heredando stdout/stderr. |
| Alta | Artefactos locales antiguos podían subirse como CI tras fallar la compilación. | Invalidación inicial bajo bloqueo, saneamiento de historial/estado y omisión de panic/operandos; regresiones CI sin selección y con error de compilación. |
| Media | La captura truncada podía borrar el resultado final de `libtest`. | Cola de protocolo independiente de 1 KiB; test correcto con 2 MB de salida y captura de un byte. |
| Media | Alias del destino, espera de bloqueo y lecturas de persistencia no tenían límites suficientes. | Bloqueo en destino canónico, espera máxima de 10 s, lecturas de 16 MiB y rechazo de enlaces/archivos especiales. |
| Media | Un historial local ilegible se reemplazaba por otro vacío. | Se propaga el error sin sobrescribir los bytes y se rechazan escrituras de estado/historial superiores a 16 MiB. |
| Media | Contexto fuente podía leer rutas ajenas y archivos excesivos. | Archivo `.rs` regular dentro del workspace, máximo 1 MiB; presupuesto de índice de 64 MiB/100.000 entradas. |
| Media | Argumentos recuperados de la última ejecución no se revalidaban. | Se vuelve a aplicar el contrato de CLI antes de reproducirlos. |
| Media | Texto de los tests podía emitir controles de terminal. | Escape de controles y saneamiento conservador de secretos, incluido el formato npm; pruebas con datos sintéticos. |
| Media | Regresiones de tiempo confundían nombres idénticos en targets distintos. | Historial por nombre y target; lectura compatible del esquema anterior. |
| Baja | Indexación de líneas repetía recorridos y ciertos timestamps podían desbordarse. | Recorrido lineal y conversión defensiva; microbenchmark y regresiones. |
| Media | Reintentos de release podían reemplazar assets ya publicados por bytes distintos. | Verificación SHA-256 y subida solo de assets ausentes, sin `--clobber`. |

## Validación

La suite local de Windows x64 pasa 61 pruebas; cuatro están desactivadas por
defecto (dos demostraciones de fallo y dos benchmarks manuales). Los siete casos
end-to-end incluyen sus distintos escenarios internos. También se ejecutan
`cargo fmt`, Clippy con `-D warnings`, build release y empaquetado con lockfile.

RustSec no identifica avisos conocidos en las 79 dependencias bloqueadas en la
consulta de esta fecha. La CI exige pruebas en Linux/Windows/macOS, MSRV 1.85,
formato, Clippy, empaquetado, uso real de `cargo tester --ci` y auditoría de
dependencias. El release exige además compilación y tests por plataforma, prueba
de instalación/desinstalación Windows y checksums de los assets.

Los resultados de rendimiento y sus limitaciones están en
[EXECUTION_MODEL.md](EXECUTION_MODEL.md). El modo aislado no es más rápido en el
fixture de tests triviales medido; el ahorro en indexación no cambia ese hecho.

## Límites que permanecen

- Solo harness estándar `libtest`: no doctests ni harness personalizados.
- Tests y compiladores confiables, con permisos del usuario; no hay sandbox ni
  cota de memoria total para el test. Un proceso que escape deliberadamente del
  grupo queda fuera del control garantizado.
- Saneamiento de texto por patrones, no detección universal de secretos.
- Reemplazo atómico por archivo y bloqueo común, no transacción indivisible de
  cuatro archivos ni protección contra un actor local con iguales permisos.
- Instalador sin Authenticode; checksums no sustituyen una firma del editor.
- Los jobs y assets definitivos deben comprobarse en el
  [workflow Release](https://github.com/Roger08G/cargo-tester/actions/workflows/release.yml)
  del tag publicado; una prueba local no acredita por sí sola el release remoto.
