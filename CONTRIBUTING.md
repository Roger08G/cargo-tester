# Contribuir a cargo-tester

Gracias por contribuir. Los cambios deben mantener la salida determinista, la
compatibilidad multiplataforma y los artefactos en formatos estables.

## Preparación

Requisitos:

- Rust `1.85` o posterior.
- Cargo y rustup.
- Componentes `rustfmt` y `clippy`.

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
- Los errores incluyen contexto suficiente para actuar sobre ellos.
- No se incorporan refactors ajenos al objetivo del cambio.

## Pull requests

Explica el problema, el comportamiento anterior, el nuevo comportamiento y las
comprobaciones ejecutadas. Incluye ejemplos de salida cuando el cambio sea
visible para el usuario.
