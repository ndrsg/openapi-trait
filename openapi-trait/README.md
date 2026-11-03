# openapi-trait

[![Build status](https://github.com/ndrsg/openapi-trait/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/ndrsg/openapi-trait/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/openapi-trait)](https://crates.io/crates/openapi-trait)
[![Documentation](https://docs.rs/openapi-trait/badge.svg)](https://docs.rs/openapi-trait)
[![Guide](https://img.shields.io/badge/guide-mdBook-blue)](https://ndrsg.github.io/openapi-trait/)

Turn an OpenAPI spec into **Rust traits at compile time**. Implement the trait
to get a typed [axum](https://github.com/tokio-rs/axum) server, or use the
generated trait as a transport-agnostic client with a ready-made `reqwest`
implementation. You don't check in generated files or write a `build.rs`, and
the code always matches the spec.

📖 **[Read the guide](https://ndrsg.github.io/openapi-trait/)** · 📚 [API reference](https://docs.rs/openapi-trait)

```toml
[dependencies]
openapi-trait = "0.0.9"
```

## Server

```rust
#[openapi_trait::axum("openapi/pets.yaml")]
pub mod pets {}

#[derive(Clone)]
struct PetServer;

impl pets::PetsApi for PetServer {
    type Error = pets::NotImplemented;

    async fn get_pet(
        &self,
        req: pets::get_pet::Request,
        _auth: pets::BearerAuth,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pets::get_pet::Response, Self::Error> {
        Ok(pets::get_pet::Response::Status200(pets::Pet {
            id: req.pet_id,
            name: "doggie".into(),
            status: None,
        }))
    }
    // Operations you don't implement yet answer `500 not implemented`.
}

use pets::PetsApi as _;
let app: axum::Router = PetServer.router().with_state(());
```

## Client

```rust
#[openapi_trait::client("openapi/pets.yaml")]
pub mod pets {}

#[derive(Clone, openapi_trait::ReqwestClient)]
struct PetsHttpClient {
    #[openapi_trait(client)]
    http: openapi_trait::reqwest::Client,
    #[openapi_trait(base_url)]
    base_url: String,
    #[openapi_trait(auth)]
    auth: pets::PetsAuthState,
}

use pets::{PetsClient as _, PetsClientAuth as _};

let client = PetsHttpClient {
    http: openapi_trait::reqwest::Client::new(),
    base_url: "http://localhost:3000".into(),
    auth: Default::default(),
}
.with_bearer_auth("secret");

let pet = client.get_pet(pets::get_pet::Request { pet_id: 1 }, None).await?;
```

`pets::PetsClient` is a plain trait, so you can also implement it yourself as a
test double or on top of another transport.

Runnable versions of both examples are in
[`openapi-trait/examples`](https://github.com/ndrsg/openapi-trait/tree/main/openapi-trait/examples).

## Features

- **Models.** Structs and enums from `components/schemas`, including
  `oneOf`/`anyOf`/`allOf`, `additionalProperties`, and `date-time`/`uuid`
  formats.
- **Typed operations.** A module per operation with a `Request` struct for
  path, query, header and body inputs, and a `Response` enum with one variant
  per status code.
- **Security.** `apiKey`, HTTP bearer and HTTP basic. On the server,
  credentials are extracted and missing ones get a `401`. On the client, they
  are injected into every call.
- **Streaming.** Binary request and response bodies are streamed, never
  buffered.
- **Validation (opt-in).** Schema constraints become
  [`serde_valid`](https://docs.rs/serde_valid) rules.
- **Go-to-definition.** Add `source_file`
  (`#[openapi_trait::axum("openapi/pets.yaml", source_file)]`) and F12 in your
  IDE opens the generated code. It is written to a self-gitignored
  `.openapi-trait/` folder ([details](https://ndrsg.github.io/openapi-trait/go-to-definition.html)).
- **Debug dump.** Set `OPENAPI_TRAIT_DEBUG=1` to write the generated code to
  disk.

See the [OpenAPI support matrix](https://ndrsg.github.io/openapi-trait/openapi-support.html)
for details.

## License

MIT
