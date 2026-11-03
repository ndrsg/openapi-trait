//! Regression tests for `KNOWN_ISSUES.md` S1 (path params bound in URL order).
#![allow(missing_docs)]

#[openapi_trait::axum("assets/testdata/path_params.openapi.yaml")]
pub mod pp_server {}

#[cfg(feature = "reqwest-client")]
#[openapi_trait::client("assets/testdata/path_params.openapi.yaml")]
pub mod pp_client {}

use pp_server::PpServerApi as _;

#[derive(Clone)]
struct Server;

impl pp_server::PpServerApi for Server {
    type Error = pp_server::NotImplemented;

    async fn get_swapped(
        &self,
        req: pp_server::GetSwappedRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pp_server::GetSwappedResponse, Self::Error> {
        Ok(pp_server::GetSwappedResponse::Status200(format!(
            "a={} b={}",
            req.a, req.b
        )))
    }

    async fn get_typed(
        &self,
        req: pp_server::GetTypedRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pp_server::GetTypedResponse, Self::Error> {
        // `id` is an `i64`: this only compiles if the field kept its type.
        let id: i64 = req.id;
        Ok(pp_server::GetTypedResponse::Status200(format!(
            "id={id} slug={}",
            req.slug
        )))
    }
}

async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Server.router().with_state(()))
            .await
            .unwrap();
    });
    format!("http://{addr}")
}

async fn get(url: String) -> (u16, String) {
    let resp = reqwest::get(url).await.unwrap();
    (resp.status().as_u16(), resp.text().await.unwrap())
}

#[tokio::test]
async fn path_params_are_bound_in_url_order() {
    let base = spawn_server().await;
    let (status, body) = get(format!("{base}/swapped/AAA/BBB")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, r#""a=AAA b=BBB""#);
}

#[tokio::test]
async fn reversed_path_params_with_different_types_parse() {
    let base = spawn_server().await;
    let (status, body) = get(format!("{base}/typed/7/hello")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, r#""id=7 slug=hello""#);
}

#[cfg(feature = "reqwest-client")]
mod client {
    use super::{pp_client, spawn_server};
    use pp_client::PpClientClient as _;

    #[derive(Clone, openapi_trait::ReqwestClient)]
    struct Client {
        #[openapi_trait(client)]
        http: ::reqwest::Client,
        #[openapi_trait(base_url)]
        base_url: String,
    }

    #[tokio::test]
    async fn client_round_trips_reordered_path_params() {
        let client = Client {
            http: ::reqwest::Client::new(),
            base_url: spawn_server().await,
        };

        let resp = client
            .get_swapped(
                pp_client::GetSwappedRequest {
                    b: "BBB".into(),
                    a: "AAA".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            matches!(resp, pp_client::GetSwappedResponse::Status200(ref s) if s == "a=AAA b=BBB")
        );
    }
}
