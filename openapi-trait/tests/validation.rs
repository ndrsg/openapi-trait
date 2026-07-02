//! Integration test for the `validation` feature: generated models derive
//! `serde_valid::Validate` and carry `#[validate(...)]` attributes reflecting
//! the `OpenAPI` schema constraints. Run with `--features validation`.
#![cfg(feature = "validation")]

use openapi_trait::serde_valid::Validate as _;

#[openapi_trait::axum("assets/testdata/validation.openapi.yaml")]
pub mod example {}

/// A widget that satisfies every declared constraint.
fn valid_widget() -> example::Widget {
    example::Widget {
        name: "gadget".into(),
        count: 25,
        ratio: 0.5,
        tags: vec!["a".into(), "b".into()],
        inner: example::Inner { code: "abc".into() },
    }
}

#[test]
fn valid_widget_passes() {
    assert!(valid_widget().validate().is_ok());
}

#[test]
fn string_min_length_enforced() {
    let mut w = valid_widget();
    w.name = "a".into(); // below minLength = 2
    assert!(w.validate().is_err());
}

#[test]
fn string_max_length_enforced() {
    let mut w = valid_widget();
    w.name = "abcdefghijk".into(); // above maxLength = 10
    assert!(w.validate().is_err());
}

#[test]
fn string_pattern_enforced() {
    let mut w = valid_widget();
    w.name = "ABC".into(); // violates ^[a-z]+$
    assert!(w.validate().is_err());
}

#[test]
fn integer_bounds_and_multiple_of_enforced() {
    let mut w = valid_widget();
    w.count = 0; // below minimum = 1
    assert!(w.validate().is_err());

    let mut w = valid_widget();
    w.count = 200; // above maximum = 100
    assert!(w.validate().is_err());

    let mut w = valid_widget();
    w.count = 23; // not a multiple of 5
    assert!(w.validate().is_err());
}

#[test]
fn number_exclusive_maximum_enforced() {
    let mut w = valid_widget();
    w.ratio = 1.0; // exclusiveMaximum = 1, so 1.0 is invalid
    assert!(w.validate().is_err());
}

#[test]
fn array_min_items_enforced() {
    let mut w = valid_widget();
    w.tags = vec![]; // below minItems = 1
    assert!(w.validate().is_err());
}

#[test]
fn array_unique_items_enforced() {
    let mut w = valid_widget();
    w.tags = vec!["dup".into(), "dup".into()]; // uniqueItems violated
    assert!(w.validate().is_err());
}

#[test]
fn nested_ref_is_validated_recursively() {
    let mut w = valid_widget();
    w.inner.code = "xy".into(); // Inner.code below minLength = 3
    assert!(
        w.validate().is_err(),
        "top-level validate() should recurse into the referenced Inner model"
    );
}
