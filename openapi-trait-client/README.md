# openapi-trait-client

Transport-agnostic client procedural macro backend for `openapi-trait`.

This crate is not intended for direct use. In normal projects, depend on
[`openapi-trait`](https://docs.rs/openapi-trait) and use the re-exported macro:

```rust
#[openapi_trait::client("openapi/petstore.yaml")]
pub mod petstore {}
```

## What it generates

Given an annotated `mod`, the macro reads an OpenAPI document at compile time
and replaces the module contents with:

- `serde`-derived Rust structs for `components/schemas`
- `{OperationId}Request` structs for path, query, header, and body inputs
- `{OperationId}Response` enums whose variants map to HTTP status codes
- A `{ModName}Client` trait with one method per `operationId`

The generated trait name comes from the annotated module name, so `mod petstore
{}` produces `petstore::PetstoreClient`.

## Reqwest derive support

This crate also defines the `ReqwestClient` derive that powers the
feature-gated blanket reqwest implementation re-exported from `openapi-trait`.
By default, the derive expects named fields called `client` and `base_url`, and
those defaults can be overridden with:

- `#[openapi_trait(client)]`
- `#[openapi_trait(base_url)]`

## Path resolution

The macro resolves the OpenAPI file path relative to `CARGO_MANIFEST_DIR` and
uses `include_str!` so Cargo recompiles the crate when the spec changes.

## Crate role

This crate isolates client-side code generation and optional reqwest-backed
integration so the public `openapi-trait` crate can expose a single entry point
without making the proc-macro implementation details part of its API surface.
