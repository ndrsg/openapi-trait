//! Integration test for top-level `oneOf` codegen (untagged, no discriminator).

#[openapi_trait::axum("assets/testdata/one_of.openapi.yaml")]
pub mod fruity {}

#[test]
fn fruit_variants_serialize_to_their_payload() {
    // The fixture's three branches all have only optional fields, so
    // deserialization is intrinsically ambiguous under serde's untagged rules
    // (it always picks the first matching variant). We assert one-way
    // serialization here; the round-trip path is exercised by the anyOf test
    // where branches are non-overlapping.
    assert_eq!(
        serde_json::to_string(&fruity::Fruit::Apple(fruity::Apple {
            kind: Some("granny smith".into()),
        }))
        .unwrap(),
        r#"{"kind":"granny smith"}"#,
    );
    assert_eq!(
        serde_json::to_string(&fruity::Fruit::Banana(fruity::Banana { count: Some(3) })).unwrap(),
        r#"{"count":3}"#,
    );
    assert_eq!(
        serde_json::to_string(&fruity::Fruit::Orange(fruity::Orange { sweet: Some(true) }))
            .unwrap(),
        r#"{"sweet":true}"#,
    );
}

#[test]
fn fruit_is_untagged_serialization() {
    let apple = fruity::Fruit::Apple(fruity::Apple {
        kind: Some("fuji".into()),
    });
    let json = serde_json::to_string(&apple).unwrap();
    // Untagged: payload should be the inner object directly, no wrapper key.
    assert_eq!(json, r#"{"kind":"fuji"}"#);
}
