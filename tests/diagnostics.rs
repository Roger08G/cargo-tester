use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cargo_tester::{
    diagnostics::{
        AssertionKind, Confidence, FailureCategory, analyze_failure, parse_assertion_details,
    },
    runner::{FailureOutput, PanicDetails, TestResult, TestStatus},
};

mod solution_rules {
    use super::*;

    #[test]
    fn identifies_common_runtime_failures() {
        let cases = [
            (
                "called `Option::unwrap()` on a `None` value",
                FailureCategory::OptionUnwrapNone,
            ),
            (
                "called `Result::unwrap()` on an `Err` value: invalid input",
                FailureCategory::ResultUnwrapErr,
            ),
            (
                "index out of bounds: the len is 2 but the index is 3",
                FailureCategory::OutOfBounds,
            ),
            (
                "attempt to divide by zero",
                FailureCategory::IntegerDivisionByZero,
            ),
            (
                "attempt to multiply with overflow",
                FailureCategory::IntegerOverflow,
            ),
            (
                "already borrowed: BorrowMutError",
                FailureCategory::RefCellBorrowConflict,
            ),
            (
                "called `Result::unwrap()` on an `Err` value: PoisonError { .. }",
                FailureCategory::MutexPoisoned,
            ),
            (
                "called `Result::unwrap()` on an `Err` value: SendError { .. }",
                FailureCategory::ChannelDisconnected,
            ),
            (
                "called `Result::unwrap()` on an `Err` value: NotPresent",
                FailureCategory::EnvironmentVariableMissing,
            ),
            (
                "called `Result::unwrap()` on an `Err` value: No such file or directory (os error 2)",
                FailureCategory::IoError,
            ),
            (
                "not yet implemented: parser branch",
                FailureCategory::UnimplementedCode,
            ),
        ];

        for (index, (message, expected)) in cases.into_iter().enumerate() {
            let result = failed_result(index + 1, Some(message), message, 1);
            let diagnosis = analyze_failure(
                &result,
                result.failure.as_ref().expect("failure should exist"),
            );

            assert_eq!(
                diagnosis.category, expected,
                "wrong category for message: {message}"
            );
            assert_eq!(diagnosis.confidence, Confidence::High);
            assert!(!diagnosis.action_items.is_empty());
            assert!(!diagnosis.evidence.is_empty());
        }
    }

    #[test]
    fn gives_mutex_poisoning_precedence_when_not_wrapped_in_result() {
        let message = "mutex is poisoned";
        let result = failed_result(1, Some(message), message, 1);
        let diagnosis = diagnosis_for(&result);

        assert_eq!(diagnosis.category, FailureCategory::MutexPoisoned);
    }

    #[test]
    fn points_to_the_child_thread_when_join_propagates_a_panic() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let source = std::env::temp_dir().join(format!(
            "cargo_tester_join_{}_{}.rs",
            std::process::id(),
            unique
        ));
        fs::write(
            &source,
            "fn wait_for_worker() { worker.join().unwrap(); }\n",
        )
        .expect("source fixture should be written");
        let message = "called `Result::unwrap()` on an `Err` value: Any { .. }";
        let mut result = failed_result(1, Some(message), message, 1);
        result.file = source.display().to_string();
        result
            .failure
            .as_mut()
            .unwrap()
            .panic
            .as_mut()
            .unwrap()
            .file = source.display().to_string();

        let diagnosis = diagnosis_for(&result);

        assert_eq!(diagnosis.category, FailureCategory::ThreadJoinPanic);
        assert!(diagnosis.summary.contains("hilo secundario"));

        fs::remove_file(source).expect("source fixture should be removed");
    }

    #[test]
    fn identifies_assert_eq_and_preserves_compared_values() {
        let output = "\
assertion `left == right` failed: values differ
  left: 2
 right: 3
";
        let result = failed_result(1, Some("values differ"), output, 1);
        let failure = result.failure.as_ref().expect("failure should exist");
        let assertion = parse_assertion_details(failure).expect("assertion should parse");
        let diagnosis = analyze_failure(&result, failure);

        assert_eq!(assertion.kind, AssertionKind::AssertEq);
        assert_eq!(assertion.left.as_deref(), Some("2"));
        assert_eq!(assertion.right.as_deref(), Some("3"));
        assert_eq!(diagnosis.category, FailureCategory::AssertEqMismatch);
        assert!(diagnosis.summary.contains("Valor real: `2`"));
        assert!(diagnosis.summary.contains("esperado: `3`"));
    }

    #[test]
    fn identifies_assert_ne_with_equal_values() {
        let output = "\
assertion `left != right` failed
  left: \"same\"
 right: \"same\"
";
        let result = failed_result(1, Some("assertion failed"), output, 1);

        assert_eq!(
            diagnosis_for(&result).category,
            FailureCategory::AssertNeEqualValues
        );
    }

    #[test]
    fn identifies_plain_assertions() {
        let result = failed_result(
            1,
            Some("assertion failed: amount > 0"),
            "assertion failed: amount > 0",
            1,
        );

        assert_eq!(
            diagnosis_for(&result).category,
            FailureCategory::AssertionFailed
        );
    }

    #[test]
    fn uses_source_context_for_contains_assertions() {
        let result = failed_result(
            1,
            Some("initializable inheritance patch was not detected"),
            "initializable inheritance patch was not detected",
            15,
        );
        let diagnosis = diagnosis_for(&result);

        assert_eq!(diagnosis.category, FailureCategory::MissingExpectedItem);
        assert!(diagnosis.summary.contains("detected_patches"));
        assert!(
            diagnosis
                .summary
                .contains("initializable_inheritance_patch")
        );
    }

    #[test]
    fn identifies_should_panic_contract_failures() {
        let missing_panic = failed_result(1, None, "note: test did not panic as expected", 1);
        let wrong_message = failed_result(
            2,
            None,
            "panic did not contain expected string\n panic message: wrong",
            1,
        );

        assert_eq!(
            diagnosis_for(&missing_panic).category,
            FailureCategory::ShouldPanicNotTriggered
        );
        assert_eq!(
            diagnosis_for(&wrong_message).category,
            FailureCategory::ShouldPanicMessageMismatch
        );
    }

    #[test]
    fn identifies_snapshot_and_stack_failures_without_panic_metadata() {
        let snapshot = failed_result(1, None, "snapshot assertion for 'response' failed", 1);
        let stack = failed_result(2, None, "fatal runtime error: stack overflow", 1);

        assert_eq!(
            diagnosis_for(&snapshot).category,
            FailureCategory::SnapshotMismatch
        );
        assert_eq!(
            diagnosis_for(&stack).category,
            FailureCategory::StackOverflow
        );
    }

    #[test]
    fn falls_back_to_a_medium_confidence_generic_diagnosis() {
        let result = failed_result(1, Some("domain invariant rejected"), "", 1);
        let diagnosis = diagnosis_for(&result);

        assert_eq!(diagnosis.category, FailureCategory::GenericPanic);
        assert_eq!(diagnosis.confidence, Confidence::Medium);
        assert!(diagnosis.summary.contains("domain invariant rejected"));
    }
}

fn diagnosis_for(result: &TestResult) -> cargo_tester::diagnostics::FailureDiagnosis {
    analyze_failure(
        result,
        result.failure.as_ref().expect("failure should exist"),
    )
}

fn failed_result(id: usize, message: Option<&str>, output: &str, panic_line: usize) -> TestResult {
    TestResult {
        id,
        full_name: format!("diagnostics::case_{id}"),
        file: "tests/example.rs".to_owned(),
        family: "diagnostics".to_owned(),
        test: format!("case_{id}"),
        line: Some(1),
        source_line: Some(format!("fn case_{id}() {{")),
        status: TestStatus::Fail,
        duration: Duration::from_millis(4),
        executable: PathBuf::from("target/debug/deps/diagnostics-test"),
        target_source: PathBuf::from("tests/diagnostics.rs"),
        failure: Some(FailureOutput {
            stdout: output.to_owned(),
            stderr: String::new(),
            exit_code: Some(101),
            panic: message.map(|message| PanicDetails {
                file: "tests/example.rs".to_owned(),
                line: panic_line,
                column: Some(13),
                message: Some(message.to_owned()),
            }),
        }),
    }
}
