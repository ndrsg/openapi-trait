# openapi-trait-axum

Axum-specific procedural macro backend for `openapi-trait`.

This crate is not intended for direct use. In normal projects, depend on
[`openapi-trait`](https://docs.rs/openapi-trait) and use the re-exported
attribute macro:

```rust
#[openapi_trait::axum("openapi/petstore.yaml")]
pub mod petstore {}
```

## What it generates

Given an annotated `mod`, the macro reads an OpenAPI document at compile time
and replaces the module contents with:

- `serde`-derived Rust structs for `components/schemas`
- A module per operation (`get_pet`) with a `Request` struct for path, query,
  header, and body inputs and a `Response` enum implementing
  `axum::response::IntoResponse`
- A `{ModName}Api` trait with one method per `operationId`, each taking a
  `{operation}::Context`
- A `router()` helper that builds an `axum::Router<S>` for the implementation

The generated trait name comes from the annotated module name, so `mod petstore
{}` produces `petstore::PetstoreApi`.

## Path resolution

The macro resolves the OpenAPI file path relative to `CARGO_MANIFEST_DIR` and
uses `include_str!` so Cargo recompiles the crate when the spec changes.

## Errors

Macro expansion fails at compile time when:

- the OpenAPI file cannot be read
- the document cannot be parsed as OpenAPI YAML or JSON
- an operation is missing an `operationId`

## Crate role

This crate exists so the public `openapi-trait` crate can keep a small,
ergonomic surface while the axum-specific code generation stays isolated.
