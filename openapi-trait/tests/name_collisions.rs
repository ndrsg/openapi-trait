//! Regression tests for `KNOWN_ISSUES.md` C5: a type synthesized for an inline
//! schema used to reuse a name that a component schema or an operation type
//! already had ("the name `PersonAddress` is defined multiple times"). It now
//! gets a numeric suffix instead.

#[openapi_trait::axum("assets/testdata/name_collisions.openapi.yaml")]
pub mod example {}

#[openapi_trait::client("assets/testdata/name_collisions.openapi.yaml")]
pub mod client {}

use example::{
    Bag, BagAdditionalProperties, BagAdditionalProperties2, Colors, Colors2, CreateThing,
    CreateThingKindPath, CreateThingKindPath2, CreateThingRequest, CreateThingRequest2, Person,
    PersonAddress, PersonAddress2,
};

#[test]
fn inline_property_types_get_a_suffix() {
    // The component keeps its name and fields.
    let component = PersonAddress {
        x: Some("x".into()),
    };
    // The inline type gets the suffix and keeps its own fields.
    let person = Person {
        address: Some(PersonAddress2 {
            y: Some("y".into()),
        }),
    };
    assert_eq!(
        serde_json::to_value(&person).unwrap(),
        serde_json::json!({"address": {"y": "y"}})
    );
    assert_eq!(
        serde_json::to_value(&component).unwrap(),
        serde_json::json!({"x": "x"})
    );
}

#[test]
fn inline_types_avoid_operation_type_names() {
    let body = CreateThing {
        request: Some(CreateThingRequest2 {
            z: Some("z".into()),
        }),
    };
    // The operation's request struct is untouched.
    let request = CreateThingRequest {
        kind: CreateThingKindPath2::B,
        body,
    };
    assert_eq!(request.kind.to_string(), "b");
    let component: CreateThingKindPath = serde_json::from_str(r#"{"w": "w"}"#).unwrap();
    assert_eq!(component.w.as_deref(), Some("w"));
}

#[test]
fn additional_properties_types_get_a_suffix() {
    let bag: Bag = serde_json::from_str(r#"{"label": "l", "extra": {"count": 2}}"#).unwrap();
    let extra: &BagAdditionalProperties2 = &bag.additional_properties["extra"];
    assert_eq!(extra.count, Some(2));
    let component: BagAdditionalProperties =
        serde_json::from_str(r#"{"unrelated": true}"#).unwrap();
    assert_eq!(component.unrelated, Some(true));
}

#[test]
fn array_alias_item_enum_gets_a_suffix() {
    let colors: Colors = serde_json::from_str(r#"["red", "green"]"#).unwrap();
    assert!(matches!(colors[..], [Colors2::Red, Colors2::Green]));
}

#[test]
fn client_uses_the_same_names() {
    let _ = client::Person {
        address: Some(client::PersonAddress2 { y: None }),
    };
    let _ = client::CreateThingKindPath2::A;
}
