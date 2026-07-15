mod sandbox {
    mod tests {
        #[test]
        fn patches_missing_initializable_inheritance_only_in_sandbox_copy() {
            let patches = ["initializable_inheritance_patch", "access_control_patch"];

            assert!(patches.contains(&"initializable_inheritance_patch"));
        }

        #[test]
        #[cfg_attr(not(feature = "demo-failures"), ignore = "intentional demo failure")]
        fn fails_when_expected_patch_is_missing() {
            let detected_patches = ["access_control_patch", "reentrancy_guard_patch"];

            assert!(
                detected_patches.contains(&"initializable_inheritance_patch"),
                "demo failure: initializable inheritance patch was not detected"
            );
        }

        #[test]
        #[cfg_attr(not(feature = "demo-failures"), ignore = "intentional demo failure")]
        fn fails_when_discovered_test_count_is_wrong() {
            let discovered_tests = ["core_tests", "sandbox_tests"].len();
            let expected_tests = discovered_tests + 1;

            assert_eq!(
                discovered_tests, expected_tests,
                "demo failure: expected one more discovered test"
            );
        }
    }
}
