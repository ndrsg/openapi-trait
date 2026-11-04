//! Regression tests: recursive schemas used to fail with "recursive type has
//! infinite size". The edge that closes a cycle is now a `Box<T>`, which serde
//! treats transparently.

#[openapi_trait::axum("assets/testdata/recursive.openapi.yaml")]
pub mod example {}

use example::{BinOp, Expr, Literal, Node, PersonAddress, A, B};

/// Deserialize `json`, serialize it again, and check the JSON is unchanged.
fn round_trip<T>(json: &str) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let value: T = serde_json::from_str(json).unwrap();
    let back: serde_json::Value = serde_json::to_value(&value).unwrap();
    let expected: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(back, expected);
    value
}

#[test]
fn self_reference_is_boxed() {
    let node: Node = round_trip(
        r#"{"name": "leaf", "parent": {"name": "root"}, "children": [{"name": "kid"}]}"#,
    );
    let parent: Box<Node> = node.parent.unwrap();
    assert_eq!(parent.name, "root");
    assert!(parent.parent.is_none());
    // Behind a `Vec`, so no box.
    let children: Vec<Node> = node.children.unwrap();
    assert_eq!(children[0].name, "kid");
}

#[test]
fn mutual_recursion_is_boxed() {
    let a: A = round_trip(r#"{"b": {"a": {"b": {}}}}"#);
    let b: Box<B> = a.b.unwrap();
    let inner: Box<A> = b.a.unwrap();
    assert!(inner.b.unwrap().a.is_none());
}

#[test]
fn recursion_through_one_of_boxes_the_fields_not_the_variants() {
    let expr: Expr = round_trip(
        r#"{"op": "+", "left": {"value": 1}, "right": {"op": "*", "left": {"value": 2}, "right": {"value": 3}}}"#,
    );
    // The variant holds `BinOp` directly; its fields are boxed.
    let Expr::BinOp(BinOp { op, left, right }) = expr else {
        panic!("expected a BinOp");
    };
    assert_eq!(op, "+");
    let left: Box<Expr> = left;
    assert!(matches!(*left, Expr::Literal(Literal { value: 1 })));
    assert!(matches!(*right, Expr::BinOp(_)));
}

#[test]
fn recursion_through_an_inline_object_is_boxed() {
    let person: example::Person = round_trip(r#"{"address": {"resident": {}}}"#);
    let address: PersonAddress = person.address.unwrap();
    let resident: Box<example::Person> = address.resident.unwrap();
    assert!(resident.address.is_none());
}

#[openapi_trait::client("assets/testdata/recursive.openapi.yaml")]
pub mod client {}

#[test]
fn client_types_are_boxed_too() {
    let node: client::Node = round_trip(r#"{"name": "leaf", "parent": {"name": "root"}}"#);
    let _: Box<client::Node> = node.parent.unwrap();
}

/// Recursive models still derive `Validate`. `serde_valid` can't recurse into
/// `Option<Box<T>>`, so the boxed `parent` isn't checked, but the model's own
/// constraints are.
#[cfg(feature = "validation")]
#[test]
fn recursive_models_still_validate() {
    use openapi_trait::serde_valid::Validate;

    let ok: Node = serde_json::from_str(r#"{"name": "a", "parent": {"name": "b"}}"#).unwrap();
    assert!(ok.validate().is_ok());
    let bad: Node = serde_json::from_str(r#"{"name": ""}"#).unwrap();
    assert!(bad.validate().is_err());
}
