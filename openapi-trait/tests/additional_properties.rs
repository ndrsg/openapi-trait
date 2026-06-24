//! Integration test for `additionalProperties` codegen.
//!
//! Objects whose only content is `additionalProperties` become `HashMap` type
//! aliases; objects mixing declared properties with `additionalProperties` get
//! a flattened `HashMap` catch-all field.

use std::collections::HashMap;

#[openapi_trait::axum("assets/testdata/additional_properties.openapi.yaml")]
pub mod example {}

#[test]
fn string_map_is_a_hashmap_of_strings() {
    let mut map: example::StringMap = HashMap::new();
    map.insert("a".to_owned(), "1".to_owned());
    map.insert("b".to_owned(), "2".to_owned());

    let json = serde_json::to_value(&map).unwrap();
    assert_eq!(json["a"], "1");
    assert_eq!(json["b"], "2");
}

#[test]
fn any_map_holds_arbitrary_json() {
    let mut map: example::AnyMap = HashMap::new();
    map.insert("n".to_owned(), serde_json::json!(42));
    map.insert("s".to_owned(), serde_json::json!("hi"));

    let json = serde_json::to_value(&map).unwrap();
    assert_eq!(json["n"], 42);
    assert_eq!(json["s"], "hi");
}

#[test]
fn label_map_values_use_the_referenced_schema() {
    let mut map: example::LabelMap = HashMap::new();
    map.insert(
        "primary".to_owned(),
        example::Label {
            text: "ok".to_owned(),
        },
    );

    let json = serde_json::to_string(&map).unwrap();
    let back: example::LabelMap = serde_json::from_str(&json).unwrap();
    assert_eq!(back["primary"].text, "ok");
}

#[test]
fn config_flattens_extra_properties_alongside_declared_ones() {
    let mut extra = HashMap::new();
    extra.insert("retries".to_owned(), 3);
    extra.insert("timeout".to_owned(), 30);
    let config = example::Config {
        name: "svc".to_owned(),
        additional_properties: extra,
    };

    let json = serde_json::to_value(&config).unwrap();
    // Declared field and the extra entries sit at the same level on the wire.
    assert_eq!(json["name"], "svc");
    assert_eq!(json["retries"], 3);
    assert_eq!(json["timeout"], 30);

    // Round-trip: unknown keys are collected back into the catch-all map.
    let back: example::Config = serde_json::from_value(json).unwrap();
    assert_eq!(back.name, "svc");
    assert_eq!(back.additional_properties["retries"], 3);
    assert!(!back.additional_properties.contains_key("name"));
}

#[test]
fn inline_object_property_is_synthesized_into_a_named_struct() {
    let envelope = example::Envelope {
        payload: example::EnvelopePayload {
            id: "abc".to_owned(),
            note: Some("hi".to_owned()),
        },
    };

    let json = serde_json::to_value(&envelope).unwrap();
    assert_eq!(json["payload"]["id"], "abc");
    assert_eq!(json["payload"]["note"], "hi");

    let back: example::Envelope = serde_json::from_value(json).unwrap();
    assert_eq!(back.payload.id, "abc");
    assert_eq!(back.payload.note.as_deref(), Some("hi"));
}
