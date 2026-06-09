//! Integration tests for security-scheme codegen.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/security.openapi.yaml")]
pub mod sec_axum {}

#[openapi_trait::client("assets/testdata/security.openapi.yaml")]
pub mod sec_client {}

use sec_axum::SecAxumApi as _;
use sec_client::SecClientClient as _;

#[derive(Clone)]
struct MockServer;

impl sec_axum::SecAxumApi for MockServer {
    type Error = sec_axum::NotImplemented;

    async fn get_me(
        &self,
        _req: sec_axum::GetMeRequest,
        auth: sec_axum::ApiKeyAuth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::GetMeResponse, Self::Error> {
        Ok(sec_axum::GetMeResponse::Status200(sec_axum::OkPayload {
            ok: auth.0 == "k",
        }))
    }

    async fn get_admin(
        &self,
        _req: sec_axum::GetAdminRequest,
        auth: sec_axum::BearerAuth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::GetAdminResponse, Self::Error> {
        Ok(sec_axum::GetAdminResponse::Status200(
            sec_axum::TokenPayload { token: auth.0 },
        ))
    }

    async fn get_flex(
        &self,
        _req: sec_axum::GetFlexRequest,
        auth: sec_axum::GetFlexAuth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::GetFlexResponse, Self::Error> {
        let kind = match auth {
            sec_axum::GetFlexAuth::BearerAuth(b) => format!("bearer:{}", b.0),
            sec_axum::GetFlexAuth::BasicAuth(b) => format!("basic:{}:{}", b.username, b.password),
        };
        Ok(sec_axum::GetFlexResponse::Status200(
            sec_axum::KindPayload { kind },
        ))
    }

    async fn get_public(
        &self,
        _req: sec_axum::GetPublicRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::GetPublicResponse, Self::Error> {
        Ok(sec_axum::GetPublicResponse::Status200(
            sec_axum::OkPayload { ok: true },
        ))
    }

    async fn get_cookied(
        &self,
        _req: sec_axum::GetCookiedRequest,
        auth: sec_axum::SessionCookie,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::GetCookiedResponse, Self::Error> {
        Ok(sec_axum::GetCookiedResponse::Status200(
            sec_axum::SessionPayload { session: auth.0 },
        ))
    }
}

async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, MockServer.router().with_state(()))
            .await
            .unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn me_requires_api_key_header() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();

    // missing header → 401
    let resp = http.get(format!("{base}/me")).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // with header → 200 echoing decoded value
    let resp = http
        .get(format!("{base}/me"))
        .header("X-API-Key", "k")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn admin_requires_bearer() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{base}/admin"))
        .header("Authorization", "Bearer abc")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["token"], "abc");

    let resp = http.get(format!("{base}/admin")).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn flex_accepts_either_scheme() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{base}/flex"))
        .header("Authorization", "Bearer tok")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "bearer:tok");

    // Basic auth — base64 of "u:p"
    let resp = http
        .get(format!("{base}/flex"))
        .header("Authorization", "Basic dTpw")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "basic:u:p");

    let resp = http.get(format!("{base}/flex")).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_is_open() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();
    let resp = http.get(format!("{base}/public")).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn cookied_extracts_session() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{base}/cookied"))
        .header("Cookie", "session=xyz; other=ignored")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["session"], "xyz");
}

// ---- Client side: credential injection ----

#[derive(Clone, openapi_trait::ReqwestClient)]
struct DerivedClient {
    #[openapi_trait(client)]
    http: ::reqwest::Client,
    #[openapi_trait(base_url)]
    endpoint: String,
    #[openapi_trait(auth)]
    creds: sec_client::SecClientAuthState,
}

#[tokio::test]
async fn client_injects_bearer_token() {
    let base = spawn_server().await;

    let client = {
        use sec_client::SecClientClientAuth as _;
        DerivedClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base,
            creds: sec_client::SecClientAuthState::default(),
        }
        .with_bearer_auth("xyz")
    };

    let resp = client
        .get_admin(sec_client::GetAdminRequest {}, None)
        .await
        .unwrap();
    match resp {
        sec_client::GetAdminResponse::Status200(body) => assert_eq!(body.token, "xyz"),
    }
}

#[tokio::test]
async fn client_missing_credential_errors_before_send() {
    let client = DerivedClient {
        http: openapi_trait::reqwest::Client::new(),
        endpoint: "http://127.0.0.1:1".into(), // never reached
        creds: sec_client::SecClientAuthState::default(),
    };

    let err = client
        .get_admin(sec_client::GetAdminRequest {}, None)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("bearerAuth"), "got: {msg}");
}
