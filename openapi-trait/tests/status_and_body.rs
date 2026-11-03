//! Regression tests for `KNOWN_ISSUES.md` S2 (status codes outside the old
//! hard-coded table were sent as 500).
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
        req: sb::GetPartialRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sb::GetPartialResponse, Self::Error> {
        Ok(match req.which {
            206 => sb::GetPartialResponse::Status206("part".into()),
            308 => sb::GetPartialResponse::Status308,
            _ => sb::GetPartialResponse::Status418,
        })
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
