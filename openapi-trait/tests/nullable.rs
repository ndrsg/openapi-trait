//! Regression tests for `KNOWN_ISSUES.md` C6: `nullable: true` must turn the
//! generated type into `Option<T>`, even for required fields.

#[openapi_trait::axum("assets/testdata/nullable.openapi.yaml")]
pub mod example {}

#[test]
fn required_nullable_fields_accept_null() {
    let thing: example::Thing = serde_json::from_str(
        r#"{
            "name": null,
            "tags": null,
            "scores": [1, null, 3],
            "labels": {"a": "x", "b": null},
            "color": null,
            "owner": null
        }"#,
    )
    .unwrap();

    assert_eq!(thing.name, None);
    assert!(thing.tags.is_none());
    assert_eq!(thing.scores, vec![Some(1), None, Some(3)]);
    assert_eq!(thing.labels.get("a"), Some(&Some("x".to_string())));
    assert_eq!(thing.labels.get("b"), Some(&None));
    assert!(thing.color.is_none());
    assert!(thing.owner.is_none());
    assert!(thing.nickname.is_none());
}

#[test]
fn required_nullable_fields_accept_values() {
    let thing: example::Thing = serde_json::from_str(
        r#"{
            "name": "n",
            "tags": ["t"],
            "scores": [],
            "labels": {},
            "color": "green",
            "owner": {"id": 7}
        }"#,
    )
    .unwrap();

    assert_eq!(thing.name.as_deref(), Some("n"));
    assert_eq!(thing.tags, Some(vec!["t".to_string()]));
    assert!(matches!(thing.color, Some(example::ThingColor::Green)));
    assert_eq!(thing.owner.unwrap().inner_1.id, 7);
}

#[test]
fn nullable_query_param_stays_plain() {
    // A query string can't carry JSON `null`, so `nullable` doesn't apply.
    let req = example::ListThingsRequest {
        tag: String::from("x"),
    };
    assert_eq!(req.tag, "x");
}
