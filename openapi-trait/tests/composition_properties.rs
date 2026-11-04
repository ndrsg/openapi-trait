//! A schema's own `properties` next to `oneOf` / `allOf` / `anyOf` are kept.

use serde_json::json;

#[openapi_trait::axum("assets/testdata/composition_properties.openapi.yaml")]
pub mod m {}

/// Deserialize `value` as `T`, check it equals `expected`, and check it
/// serializes back to `value`.
fn round_trip<T>(value: &serde_json::Value, expected: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let parsed: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(&parsed, expected);
    assert_eq!(&serde_json::to_value(&parsed).unwrap(), value);
}

fn cat() -> m::Cat {
    m::Cat {
        meow: "purr".into(),
    }
}

fn dog() -> m::Dog {
    m::Dog {
        bark: "woof".into(),
    }
}

#[test]
fn one_of_keeps_own_properties() {
    round_trip(
        &json!({"id": 1, "meow": "purr"}),
        &m::PetOneOf {
            id: 1,
            one_of: m::PetOneOfOneOf::Cat(cat()),
        },
    );
    round_trip(
        &json!({"id": 2, "bark": "woof"}),
        &m::PetOneOf {
            id: 2,
            one_of: m::PetOneOfOneOf::Dog(dog()),
        },
    );
}

#[test]
fn any_of_keeps_own_properties() {
    round_trip(
        &json!({"id": 1, "bark": "woof"}),
        &m::PetAnyOf {
            id: 1,
            any_of: m::PetAnyOfAnyOf::Dog(dog()),
        },
    );
}

#[test]
fn all_of_keeps_own_properties() {
    round_trip(
        &json!({"id": 1, "meow": "purr"}),
        &m::PetAllOf {
            id: 1,
            inner_1: cat(),
        },
    );
}

#[test]
fn own_properties_are_required_as_declared() {
    assert!(serde_json::from_value::<m::PetOneOf>(json!({"meow": "purr"})).is_err());
}

#[test]
fn discriminator_tag_stays_with_the_enum() {
    round_trip(
        &json!({"kind": "dog", "id": 7, "bark": "woof"}),
        &m::Animal {
            id: 7,
            one_of: m::AnimalOneOf::Dog(dog()),
        },
    );
    assert!(serde_json::from_value::<m::Animal>(json!({"kind": "cow", "id": 7})).is_err());
}

#[test]
fn several_composition_keywords_are_combined() {
    round_trip(
        &json!({"name": "Rex", "bark": "woof"}),
        &m::NamedPet {
            inner_1: m::Named { name: "Rex".into() },
            one_of: m::NamedPetOneOf::Dog(dog()),
        },
    );
}

#[test]
fn all_of_collects_additional_properties() {
    let extensible = m::Extensible {
        id: 1,
        inner_1: m::Named { name: "x".into() },
        additional_properties: [("color".to_owned(), "red".to_owned())].into(),
    };
    round_trip(&json!({"id": 1, "name": "x", "color": "red"}), &extensible);
}

#[test]
fn one_of_next_to_additional_properties_is_untyped() {
    let value = json!({"id": 1, "meow": "purr", "extra": true});
    let loose: m::Loose = value.clone();
    round_trip(&value, &loose);
}

#[test]
fn recursive_own_property_is_boxed() {
    let chain = m::Chain {
        next: Some(Box::new(m::Chain {
            next: None,
            one_of: m::ChainOneOf::Dog(dog()),
        })),
        one_of: m::ChainOneOf::Cat(cat()),
    };
    round_trip(&json!({"next": {"bark": "woof"}, "meow": "purr"}), &chain);
}

#[test]
fn inline_composition_keeps_own_properties() {
    round_trip(
        &json!({"pet": {"id": 3, "meow": "purr"}}),
        &m::Owner {
            pet: m::OwnerPet {
                id: 3,
                one_of: m::OwnerPetOneOf::Cat(cat()),
            },
        },
    );
}
