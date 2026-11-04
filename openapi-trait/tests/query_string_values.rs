//! String query and header values arrive exactly as sent, even when they look
//! like JSON. Numbers and enums behind a `$ref` still parse.

#[openapi_trait::axum("assets/testdata/query_string_values.openapi.yaml")]
mod m {}

use m::MApi as _;

#[derive(Clone)]
struct EchoServer;

impl m::MApi for EchoServer {
    type Error = m::NotImplemented;

    async fn echo(&self, ctx: m::echo::Context) -> Result<m::echo::Response, Self::Error> {
        let req = ctx.req;
        Ok(m::echo::Response::Status200(m::Echo {
            s: req.s,
            opt: req.opt,
            tags: req.tags,
            csv: req.csv,
            code: req.x_code,
            n: req.n,
            color: match req.color {
                m::echo::ColorQuery::Red => m::Color::Red,
                m::echo::ColorQuery::Green => m::Color::Green,
            },
        }))
    }
}

async fn spawn() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, EchoServer.router()).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn string_values_arrive_exactly_as_sent() {
    let base = spawn().await;
    let client = reqwest::Client::new();
    for value in [r#""hi""#, "123", "true", "null", "plain", "", r#"{"a":1}"#] {
        let csv = format!("{value},x");
        let response = client
            .get(format!("{base}/echo"))
            .query(&[
                ("s", value),
                ("opt", value),
                ("tags", value),
                ("tags", "x"),
                ("csv", &csv),
                ("n", "5"),
                ("color", "green"),
            ])
            .header("X-Code", value)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "value {value:?}");
        let echo: m::Echo = response.json().await.unwrap();
        assert_eq!(
            echo,
            m::Echo {
                s: value.into(),
                opt: Some(value.into()),
                tags: vec![value.into(), "x".into()],
                csv: vec![value.into(), "x".into()],
                code: value.into(),
                n: 5,
                color: m::Color::Green,
            },
            "value {value:?}"
        );
    }
}

#[tokio::test]
async fn a_quoted_number_is_rejected() {
    let base = spawn().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/echo"))
        .query(&[
            ("s", "x"),
            ("tags", "x"),
            ("csv", "x"),
            ("n", r#""5""#),
            ("color", "red"),
        ])
        .header("X-Code", "c")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
