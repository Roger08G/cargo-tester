use crate::diagnostics::{
    Confidence, FailureCategory, FailureDiagnosis, context::DiagnosticContext,
};

pub(super) fn diagnose(context: &DiagnosticContext<'_>) -> Option<FailureDiagnosis> {
    if context.output_contains_any(&[
        "panic did not contain expected string",
        "panic message does not contain expected substring",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::ShouldPanicMessageMismatch,
            Confidence::High,
            format!(
                "revisa {}; el test hizo panic, pero el mensaje no coincide con `#[should_panic(expected = ...)]`. Corrige el mensaje esperado o la causa real del panic.",
                context.target
            ),
            vec![
                "Compara el texto de `expected` con el mensaje capturado.".to_owned(),
                "Evita expectativas demasiado fragiles si solo importa una parte estable del error."
                    .to_owned(),
                context.rerun_action(),
            ],
        ));
    }

    if context.output_contains_any(&["test did not panic as expected"]) {
        return Some(context.diagnosis(
            FailureCategory::ShouldPanicNotTriggered,
            Confidence::High,
            format!(
                "revisa {}; el test esta marcado con `#[should_panic]`, pero el codigo termino normalmente. Ajusta la entrada para alcanzar el panic esperado o elimina el atributo si el contrato cambio.",
                context.target
            ),
            vec![
                "Confirma si el comportamiento correcto todavia debe producir un panic.".to_owned(),
                "Revisa que el caso de prueba alcance la rama prevista.".to_owned(),
                "Ajusta la entrada, la expectativa o el atributo del test.".to_owned(),
            ],
        ));
    }

    if context.output_contains_any(&["snapshot assertion"])
        && context.output_contains_any(&["failed"])
    {
        return Some(context.diagnosis(
            FailureCategory::SnapshotMismatch,
            Confidence::High,
            format!(
                "revisa {}; la salida actual no coincide con el snapshot. Inspecciona el diff y acepta el nuevo snapshot solo si el cambio es intencionado.",
                context.target
            ),
            vec![
                "Revisa el diff completo entre snapshot esperado y salida actual.".to_owned(),
                "Corrige la regresion si el cambio no era intencionado.".to_owned(),
                "Actualiza el snapshot unicamente despues de validar el nuevo contrato.".to_owned(),
            ],
        ));
    }

    if context.message_contains_any(&["not implemented", "not yet implemented"]) {
        return Some(context.diagnosis(
            FailureCategory::UnimplementedCode,
            Confidence::High,
            format!(
                "revisa {}; el caso alcanza codigo sin implementar. Implementa esa rama o evita que el test dependa de una ruta marcada con `todo!` o `unimplemented!`.",
                context.target
            ),
            vec![
                "Busca todo!(), unimplemented!() o un panic equivalente en la ruta.".to_owned(),
                "Implementa la rama necesaria para este escenario.".to_owned(),
                context.rerun_action(),
            ],
        ));
    }

    None
}
