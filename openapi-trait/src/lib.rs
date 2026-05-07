//! Generate typed Rust traits from `OpenAPI` specifications.
//!
//! This crate exposes the [`axum`] attribute macro, which reads an
//! `OpenAPI` specification file at compile time and generates inside the
//! annotated `mod`:
//!
//! - Rust structs for every schema defined in `components/schemas`
//! - A `{OperationId}Request` struct per operation that bundles all parameters
//!   and the request body
//! - A `{OperationId}Response` enum per operation whose variants map to HTTP
//!   status codes
//! - `impl axum::response::IntoResponse` for every response enum
//! - A `{Title}Api` trait with one `async fn` per operation, keyed by
//!   `operationId`
//! - A `router` method on the trait that wires all operations to an
//!   [`axum::Router`](https://docs.rs/axum/latest/axum/struct.Router.html)
//!
//! # Quick start
//!
//! ```rust,ignore
//! #[openapi_trait::axum("openapi/petstore.yaml")]
//! pub mod petstore {}
//!
//! #[derive(Clone)]
//! struct MyServer;
//!
//! impl petstore::PetstoreApi for MyServer {
//!     type Error = std::convert::Infallible;
//!
//!     async fn list_pets(
//!         &self,
//!         req: petstore::ListPetsRequest,
//!         state: axum::extract::State<()>,
//!         headers: axum::http::HeaderMap,
//!     ) -> Result<petstore::ListPetsResponse, Self::Error> {
//!         Ok(petstore::ListPetsResponse::Status200(vec![]))
//!     }
//! }
//!
//! // Wire up an axum router.
//! let app = MyServer.router().with_state(());
//! ```
//!
//! See the [`axum`] macro documentation for the full list of generated items
//! and compile-time error conditions.

/// The `openapi_trait` attribute macro, wired to the axum backend.
///
/// Re-exported from [`openapi_trait_axum`]. Apply it to a `mod` block with a
/// path to an `OpenAPI` YAML or JSON file. The path is resolved relative to
/// `CARGO_MANIFEST_DIR`.
#[doc(inline)]
pub use openapi_trait_axum::openapi_trait as axum;
