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
