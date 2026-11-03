//! Regression tests for ANALYSIS.md #2: `operationId`s and parameter names that
//! are Rust keywords (or otherwise awkward) must produce keyword-safe `r#`
//! identifiers, not opaque proc-macro panics. The mere fact that these modules
//! expand and compile proves the codegen handles `type` (a keyword method/field)
//! and `list-pets` (a hyphenated id) across the axum, client, and reqwest paths.

#[openapi_trait::axum("assets/testdata/awkward_idents.openapi.yaml")]
pub mod awk_server {}

#[openapi_trait::client("assets/testdata/awkward_idents.openapi.yaml")]
pub mod awk_client {}

use awk_client::AwkClientClient as _;
use awk_server::AwkServerApi as _;

// ---- axum server side: default impls compile and wire into a router ----

#[derive(Clone)]
struct Server;

impl awk_server::AwkServerApi for Server {
    type Error = awk_server::NotImplemented;
}

#[test]
fn server_router_compiles_with_keyword_operations() {
    let _router = Server.router();
}

// ---- client side: implement the trait, exercising the raw-ident method ----

struct ClientImpl;

impl awk_client::AwkClientClient for ClientImpl {
    type Error = ::std::convert::Infallible;

    async fn r#type(
        &self,
        _req: awk_client::TypeRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<awk_client::TypeResponse, Self::Error> {
        Ok(awk_client::TypeResponse::Status200)
    }

    async fn list_pets(
        &self,
        _req: awk_client::ListPetsRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<awk_client::ListPetsResponse, Self::Error> {
        Ok(awk_client::ListPetsResponse::Status200)
    }

    async fn list_versions(
        &self,
        _req: awk_client::ListVersionsRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<awk_client::ListVersionsResponse, Self::Error> {
        Ok(awk_client::ListVersionsResponse::Status200)
    }
}

#[test]
fn keyword_parameter_becomes_a_raw_struct_field() {
    let req = awk_client::TypeRequest {
        r#type: Some("x".into()),
    };
    assert_eq!(req.r#type.as_deref(), Some("x"));
}

#[tokio::test]
async fn raw_keyword_method_is_callable() {
    let client = ClientImpl;

    let resp = client
        .r#type(awk_client::TypeRequest { r#type: None }, None)
        .await
        .unwrap();
    assert!(matches!(resp, awk_client::TypeResponse::Status200));

    let resp = client
        .list_pets(awk_client::ListPetsRequest {}, None)
        .await
        .unwrap();
    assert!(matches!(resp, awk_client::ListPetsResponse::Status200));
}

// ---- `KNOWN_ISSUES.md` C3: enum values / schema names that aren't identifiers ----

/// Serialize `value`, check the wire form, and deserialize it back.
fn round_trip<T>(value: &T, wire: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(json, format!("\"{wire}\""));
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(format!("{back:?}"), format!("{value:?}"));
}

#[test]
fn non_identifier_enum_values_round_trip() {
    use awk_server::Version;
    let cases = [
        (Version::V1_0, "1.0"),
        (Version::V2_0, "2.0"),
        (Version::V1, "+1"),
        (Version::Empty, ""),
        (Version::VSelf, "Self"),
        (Version::VSelf2, "self"),
        (Version::V404, "404"),
        (Version::Super, "super"),
        (Version::Crate, "crate"),
    ];
    for (value, wire) in &cases {
        round_trip(value, wire);
    }
}

#[test]
fn colliding_enum_values_get_distinct_variants() {
    use awk_server::Sort;
    round_trip(&Sort::CreatedAt, "created_at");
    round_trip(&Sort::CreatedAt2, "createdAt");
    round_trip(&Sort::CreatedAt3, "-created_at");
}

#[test]
fn non_identifier_schema_names_are_repaired() {
    let cfg = awk_server::_2faConfig {
        enabled: Some(true),
        version: Some(awk_server::Version::V2_0),
    };
    let outer = awk_server::_Self {
        second_factor: Some(cfg),
    };
    let json = serde_json::to_value(&outer).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "second_factor": { "enabled": true, "version": "2.0" } })
    );
    let _: awk_server::Type = String::new();
}

#[test]
fn non_identifier_query_enum_values_are_repaired() {
    let _ = awk_client::ListVersionsRequest {
        v: awk_client::ListVersionsVQuery::V1_0,
    };
    let _ = awk_server::ListVersionsVQuery::Empty;
}
