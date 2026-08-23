# Changelog

Este archivo existe únicamente en las ramas y tags de release. `main` conserva
la documentación vigente, pero no incorpora el historial de versiones.

## [1.0.0] - 2026-08-23

Primer release preparado con puertas de producción completas. Esta versión
mantiene el propósito de v0.1.0 —ejecutar tests de Rust y presentar resultados
estructurados— y formaliza sus contratos operativos, de privacidad y de
distribución.

### Ejecución y fiabilidad

- Se documentó el modelo real: cada test se ejecuta en un proceso aislado y la
  herramienta no promete ser más rápida ni consumir menos memoria que
  `cargo test`.
- Se añadieron timeouts configurables para compilación, descubrimiento y cada
  test individual.
- Ctrl-C, SIGTERM y los timeouts terminan el grupo de procesos del test,
  incluidos sus descendientes asociados.
- stdout y stderr se drenan de forma continua y se conservan con límites
  independientes para impedir crecimiento de memoria sin límite.
- Los timeouts tienen estado `TIMEOUT`, conteo propio y código de salida fallido.
- El benchmark manual reproducible compara cinco muestras alternadas frente a
  `cargo test` sin imponer un umbral de rendimiento artificial.

### Privacidad y persistencia

- El modo `--ci` omite salida capturada, contexto fuente, directorio de trabajo
  y argumentos de ejecución en los artefactos.
- Se añadieron rutas relativas, saneamiento de formas habituales de secretos y
  truncado en el momento de captura. El saneamiento se documenta como defensa
  en profundidad, no como garantía para texto arbitrario.
- `summary.txt`, `details.json`, `history.json` y `last-run.json` se escriben
  mediante reemplazo atómico.
- Un bloqueo entre procesos serializa la lectura y actualización de los cuatro
  reportes para evitar JSON parcial y pérdida de historial en ejecuciones
  simultáneas.
- `--last-failed` distingue tests con el mismo nombre pertenecientes a targets
  distintos.

### Compatibilidad y cobertura

- Se definió soporte explícito para binarios con el harness estándar `libtest`,
  incluidos unit tests, integration tests, workspaces y selecciones de tests
  ignorados.
- Doctests, benchmarks como protocolo de resultados y harness personalizados
  quedan fuera del contrato y se rechazan cuando se detectan.
- Los argumentos que interfieren con el protocolo interno de `libtest` se
  rechazan en la CLI en lugar de producir resultados ambiguos.
- Se añadieron fixtures end-to-end para fallos reales, privacidad CI, salida
  masiva, tests bloqueados, procesos descendientes, tests ignorados, nombres
  duplicados en workspaces, errores de compilación, harness personalizados y
  persistencia concurrente.

### Reportes y distribución

- Se eliminó por completo la generación de diagnósticos o posibles soluciones;
  los reportes contienen únicamente hechos observados del test y del fallo.
- `details.json` usa el esquema 4, `history.json` el esquema 2 y `last-run.json`
  el esquema 3, manteniendo lectura de los esquemas anteriores admitidos.
- La CI valida Linux, Windows, macOS, el MSRV Rust 1.85, formato, Clippy,
  empaquetado, E2E y avisos conocidos de RustSec.
- Todas las GitHub Actions están fijadas por SHA y cada job tiene un timeout.
- El release publica archivos portables para los tres sistemas, un instalador
  Windows x86-64 probado mediante instalación/desinstalación, un archivo fuente
  explícito y `SHA256SUMS.txt`.
- El compilador Inno Setup usado por CI se descarga desde su release oficial y
  se verifica por SHA-256 antes de ejecutarlo.

### Cambios incompatibles desde v0.1.0

- La opción eliminada `output.solution` ya no se acepta en `tester.toml`.
- Los consumidores de JSON deben admitir los nuevos números de esquema y el
  estado `TIMEOUT`.
- En CI, `last-run.json` ya no conserva automáticamente argumentos para
  `--last-failed`; deben volver a proporcionarse de forma explícita.
- `--doc` y opciones del harness reservadas por `cargo-tester` producen ahora
  un error claro.

### Límites conocidos

- El arranque de un proceso por test tiene un coste medible y puede resultar
  mucho más lento que `cargo test` en suites de tests muy breves.
- El parser depende del protocolo textual del harness estándar `libtest`; un
  harness personalizado que lo imite exactamente queda fuera del contrato.
- Un daemon que se desvincule deliberadamente del grupo del test puede escapar
  al terminador.
- Los ejecutables de v1.0.0 no están firmados con Authenticode ni notarizados;
  deben verificarse con `SHA256SUMS.txt`.

## [0.1.0] - 2026-07-15

Release inicial con ejecución paralela por test, filtros, grupos secuenciales,
detalles de fallos, historial de duraciones, repetición del último conjunto
fallido, salida para CI y archivos portables para Windows, Linux y macOS.

[1.0.0]: https://github.com/Roger08G/cargo-tester/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/Roger08G/cargo-tester/releases/tag/v0.1.0
