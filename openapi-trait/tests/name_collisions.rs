//! Regression tests for `KNOWN_ISSUES.md` C5 and the `{Op}Request` clash:
//! generated types used to share one namespace, so a `CreateThingRequest`
//! schema next to operation `createThing`, or an inline `Person.address` next
//! to a `PersonAddress` schema, defined a name twice.
//!
//! Operation types now live in a module per operation, and a type synthesized
//! for an inline schema gets a numeric suffix when its name is taken.

#[openapi_trait::axum("assets/testdata/name_collisions.openapi.yaml")]
pub mod example {}

#[openapi_trait::client("assets/testdata/name_collisions.openapi.yaml")]
pub mod client {}

use example::{
    create_thing, Bag, BagAdditionalProperties, BagAdditionalProperties2, Colors, Colors2,
    CreateThingKindPath, CreateThingRequest, Person, PersonAddress, PersonAddress2,
};

#[test]
fn schemas_can_share_names_with_operation_types() {
    // The operation's request struct holds the schema of the same name.
    let request = create_thing::Request {
        kind: create_thing::KindPath::B,
        body: CreateThingRequest { name: "n".into() },
    };
    assert_eq!(request.kind.to_string(), "b");
    let _: CreateThingKindPath = serde_json::from_str(r#"{"w": "w"}"#).unwrap();

    // Inside the module, `Response` and `Request` schemas aren't shadowed by
    // the operation's own `Response` and `Request`.
    let ok = create_thing::Response::Status200(example::Response { id: Some(1) });
    let rejected = create_thing::Response::Status400(example::Request {
        reason: Some("no".into()),
    });
    assert!(matches!(ok, create_thing::Response::Status200(_)));
    assert!(matches!(rejected, create_thing::Response::Status400(_)));
}

#[test]
fn inline_property_types_get_a_suffix() {
    let component = PersonAddress {
        x: Some("x".into()),
    };
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
    let _ = client::create_thing::Request {
        kind: client::create_thing::KindPath::A,
        body: client::CreateThingRequest { name: "n".into() },
    };
    let _ = client::Person {
        address: Some(client::PersonAddress2 { y: None }),
    };
}
