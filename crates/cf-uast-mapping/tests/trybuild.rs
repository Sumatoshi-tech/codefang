//! Compile-pass/fail pinning for the `uast_language!` macro grammar: malformed
//! invocations must fail to compile with a diagnostic pointing at the offending
//! construct, and the canonical forms must keep compiling.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_canonical.rs");
    t.compile_fail("tests/ui/fail_unknown_key.rs");
    t.compile_fail("tests/ui/fail_missing_type.rs");
    t.compile_fail("tests/ui/fail_bad_token_form.rs");
    t.compile_fail("tests/ui/fail_bad_vocab.rs");
}
