//! A handler receives one `{op}::Context`: the typed request, the credentials
//! (for secured operations) and the raw headers. Application state lives in
//! the implementing type, and `router()` merges into a router with state.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/json_media_types.openapi.yaml")]
pub mod server {}

#[openapi_trait::axum("assets/testdata/security.openapi.yaml")]
pub mod secured {}

use server::ServerApi as _;

/// State lives in `self`.
#[derive(Clone)]
struct Server {
    greeting: &'static str,
}

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn patch_thing(
        &self,
        ctx: server::patch_thing::Context,
    ) -> Result<server::patch_thing::Response, Self::Error> {
        let trace = ctx
            .headers
            .get("x-trace")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-");
        let name = ctx.req.body.name.unwrap_or_default();
        Ok(server::patch_thing::Response::Status200(format!(
            "{} {name} {trace}",
            self.greeting
        )))
    }
}

#[tokio::test]
async fn handlers_can_be_called_directly_with_a_context() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-trace", "t1".parse().unwrap());
    let req = server::patch_thing::Request {
        id: 1,
        body: server::ThingPatch {
            name: Some("box".into()),
        },
    };
    let ctx = server::patch_thing::Context::new(req, headers);

    let response = Server { greeting: "hi" }.patch_thing(ctx).await.unwrap();
    let server::patch_thing::Response::Status200(text) = response else {
        panic!("expected 200, got {response:?}");
    };
    assert_eq!(text, "hi box t1");
}

#[test]
fn secured_contexts_carry_the_credentials() {
    let ctx = secured::get_admin::Context::new(
        secured::get_admin::Request {},
        secured::BearerAuth("tok".into()),
        axum::http::HeaderMap::new(),
    );
    let secured::get_admin::Context { auth, .. } = ctx;
    assert_eq!(auth.0, "tok");
}

/// The generated router has no state of its own, so it merges into an
/// application router that does.
#[tokio::test]
async fn router_merges_into_a_router_with_state() {
    #[derive(Clone)]
    struct AppState;

    let app: axum::Router = axum::Router::<AppState>::new()
        .route(
            "/health",
            axum::routing::get(|_: axum::extract::State<AppState>| async { "ok" }),
        )
        .merge(Server { greeting: "hey" }.router().with_state(()))
        .with_state(AppState);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let http = reqwest::Client::new();
    let health = http
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.text().await.unwrap(), "ok");
    let patched = http
        .patch(format!("http://{addr}/things/1"))
        .header("content-type", "application/merge-patch+json")
        .header("x-trace", "t2")
        .body(r#"{"name": "box"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(patched.text().await.unwrap(), r#""hey box t2""#);
}
