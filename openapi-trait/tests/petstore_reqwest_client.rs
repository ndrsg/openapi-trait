//! Integration tests for the feature-gated reqwest client generator.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::client("assets/testdata/petstore.openapi.yaml")]
pub mod petstore {}

#[openapi_trait::axum("assets/testdata/petstore.openapi.yaml")]
mod petstore_server {}

use petstore::PetstoreClient as _;
use petstore_server::PetstoreServerApi as _;

#[derive(Clone)]
struct RichMockPetstore;

#[derive(Clone, openapi_trait::ReqwestClient)]
struct DerivedPetstoreClient {
    #[openapi_trait(client)]
    http: ::reqwest::Client,
    #[openapi_trait(base_url)]
    endpoint: String,
    #[openapi_trait(auth)]
    creds: petstore::PetstoreAuthState,
}

impl petstore_server::PetstoreServerApi for RichMockPetstore {
    type Error = petstore_server::NotImplemented;

    async fn get_pet_by_id(
        &self,
        req: petstore_server::GetPetByIdRequest,
        _auth: petstore_server::ApiKey,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore_server::GetPetByIdResponse, Self::Error> {
        if req.pet_id == 42 {
            Ok(petstore_server::GetPetByIdResponse::Status200(
                petstore_server::Pet {
                    id: Some(42),
                    name: "doggie".into(),
                    photo_urls: vec!["https://example.com/photo.jpg".into()],
                    category: None,
                    tags: None,
                    status: Some("available".into()),
                },
            ))
        } else {
            Ok(petstore_server::GetPetByIdResponse::Status404)
        }
    }

    async fn find_pets_by_status(
        &self,
        req: petstore_server::FindPetsByStatusRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore_server::FindPetsByStatusResponse, Self::Error> {
        match req.status {
            petstore_server::FindPetsByStatusStatusQuery::Available => {
                Ok(petstore_server::FindPetsByStatusResponse::Status200(vec![
                    petstore_server::Pet {
                        id: Some(7),
                        name: "available".into(),
                        photo_urls: vec![],
                        category: None,
                        tags: None,
                        status: Some("available".into()),
                    },
                ]))
            }
            _ => Ok(petstore_server::FindPetsByStatusResponse::Status400),
        }
    }

    async fn add_pet(
        &self,
        req: petstore_server::AddPetRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore_server::AddPetResponse, Self::Error> {
        Ok(petstore_server::AddPetResponse::Status200(req.body))
    }
}

async fn spawn_server() -> std::string::String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, RichMockPetstore.router().with_state(()))
            .await
            .unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn generated_reqwest_client_handles_path_params() {
    let base_url = spawn_server().await;
    let client = {
        use petstore::PetstoreClientAuth as _;
        DerivedPetstoreClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base_url,
            creds: petstore::PetstoreAuthState::default(),
        }
        .with_api_key("k")
    };

    let response = client
        .get_pet_by_id(petstore::GetPetByIdRequest { pet_id: 42 }, None)
        .await
        .unwrap();

    match response {
        petstore::GetPetByIdResponse::Status200(pet) => assert_eq!(pet.id, Some(42)),
        _ => panic!("expected 200 response"),
    }
}

#[tokio::test]
async fn generated_reqwest_client_handles_query_params() {
    let base_url = spawn_server().await;
    let client = {
        use petstore::PetstoreClientAuth as _;
        DerivedPetstoreClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base_url,
            creds: petstore::PetstoreAuthState::default(),
        }
        .with_api_key("k")
    };

    let response = client
        .find_pets_by_status(
            petstore::FindPetsByStatusRequest {
                status: petstore::FindPetsByStatusStatusQuery::Available,
            },
            None,
        )
        .await
        .unwrap();

    match response {
        petstore::FindPetsByStatusResponse::Status200(pets) => assert_eq!(pets.len(), 1),
        _ => panic!("expected 200 response"),
    }
}

#[tokio::test]
async fn generated_reqwest_client_handles_json_bodies() {
    let base_url = spawn_server().await;
    let client = {
        use petstore::PetstoreClientAuth as _;
        DerivedPetstoreClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base_url,
            creds: petstore::PetstoreAuthState::default(),
        }
        .with_api_key("k")
    };

    let response = client
        .add_pet(
            petstore::AddPetRequest {
                body: petstore::Pet {
                    id: Some(8),
                    name: "from-client".into(),
                    photo_urls: vec![],
                    category: None,
                    tags: None,
                    status: Some("pending".into()),
                },
            },
            None,
        )
        .await
        .unwrap();

    match response {
        petstore::AddPetResponse::Status200(pet) => assert_eq!(pet.name, "from-client"),
        _ => panic!("expected 200 response"),
    }
}

/// Mock server that records the headers received on `get_pet_by_id`, so tests
/// can assert that per-request [`openapi_trait::RequestOptions`] reach the wire.
#[derive(Clone, Default)]
struct RecordingPetstore {
    received: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

impl petstore_server::PetstoreServerApi for RecordingPetstore {
    type Error = petstore_server::NotImplemented;

    async fn get_pet_by_id(
        &self,
        req: petstore_server::GetPetByIdRequest,
        _auth: petstore_server::ApiKey,
        _state: axum::extract::State<()>,
        headers: axum::http::HeaderMap,
    ) -> Result<petstore_server::GetPetByIdResponse, Self::Error> {
        {
            let mut received = self.received.lock().unwrap();
            for (name, value) in &headers {
                received.push((
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                ));
            }
        }
        Ok(petstore_server::GetPetByIdResponse::Status200(
            petstore_server::Pet {
                id: Some(req.pet_id),
                name: "doggie".into(),
                photo_urls: vec![],
                category: None,
                tags: None,
                status: None,
            },
        ))
    }

    async fn find_pets_by_status(
        &self,
        _req: petstore_server::FindPetsByStatusRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore_server::FindPetsByStatusResponse, Self::Error> {
        Ok(petstore_server::FindPetsByStatusResponse::Status400)
    }

    async fn add_pet(
        &self,
        req: petstore_server::AddPetRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore_server::AddPetResponse, Self::Error> {
        Ok(petstore_server::AddPetResponse::Status200(req.body))
    }
}

#[tokio::test]
async fn generated_reqwest_client_applies_per_request_headers_and_auth() {
    let recorder = RecordingPetstore::default();
    let received = recorder.received.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, recorder.router().with_state(()))
            .await
            .unwrap();
    });

    let client = {
        use petstore::PetstoreClientAuth as _;
        DerivedPetstoreClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base_url,
            creds: petstore::PetstoreAuthState::default(),
        }
        .with_api_key("k")
    };

    client
        .get_pet_by_id(
            petstore::GetPetByIdRequest { pet_id: 42 },
            Some(
                openapi_trait::RequestOptions::new()
                    .bearer_auth("token-123")
                    .header("x-request-id", "abc-789"),
            ),
        )
        .await
        .unwrap();

    let received = received.lock().unwrap().clone();
    assert!(
        received
            .iter()
            .any(|(name, value)| name == "authorization" && value == "Bearer token-123"),
        "expected bearer auth header, got {received:?}"
    );
    assert!(
        received
            .iter()
            .any(|(name, value)| name == "x-request-id" && value == "abc-789"),
        "expected custom header, got {received:?}"
    );
}
