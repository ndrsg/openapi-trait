//! Integration test for top-level `anyOf` codegen (untagged enum).

#[openapi_trait::axum("assets/testdata/any_of.openapi.yaml")]
pub mod search {}

#[test]
fn search_term_accepts_string_and_integer() {
    let s: search::SearchTerm = serde_json::from_str(r#""hello""#).unwrap();
    let n: search::SearchTerm = serde_json::from_str("42").unwrap();

    // Re-serialize and confirm the wire format is unchanged in both directions.
    assert_eq!(serde_json::to_string(&s).unwrap(), r#""hello""#);
    assert_eq!(serde_json::to_string(&n).unwrap(), "42");
}
