//! Regression tests: request and response bodies with a JSON media type other
//! than `application/json` (`+json` suffixes, `charset` parameters) are typed,
//! and sent with their declared `Content-Type`.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/json_media_types.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/json_media_types.openapi.yaml")]
pub mod remote {}

use remote::RemoteClient as _;
use server::ServerApi as _;

#[derive(Clone)]
struct Server;

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn patch_thing(
        &self,
        ctx: server::patch_thing::Context,
    ) -> Result<server::patch_thing::Response, Self::Error> {
        let server::patch_thing::Context { req, headers, .. } = ctx;
        if req.id != 1 {
            return Ok(server::patch_thing::Response::Status404(server::Problem {
                title: "no such thing".into(),
            }));
        }
        let content_type = headers
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let name = req.body.name.unwrap_or_default();
        Ok(server::patch_thing::Response::Status200(format!(
            "{content_type} {name}"
        )))
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

#[tokio::test]
async fn client_sends_the_declared_json_content_type() {
    let client = Client {
        http: reqwest::Client::new(),
        endpoint: spawn_server().await,
    };
    let response = client
        .patch_thing(
            remote::patch_thing::Request {
                id: 1,
                body: remote::ThingPatch {
                    name: Some("x".into()),
                },
            },
            None,
        )
        .await
        .unwrap();
    match response {
        remote::patch_thing::Response::Status200(echo) => {
            assert_eq!(echo, "application/merge-patch+json x");
        }
        other @ remote::patch_thing::Response::Status404(_) => {
            panic!("unexpected response: {other:?}")
        }
    }
}

#[tokio::test]
async fn client_decodes_problem_json_responses() {
    let client = Client {
        http: reqwest::Client::new(),
        endpoint: spawn_server().await,
    };
    let response = client
        .patch_thing(
            remote::patch_thing::Request {
                id: 2,
                body: remote::ThingPatch { name: None },
            },
            None,
        )
        .await
        .unwrap();
    match response {
        remote::patch_thing::Response::Status404(problem) => {
            assert_eq!(problem.title, "no such thing");
        }
        other @ remote::patch_thing::Response::Status200(_) => {
            panic!("unexpected response: {other:?}")
        }
    }
}

#[tokio::test]
async fn server_sends_the_declared_response_content_type() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();

    let ok = http
        .patch(format!("{base}/things/1"))
        .header("content-type", "application/merge-patch+json")
        .body(r#"{"name":"y"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status().as_u16(), 200);
    assert_eq!(
        ok.headers()["content-type"],
        "application/json; charset=utf-8"
    );

    let missing = http
        .patch(format!("{base}/things/2"))
        .header("content-type", "application/merge-patch+json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status().as_u16(), 404);
    assert_eq!(
        missing.headers()["content-type"],
        "application/problem+json"
    );
    let body: serde_json::Value = missing.json().await.unwrap();
    assert_eq!(body["title"], "no such thing");
}
