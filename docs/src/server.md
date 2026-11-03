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

Each method receives one argument, the operation's `{operation}::Context`
(see [below](#the-handlers-context)), and returns
`Result<{operation}::Response, Self::Error>`. The response enum has one
variant per declared status code (`Status200(Pet)`, `Status404`, …), and each
variant implements `IntoResponse` with the matching status and body.

## The handler's context

`{operation}::Context` holds everything the router extracted for one request:

| Field | Type | Present |
|---|---|---|
| `req` | `{operation}::Request`: path, query and header parameters as typed fields, and the request body as `body` | always |
| `auth` | the credentials, see [Security](security.md) | only if the operation is secured |
| `headers` | `axum::http::HeaderMap`, for anything the spec doesn't model | always |

Spec parameters always live in `req`, never on the context itself, so a
parameter called `auth` or `headers` can't clash with these fields.

There are several ways to get at the fields. They all compile to the same
thing, so pick whichever reads best for the handler.

### Field access

Read and move fields straight off `ctx`. Moving one field out, such as the
body, leaves the others usable:

```rust,ignore
async fn create_pet(
    &self,
    ctx: pets::create_pet::Context,
) -> Result<pets::create_pet::Response, Self::Error> {
    let body = ctx.req.body;                     // moves the body out
    let trace = ctx.headers.get("x-trace-id");   // other fields still work
    if ctx.auth.0 != "secret" { /* … */ }
    // …
}
```

### Destructuring in the body

Name only the fields you need and end the pattern with `..`:

```rust,ignore
let pets::create_pet::Context { req, auth, .. } = ctx;
```

### Destructuring in the signature

A pattern can replace the argument name. This keeps the body free of
unpacking, at the cost of a longer signature:

```rust,ignore
async fn get_pet(
    &self,
    pets::get_pet::Context { req, .. }: pets::get_pet::Context,
) -> Result<pets::get_pet::Response, Self::Error> {
    // `req` is in scope here
}
```

### Nested destructuring

Patterns nest, so you can unpack the request's parameters in the same step:

```rust,ignore
let pets::list_pets::Context {
    req: pets::list_pets::Request { limit, .. },
    headers,
    ..
} = ctx;
```

`Request` has no private field, so `..` is optional inside it. Leave it in
anyway: without it, the pattern breaks when the spec gains a parameter.

### Why `..` is required

`Context` has a private field. Outside the generated module, a pattern
without `..` is rejected (`pattern requires .. due to inaccessible fields`),
and so is a struct literal. That's deliberate: it lets new fields, such as the
request URI, be added to `Context` without breaking your handlers.

### No `Deref` to the request

`Context` doesn't implement `Deref<Target = Request>`, so you write
`ctx.req.pet_id`, not `ctx.pet_id`. `Deref` would save typing `.req` when
reading a field, but nothing else:

| With `Deref` | Result |
|---|---|
| `ctx.pet_id` (read a `Copy` field) | works |
| `&ctx.body` (borrow) | works |
| `let body = ctx.body;` (move) | error: cannot move out of a dereference |
| `let Context { pet_id, .. } = ctx;` (pattern) | error: patterns never look through `Deref` |
| `ctx.headers` when the spec has a `headers` parameter | compiles, but silently reads the context's field, not the parameter |

The last row is the reason it's left out: `Deref` would let a spec parameter
be shadowed without any warning. With `ctx.req.…` it's always clear which
one you mean.

### Calling a handler directly

To call a handler without going through HTTP, for example in a unit test,
build a context with `Context::new`. It takes the request, then the
credentials (only for secured operations), then the headers:

```rust,ignore
let ctx = pets::get_pet::Context::new(
    pets::get_pet::Request { pet_id: 1 },
    pets::BearerAuth("secret".into()),
    axum::http::HeaderMap::new(),
);
let response = PetServer::default().get_pet(ctx).await?;
```

Call it through the trait (`use pets::PetsApi as _;`). The method has no
inherent version.

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
