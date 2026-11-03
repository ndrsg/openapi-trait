//! Regression tests for `KNOWN_ISSUES.md` S2 (status codes outside the old
//! hard-coded table were sent as 500) and S3 (optional request bodies were
//! required in practice).
#![allow(missing_docs)]

#[openapi_trait::axum("assets/testdata/status_and_body.openapi.yaml")]
pub mod sb {}

use sb::SbApi as _;

#[derive(Clone)]
struct Server;

impl sb::SbApi for Server {
    type Error = sb::NotImplemented;

    async fn get_partial(
        &self,
        req: sb::get_partial::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sb::get_partial::Response, Self::Error> {
        Ok(match req.which {
            206 => sb::get_partial::Response::Status206("part".into()),
            308 => sb::get_partial::Response::Status308,
            _ => sb::get_partial::Response::Status418,
        })
    }

    async fn opt_body(
        &self,
        req: sb::opt_body::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sb::opt_body::Response, Self::Error> {
        Ok(sb::opt_body::Response::Status200(format!("{:?}", req.body)))
    }

    async fn opt_bin_body(
        &self,
        req: sb::opt_bin_body::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sb::opt_bin_body::Response, Self::Error> {
        let text = match req.body {
            None => "none".to_owned(),
            Some(body) => {
                let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
                format!("some:{}", String::from_utf8_lossy(&bytes))
            }
        };
        Ok(sb::opt_bin_body::Response::Status200(text))
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

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn declared_status_codes_are_sent_as_declared() {
    let base = spawn_server().await;
    for code in [206u16, 308, 418] {
        let resp = http()
            .get(format!("{base}/partial?which={code}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), code);
    }
}

#[tokio::test]
async fn optional_json_body_may_be_omitted() {
    let base = spawn_server().await;

    let resp = http().post(format!("{base}/opt")).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(resp.text().await.unwrap(), r#""None""#);

    let resp = http()
        .post(format!("{base}/opt"))
        .json(&5)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(resp.text().await.unwrap(), r#""Some(5)""#);
}

#[tokio::test]
async fn optional_json_body_still_rejects_malformed_json() {
    let base = spawn_server().await;
    let resp = http()
        .post(format!("{base}/opt"))
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error(), "{}", resp.status());
}

#[tokio::test]
async fn optional_binary_body_may_be_omitted() {
    let base = spawn_server().await;

    let resp = http().post(format!("{base}/opt-bin")).send().await.unwrap();
    assert_eq!(resp.text().await.unwrap(), r#""none""#);

    let resp = http()
        .post(format!("{base}/opt-bin"))
        .header("content-type", "application/octet-stream")
        .body("")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), r#""none""#);

    let resp = http()
        .post(format!("{base}/opt-bin"))
        .header("content-type", "application/octet-stream")
        .body("abc")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.text().await.unwrap(), r#""some:abc""#);
}
