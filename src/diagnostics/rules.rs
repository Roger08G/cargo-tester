mod assertions;
mod concurrency;
mod environment;
mod expectations;
mod runtime;

use crate::runner::{FailureOutput, TestResult};

use super::{Confidence, FailureCategory, FailureDiagnosis, context::DiagnosticContext};

pub(super) fn analyze(result: &TestResult, failure: &FailureOutput) -> FailureDiagnosis {
    let context = DiagnosticContext::new(result, failure);

    assertions::specific(&context)
        .or_else(|| expectations::diagnose(&context))
        .or_else(|| concurrency::diagnose(&context))
        .or_else(|| environment::diagnose(&context))
        .or_else(|| runtime::diagnose(&context))
        .or_else(|| assertions::plain(&context))
        .unwrap_or_else(|| generic(&context))
}

fn generic(context: &DiagnosticContext<'_>) -> FailureDiagnosis {
    context.diagnosis(
        FailureCategory::GenericPanic,
        Confidence::Medium,
        format!(
            "revisa {}; el test hizo panic con `{}`. Valida la entrada, el setup y el contrato del caso antes de modificar su expectativa.",
            context.target, context.message
        ),
        vec![
            context.rerun_action(),
            "Lee la primera ubicacion de panic y confirma el estado que llega a ella.".to_owned(),
            "Corrige el codigo si el test describe el contrato vigente; corrige el test si la expectativa esta obsoleta.".to_owned(),
        ],
    )
}
