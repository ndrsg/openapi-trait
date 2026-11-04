# Crates and features

## Which crate to depend on

Depend on **`openapi-trait`** only. The other crates are internal and don't
promise a stable API.

| Crate | Role |
|---|---|
| [`openapi-trait`](https://crates.io/crates/openapi-trait) | Public entry point: re-exports the macros and holds the runtime support types (`RequestOptions`, `ByteStream`, …) |
| [`openapi-trait-axum`](https://crates.io/crates/openapi-trait-axum) | Proc-macro for the `axum` attribute |
| [`openapi-trait-client`](https://crates.io/crates/openapi-trait-client) | Proc-macros for the `client` attribute and `ReqwestClient` derive |
| [`openapi-trait-shared`](https://crates.io/crates/openapi-trait-shared) | Framework-agnostic code generation used by the macros above |

## Cargo features

| Feature | Default | Effect |
|---|:-:|---|
| `reqwest-client` | ✅ | Blanket reqwest client implementation, `#[derive(ReqwestClient)]`, `ByteStream`, and the `reqwest` re-export |
| `validation` | | `serde_valid::Validate` on generated models; see [Validation](validation.md) |

## Re-exports

Generated code refers to these through `openapi_trait::…`, so you don't have
to add them yourself: `serde`, `serde_json`, `chrono`, `uuid`, and, with the
matching feature, `reqwest`. Generated types carry
`#[serde(crate = "::openapi_trait::serde")]`, so their derives work without a
`serde` dependency.

Servers still need `axum` as a dependency, so you pick its version. With the
`validation` feature, you also need `serde_valid = "2"`: it is re-exported as
`openapi_trait::serde_valid`, but its derive only works when the crate is a
direct dependency (see [Validation](validation.md)).

## Minimum Rust version

Rust **1.89**.

## Changelog

Release notes are in
[`openapi-trait/CHANGELOG.md`](https://github.com/ndrsg/openapi-trait/blob/main/openapi-trait/CHANGELOG.md)
and on the
[GitHub releases page](https://github.com/ndrsg/openapi-trait/releases).
