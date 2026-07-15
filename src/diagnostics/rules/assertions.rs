use crate::diagnostics::{
    AssertionKind, Confidence, FailureCategory, FailureDiagnosis, context::DiagnosticContext,
};

pub(super) fn specific(context: &DiagnosticContext<'_>) -> Option<FailureDiagnosis> {
    if let Some(contains) = context.contains_check() {
        let collection = contains
            .collection
            .as_deref()
            .map(|name| format!(" `{name}`"))
            .unwrap_or_else(|| " la coleccion evaluada".to_owned());
        return Some(context.diagnosis(
            FailureCategory::MissingExpectedItem,
            Confidence::High,
            format!(
                "revisa {}; el assert comprueba que{collection} contenga `{}`, pero ese valor no llega al test. Anadelo al setup o fixture si debe existir, o corrige la logica que construye{collection}.",
                context.target, contains.expected
            ),
            vec![
                format!(
                    "Comprueba por que{collection} no contiene `{}`.",
                    contains.expected
                ),
                "Si el dato debe existir, actualiza el fixture o la preparacion del test."
                    .to_owned(),
                "Si lo calcula el codigo bajo prueba, corrige esa ruta antes del assert."
                    .to_owned(),
            ],
        ));
    }

    let assertion = context.assertion.as_ref()?;
    match assertion.kind {
        AssertionKind::AssertEq => Some(context.diagnosis(
            FailureCategory::AssertEqMismatch,
            Confidence::High,
            format!(
                "revisa {}; `assert_eq!` compara valores distintos.{} Corrige la logica que produce el valor real o actualiza la expectativa si el contrato ha cambiado.",
                context.target,
                context.assertion_values(assertion)
            ),
            vec![
                "Identifica que expresion es el resultado real y cual es la expectativa.".to_owned(),
                "Corrige el calculo o ajusta la expectativa documentando el cambio.".to_owned(),
                context.rerun_action(),
            ],
        )),
        AssertionKind::AssertNe => Some(context.diagnosis(
            FailureCategory::AssertNeEqualValues,
            Confidence::High,
            format!(
                "revisa {}; `assert_ne!` esperaba valores distintos, pero son iguales.{} Cambia los datos del caso si deben divergir o corrige la logica que los produce.",
                context.target,
                context.assertion_values(assertion)
            ),
            vec![
                "Identifica que entradas hacen que ambos valores terminen iguales.".to_owned(),
                "Corrige el caso de prueba o la rama que deberia producir otro valor.".to_owned(),
                context.rerun_action(),
            ],
        )),
        AssertionKind::Assert => None,
    }
}

pub(super) fn plain(context: &DiagnosticContext<'_>) -> Option<FailureDiagnosis> {
    context.assertion.as_ref()?;
    Some(context.diagnosis(
        FailureCategory::AssertionFailed,
        Confidence::High,
        format!(
            "revisa {}; la condicion de `assert!` evalua a false. Comprueba el estado preparado y la logica que deberia satisfacerla.",
            context.target
        ),
        vec![
            "Lee la condicion exacta del assert y valida sus operandos.".to_owned(),
            "Comprueba fixtures, mocks y setup previos.".to_owned(),
            context.rerun_action(),
        ],
    ))
}
