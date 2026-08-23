# Contribuir a cargo-tester

Gracias por contribuir. Los cambios deben mantener la salida determinista, la
compatibilidad multiplataforma y los artefactos en formatos estables.

## Preparación

Requisitos:

- Rust `1.85` o posterior.
- Cargo y rustup.
- Componentes `rustfmt` y `clippy`.
- `cargo-audit` para ejecutar la auditoría local de dependencias.

```console
rustup component add rustfmt clippy
cargo build --locked
```

## Flujo de trabajo

1. Crea una rama pequeña y centrada en un único cambio.
2. Añade cobertura proporcional al comportamiento modificado.
3. Actualiza el README cuando cambie la CLI, `tester.toml` o un reporte.
4. Ejecuta todas las comprobaciones locales.

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo package --locked
cargo audit
```

Los dos fallos demostrativos de `tests/example.rs` solo deben activarse para
comprobar manualmente los reportes:

```console
cargo tester --features demo-failures --details --group sandbox
```

## Criterios de aceptación

- Los tests pasan en Linux, Windows y macOS.
- El crate sigue compilando con el MSRV declarado.
- La salida de archivo no contiene ANSI, emojis ni bordes Unicode.
- Los cambios de JSON conservan un esquema explícito y documentado.
- Los fixtures end-to-end cubren cualquier cambio de procesos, persistencia,
  privacidad o compatibilidad con `libtest`.
- Los errores incluyen contexto suficiente para actuar sobre ellos.
- No se incorporan refactors ajenos al objetivo del cambio.

## Releases

- `Cargo.toml` y el tag deben declarar exactamente la misma versión.
- Los tags de producción se crean desde una rama `vX.Y.Z` validada por CI.
- `CHANGELOG.md` vive solo en esas ramas de release y nunca se incorpora a
  `main`.
- El workflow debe publicar archivos portables, instalador Windows, fuentes y
  checksums antes de marcar el release como final.

## Pull requests

Explica el problema, el comportamiento anterior, el nuevo comportamiento y las
comprobaciones ejecutadas. Incluye ejemplos de salida cuando el cambio sea
visible para el usuario.
