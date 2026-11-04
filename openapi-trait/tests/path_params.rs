//! Regression tests for path parameters: they are bound in URL order,
//! string-enum path params get a generated type (or the component's type for a
//! `$ref`), an operation-level param overrides a path-level one with the same
//! name and location, and the client writes values in their JSON string form.
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
        ctx: pp_server::get_swapped::Context,
    ) -> Result<pp_server::get_swapped::Response, Self::Error> {
        let pp_server::get_swapped::Context { req, .. } = ctx;
        Ok(pp_server::get_swapped::Response::Status200(format!(
            "a={} b={}",
            req.a, req.b
        )))
    }

    async fn get_typed(
        &self,
        ctx: pp_server::get_typed::Context,
    ) -> Result<pp_server::get_typed::Response, Self::Error> {
        let pp_server::get_typed::Context { req, .. } = ctx;
        // `id` is an `i64`: this only compiles if the field kept its type.
        let id: i64 = req.id;
        Ok(pp_server::get_typed::Response::Status200(format!(
            "id={id} slug={}",
            req.slug
        )))
    }

    async fn get_thing(
        &self,
        ctx: pp_server::get_thing::Context,
    ) -> Result<pp_server::get_thing::Response, Self::Error> {
        let pp_server::get_thing::Context { req, .. } = ctx;
        let kind = match req.kind {
            pp_server::get_thing::KindPath::A => "A",
            pp_server::get_thing::KindPath::B => "B",
            pp_server::get_thing::KindPath::XY => "XY",
        };
        Ok(pp_server::get_thing::Response::Status200(kind.to_owned()))
    }

    async fn get_dup(
        &self,
        ctx: pp_server::get_dup::Context,
    ) -> Result<pp_server::get_dup::Response, Self::Error> {
        let pp_server::get_dup::Context { req, .. } = ctx;
        let id: i64 = req.id;
        Ok(pp_server::get_dup::Response::Status200(id * 2))
    }

    async fn get_shape(
        &self,
        ctx: pp_server::get_shape::Context,
    ) -> Result<pp_server::get_shape::Response, Self::Error> {
        // `shape` is the component type, so it can be returned as the body.
        let shape: pp_server::Shape = ctx.req.shape;
        Ok(pp_server::get_shape::Response::Status200(shape))
    }

    async fn get_at(
        &self,
        ctx: pp_server::get_at::Context,
    ) -> Result<pp_server::get_at::Response, Self::Error> {
        Ok(pp_server::get_at::Response::Status200(ctx.req.when))
    }
}

async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Server.router()).await.unwrap();
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

#[test]
fn component_enum_displays_its_wire_value() {
    assert_eq!(
        pp_server::Shape::RoundedSquare.to_string(),
        "rounded-square"
    );
}

#[tokio::test]
async fn component_enum_path_param_is_deserialized_by_wire_value() {
    let base = spawn_server().await;
    let (status, body) = get(format!("{base}/shapes/rounded-square")).await;
    assert_eq!((status, body.as_str()), (200, r#""rounded-square""#));
    let (status, _) = get(format!("{base}/shapes/triangle")).await;
    assert_eq!(status, 400);
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

    #[tokio::test]
    async fn client_round_trips_component_enum_and_date_time_path_params() {
        let client = Client {
            http: ::reqwest::Client::new(),
            base_url: spawn_server().await,
        };

        let resp = client
            .get_shape(
                pp_client::get_shape::Request {
                    shape: pp_client::Shape::RoundedSquare,
                },
                None,
            )
            .await
            .unwrap();
        assert!(matches!(
            resp,
            pp_client::get_shape::Response::Status200(pp_client::Shape::RoundedSquare)
        ));

        let when = "2026-11-30T10:15:00Z"
            .parse::<openapi_trait::chrono::DateTime<openapi_trait::chrono::Utc>>()
            .unwrap();
        let resp = client
            .get_at(pp_client::get_at::Request { when }, None)
            .await
            .unwrap();
        assert!(matches!(resp, pp_client::get_at::Response::Status200(at) if at == when));
    }

    /// A `date-time` path parameter is sent in its RFC 3339 (JSON) form, not
    /// as chrono's `Display` output (`2026-11-30 10:15:00 UTC`), which only
    /// a chrono-based server would accept.
    #[tokio::test]
    async fn client_sends_date_time_path_params_as_rfc3339() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = seen.clone();
        let app = axum::Router::new().fallback(move |uri: axum::http::Uri| {
            let sink = sink.clone();
            async move {
                *sink.lock().unwrap() = uri.path().to_owned();
                axum::Json("2026-11-30T10:15:00Z")
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let client = Client {
            http: ::reqwest::Client::new(),
            base_url,
        };
        let when = "2026-11-30T10:15:00Z".parse().unwrap();
        client
            .get_at(pp_client::get_at::Request { when }, None)
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), "/at/2026%2D11%2D30T10%3A15%3A00Z");
    }
}
