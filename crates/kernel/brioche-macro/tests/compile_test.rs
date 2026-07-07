//! Compile tests for `brioche-macro` procedural macros.
//!
//! Uses `trybuild` to verify that `BriocheExtensionType` derive rejects
//! invalid input (HashMap, HashSet, missing Clone) and accepts valid input,
//! and that the plugin/offload-task attribute macros accept valid input and
//! reject malformed input.
//!
//! Refs: I-Core-ExtensionType

mod compile_tests {
    #[test]
    fn extension_type_passes() {
        let t = trybuild::TestCases::new();

        t.pass("tests/ui/pass_basic.rs");
        t.pass("tests/ui/pass_enum.rs");
        t.pass("tests/ui/pass_ext_id.rs");
        t.pass("tests/ui/pass_incremental_snapshot.rs");
        t.pass("tests/ui/pass_nested_deterministic_vec.rs");
        t.pass("tests/ui/pass_nested_deterministic.rs");
        t.pass("tests/ui/pass_nested_carrier.rs");
    }

    #[test]
    fn extension_type_failures() {
        let t = trybuild::TestCases::new();

        t.compile_fail("tests/ui/fail_hashmap.rs");
        t.compile_fail("tests/ui/fail_hashset.rs");
        t.compile_fail("tests/ui/fail_aliased_hashmap.rs");
        t.compile_fail("tests/ui/fail_enum_hashmap.rs");
        t.compile_fail("tests/ui/fail_missing_clone.rs");
        t.compile_fail("tests/ui/fail_ui_type.rs");
        t.compile_fail("tests/ui/fail_manual_impl.rs");
        t.compile_fail("tests/ui/fail_unknown_attr.rs");
        t.compile_fail("tests/ui/fail_vec_undetermined.rs");
        t.compile_fail("tests/ui/fail_nested_hashmap.rs");
        t.compile_fail("tests/ui/fail_nested_indexmap.rs");
    }

    #[test]
    fn plugin_authoring() {
        let t = trybuild::TestCases::new();

        t.pass("tests/ui/pass_plugin.rs");
        t.compile_fail("tests/ui/fail_plugin_missing_name.rs");
        t.compile_fail("tests/ui/fail_plugin_unknown_cap.rs");
    }

    #[test]
    fn offload_task() {
        let t = trybuild::TestCases::new();

        t.pass("tests/ui/pass_offload_task.rs");
        t.compile_fail("tests/ui/fail_offload_task_zero_args.rs");
    }
}
