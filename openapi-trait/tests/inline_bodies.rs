//! Regression tests for `KNOWN_ISSUES.md` D2: inline request and response body
//! schemas used to become `serde_json::Value`. They now get generated types
//! named `Body` and `Status{code}Body` in the operation's module.
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
        ctx: server::create_thing::Context,
    ) -> Result<server::create_thing::Response, Self::Error> {
        let server::create_thing::Context { req, .. } = ctx;
        let server::create_thing::Body { name, size } = req.body;
        if name.is_empty() {
            return Ok(server::create_thing::Response::Status400(
                server::create_thing::Status400Body::Problem(server::Problem {
                    title: "empty name".into(),
                }),
            ));
        }
        let id = match size {
            Some(server::create_thing::BodySize::Large) => 2,
            _ => 1,
        };
        Ok(server::create_thing::Response::Status201(
            server::create_thing::Status201Body { id, name },
        ))
    }

    async fn list_things(
        &self,
        _ctx: server::list_things::Context,
    ) -> Result<server::list_things::Response, Self::Error> {
        Ok(server::list_things::Response::Status200(vec![
            server::list_things::Status200Body { id: Some(7) },
        ]))
    }

    async fn get_status(
        &self,
        _ctx: server::get_status::Context,
    ) -> Result<server::get_status::Response, Self::Error> {
        Ok(server::get_status::Response::Status200(
            server::get_status::Status200Body::Down,
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
        axum::serve(listener, Server.router()).await.unwrap();
    });
    Client {
        http: reqwest::Client::new(),
        endpoint: format!("http://{addr}"),
    }
}

#[tokio::test]
async fn inline_request_and_response_objects_are_typed() {
    let client = client().await;
    let body = remote::create_thing::Body {
        name: "box".into(),
        size: Some(remote::create_thing::BodySize::Large),
    };
    let response = client
        .create_thing(remote::create_thing::Request { body }, None)
        .await
        .unwrap();
    let remote::create_thing::Response::Status201(created) = response else {
        panic!("expected 201, got {response:?}");
    };
    assert_eq!(created.id, 2);
    assert_eq!(created.name, "box");
}

#[tokio::test]
async fn inline_composition_response_is_typed() {
    let client = client().await;
    let body = remote::create_thing::Body {
        name: String::new(),
        size: None,
    };
    let response = client
        .create_thing(remote::create_thing::Request { body }, None)
        .await
        .unwrap();
    let remote::create_thing::Response::Status400(remote::create_thing::Status400Body::Problem(p)) =
        response
    else {
        panic!("expected a 400 problem, got {response:?}");
    };
    assert_eq!(p.title, "empty name");
}

#[tokio::test]
async fn inline_array_items_and_enums_are_typed() {
    let client = client().await;
    let remote::list_things::Response::Status200(things) = client
        .list_things(remote::list_things::Request {}, None)
        .await
        .unwrap();
    assert_eq!(things[0].id, Some(7));

    // The inline enum lives in the operation's module, so it doesn't clash
    // with the `GetStatusStatus200Body` component schema.
    let remote::get_status::Response::Status200(status) = client
        .get_status(remote::get_status::Request {}, None)
        .await
        .unwrap();
    assert!(matches!(status, remote::get_status::Status200Body::Down));
    let _: remote::GetStatusStatus200Body = serde_json::from_str("{}").unwrap();
}
