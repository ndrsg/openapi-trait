# openapi-trait

A Rust proc-macro attribute that reads an OpenAPI 3.0 or 3.1 specification file
at **compile time** and generates a typed Rust trait from it, so you can implement
your API server with full type safety and no boilerplate.

```rust
use openapi_trait::openapi_trait;

#[openapi_trait("openapi/petstore.yaml")]
pub mod petstore {}

struct MyServer;

impl petstore::PetstoreApi for MyServer {
    async fn list_pets(&self, limit: Option<i32>) -> petstore::ListPetsResponse {
        petstore::ListPetsResponse::Ok200(vec![])
    }
}

// Wire up an axum router — generated automatically.
let app: axum::Router = petstore::router(MyServer);
```

## What gets generated

For every OpenAPI spec the macro emits inside the target module:

| Generated item | Source |
|---|---|
| Structs with `serde` derives | `components/schemas` |
| Per-operation response enums | HTTP status codes per operation |
| `impl axum::response::IntoResponse` | For every response enum |
| `{Title}Api` trait | One `async fn` per `operationId` |
| `router<T: {Title}Api>(api: T) -> axum::Router` | Wires all operations to axum routes |

## Crates

| Crate | Purpose |
|---|---|
| [`openapi-trait`](openapi-trait/) | Main entry point — add this to your `Cargo.toml` |
| [`openapi-trait-axum`](openapi-trait-axum/) | Axum-specific re-export with axum in scope |
| [`openapi-trait-macros`](openapi-trait-macros/) | Proc-macro internals — not for direct use |

## Usage

Add to `Cargo.toml`:

```toml
[dependencies]
openapi-trait = "0.1"
```

Or if you prefer the axum-scoped re-export:

```toml
[dependencies]
openapi-trait-axum = "0.1"
```

Then apply the macro to a `mod` block:

```rust
use openapi_trait::openapi_trait;

#[openapi_trait("openapi/petstore.yaml")]
pub mod petstore {}
```

The path is resolved relative to the crate root (`CARGO_MANIFEST_DIR`). The file
is tracked by Cargo — the crate recompiles automatically when the spec changes.

### Explicit backend

The default backend is axum. You can make it explicit:

```rust
#[openapi_trait("openapi/petstore.yaml", backend = "axum")]
pub mod petstore {}
```

## OpenAPI support

| Feature | Status |
|---|---|
| OpenAPI 3.0.x | Planned |
| OpenAPI 3.1.x | Planned |
| `components/schemas` → structs | Planned |
| Path / query parameters | Planned |
| Request bodies | Planned |
| Response enums | Planned |
| `allOf` / `anyOf` / `oneOf` | Planned (placeholder type initially) |
| Security schemes | Not planned for v0.1 |

## Extending to other frameworks

The internal `CodegenBackend` trait (in `openapi-trait-macros`) is the extension
point. Adding a new framework requires implementing that trait and registering the
backend name. See `openapi-trait-macros/src/codegen/` once implemented.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
