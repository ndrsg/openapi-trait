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
        _req: sec_axum::get_me::Request,
        auth: sec_axum::ApiKeyAuth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_me::Response, Self::Error> {
        Ok(sec_axum::get_me::Response::Status200(sec_axum::OkPayload {
            ok: auth.0 == "k",
        }))
    }

    async fn get_admin(
        &self,
        _req: sec_axum::get_admin::Request,
        auth: sec_axum::BearerAuth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_admin::Response, Self::Error> {
        Ok(sec_axum::get_admin::Response::Status200(
            sec_axum::TokenPayload { token: auth.0 },
        ))
    }

    async fn get_flex(
        &self,
        _req: sec_axum::get_flex::Request,
        auth: sec_axum::get_flex::Auth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_flex::Response, Self::Error> {
        let kind = match auth {
            sec_axum::get_flex::Auth::BearerAuth(b) => format!("bearer:{}", b.0),
            sec_axum::get_flex::Auth::BasicAuth(b) => {
                format!("basic:{}:{}", b.username, b.password)
            }
        };
        Ok(sec_axum::get_flex::Response::Status200(
            sec_axum::KindPayload { kind },
        ))
    }

    async fn get_public(
        &self,
        _req: sec_axum::get_public::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_public::Response, Self::Error> {
        Ok(sec_axum::get_public::Response::Status200(
            sec_axum::OkPayload { ok: true },
        ))
    }

    async fn get_cookied(
        &self,
        _req: sec_axum::get_cookied::Request,
        auth: sec_axum::SessionCookie,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_cookied::Response, Self::Error> {
        Ok(sec_axum::get_cookied::Response::Status200(
            sec_axum::SessionPayload { session: auth.0 },
        ))
    }

    async fn get_oauth(
        &self,
        _req: sec_axum::get_oauth::Request,
        auth: sec_axum::Oauth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_oauth::Response, Self::Error> {
        Ok(sec_axum::get_oauth::Response::Status200(
            sec_axum::TokenPayload { token: auth.0 },
        ))
    }

    async fn get_oidc(
        &self,
        _req: sec_axum::get_oidc::Request,
        auth: sec_axum::Oidc,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_oidc::Response, Self::Error> {
        Ok(sec_axum::get_oidc::Response::Status200(
            sec_axum::TokenPayload { token: auth.0 },
        ))
    }

    async fn get_maybe(
        &self,
        _req: sec_axum::get_maybe::Request,
        auth: Option<sec_axum::BearerAuth>,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_maybe::Response, Self::Error> {
        let kind = auth.map_or_else(|| "anonymous".to_owned(), |b| format!("bearer:{}", b.0));
        Ok(sec_axum::get_maybe::Response::Status200(
            sec_axum::KindPayload { kind },
        ))
    }

    async fn get_maybe_flex(
        &self,
        _req: sec_axum::get_maybe_flex::Request,
        auth: Option<sec_axum::get_maybe_flex::Auth>,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<sec_axum::get_maybe_flex::Response, Self::Error> {
        let kind = match auth {
            None => "anonymous".to_owned(),
            Some(sec_axum::get_maybe_flex::Auth::BearerAuth(b)) => format!("bearer:{}", b.0),
            Some(sec_axum::get_maybe_flex::Auth::BasicAuth(b)) => format!("basic:{}", b.username),
        };
        Ok(sec_axum::get_maybe_flex::Response::Status200(
            sec_axum::KindPayload { kind },
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

#[tokio::test]
async fn oauth2_and_oidc_require_a_bearer_token() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();

    for path in ["oauth", "oidc"] {
        let resp = http.get(format!("{base}/{path}")).send().await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED, "{path}");

        let resp = http
            .get(format!("{base}/{path}"))
            .header("Authorization", "Bearer tok")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK, "{path}");
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["token"], "tok", "{path}");
    }
}

#[tokio::test]
async fn optional_auth_allows_anonymous_requests() {
    let base = spawn_server().await;
    let http = reqwest::Client::new();

    for path in ["maybe", "maybe-flex"] {
        let resp = http.get(format!("{base}/{path}")).send().await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK, "{path}");
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["kind"], "anonymous", "{path}");

        let resp = http
            .get(format!("{base}/{path}"))
            .header("Authorization", "Bearer tok")
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["kind"], "bearer:tok", "{path}");
    }

    let resp = http
        .get(format!("{base}/maybe-flex"))
        .header("Authorization", "Basic dTpw")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "basic:u");
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
        .get_admin(sec_client::get_admin::Request {}, None)
        .await
        .unwrap();
    match resp {
        sec_client::get_admin::Response::Status200(body) => assert_eq!(body.token, "xyz"),
    }
}

#[tokio::test]
async fn per_request_bearer_overrides_scheme_credential() {
    let base = spawn_server().await;

    let client = {
        use sec_client::SecClientClientAuth as _;
        DerivedClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base,
            creds: sec_client::SecClientAuthState::default(),
        }
        .with_bearer_auth("scheme-token")
    };

    // The per-request token must replace the scheme-level `Authorization`
    // header, not append a second one. If it appended, the server's extractor
    // would see "scheme-token" (the first header) instead.
    let resp = client
        .get_admin(
            sec_client::get_admin::Request {},
            Some(openapi_trait::RequestOptions::new().bearer_auth("override-token")),
        )
        .await
        .unwrap();
    match resp {
        sec_client::get_admin::Response::Status200(body) => {
            assert_eq!(body.token, "override-token");
        }
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
        .get_admin(sec_client::get_admin::Request {}, None)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("bearerAuth"), "got: {msg}");
}

#[tokio::test]
async fn per_request_bearer_works_without_client_credential() {
    let base = spawn_server().await;

    // Client built with no configured scheme credential. Previously this made
    // any request fail fast with `MissingCredential`; a per-request token must
    // now satisfy the operation's auth requirement.
    let client = DerivedClient {
        http: openapi_trait::reqwest::Client::new(),
        endpoint: base,
        creds: sec_client::SecClientAuthState::default(),
    };

    let resp = client
        .get_admin(
            sec_client::get_admin::Request {},
            Some(openapi_trait::RequestOptions::new().bearer_auth("per-request-token")),
        )
        .await
        .unwrap();
    match resp {
        sec_client::get_admin::Response::Status200(body) => {
            assert_eq!(body.token, "per-request-token");
        }
    }
}

#[tokio::test]
async fn client_sends_oauth2_and_oidc_tokens_as_bearer() {
    let base = spawn_server().await;

    let client = {
        use sec_client::SecClientClientAuth as _;
        DerivedClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base,
            creds: sec_client::SecClientAuthState::default(),
        }
        .with_oauth("oauth-token")
        .with_oidc("oidc-token")
    };

    match client
        .get_oauth(sec_client::get_oauth::Request {}, None)
        .await
        .unwrap()
    {
        sec_client::get_oauth::Response::Status200(body) => assert_eq!(body.token, "oauth-token"),
    }
    match client
        .get_oidc(sec_client::get_oidc::Request {}, None)
        .await
        .unwrap()
    {
        sec_client::get_oidc::Response::Status200(body) => assert_eq!(body.token, "oidc-token"),
    }
}

#[tokio::test]
async fn client_calls_optional_auth_operations_with_and_without_credentials() {
    let base = spawn_server().await;

    let anonymous = DerivedClient {
        http: openapi_trait::reqwest::Client::new(),
        endpoint: base.clone(),
        creds: sec_client::SecClientAuthState::default(),
    };
    match anonymous
        .get_maybe(sec_client::get_maybe::Request {}, None)
        .await
        .unwrap()
    {
        sec_client::get_maybe::Response::Status200(body) => assert_eq!(body.kind, "anonymous"),
    }
    match anonymous
        .get_maybe_flex(sec_client::get_maybe_flex::Request {}, None)
        .await
        .unwrap()
    {
        sec_client::get_maybe_flex::Response::Status200(body) => assert_eq!(body.kind, "anonymous"),
    }

    let authed = {
        use sec_client::SecClientClientAuth as _;
        anonymous.with_bearer_auth("tok")
    };
    match authed
        .get_maybe(sec_client::get_maybe::Request {}, None)
        .await
        .unwrap()
    {
        sec_client::get_maybe::Response::Status200(body) => assert_eq!(body.kind, "bearer:tok"),
    }
    match authed
        .get_maybe_flex(sec_client::get_maybe_flex::Request {}, None)
        .await
        .unwrap()
    {
        sec_client::get_maybe_flex::Response::Status200(body) => {
            assert_eq!(body.kind, "bearer:tok");
        }
    }
}
