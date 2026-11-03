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
to add them yourself: `chrono`, `uuid`, and, with the matching feature,
`reqwest` and `serde_valid`.

Your crate still needs the dependencies that generated code uses directly:
`serde` (with `derive`), `serde_json`, and `axum` for servers.

## Minimum Rust version

Rust **1.75**, the first release with `async fn` in traits.

## Changelog

Release notes are in
[`openapi-trait/CHANGELOG.md`](https://github.com/ndrsg/openapi-trait/blob/main/openapi-trait/CHANGELOG.md)
and on the
[GitHub releases page](https://github.com/ndrsg/openapi-trait/releases).
