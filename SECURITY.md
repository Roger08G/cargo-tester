# Política de seguridad

## Versiones soportadas

| Versión | Soporte de seguridad |
| --- | --- |
| `1.x` | Sí |
| `0.x` | No |

## Reportar una vulnerabilidad

No publiques vulnerabilidades como issues públicos. Utiliza la opción
**Security > Report a vulnerability** del repositorio para abrir un aviso
privado con:

- Versión afectada.
- Entorno y comando utilizado.
- Impacto observado o potencial.
- Pasos mínimos para reproducirlo.
- Mitigación conocida, si existe.

Evita adjuntar reportes que contengan secretos, tokens, variables de entorno o
rutas privadas sin haberlos saneado antes.

## Datos de los reportes

`summary.txt`, `details.json`, `history.json` y `last-run.json` deben tratarse
como datos potencialmente sensibles. Fuera del modo CI, `--details` puede
incluir stdout, stderr y contexto fuente porque resultan útiles para depuración
local.

El modo `--ci` aplica estas restricciones antes de generar artefactos:

- Omite stdout y stderr capturados.
- Omite el contexto fuente, el directorio de trabajo y los argumentos usados.
- Convierte rutas absolutas del workspace en rutas relativas y oculta las rutas
  externas.
- Sanea asignaciones habituales de secretos y formatos conocidos de tokens.
- Trunca cada stream en el límite `execution.max-output-bytes` durante la
  ejecución, aunque el stream no vaya a persistirse.

El saneamiento usa patrones conservadores y no puede reconocer cualquier dato
privado posible. No imprimas credenciales ni información personal desde los
tests, revisa el artefacto antes de compartirlo y reduce su retención al mínimo
necesario. Para reportes locales también se puede aplicar una política estricta:

```toml
[privacy]
include-captured-output = false
include-source-context = false
redact = true
```

La CI incluida sube solo los cuatro reportes conocidos y los conserva durante
7 días. Si el repositorio contiene información especialmente sensible, desactiva
la subida de artefactos o reduce aún más esa retención.

## Integridad de distribución

Cada release publica `SHA256SUMS.txt` para el instalador, los archivos portables
y el archivo fuente. Descarga el checksum desde el mismo release y compáralo
antes de ejecutar un binario.

Los ejecutables de v1.0.0 no disponen de firma Authenticode ni de notarización.
El checksum detecta alteraciones, pero no sustituye una firma de código. Una
firma verificable debe incorporarse antes de afirmar que los binarios están
firmados.

Las dependencias se fijan en `Cargo.lock`, Dependabot revisa actualizaciones y la
CI consulta la base de avisos de RustSec. Un resultado limpio solo cubre los
avisos conocidos en el momento de la ejecución.

Se confirmará la recepción del informe y se evaluará su alcance antes de hacer
públicos los detalles. No se promete un plazo fijo mientras el proyecto esté en
fase inicial.
