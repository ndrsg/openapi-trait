//! End-to-end coverage for `OpenAPI` query `style`/`explode` serialization on both
//! the generated reqwest client and the generated axum server.
#![cfg(feature = "reqwest-client")]

use std::sync::{Arc, Mutex};

#[openapi_trait::client("assets/testdata/query_styles.openapi.yaml")]
pub mod qc {}

#[openapi_trait::axum("assets/testdata/query_styles.openapi.yaml")]
mod qs {}

use qc::QcClient as _;
use qs::QsApi as _;

#[derive(Clone, openapi_trait::ReqwestClient)]
struct Client {
    client: ::reqwest::Client,
    base_url: String,
}

/// A generated server that echoes the parsed query params back as JSON, so a
/// client can assert an exact round-trip.
#[derive(Clone)]
struct Echo;

impl qs::QsApi for Echo {
    type Error = qs::NotImplemented;

    async fn search_items(
        &self,
        req: qs::SearchItemsRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<qs::SearchItemsResponse, Self::Error> {
        Ok(qs::SearchItemsResponse::Status200(qs::SearchResult {
            tag: req.tag.unwrap_or_default(),
            csv: req.csv.unwrap_or_default(),
            space: req.space.unwrap_or_default(),
            pipe: req.pipe.unwrap_or_default(),
            ids: req.ids.unwrap_or_default(),
            q: req.q,
        }))
    }
}

async fn spawn_echo_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Echo.router().with_state(()))
            .await
            .unwrap();
    });
    format!("http://{addr}")
}

fn sample_request() -> qc::SearchItemsRequest {
    qc::SearchItemsRequest {
        tag: Some(vec!["a".into(), "b".into()]),
        csv: Some(vec!["x".into(), "y".into()]),
        space: Some(vec!["m".into(), "n".into()]),
        pipe: Some(vec!["p".into(), "q".into()]),
        ids: Some(vec![1, 2]),
        q: Some("hi".into()),
    }
}

/// The client must emit each style's exact wire form: repeated keys for exploded
/// `form`, and comma/space/pipe-joined single values otherwise.
#[tokio::test]
async fn client_serializes_query_styles() {
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let sink = captured.clone();
    let app = axum::Router::new().route(
        "/items",
        axum::routing::get(
            move |axum::extract::RawQuery(raw): axum::extract::RawQuery| {
                let sink = sink.clone();
                async move {
                    *sink.lock().unwrap() = raw;
                    axum::Json(serde_json::json!({
                        "tag": [], "csv": [], "space": [], "pipe": [], "ids": []
                    }))
                }
            },
        ),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = Client {
        client: openapi_trait::reqwest::Client::new(),
        base_url,
    };
    client.search_items(sample_request(), None).await.unwrap();

    let raw = captured.lock().unwrap().clone().expect("query captured");
    // space encodes as '+', comma as %2C, pipe as %7C under form-urlencoding.
    assert_eq!(
        raw,
        "tag=a&tag=b&csv=x%2Cy&space=m+n&pipe=p%7Cq&ids=1%2C2&q=hi"
    );
}

/// The server must decode each style back into the right `Vec`, given canonical
/// percent-encoded wire input.
#[tokio::test]
async fn server_parses_query_styles() {
    let base_url = spawn_echo_server().await;
    let url =
        format!("{base_url}/items?tag=a&tag=b&csv=x%2Cy&space=m%20n&pipe=p%7Cq&ids=1%2C2&q=hi");

    let body: serde_json::Value = openapi_trait::reqwest::Client::new()
        .get(url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["tag"], serde_json::json!(["a", "b"]));
    assert_eq!(body["csv"], serde_json::json!(["x", "y"]));
    assert_eq!(body["space"], serde_json::json!(["m", "n"]));
    assert_eq!(body["pipe"], serde_json::json!(["p", "q"]));
    assert_eq!(body["ids"], serde_json::json!([1, 2]));
    assert_eq!(body["q"], serde_json::json!("hi"));
}

/// Client-encode then server-decode, both generated from the same spec, must
/// preserve every array intact.
#[tokio::test]
async fn client_server_round_trip() {
    let base_url = spawn_echo_server().await;
    let client = Client {
        client: openapi_trait::reqwest::Client::new(),
        base_url,
    };

    let response = client.search_items(sample_request(), None).await.unwrap();
    // The spec declares a single 200 response, so this is the only variant.
    let qc::SearchItemsResponse::Status200(result) = response;

    assert_eq!(result.tag, ["a", "b"]);
    assert_eq!(result.csv, ["x", "y"]);
    assert_eq!(result.space, ["m", "n"]);
    assert_eq!(result.pipe, ["p", "q"]);
    assert_eq!(result.ids, [1, 2]);
    assert_eq!(result.q.as_deref(), Some("hi"));
}

/// Omitted optional array params must serialize to nothing and deserialize to
/// empty, not to a stray `key=` pair.
#[tokio::test]
async fn empty_optional_params_are_absent() {
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let sink = captured.clone();
    let app = axum::Router::new().route(
        "/items",
        axum::routing::get(
            move |axum::extract::RawQuery(raw): axum::extract::RawQuery| {
                let sink = sink.clone();
                async move {
                    *sink.lock().unwrap() = raw;
                    axum::Json(serde_json::json!({
                        "tag": [], "csv": [], "space": [], "pipe": [], "ids": []
                    }))
                }
            },
        ),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = Client {
        client: openapi_trait::reqwest::Client::new(),
        base_url,
    };
    client
        .search_items(
            qc::SearchItemsRequest {
                tag: None,
                csv: None,
                space: None,
                pipe: None,
                ids: None,
                q: None,
            },
            None,
        )
        .await
        .unwrap();

    // No params supplied -> no query string at all.
    assert_eq!(captured.lock().unwrap().clone(), None);
}
