//! Codegen warnings are reported as compiler warnings (D4).
//!
//! The spec makes both macros warn. Each warning is a `deprecated` lint, so
//! the `expect` below is fulfilled; if the warnings went missing, the
//! unfulfilled expectation would fail the build.
#![expect(deprecated, reason = "the spec triggers codegen warnings")]
#![deny(unfulfilled_lint_expectations)]

#[openapi_trait::axum("assets/testdata/codegen_warnings.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/codegen_warnings.openapi.yaml")]
pub mod client {}

#[test]
fn the_operation_is_still_generated() {
    let _ = server::list_pets::Response::Status200;
    let _ = client::list_pets::Response::Status200;
}
