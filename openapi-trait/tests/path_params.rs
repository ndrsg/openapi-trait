//! Regression tests for `KNOWN_ISSUES.md` S1 (path params bound in URL order),
//! C1 (string-enum path params get a generated type) and C2 (an operation-level
//! param overrides a path-level one with the same name and location).
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
        req: pp_server::get_swapped::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pp_server::get_swapped::Response, Self::Error> {
        Ok(pp_server::get_swapped::Response::Status200(format!(
            "a={} b={}",
            req.a, req.b
        )))
    }

    async fn get_typed(
        &self,
        req: pp_server::get_typed::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pp_server::get_typed::Response, Self::Error> {
        // `id` is an `i64`: this only compiles if the field kept its type.
        let id: i64 = req.id;
        Ok(pp_server::get_typed::Response::Status200(format!(
            "id={id} slug={}",
            req.slug
        )))
    }

    async fn get_thing(
        &self,
        req: pp_server::get_thing::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pp_server::get_thing::Response, Self::Error> {
        let kind = match req.kind {
            pp_server::get_thing::KindPath::A => "A",
            pp_server::get_thing::KindPath::B => "B",
            pp_server::get_thing::KindPath::XY => "XY",
        };
        Ok(pp_server::get_thing::Response::Status200(kind.to_owned()))
    }

    async fn get_dup(
        &self,
        req: pp_server::get_dup::Request,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pp_server::get_dup::Response, Self::Error> {
        let id: i64 = req.id;
        Ok(pp_server::get_dup::Response::Status200(id * 2))
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

#[tokio::test]
async fn enum_path_param_is_deserialized_by_wire_value() {
    let base = spawn_server().await;
    let (status, body) = get(format!("{base}/things/b")).await;
    assert_eq!((status, body.as_str()), (200, r#""B""#));
    let (status, body) = get(format!("{base}/things/x-y")).await;
    assert_eq!((status, body.as_str()), (200, r#""XY""#));
    let (status, _) = get(format!("{base}/things/nope")).await;
    assert_eq!(status, 400);
}

#[test]
fn enum_path_param_displays_its_wire_value() {
    assert_eq!(pp_server::get_thing::KindPath::XY.to_string(), "x-y");
}

#[tokio::test]
async fn operation_level_param_overrides_path_level_param() {
    let base = spawn_server().await;
    let (status, body) = get(format!("{base}/dup/21")).await;
    assert_eq!((status, body.as_str()), (200, "42"));
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
    async fn client_round_trips_enum_and_reordered_path_params() {
        let client = Client {
            http: ::reqwest::Client::new(),
            base_url: spawn_server().await,
        };

        let resp = client
            .get_thing(
                pp_client::get_thing::Request {
                    kind: pp_client::get_thing::KindPath::B,
                },
                None,
            )
            .await
            .unwrap();
        assert!(matches!(resp, pp_client::get_thing::Response::Status200(ref s) if s == "B"));

        let resp = client
            .get_swapped(
                pp_client::get_swapped::Request {
                    b: "BBB".into(),
                    a: "AAA".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            matches!(resp, pp_client::get_swapped::Response::Status200(ref s) if s == "a=AAA b=BBB")
        );

        let resp = client
            .get_dup(pp_client::get_dup::Request { id: 5 }, None)
            .await
            .unwrap();
        assert!(matches!(resp, pp_client::get_dup::Response::Status200(10)));
    }
}
