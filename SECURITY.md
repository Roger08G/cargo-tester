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
- Omite el mensaje libre del panic y los operandos de las aserciones; las
  anotaciones GitHub no incorporan ese texto.
- Invalida los reportes previos y sanea el estado persistido antes de compilar,
  para que una ejecución fallida no exponga artefactos locales antiguos.
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

Los tests, `build.rs`, configuración de Cargo y compiladores se ejecutan con los
permisos del usuario: esta herramienta no es un sandbox. Los descendientes que
abandonen deliberadamente su grupo quedan fuera del contrato de cancelación.

El contexto fuente solo lee archivos `.rs` regulares dentro del workspace
canónico y de hasta 1 MiB; el índice está limitado a 64 MiB y 100.000 entradas.
Historial y estado se limitan a 16 MiB por lectura y no aceptan enlaces ni
archivos especiales. El bloqueo compartido en el destino canónico espera un
máximo de 10 segundos. Los reportes nuevos usan permisos `0600` en Unix; en
Windows heredan la ACL del directorio, que debe pertenecer al usuario.

Estas medidas no protegen frente a otro proceso con los mismos permisos que
modifique concurrentemente el workspace o los binarios. Mantén privados los
directorios de trabajo y de artefactos.

La limpieza CI de la aplicación comienza después de resolver la configuración.
Si esta es inválida, no es posible confiar en su directorio de salida. Un runner
reutilizado debe limpiar los cuatro reportes conocidos antes de invocar la
herramienta. El workflow incluido realiza esa limpieza explícita.

Cada release publica `SHA256SUMS.txt` para el instalador, los archivos portables
y el archivo fuente. Descarga el checksum desde el mismo release y compáralo
antes de ejecutar un binario.

Los ejecutables de v1.1.0 no disponen de firma Authenticode ni de notarización.
El checksum detecta alteraciones, pero no sustituye una firma de código. Una
firma verificable debe incorporarse antes de afirmar que los binarios están
firmados.

Las dependencias se fijan en `Cargo.lock`, Dependabot revisa actualizaciones y la
CI consulta la base de avisos de RustSec. Un resultado limpio solo cubre los
avisos conocidos en el momento de la ejecución.

Se confirmará la recepción del informe y se evaluará su alcance antes de hacer
públicos los detalles. No se promete un plazo fijo mientras el proyecto esté en
fase inicial.
