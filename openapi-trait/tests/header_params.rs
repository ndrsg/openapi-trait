//! Regression tests: header parameters used to be `String` whatever their
//! schema. They now get the schema's Rust type (a string enum gets an
//! `{Param}Header` enum). The server parses each value like a query scalar and
//! returns 400 when it doesn't parse; arrays use the `simple` style (`a,b,c`).
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/header_params.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/header_params.openapi.yaml")]
pub mod remote {}

use remote::RemoteClient as _;
use server::ServerApi as _;

#[derive(Clone)]
struct Server;

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn echo_headers(
        &self,
        ctx: server::echo_headers::Context,
    ) -> Result<server::echo_headers::Response, Self::Error> {
        let req = ctx.req;
        let x_limit: i32 = req.x_limit;
        let x_verbose: Option<bool> = req.x_verbose;
        let x_mode: server::echo_headers::XModeHeader = req.x_mode;
        let x_since: Option<openapi_trait::chrono::DateTime<openapi_trait::chrono::Utc>> =
            req.x_since;
        let x_request_id: Option<openapi_trait::uuid::Uuid> = req.x_request_id;
        let x_note: Option<String> = req.x_note;
        let x_ids: Option<Vec<i64>> = req.x_ids;
        let x_tags: Option<Vec<String>> = req.x_tags;
        Ok(server::echo_headers::Response::Status200(vec![
            format!("limit={x_limit}"),
            format!("verbose={x_verbose:?}"),
            format!("mode={x_mode:?}"),
            format!("since={:?}", x_since.map(|d| d.timestamp())),
            format!("id={x_request_id:?}"),
            format!("note={x_note:?}"),
            format!("ids={x_ids:?}"),
            format!("tags={x_tags:?}"),
        ]))
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
async fn typed_headers_round_trip() {
    let client = Client {
        http: reqwest::Client::new(),
        endpoint: spawn_server().await,
    };
    let id = openapi_trait::uuid::Uuid::from_u128(0x1234);
    let since = openapi_trait::chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let remote::echo_headers::Response::Status200(echoed) = client
        .echo_headers(
            remote::echo_headers::Request {
                x_limit: -7,
                x_verbose: Some(true),
                x_mode: remote::echo_headers::XModeHeader::SlowMo,
                x_since: Some(since),
                x_request_id: Some(id),
                x_note: Some("\"quoted\"".to_owned()),
                x_ids: Some(vec![1, 2, 3]),
                x_tags: Some(vec!["a".to_owned(), "b".to_owned()]),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        echoed,
        [
            "limit=-7".to_owned(),
            "verbose=Some(true)".to_owned(),
            "mode=SlowMo".to_owned(),
            "since=Some(1700000000)".to_owned(),
            format!("id=Some({id:?})"),
            // A plain string header is passed through as is, quotes included.
            "note=Some(\"\\\"quoted\\\"\")".to_owned(),
            "ids=Some([1, 2, 3])".to_owned(),
            "tags=Some([\"a\", \"b\"])".to_owned(),
        ]
    );
}

#[tokio::test]
async fn optional_headers_can_be_omitted() {
    let base_url = spawn_server().await;
    let echoed: Vec<String> = reqwest::Client::new()
        .get(format!("{base_url}/echo"))
        .header("X-Limit", "5")
        .header("X-Mode", "fast")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed[0], "limit=5");
    assert_eq!(echoed[1], "verbose=None");
    assert_eq!(echoed[2], "mode=Fast");
    assert_eq!(echoed[6], "ids=None");
}

#[tokio::test]
async fn bad_header_values_are_rejected() {
    let base_url = spawn_server().await;
    let http = reqwest::Client::new();
    for (name, value) in [
        ("X-Limit", "lots"),
        ("X-Mode", "medium"),
        ("X-Verbose", "maybe"),
        ("X-Since", "yesterday"),
        ("X-Ids", "1,two"),
    ] {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Limit", "5".parse().unwrap());
        headers.insert("X-Mode", "fast".parse().unwrap());
        headers.insert(name, value.parse().unwrap());
        let response = http
            .get(format!("{base_url}/echo"))
            .headers(headers)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{name}: {value}"
        );
    }
}

#[tokio::test]
async fn missing_required_header_is_rejected() {
    let base_url = spawn_server().await;
    let response = reqwest::Client::new()
        .get(format!("{base_url}/echo"))
        .header("X-Mode", "fast")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        response.text().await.unwrap(),
        "missing required header `X-Limit`"
    );
}
