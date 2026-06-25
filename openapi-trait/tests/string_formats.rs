//! Integration test for specialized string `format` mapping: `date`,
//! `date-time`, and `uuid` get typed `chrono`/`uuid` fields (re-exported through
//! the facade), while `email` and `binary` keep their existing handling.

use openapi_trait::chrono::{DateTime, NaiveDate, Utc};
use openapi_trait::uuid::Uuid;

#[openapi_trait::axum("assets/testdata/string_formats.openapi.yaml")]
pub mod formats {}

use formats::FormatsApi as _;

/// Smoke test: the generated server trait and `router()` compile and wire up
/// around the specialized `chrono`/`uuid` field types.
#[derive(Clone)]
struct MockFormats;

impl formats::FormatsApi for MockFormats {
    type Error = formats::NotImplemented;
}

#[test]
fn router_compiles_with_typed_format_fields() {
    let _router = MockFormats.router();
}

#[test]
fn typed_formats_deserialize_into_specialized_types() {
    let json = r#"{
        "id": "67e55044-10b1-426f-9247-bb680e5fe0c8",
        "createdAt": "2024-01-02T03:04:05Z",
        "birthDate": "1990-05-06",
        "email": "alice@example.com",
        "payload": [1, 2, 3]
    }"#;

    let event: formats::Event = serde_json::from_str(json).expect("deserializes");

    // The typed fields are real chrono/uuid values, not strings.
    let id: Uuid = event.id;
    assert_eq!(
        id,
        Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap()
    );

    let created_at: DateTime<Utc> = event.created_at;
    assert_eq!(created_at.timestamp(), 1_704_164_645);

    let birth_date: NaiveDate = event.birth_date;
    assert_eq!(birth_date, NaiveDate::from_ymd_opt(1990, 5, 6).unwrap());

    // email stays String; binary stays Vec<u8>.
    let email: Option<String> = event.email;
    assert_eq!(email.as_deref(), Some("alice@example.com"));
    let payload: Option<Vec<u8>> = event.payload;
    assert_eq!(payload, Some(vec![1, 2, 3]));
}

#[test]
fn typed_formats_round_trip_through_serde() {
    let json = r#"{"id":"67e55044-10b1-426f-9247-bb680e5fe0c8","createdAt":"2024-01-02T03:04:05Z","birthDate":"1990-05-06","email":"alice@example.com","payload":[1,2,3]}"#;

    let event: formats::Event = serde_json::from_str(json).expect("deserializes");
    let back = serde_json::to_string(&event).expect("serializes");

    assert_eq!(back, json);
}
