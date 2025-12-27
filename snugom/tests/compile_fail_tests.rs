//! Compile-fail tests for invalid SnugomEntity search attribute combinations.
//!
//! These tests verify that the derive macro produces clear error messages for:
//! - Entry 66: `sortable` on String without type specification
//! - Entry 67: `searchable` on numeric types
//! - Entry 68: `searchable` on enum types
//! - Entry 69: `filterable(text)` on numeric types
//! - Entry 70: `filterable(geo)` on numeric types

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
