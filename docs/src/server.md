# Implementing a server

`#[openapi_trait::axum(...)]` generates a `{Mod}Api<S>` trait with one async
method per operation, plus a `router()` method that wires it to an
`axum::Router`.

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:macro}}
```

## Implement the trait

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:impl}}
```

Each method receives, in order:

1. **`req`**, the operation's `{OperationId}Request` struct. It holds path,
   query and header parameters as typed fields, and the request body as
   `body`.
2. **`auth`**, only if the operation is secured. See [Security](security.md).
3. **`state`**, an `axum::extract::State<S>` for your application state.
4. **`headers`**, the raw `axum::http::HeaderMap`, for anything the spec
   doesn't model.

It returns `Result<{OperationId}Response, Self::Error>`. The response enum has
one variant per declared status code (`Status200(Pet)`, `Status404`, …), and
each variant implements `IntoResponse` with the matching status and body.

## Application state

The type parameter `S` of `PetsApi<S>` is your axum state type. It defaults to
`()`, so stateless servers can write `impl pets::PetsApi for MyServer`.

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:state}}
```

## Start the router

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:main}}
```

`router()` returns a plain `axum::Router<S>`, so you can `.merge()` it, `.nest()`
it, or add tower layers like any other router.

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
such as `404`, belong in the `Ok` variant (`Ok(GetPetResponse::Status404)`).

## Binary bodies

When an operation takes or returns `application/octet-stream` (or another
binary media type), the body is a streamed `axum::body::Body` rather than a
`Vec<u8>`, so large uploads and downloads don't have to fit in memory. See
[OpenAPI support](openapi-support.md).
