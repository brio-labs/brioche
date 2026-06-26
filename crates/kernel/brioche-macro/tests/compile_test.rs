//! Compile tests for `brioche-macro` procedural macros.
//!
//! Uses `trybuild` to verify that `BriocheExtensionType` derive rejects
//! invalid input (HashMap, HashSet, missing Clone) and accepts valid input,
//! and that the plugin/offload-task attribute macros accept valid input and
//! reject malformed input.
//!
//! Refs: I-Core-ExtensionType

#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();

    // `BriocheExtensionType` positive cases.
    t.pass("tests/ui/pass_basic.rs");
    t.pass("tests/ui/pass_enum.rs");
    t.pass("tests/ui/pass_ext_id.rs");
    t.pass("tests/ui/pass_incremental_snapshot.rs");
    t.pass("tests/ui/pass_nested_deterministic_vec.rs");

    // Plugin authoring positive/negative cases.
    t.pass("tests/ui/pass_plugin.rs");
    t.compile_fail("tests/ui/fail_plugin_missing_name.rs");
    t.compile_fail("tests/ui/fail_plugin_unknown_cap.rs");

    // CPU-offload positive/negative cases.
    t.pass("tests/ui/pass_offload_task.rs");
    t.compile_fail("tests/ui/fail_offload_task_zero_args.rs");

    // `BriocheExtensionType` negative cases.
    t.compile_fail("tests/ui/fail_hashmap.rs");
    t.compile_fail("tests/ui/fail_hashset.rs");
    t.compile_fail("tests/ui/fail_aliased_hashmap.rs");
    t.compile_fail("tests/ui/fail_enum_hashmap.rs");
    t.compile_fail("tests/ui/fail_missing_clone.rs");
    t.compile_fail("tests/ui/fail_ui_type.rs");
    t.compile_fail("tests/ui/fail_manual_impl.rs");
    t.compile_fail("tests/ui/fail_unknown_attr.rs");
    t.compile_fail("tests/ui/fail_vec_undetermined.rs");
}
