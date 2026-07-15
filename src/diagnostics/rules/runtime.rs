use crate::diagnostics::{
    Confidence, FailureCategory, FailureDiagnosis, context::DiagnosticContext,
};

pub(super) fn diagnose(context: &DiagnosticContext<'_>) -> Option<FailureDiagnosis> {
    if context.message_contains_any(&["called `Option::unwrap()` on a `None` value"]) {
        return Some(context.diagnosis(
            FailureCategory::OptionUnwrapNone,
            Confidence::High,
            format!(
                "revisa {}; se extrae un `Option::None`. Asegura que el valor exista en el setup o maneja `None` de forma explicita.",
                context.target
            ),
            vec![
                "Localiza el unwrap en la linea del panic.".to_owned(),
                "Usa match, if let u ok_or si `None` es un resultado valido.".to_owned(),
                "Corrige el fixture o la entrada si el valor era obligatorio.".to_owned(),
            ],
        ));
    }

    if context.message_contains_any(&[
        "index out of bounds",
        "range end index",
        "range start index",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::OutOfBounds,
            Confidence::High,
            format!(
                "revisa {}; hay un acceso fuera de los limites de una coleccion o slice. Corrige el indice o garantiza el tamano minimo antes del acceso.",
                context.target
            ),
            vec![
                "Comprueba `len()` justo antes del acceso.".to_owned(),
                "Ajusta el indice, el rango o el fixture que construye la coleccion.".to_owned(),
                "Usa `get()` si la ausencia del elemento debe manejarse sin panic.".to_owned(),
            ],
        ));
    }

    if context.message_contains_any(&[
        "attempt to divide by zero",
        "attempt to calculate the remainder with a divisor of zero",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::IntegerDivisionByZero,
            Confidence::High,
            format!(
                "revisa {}; el divisor es cero. Valida la entrada antes de dividir o corrige el fixture que produce ese valor.",
                context.target
            ),
            vec![
                "Inspecciona como se calcula el divisor.".to_owned(),
                "Define el comportamiento esperado para cero antes de realizar la operacion."
                    .to_owned(),
                "Anade un caso de prueba especifico para divisor cero.".to_owned(),
            ],
        ));
    }

    if context.message_contains_any(&[
        "attempt to add with overflow",
        "attempt to subtract with overflow",
        "attempt to multiply with overflow",
        "attempt to divide with overflow",
        "attempt to negate with overflow",
    ]) {
        return Some(context.diagnosis(
            FailureCategory::IntegerOverflow,
            Confidence::High,
            format!(
                "revisa {}; una operacion aritmetica desborda en modo debug. Valida limites o usa una operacion checked, saturating o wrapping solo con la semantica adecuada.",
                context.target
            ),
            vec![
                "Identifica los operandos que llegan a la operacion.".to_owned(),
                "Ajusta el tipo numerico o valida sus limites.".to_owned(),
                "No ocultes el overflow con wrapping si el dominio no lo permite.".to_owned(),
            ],
        ));
    }

    if context.output_contains_any(&["stack overflow", "has overflowed its stack"]) {
        return Some(context.diagnosis(
            FailureCategory::StackOverflow,
            Confidence::High,
            format!(
                "revisa {}; el proceso agoto la pila. Busca recursion sin caso base, ciclos de llamadas o estructuras locales excesivamente grandes.",
                context.target
            ),
            vec![
                "Comprueba el caso base y el progreso de cada llamada recursiva.".to_owned(),
                "Detecta ciclos indirectos entre funciones.".to_owned(),
                "Mueve buffers grandes al heap si la recursion no es la causa.".to_owned(),
            ],
        ));
    }

    if context.message_contains_any(&["called `Result::unwrap()` on an `Err` value"]) {
        return Some(context.diagnosis(
            FailureCategory::ResultUnwrapErr,
            Confidence::High,
            format!(
                "revisa {}; se extrae un `Result::Err`. Corrige la causa del error o propagalo y manejalo de forma explicita.",
                context.target
            ),
            vec![
                "Lee el valor Err completo en la salida capturada.".to_owned(),
                "Corrige la entrada o dependencia que genera el error.".to_owned(),
                "Usa `?` o manejo explicito si el error forma parte del flujo esperado.".to_owned(),
            ],
        ));
    }

    None
}
