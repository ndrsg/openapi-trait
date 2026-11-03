# Implementing a server

`#[openapi_trait::axum(...)]` generates a `{Mod}Api` trait with one async
method per operation, plus a `router()` method that wires it to an
`axum::Router`.

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:macro}}
```

## Implement the trait

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:impl}}
```

Each method receives one argument, the operation's `{operation}::Context`:

- **`req`**, the operation's `{operation}::Request` struct. It holds path,
  query and header parameters as typed fields, and the request body as
  `body`.
- **`auth`**, only if the operation is secured. See [Security](security.md).
- **`headers`**, the raw `axum::http::HeaderMap`, for anything the spec
  doesn't model.

Use the fields directly (`ctx.req.pet_id`) or destructure what you need:

```rust,ignore
let pets::get_pet::Context { req, auth, .. } = ctx;
```

The `..` is required: `Context` has a private field, so new fields can be
added without breaking your code. To call a handler directly, for example
in a unit test, build one with `Context::new(req, auth, headers)` (`auth`
only for secured operations).

It returns `Result<{operation}::Response, Self::Error>`. The response enum has
one variant per declared status code (`Status200(Pet)`, `Status404`, …), and
each variant implements `IntoResponse` with the matching status and body.

## Application state

Keep application state in the type that implements the trait, and reach it
through `&self`. The router wraps the implementation in an `Arc`, so it is
shared, not cloned, per request.

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:state}}
```

## Start the router

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:main}}
```

`router()` returns a plain `axum::Router`, so you can `.merge()` it, `.nest()`
it, or add tower layers like any other router. To combine it with routes that
use axum state, convert it with `.with_state(())`, which yields a
`Router<S>` for any `S`:

```rust,ignore
let app = axum::Router::new()
    .route("/health", get(health))
    .merge(PetServer::default().router().with_state(()))
    .with_state(app_state);
```

## Unimplemented operations and errors

Every trait method has a default body that returns
`Err(NotImplemented.into())`. You can implement operations one at a time;
routes you haven't implemented yet answer `500 not implemented`.

`Self::Error` must implement `axum::response::IntoResponse` and
`From<pets::NotImplemented>`. Using `pets::NotImplemented` itself is the
simplest choice. For real error handling, use your own type:

```rust,ignore
enum AppError {
    NotImplemented,
    Database(sqlx::Error),
}

impl From<pets::NotImplemented> for AppError {
    fn from(_: pets::NotImplemented) -> Self { Self::NotImplemented }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        // map to a status code and body of your choice
        # todo!()
    }
}
```

An `Err` is for failures *outside* the contract. Responses the spec declares,
such as `404`, belong in the `Ok` variant (`Ok(get_pet::Response::Status404)`).

## Binary bodies

When an operation takes or returns `application/octet-stream` (or another
binary media type), the body is a streamed `axum::body::Body` rather than a
`Vec<u8>`, so large uploads and downloads don't have to fit in memory. See
[OpenAPI support](openapi-support.md).
