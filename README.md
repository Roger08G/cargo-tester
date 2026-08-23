<h1 align="center">Cargo Tester</h1>

<p align="center">
  Subcomando de Cargo para ejecutar tests de Rust con reportes estructurados y legibles.
</p>

<p align="center">
  <a href="https://github.com/Roger08G/cargo-tester/actions/workflows/ci.yml"><img src="https://github.com/Roger08G/cargo-tester/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Roger08G/cargo-tester/releases"><img src="https://img.shields.io/github/v/release/Roger08G/cargo-tester?display_name=tag&sort=semver" alt="Versión"></a>
  <a href="https://github.com/Roger08G/cargo-tester/stargazers"><img src="https://img.shields.io/github/stars/Roger08G/cargo-tester?style=flat" alt="Estrellas"></a>
  <a href="https://github.com/Roger08G/cargo-tester/network/members"><img src="https://img.shields.io/github/forks/Roger08G/cargo-tester?style=flat" alt="Forks"></a>
  <img src="https://img.shields.io/badge/Rust-1.85-orange?logo=rust" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Roger08G/cargo-tester" alt="Licencia"></a>
</p>

`cargo-tester` es un subcomando de Cargo para ejecutar tests de Rust y convertir
sus resultados en una salida clara, medible y útil tanto en terminal como en CI.

Ejecutar los tests:

```console
cargo tester
```

![banner](./assets/test.png)

Ver los detalles de los tests fallidos:

```console
cargo tester --details
```

## Características

- Tabla estable con archivo, familia, nombre, estado y duración.
- Filtros por nombre, familia, grupo y último resultado fallido.
- Ejecución paralela configurable con grupos que pueden forzarse a secuencial.
- Timeout por test, cancelación del grupo de procesos y captura de salida acotada.
- Detalles de panic con archivo, línea, función, salida capturada y contexto.
- Historial acotado y detección de regresiones de rendimiento.
- Reportes persistentes en texto plano y JSON.
- Salida específica para CI y anotaciones automáticas en GitHub Actions.
- Compatibilidad explícita con binarios que usan el harness estándar `libtest`.

## Requisitos

- Rust `1.85` o posterior.
- Cargo disponible en `PATH`.
- Un proyecto o workspace Rust con tests compilables.

## Instalación

Con Cargo, fijando la versión de producción:

```console
cargo install --git https://github.com/Roger08G/cargo-tester --tag v1.0.0 --locked
```

Desde una copia local del repositorio:

```console
cargo install --path . --locked
```

En Windows x86-64 se puede usar el instalador
`cargo-tester-v1.0.0-windows-x86_64-setup.exe` publicado en
[GitHub Releases](https://github.com/Roger08G/cargo-tester/releases/latest). Instala `cargo-tester.exe` en el
directorio `bin` de `CARGO_HOME` o, si la variable no está definida, en
`%USERPROFILE%\.cargo\bin`.

El release también contiene archivos portables para Windows, Linux y macOS, un
archivo fuente explícito y `SHA256SUMS.txt`. Verifica el instalador antes de
ejecutarlo:

```powershell
(Get-FileHash .\cargo-tester-v1.0.0-windows-x86_64-setup.exe -Algorithm SHA256).Hash
Get-Content .\SHA256SUMS.txt
```

Los binarios de v1.0.0 no están firmados con Authenticode. Windows puede mostrar
una advertencia; comprueba siempre el SHA-256 descargado desde el release.

Después de instalarlo, el comando queda disponible globalmente:

```console
cargo tester --help
```

## Inicio rápido

Ejecutar todos los tests:

```console
cargo tester
```

Filtrar por nombre completo, módulo o parte del nombre:

```console
cargo tester parser::tests
cargo tester parses_a_valid_document
```

Mostrar únicamente las filas fallidas:

```console
cargo tester --failed
```

Obtener información estructurada de los fallos:

```console
cargo tester --details
cargo tester --details --failed
```

Repetir los tests que fallaron en la ejecución anterior:

```console
cargo tester --last-failed
cargo tester --last-failed --details
```

`--last-failed` conserva en ejecuciones locales las opciones de Cargo y del
harness necesarias para reproducir la selección anterior. El modo `--ci` omite
esos argumentos por privacidad; en ese caso deben indicarse de nuevo.

## Comandos

| Opción | Descripción |
| --- | --- |
| `[FILTER]` | Selecciona tests cuyo nombre contiene el filtro. |
| `--details` | Muestra detalles de los fallos y genera `details.json`. |
| `--failed` | Muestra solo fallos en la tabla, manteniendo los totales reales. |
| `--last-failed` | Ejecuta los tests fallidos de la última ejecución. |
| `--group <NAME>` | Ejecuta un grupo definido en `tester.toml`. |
| `--history` | Muestra el historial sin ejecutar tests. |
| `--ci` | Fuerza salida ASCII y genera artefactos aptos para CI. |
| `--no-color` | Desactiva los colores ANSI. |
| `-h`, `--help` | Muestra la ayuda. |
| `-V`, `--version` | Muestra la versión instalada. |

### Opciones de Cargo

Las opciones de compilación se escriben antes del filtro. Se admiten las
selecciones habituales, entre ellas `--workspace`, `--package`, `--features`,
`--all-features`, `--no-default-features`, `--lib`, `--test`, `--all-targets`,
`--release`, `--target`, `--profile`, `--locked` y `--offline`.

```console
cargo tester --workspace --all-features
cargo tester --test integration login
cargo tester --package api --features metrics auth::tests
```

### Opciones del harness

Los argumentos escritos después de `--` se pasan al harness de tests:

```console
cargo tester api::tests -- --nocapture
cargo tester -- --include-ignored
cargo tester -- --test-threads=1
```

`--exact`, `--list`, `--skip`, `--format` y `--color` están reservados porque
`cargo-tester` los utiliza para descubrir y clasificar cada test.

La selección `--ignored` y `--include-ignored` está soportada. Los doctests
(`--doc`) y los harness personalizados (`harness = false`) no lo están y
producen un error explícito. Consulta el
[contrato del modelo de ejecución](docs/EXECUTION_MODEL.md) para conocer la
semántica y los límites exactos.

## Configuración

La configuración se busca en este orden:

1. `tester.toml` en la raíz del proyecto.
2. `.cargo/tester.toml`.

Si no existe ninguno, se utilizan los valores predeterminados.

```toml
[output]
color = true
unicode = true
emoji = false
output-path = ".cargo/tester-output/"
show-passed = true
show-failed = true
show-ignored = true
max-width = 140

[execution]
# 0 utiliza el paralelismo disponible; 1 fuerza ejecución secuencial.
jobs = 0
test-timeout = "5m"
discovery-timeout = "15m"
# La salida estructurada de Cargo puede ser mayor que la de un solo test.
max-discovery-output-bytes = 16777216
# Límite independiente para stdout y stderr de cada proceso.
max-output-bytes = 262144

[privacy]
include-captured-output = true
include-source-context = true
redact = false

[timing]
slow-threshold = "1s"
very-slow-threshold = "5s"

[history]
enabled = true
max-runs = 50
show-runs-history = 20
regression-threshold-percent = 50
minimum-test-duration = "100ms"

[groups.parser]
patterns = ["parser::*", "ast::*"]

[groups.integration]
patterns = ["integration::*"]
sequential = true
```

### Salida

- `color`: activa ANSI cuando stdout es una terminal y `NO_COLOR` no está
  definido.
- `unicode`: usa bordes Unicode. Los reportes persistentes siempre son ASCII.
- `emoji`: habilita iconos en los detalles.
- `output-path`: directorio donde se guardan los artefactos.
- `show-passed`, `show-failed`, `show-ignored`: controlan las filas visibles.
- `max-width`: limita el ancho de la tabla; el mínimo permitido es `60`.

Las opciones de visibilidad nunca alteran el resumen ni el código de salida.

### Ejecución y grupos

`jobs = 0` utiliza el paralelismo disponible. Un valor positivo fija el número
máximo de procesos simultáneos. Los tests de un grupo con `sequential = true`
se ejecutan sin concurrencia, apropiado para recursos compartidos como puertos o
archivos temporales globales.

Cada test se ejecuta en un proceso aislado. `test-timeout` limita ese proceso y
`discovery-timeout` limita la compilación y cada consulta `--list`.
`max-discovery-output-bytes` acota la salida estructurada necesaria para el
descubrimiento; si no cabe completa, la herramienta pide aumentar el límite en
lugar de trabajar con una lista parcial. `max-output-bytes` conserva como máximo
ese número de bytes de stdout y de stderr por proceso de test sin dejar de
drenar sus tuberías. Un valor cero no es válido.

Los patrones de grupos utilizan globbing sobre el nombre completo del test:

```console
cargo tester --group parser
```

### Privacidad

En ejecuciones locales, `include-captured-output` e
`include-source-context` controlan si `--details` conserva la salida y el
contexto fuente. `redact` sustituye rutas absolutas del workspace/home y formas
habituales de secretos antes de persistirlas.

`--ci` aplica una política más restrictiva: activa el saneamiento y omite por
completo la salida capturada, el contexto fuente, el directorio de trabajo y los
argumentos de ejecución en los artefactos. El saneamiento es una defensa en
profundidad, no una garantía para texto arbitrario; los tests no deben imprimir
secretos. Consulta [SECURITY.md](SECURITY.md) antes de publicar artefactos.

### Historial y regresiones

`history.json` conserva como máximo `max-runs` ejecuciones. `cargo tester
--history` muestra las últimas `show-runs-history`.

Una regresión se informa cuando un test que superaba
`minimum-test-duration` aumenta al menos el porcentaje configurado en
`regression-threshold-percent`. Solo se comparan ejecuciones correctas.

## Detalles de fallos

Con `--details`, cada fallo incluye:

- ID, familia, nombre completo y duración.
- Archivo y línea de definición del test.
- Ubicación exacta y mensaje del panic.
- Salida estándar y salida de error capturadas.
- Aserción detectada y valores comparados cuando están disponibles.
- Contexto de código alrededor de la línea del fallo.
- Comandos para repetir el test.

## Archivos generados

Con la ruta por defecto:

```text
.cargo/tester-output/
|-- summary.txt
|-- details.json
|-- history.json
`-- last-run.json
```

| Archivo | Contenido |
| --- | --- |
| `summary.txt` | Última tabla en ASCII y sin secuencias ANSI. |
| `details.json` | Reporte estructurado de tests, fallos y contexto. |
| `history.json` | Historial acotado de estados y duraciones. |
| `last-run.json` | Fallos y argumentos necesarios para `--last-failed`. |

`details.json` se genera con `--details` o `--ci`. En una ejecución normal se
elimina para evitar que un reporte antiguo parezca actual.
El esquema actual es `4` y limita el contenido a datos observados del test y de
su fallo.

Las escrituras se realizan mediante reemplazo atómico. Un bloqueo común por
directorio de salida serializa las actualizaciones de ejecuciones concurrentes,
evitando JSON parcial y pérdidas del historial.

## Integración continua

`--ci` desactiva colores, Unicode y emojis, aplica la política de privacidad CI
y genera `details.json`, incluso cuando ningún test coincide:

```console
cargo tester --ci --workspace --all-features
```

Cuando `GITHUB_ACTIONS` está definido, cada fallo se publica además como una
anotación con archivo, línea, nombre y mensaje.

Este repositorio incluye:

- CI en Linux, Windows y macOS.
- Comprobación del MSRV declarado (`1.85`).
- Formato, Clippy, tests, empaquetado y prueba end-to-end.
- Fixtures end-to-end para fallos, workspaces, ignorados, salida masiva,
  timeouts, concurrencia, errores de compilación y harness personalizados.
- Publicación de reportes como artefactos de GitHub Actions.
- Retención de artefactos limitada a 7 días y lista cerrada de archivos.
- Auditoría RustSec de las dependencias bloqueadas.
- Validación previa al release, binarios multiplataforma, instalador Windows,
  checksums SHA-256 y archivo fuente al crear tags `v*`.
- Dependabot para Cargo y GitHub Actions.

## Desarrollo

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo package --locked
```

El benchmark manual del modelo aislado frente a `cargo test` se documenta en
[docs/EXECUTION_MODEL.md](docs/EXECUTION_MODEL.md). No existe una afirmación de
que un modelo sea universalmente más rápido que el otro.

`tests/example.rs` contiene dos fallos demostrativos desactivados por defecto.
Para validar visualmente el reporte completo:

```console
cargo tester --features demo-failures --details --group sandbox
```

Sin `demo-failures`, esos tests quedan ignorados y la suite permanece en verde.

## Arquitectura

```text
src/
|-- app.rs              Orquestación de una ejecución
|-- cli.rs              Argumentos de tester, Cargo y libtest
|-- config.rs           Carga y validación de tester.toml
|-- runner.rs           Descubrimiento y ejecución paralela
|-- process.rs          Timeout, cancelación y captura acotada
|-- persistence.rs      Bloqueo y reemplazo atómico de reportes
|-- privacy.rs          Saneamiento de rutas y texto sensible
|-- source.rs           Resolución de archivo, función y línea
|-- history.rs          Historial y regresiones
|-- state.rs            Estado de la última ejecución
`-- reporter/           Tabla, detalles, JSON, CI e historial
```

Consulta [CONTRIBUTING.md](CONTRIBUTING.md) para preparar cambios y
[SECURITY.md](SECURITY.md) para reportar problemas de seguridad.

## Licencia

Este proyecto se distribuye bajo la [licencia MIT](LICENSE).
