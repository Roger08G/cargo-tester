use crate::diagnostics::{
    Confidence, FailureCategory, FailureDiagnosis, context::DiagnosticContext,
};

pub(super) fn diagnose(context: &DiagnosticContext<'_>) -> Option<FailureDiagnosis> {
    if context.panic_expression.contains(".join()") && context.output_contains_any(&["any { .. }"])
    {
        return Some(context.diagnosis(
            FailureCategory::ThreadJoinPanic,
            Confidence::High,
            format!(
                "revisa {}; `join()` recibio el panic de un hilo secundario. El fallo original esta dentro de ese hilo, no en el propio `join`.",
                context.target
            ),
            vec![
                "Revisa la salida anterior para localizar el primer panic del hilo.".to_owned(),
                "Propaga un error con contexto desde el hilo cuando el fallo sea recuperable."
                    .to_owned(),
                context.rerun_action(),
            ],
        ));
    }

    if context.message_contains_any(&[
        "already borrowed: BorrowMutError",
        "already mutably borrowed: BorrowError",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::RefCellBorrowConflict,
            Confidence::High,
            format!(
                "revisa {}; un `RefCell` recibe prestamos incompatibles al mismo tiempo. Acorta el alcance del borrow existente antes de solicitar otro.",
                context.target
            ),
            vec![
                "Localiza los borrow o borrow_mut activos en ese bloque.".to_owned(),
                "Introduce un scope o `drop(guard)` para liberar el prestamo anterior.".to_owned(),
                "Revisa si la mutabilidad interior es necesaria en este diseno.".to_owned(),
            ],
        ));
    }

    if context.output_contains_any(&["poisonerror", "mutex is poisoned", "poisoned lock"]) {
        return Some(context.diagnosis(
            FailureCategory::MutexPoisoned,
            Confidence::High,
            format!(
                "revisa {}; el mutex quedo envenenado porque otro hilo hizo panic mientras mantenia el bloqueo. Corrige primero ese panic y decide si el estado puede recuperarse.",
                context.target
            ),
            vec![
                "Busca el primer panic del hilo que poseia el mutex.".to_owned(),
                "Evita mantener el guard durante operaciones que puedan hacer panic.".to_owned(),
                "Recupera `PoisonError::into_inner()` solo si el estado sigue siendo valido."
                    .to_owned(),
            ],
        ));
    }

    if context.output_contains_any(&[
        "sending on a closed channel",
        "receiving on a closed channel",
        "recverror",
        "senderror",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::ChannelDisconnected,
            Confidence::High,
            format!(
                "revisa {}; el canal esta desconectado. Conserva vivo el extremo necesario o maneja el cierre como parte normal del protocolo entre hilos.",
                context.target
            ),
            vec![
                "Comprueba donde se destruye el sender o receiver.".to_owned(),
                "Sincroniza la finalizacion de los hilos antes de enviar o recibir.".to_owned(),
                "Maneja SendError o RecvError si el cierre es esperado.".to_owned(),
            ],
        ));
    }

    None
}
