use crate::diagnostics::{
    Confidence, FailureCategory, FailureDiagnosis, context::DiagnosticContext,
};

pub(super) fn diagnose(context: &DiagnosticContext<'_>) -> Option<FailureDiagnosis> {
    if context.output_contains_any(&["environment variable not found", "notpresent", "notunicode"])
    {
        return Some(context.diagnosis(
            FailureCategory::EnvironmentVariableMissing,
            Confidence::High,
            format!(
                "revisa {}; falta una variable de entorno o su valor no es Unicode valido. Configurala en el test o maneja su ausencia explicitamente.",
                context.target
            ),
            vec![
                "Identifica el nombre de la variable leida por el caso.".to_owned(),
                "Aisla y restaura cambios globales de entorno entre tests.".to_owned(),
                "Usa un valor por defecto solo si el contrato lo permite.".to_owned(),
            ],
        ));
    }

    if context.output_contains_any(&[
        "no such file or directory",
        "permission denied",
        "os error 2",
        "os error 5",
        "os error 13",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::IoError,
            Confidence::High,
            format!(
                "revisa {}; una operacion de archivos fallo por ruta inexistente o permisos. Usa una ruta controlada por el test y conserva el error original con contexto.",
                context.target
            ),
            vec![
                "Comprueba si la ruta es absoluta o depende del directorio actual.".to_owned(),
                "Crea los directorios y fixtures necesarios en una carpeta temporal.".to_owned(),
                "Verifica permisos y elimina dependencias del entorno local.".to_owned(),
            ],
        ));
    }

    None
}
