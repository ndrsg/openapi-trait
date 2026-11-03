//! Regression tests for `KNOWN_ISSUES.md` D2: inline request and response body
//! schemas used to become `serde_json::Value`. They now get generated types
//! named `{Op}Body` and `{Op}Status{code}Body`.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/inline_bodies.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/inline_bodies.openapi.yaml")]
pub mod remote {}

use remote::RemoteClient as _;
use server::ServerApi as _;

#[derive(Clone)]
struct Server;

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn create_thing(
        &self,
        req: server::CreateThingRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<server::CreateThingResponse, Self::Error> {
        let server::CreateThingBody { name, size } = req.body;
        if name.is_empty() {
            return Ok(server::CreateThingResponse::Status400(
                server::CreateThingStatus400Body::Problem(server::Problem {
                    title: "empty name".into(),
                }),
            ));
        }
        let id = match size {
            Some(server::CreateThingBodySize::Large) => 2,
            _ => 1,
        };
        Ok(server::CreateThingResponse::Status201(
            server::CreateThingStatus201Body { id, name },
        ))
    }

    async fn list_things(
        &self,
        _req: server::ListThingsRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<server::ListThingsResponse, Self::Error> {
        Ok(server::ListThingsResponse::Status200(vec![
            server::ListThingsStatus200Body { id: Some(7) },
        ]))
    }

    async fn get_status(
        &self,
        _req: server::GetStatusRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<server::GetStatusResponse, Self::Error> {
        Ok(server::GetStatusResponse::Status200(
            server::GetStatusStatus200Body2::Down,
        ))
    }
}

#[derive(Clone, openapi_trait::ReqwestClient)]
struct Client {
    #[openapi_trait(client)]
    http: reqwest::Client,
    #[openapi_trait(base_url)]
    endpoint: String,
}

async fn client() -> Client {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Server.router().with_state(()))
            .await
            .unwrap();
    });
    Client {
        http: reqwest::Client::new(),
        endpoint: format!("http://{addr}"),
    }
}

#[tokio::test]
async fn inline_request_and_response_objects_are_typed() {
    let client = client().await;
    let body = remote::CreateThingBody {
        name: "box".into(),
        size: Some(remote::CreateThingBodySize::Large),
    };
    let response = client
        .create_thing(remote::CreateThingRequest { body }, None)
        .await
        .unwrap();
    let remote::CreateThingResponse::Status201(created) = response else {
        panic!("expected 201, got {response:?}");
    };
    assert_eq!(created.id, 2);
    assert_eq!(created.name, "box");
}

#[tokio::test]
async fn inline_composition_response_is_typed() {
    let client = client().await;
    let body = remote::CreateThingBody {
        name: String::new(),
        size: None,
    };
    let response = client
        .create_thing(remote::CreateThingRequest { body }, None)
        .await
        .unwrap();
    let remote::CreateThingResponse::Status400(remote::CreateThingStatus400Body::Problem(p)) =
        response
    else {
        panic!("expected a 400 problem, got {response:?}");
    };
    assert_eq!(p.title, "empty name");
}

#[tokio::test]
async fn inline_array_items_and_enums_are_typed() {
    let client = client().await;
    let remote::ListThingsResponse::Status200(things) = client
        .list_things(remote::ListThingsRequest {}, None)
        .await
        .unwrap();
    assert_eq!(things[0].id, Some(7));

    // `GetStatusStatus200Body` is a component schema, so the inline enum
    // takes the next free name.
    let remote::GetStatusResponse::Status200(status) = client
        .get_status(remote::GetStatusRequest {}, None)
        .await
        .unwrap();
    assert!(matches!(status, remote::GetStatusStatus200Body2::Down));
    let _: remote::GetStatusStatus200Body = serde_json::from_str("{}").unwrap();
}
