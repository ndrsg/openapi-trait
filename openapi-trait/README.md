# openapi-trait

A Rust proc-macro attribute that reads an OpenAPI specification file at
**compile time** and generates a typed Rust trait from it, so you can implement
your API server with full type safety and no boilerplate.

```rust
#[openapi_trait::axum("openapi/petstore.yaml")]
pub mod petstore {}

#[derive(Clone)]
struct MyServer;

impl petstore::PetstoreApi for MyServer {
    type Error = std::convert::Infallible;

    async fn get_pet_by_id(
        &self,
        req: petstore::GetPetByIdRequest,
        _state: axum::extract::State<()>,
        _headers: axum::http::HeaderMap,
    ) -> Result<petstore::GetPetByIdResponse, Self::Error> {
        Ok(petstore::GetPetByIdResponse::Status200(petstore::Pet {
            id: Some(req.pet_id),
            name: "doggie".into(),
            photo_urls: vec![],
            category: None,
            tags: None,
            status: None,
        }))
    }
}

// Wire up an axum router.
let app = MyServer.router().with_state(());
```

## What gets generated

For every OpenAPI spec the macro emits inside the target module:

| Generated item | Source |
|---|---|
| Structs with `serde` derives | `components/schemas` |
| `{OperationId}Request` structs | Path, query, header params + request body per operation |
| Per-operation `{OperationId}Response` enums | HTTP status codes per operation |
| `impl axum::response::IntoResponse` | For every response enum |
| `{Title}Api` trait | One `async fn` per `operationId` (default impl returns 500) |
| `router` method on the trait | Wires all operations to an `axum::Router` |

## Crates

| Crate | Purpose |
|---|---|
| [`openapi-trait`](openapi-trait/) | Main entry point — add this to your `Cargo.toml` |
| [`openapi-trait-axum`](openapi-trait-axum/) | Axum proc-macro — not for direct use |
| [`openapi-trait-shared`](openapi-trait-shared/) | Framework-agnostic codegen helpers — not for direct use |

## Usage

Add to `Cargo.toml`:

```toml
[dependencies]
openapi-trait = "0.1"
```

Then apply the macro to a `mod` block:

```rust
#[openapi_trait::axum("openapi/petstore.yaml")]
pub mod petstore {}
```

The path is resolved relative to the crate root (`CARGO_MANIFEST_DIR`). The
file is tracked by Cargo — the crate recompiles automatically when the spec
changes.

## OpenAPI support

| Feature | Status |
|---|---|
| `components/schemas` → structs | ✅ |
| Path parameters | ✅ |
| Query parameters (including string enums) | ✅ |
| Header parameters | ✅ |
| Request bodies (JSON) | ✅ |
| Response enums per operation | ✅ |
| `allOf` / `anyOf` / `oneOf` | Partial — falls back to `serde_json::Value` |
| Security schemes | Not planned for v0.1 |
