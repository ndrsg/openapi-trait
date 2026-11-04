//! Regression tests: `OpenAPI` 3.1 documents are supported. `type` lists with
//! `"null"`, `oneOf` / `anyOf` with a `{type: "null"}` branch, `const`, numeric
//! `exclusiveMinimum` / `exclusiveMaximum`, and `security: []` on an operation
//! are all honored.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::axum("assets/testdata/openapi31.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/openapi31.openapi.yaml")]
pub mod remote {}

use remote::RemoteClient as _;
use server::ServerApi as _;

#[derive(Clone)]
struct Server;

impl server::ServerApi for Server {
    type Error = server::NotImplemented;

    async fn put_thing(
        &self,
        ctx: server::put_thing::Context,
    ) -> Result<server::put_thing::Response, Self::Error> {
        let server::put_thing::Context { req, .. } = ctx;
        let mut thing = req.body;
        thing.name = req.tag.or(thing.name);
        Ok(server::put_thing::Response::Status200(thing))
    }
}

#[derive(Clone, openapi_trait::ReqwestClient)]
struct Client {
    #[openapi_trait(client)]
    http: reqwest::Client,
    #[openapi_trait(base_url)]
    endpoint: String,
    /// Left empty: `put_thing` needs no credentials.
    #[openapi_trait(auth)]
    creds: remote::RemoteAuthState,
}

async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Server.router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A thing with every nullable field set to `null`.
fn null_thing() -> remote::Thing {
    remote::Thing {
        name: None,
        owner: None,
        kind: None,
        scores: vec![Some(1), None],
        labels: [("a".to_owned(), None)].into(),
        version: remote::ThingVersion::V1,
        count: 1,
        ratio: None,
        anything: None,
    }
}

#[test]
fn nullable_fields_accept_null() {
    let thing: server::Thing = serde_json::from_str(
        r#"{
            "name": null,
            "owner": null,
            "kind": null,
            "scores": [1, null],
            "labels": {"a": null},
            "version": "v1",
            "count": 1
        }"#,
    )
    .unwrap();
    assert_eq!(thing.name, None);
    assert_eq!(thing.owner, None);
    assert_eq!(thing.kind, None);
    assert_eq!(thing.scores, vec![Some(1), None]);
    assert_eq!(thing.labels.get("a"), Some(&None));
}

#[test]
fn nullable_fields_accept_values() {
    let thing: server::Thing = serde_json::from_str(
        r#"{
            "name": "n",
            "owner": {"name": "o"},
            "kind": "large",
            "scores": [],
            "labels": {},
            "version": "v1",
            "count": 1,
            "anything": 7
        }"#,
    )
    .unwrap();
    assert_eq!(thing.name.as_deref(), Some("n"));
    assert_eq!(thing.owner.map(|o| o.name).as_deref(), Some("o"));
    assert_eq!(thing.kind, Some(server::Kind::Large));
    assert_eq!(thing.anything, Some(serde_json::json!(7)));
}

#[test]
fn required_nullable_fields_are_sent_as_null() {
    let json = serde_json::to_value(null_thing()).unwrap();
    assert_eq!(json["name"], serde_json::Value::Null);
    assert_eq!(json["owner"], serde_json::Value::Null);
    assert!(json.get("ratio").is_none(), "optional `None` is left out");
}

#[test]
fn const_only_accepts_its_value() {
    let wrong = r#"{"name": null, "owner": null, "kind": null, "scores": [],
        "labels": {}, "version": "v2", "count": 1}"#;
    assert!(serde_json::from_str::<server::Thing>(wrong).is_err());
}

#[cfg(feature = "validation")]
#[test]
fn numeric_exclusive_bounds_are_validated() {
    use openapi_trait::serde_valid::Validate as _;

    assert!(null_thing().validate().is_ok());
    let at_exclusive_minimum = remote::Thing {
        count: 0,
        ..null_thing()
    };
    assert!(at_exclusive_minimum.validate().is_err());
    let at_exclusive_maximum = remote::Thing {
        ratio: Some(1.0),
        ..null_thing()
    };
    assert!(at_exclusive_maximum.validate().is_err());
}

#[test]
fn security_none_on_an_operation_removes_the_auth_requirement() {
    // `put_thing` declares `security: []`, so its context has no `auth`; this
    // only compiles if `Context::new` takes no credentials.
    let req = server::put_thing::Request {
        id: 1,
        tag: None,
        body: serde_json::from_value(serde_json::to_value(null_thing()).unwrap()).unwrap(),
    };
    let _ = server::put_thing::Context::new(req, axum::http::HeaderMap::new());
}

#[tokio::test]
async fn round_trip_without_credentials() {
    let client = Client {
        http: reqwest::Client::new(),
        endpoint: spawn_server().await,
        creds: remote::RemoteAuthState::default(),
    };
    let response = client
        .put_thing(
            remote::put_thing::Request {
                id: 1,
                tag: Some("tagged".into()),
                body: null_thing(),
            },
            None,
        )
        .await
        .unwrap();
    let remote::put_thing::Response::Status200(thing) = response;
    assert_eq!(thing.name.as_deref(), Some("tagged"));
    assert_eq!(thing.owner, None);
    assert_eq!(thing.labels.get("a"), Some(&None));
}

#[tokio::test]
async fn operations_without_security_inherit_the_document_requirement() {
    let base = spawn_server().await;
    let status = reqwest::get(format!("{base}/things/1"))
        .await
        .unwrap()
        .status();
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}
