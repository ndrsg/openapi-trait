//! Regression tests for `KNOWN_ISSUES.md` D2 (parameters): the inline enum
//! items of an array query parameter used to be `String`. They now get an
//! `{Op}{Param}Query` enum, like a scalar query enum, which both the client
//! and the server encode as the wire value.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/query_enum_array.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/query_enum_array.openapi.yaml")]
pub mod remote {}

use remote::RemoteClient as _;
use server::ServerApi as _;

#[derive(Clone)]
struct Server;

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn filter_colors(
        &self,
        req: server::FilterColorsRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<server::FilterColorsResponse, Self::Error> {
        let colors: Vec<server::FilterColorsColorQuery> = req.color;
        let shades: Vec<server::FilterColorsShadeQuery> = req.shade.unwrap_or_default();
        let mut out: Vec<String> = colors
            .iter()
            .map(|c| {
                serde_json::to_value(c)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        out.extend(shades.iter().map(|s| format!("{s:?}")));
        Ok(server::FilterColorsResponse::Status200(out))
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
        axum::serve(listener, Server.router().with_state(()))
            .await
            .unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn enum_array_query_params_round_trip() {
    let client = Client {
        http: reqwest::Client::new(),
        endpoint: spawn_server().await,
    };
    let remote::FilterColorsResponse::Status200(parsed) = client
        .filter_colors(
            remote::FilterColorsRequest {
                color: vec![
                    remote::FilterColorsColorQuery::Red,
                    remote::FilterColorsColorQuery::DarkGreen,
                ],
                shade: Some(vec![remote::FilterColorsShadeQuery::Dark]),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(parsed, ["red", "dark-green", "Dark"]);
}

#[tokio::test]
async fn unknown_enum_values_are_rejected() {
    let base_url = spawn_server().await;
    let status = reqwest::get(format!("{base_url}/colors?color=red,blue"))
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}
