//! Integration test for top-level `allOf` codegen (merged struct).
//!
//! `Adult` and `Child` each compose `Person` (via `$ref`) with an inline object
//! that adds one extra field. The generated structs should flatten the `Person`
//! fields onto the wire and add the local field alongside.

#[openapi_trait::axum("assets/testdata/all_of.openapi.yaml")]
pub mod example {}

#[test]
fn adult_merges_person_and_has_children_flag() {
    let adult = example::Adult {
        inner_1: example::Person {
            kind: "a".into(),
            last_name: "Smith".into(),
            first_name: "Jane".into(),
        },
        has_children: true,
    };

    let json = serde_json::to_value(&adult).unwrap();
    // Person fields were flattened onto the top-level object alongside
    // `hasChildren` — no nested wrapper.
    assert_eq!(json["kind"], "a");
    assert_eq!(json["lastName"], "Smith");
    assert_eq!(json["firstName"], "Jane");
    assert_eq!(json["hasChildren"], true);
}

#[test]
fn child_merges_person_and_round_trips() {
    let child = example::Child {
        inner_1: example::Person {
            kind: "c".into(),
            last_name: "Smith".into(),
            first_name: "Tim".into(),
        },
        age: 7,
    };

    let json = serde_json::to_string(&child).unwrap();
    let back: example::Child = serde_json::from_str(&json).unwrap();
    assert_eq!(back.age, 7);
    assert_eq!(back.inner_1.first_name, "Tim");
    assert_eq!(back.inner_1.kind, "c");
}
