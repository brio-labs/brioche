#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_basic.rs");
    t.compile_fail("tests/ui/fail_hashmap.rs");
    t.compile_fail("tests/ui/fail_hashset.rs");
    t.compile_fail("tests/ui/fail_missing_clone.rs");
    t.compile_fail("tests/ui/fail_ui_type.rs");
    t.compile_fail("tests/ui/fail_manual_impl.rs");
    t.compile_fail("tests/ui/fail_unknown_attr.rs");
    t.compile_fail("tests/ui/fail_vec_undetermined.rs");
}
