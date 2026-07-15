mod context;
mod rules;

use serde::Serialize;

use crate::runner::{FailureOutput, TestResult};

pub use context::parse_assertion_details;
pub(crate) use context::{source_context_at, source_line_at};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    AssertionFailed,
    AssertEqMismatch,
    AssertNeEqualValues,
    ChannelDisconnected,
    EnvironmentVariableMissing,
    GenericPanic,
    IntegerDivisionByZero,
    IntegerOverflow,
    IoError,
    MissingExpectedItem,
    MutexPoisoned,
    OptionUnwrapNone,
    OutOfBounds,
    RefCellBorrowConflict,
    ResultUnwrapErr,
    ShouldPanicMessageMismatch,
    ShouldPanicNotTriggered,
    SnapshotMismatch,
    StackOverflow,
    ThreadJoinPanic,
    UnimplementedCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailureDiagnosis {
    pub category: FailureCategory,
    pub confidence: Confidence,
    pub summary: String,
    pub evidence: Vec<String>,
    pub action_items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssertionDetails {
    pub kind: AssertionKind,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionKind {
    Assert,
    AssertEq,
    AssertNe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceContextLine {
    pub line: usize,
    pub text: String,
    pub is_panic_line: bool,
}

pub fn analyze_failure(result: &TestResult, failure: &FailureOutput) -> FailureDiagnosis {
    rules::analyze(result, failure)
}
