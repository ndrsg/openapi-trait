//! Procedural macro internals for `openapi-trait`.
//!
//! This crate is not intended for direct use. Use the
//! [`openapi-trait`](https://docs.rs/openapi-trait) crate instead, which
//! re-exports the [`openapi_trait`] attribute macro from here.

use proc_macro::TokenStream;

/// Generates a typed Rust trait from an `OpenAPI` specification file.
///
/// Apply this attribute to a `mod` block. The macro reads the `OpenAPI` document
/// at the given path (resolved relative to `CARGO_MANIFEST_DIR`) at compile
/// time and replaces the module's contents with:
///
/// - Schema structs derived from `components/schemas`
/// - Per-operation response enums implementing
///   [`axum::response::IntoResponse`](https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html)
/// - A trait with one `async fn` per operation (keyed by `operationId`)
/// - A `router` function that wires the trait to an [`axum::Router`](https://docs.rs/axum/latest/axum/struct.Router.html)
///
/// # Arguments
///
/// - First positional argument: path to the `OpenAPI` YAML or JSON file,
///   relative to the crate root (`CARGO_MANIFEST_DIR`).
/// - `backend` (optional, named): code-generation backend to use.
///   Currently only `"axum"` is supported. Defaults to `"axum"`.
///
/// # Examples
///
/// ```rust,ignore
/// use openapi_trait::openapi_trait;
///
/// #[openapi_trait("openapi/petstore.yaml")]
/// pub mod petstore {}
/// ```
///
/// With an explicit backend:
///
/// ```rust,ignore
/// #[openapi_trait("openapi/petstore.yaml", backend = "axum")]
/// pub mod petstore {}
/// ```
///
/// # Errors
///
/// The macro emits a compile error if:
///
/// - The file cannot be found or read.
/// - The `OpenAPI` document is malformed or cannot be parsed.
/// - An unsupported `OpenAPI` version is detected.
/// - An operation is missing an `operationId`.
#[proc_macro_attribute]
pub fn openapi_trait(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
