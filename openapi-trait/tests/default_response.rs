//! Regression tests for `KNOWN_ISSUES.md` D1: the `default` response used to
//! be `Default(String)` and was always sent as 500. It is now
//! `Default { status, body, <headers> }`, typed like a numbered response. The
//! server sends `status`; the client fills it from any undeclared status.
#![cfg(feature = "reqwest-client")]

use openapi_trait::http::StatusCode;

#[openapi_trait::axum("assets/testdata/default_response.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/default_response.openapi.yaml")]
pub mod remote {}

use remote::RemoteClient as _;
use server::ServerApi as _;

#[derive(Clone)]
struct Server;

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn get_thing(
        &self,
        ctx: server::get_thing::Context,
    ) -> Result<server::get_thing::Response, Self::Error> {
        Ok(match ctx.req.id {
            1 => server::get_thing::Response::Status200("one".to_owned()),
            id => server::get_thing::Response::Default {
                status: if id == 2 {
                    StatusCode::TOO_MANY_REQUESTS
                } else {
                    StatusCode::NOT_FOUND
                },
                body: server::Error {
                    code: id,
                    message: "no such thing".to_owned(),
                },
                x_trace_id: format!("trace-{id}"),
                retry_after: (id == 2).then_some(30),
            },
        })
    }

    async fn delete_thing(
        &self,
        ctx: server::delete_thing::Context,
    ) -> Result<server::delete_thing::Response, Self::Error> {
        Ok(if ctx.req.id == 1 {
            server::delete_thing::Response::Status204
        } else {
            server::delete_thing::Response::Default {
                status: StatusCode::CONFLICT,
                body: server::delete_thing::DefaultBody {
                    reason: "in use".to_owned(),
                },
            }
        })
    }

    async fn ping(
        &self,
        _ctx: server::ping::Context,
    ) -> Result<server::ping::Response, Self::Error> {
        Ok(server::ping::Response::Default {
            status: StatusCode::ACCEPTED,
        })
    }
}

#[derive(Clone, openapi_trait::ReqwestClient)]
struct Client {
    #[openapi_trait(client)]
    http: reqwest::Client,
    #[openapi_trait(base_url)]
    endpoint: String,
}

async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Server.router()).await.unwrap();
    });
    format!("http://{addr}")
}

fn client(endpoint: String) -> Client {
    Client {
        http: reqwest::Client::new(),
        endpoint,
    }
}

#[tokio::test]
async fn server_sends_the_default_status_body_and_headers() {
    let base_url = spawn_server().await;
    let response = reqwest::get(format!("{base_url}/things/2")).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["x-trace-id"], "trace-2");
    assert_eq!(response.headers()["retry-after"], "30");
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({ "code": 2, "message": "no such thing" })
    );
}

#[tokio::test]
async fn client_decodes_the_default_response() {
    let client = client(spawn_server().await);
    let response = client
        .get_thing(remote::get_thing::Request { id: 3 }, None)
        .await
        .unwrap();
    let remote::get_thing::Response::Default {
        status,
        body,
        x_trace_id,
        retry_after,
    } = response
    else {
        panic!("expected the default response, got {response:?}");
    };
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        remote::Error {
            code: 3,
            message: "no such thing".to_owned()
        }
    );
    assert_eq!(x_trace_id, "trace-3");
    assert_eq!(retry_after, None);

    // A declared status still maps to its own variant.
    let response = client
        .get_thing(remote::get_thing::Request { id: 1 }, None)
        .await
        .unwrap();
    assert!(matches!(response, remote::get_thing::Response::Status200(ref s) if s == "one"));
}

#[tokio::test]
async fn inline_default_body_and_bodyless_default() {
    let client = client(spawn_server().await);
    let response = client
        .delete_thing(remote::delete_thing::Request { id: 2 }, None)
        .await
        .unwrap();
    let remote::delete_thing::Response::Default { status, body } = response else {
        panic!("expected the default response, got {response:?}");
    };
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body.reason, "in use");

    let response = client.ping(remote::ping::Request {}, None).await.unwrap();
    assert!(matches!(
        response,
        remote::ping::Response::Default {
            status: StatusCode::ACCEPTED
        }
    ));
}
