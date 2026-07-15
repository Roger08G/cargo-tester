use cargo_tester::runner::PanicDetails;

mod panic_parser {
    use super::*;

    #[test]
    fn parses_modern_windows_locations_and_messages() {
        let output = "\
thread 'case' panicked at tests\\example.rs:14:13:
expected patch was not detected
note: run with `RUST_BACKTRACE=1`
";

        let panic = PanicDetails::parse(output).expect("panic should parse");

        assert_eq!(panic.file, "tests\\example.rs");
        assert_eq!(panic.line, 14);
        assert_eq!(panic.column, Some(13));
        assert_eq!(
            panic.message.as_deref(),
            Some("expected patch was not detected")
        );
    }

    #[test]
    fn parses_unix_locations_without_columns() {
        let output = "\
thread 'case' panicked at tests/example.rs:27:
plain panic
";

        let panic = PanicDetails::parse(output).expect("panic should parse");

        assert_eq!(panic.file, "tests/example.rs");
        assert_eq!(panic.line, 27);
        assert_eq!(panic.column, None);
        assert_eq!(panic.message.as_deref(), Some("plain panic"));
    }

    #[test]
    fn parses_legacy_inline_panic_messages() {
        let output = "thread 'case' panicked at 'legacy message', src/lib.rs:8:4";

        let panic = PanicDetails::parse(output).expect("legacy panic should parse");

        assert_eq!(panic.file, "src/lib.rs");
        assert_eq!(panic.line, 8);
        assert_eq!(panic.column, Some(4));
        assert_eq!(panic.message.as_deref(), Some("legacy message"));
    }

    #[test]
    fn ignores_output_without_a_panic_location() {
        assert!(PanicDetails::parse("test result: FAILED").is_none());
    }
}
