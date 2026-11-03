//! Regression tests for `KNOWN_ISSUES.md` D6: generated types derive
//! `PartialEq`, so they can be compared in tests. String enums (component
//! schemas and parameter enums) also derive `Eq`, `Hash` and `Copy`.
#![allow(missing_docs)]

use std::collections::HashSet;

#[openapi_trait::axum("assets/testdata/derives.openapi.yaml")]
pub mod m {}

const fn assert_eq_hash_copy<T: Eq + std::hash::Hash + Copy>() {}

#[test]
fn string_enums_are_eq_hash_copy() {
    assert_eq_hash_copy::<m::Color>();
    assert_eq_hash_copy::<m::get_thing::KindPath>();
    assert_eq_hash_copy::<m::get_thing::OrderQuery>();
    assert_eq_hash_copy::<m::get_thing::TagQuery>();

    let set: HashSet<m::Color> = [m::Color::Red, m::Color::Green, m::Color::Red].into();
    assert_eq!(set.len(), 2);
}

#[test]
fn models_are_partial_eq() {
    let thing = m::Thing {
        weight: 1.5,
        color: m::Color::Red,
        shape: Some(m::Shape::Circle(m::Circle { radius: 2.0 })),
        extra: None,
    };
    assert_eq!(thing, thing.clone());

    let other = m::Thing {
        shape: Some(m::Shape::Square(m::Square { side: 2 })),
        ..thing.clone()
    };
    assert_ne!(thing, other);
}
