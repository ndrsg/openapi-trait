//! Integration tests for the `openapi_trait_axum` macro using the Petstore spec.

#[openapi_trait::axum("assets/testdata/petstore.openapi.yaml")]
pub mod petstore {}

use petstore::PetstoreApi as _;

// ---- Minimal implementation for compilation check ----

#[derive(Clone)]
struct MockPetstore;

impl petstore::PetstoreApi for MockPetstore {
    type Error = ::std::convert::Infallible;
}

#[test]
fn router_compiles() {
    let _router = MockPetstore.router();
}

#[test]
fn get_pet_by_id_request_has_pet_id_field() {
    let req = petstore::GetPetByIdRequest { pet_id: 42 };
    assert_eq!(req.pet_id, 42);
}

#[test]
fn find_pets_by_status_uses_enum_query() {
    let req = petstore::FindPetsByStatusRequest {
        status: petstore::FindPetsByStatusStatusQuery::Available,
    };
    let _ = req;
}

// ---- Server integration tests ----

#[tokio::test]
async fn axum_server_routes_matched_path() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let app = MockPetstore.router().with_state(());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    // Matched route: default impl returns Default → 500 INTERNAL_SERVER_ERROR
    let status = client
        .get(format!("http://{addr}/pet/42"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn axum_server_returns_404_for_unknown_route() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let app = MockPetstore.router().with_state(());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let status = reqwest::get(format!("http://{addr}/no-such-route"))
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

#[derive(Clone)]
struct RichMockPetstore;

impl petstore::PetstoreApi for RichMockPetstore {
    type Error = ::std::convert::Infallible;

    async fn get_pet_by_id(
        &self,
        req: petstore::GetPetByIdRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore::GetPetByIdResponse, Self::Error> {
        if req.pet_id == 42 {
            Ok(petstore::GetPetByIdResponse::Status200(petstore::Pet {
                id: Some(42),
                name: "doggie".into(),
                photo_urls: vec!["https://example.com/photo.jpg".into()],
                category: None,
                tags: None,
                status: None,
            }))
        } else {
            Ok(petstore::GetPetByIdResponse::Status404)
        }
    }
}

#[tokio::test]
async fn axum_server_get_pet_by_id_returns_200_and_404() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let app = RichMockPetstore.router().with_state(());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    // Known pet → 200 with JSON body
    let resp = client
        .get(format!("http://{addr}/pet/42"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["id"], 42);
    assert_eq!(body["name"], "doggie");

    // Unknown pet → 404
    let status = client
        .get(format!("http://{addr}/pet/99"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}
