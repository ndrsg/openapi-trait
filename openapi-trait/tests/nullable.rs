//! Regression tests for `KNOWN_ISSUES.md` C6 and C7: `nullable: true` must turn
//! the generated type into `Option<T>`, even for required fields, and optional
//! fields that are `None` must be left out instead of sent as `null`.

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

fn empty_thing() -> example::Thing {
    serde_json::from_str(
        r#"{"name": null, "tags": null, "scores": [], "labels": {}, "color": null, "owner": null}"#,
    )
    .unwrap()
}

#[test]
fn optional_none_fields_are_left_out() {
    let json = serde_json::to_value(empty_thing()).unwrap();
    let obj = json.as_object().unwrap();
    assert!(!obj.contains_key("nickname"));
    assert!(!obj.contains_key("note"));
}

#[test]
fn required_nullable_none_fields_are_sent_as_null() {
    let json = serde_json::to_value(empty_thing()).unwrap();
    assert_eq!(json["name"], serde_json::Value::Null);
    assert!(json.as_object().unwrap().contains_key("name"));
    assert!(json.as_object().unwrap().contains_key("owner"));
}

#[test]
fn optional_some_fields_are_sent() {
    let mut thing = empty_thing();
    thing.note = Some("hi".into());
    let json = serde_json::to_value(thing).unwrap();
    assert_eq!(json["note"], "hi");
}

#[test]
fn optional_fields_merged_by_all_of_are_left_out() {
    let tagged = example::TaggedOwner {
        inner_1: example::Owner { id: 1 },
        tag: None,
    };
    assert_eq!(serde_json::to_string(&tagged).unwrap(), r#"{"id":1}"#);
    let back: example::TaggedOwner = serde_json::from_str(r#"{"id":1}"#).unwrap();
    assert!(back.tag.is_none());
}
