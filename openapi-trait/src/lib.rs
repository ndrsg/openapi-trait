//! Generate typed Rust traits from `OpenAPI` specifications.
//!
//! This crate exposes the [`openapi_trait`] attribute macro, which reads an
//! `OpenAPI` 3.0 or 3.1 specification file at compile time and generates:
//!
//! - Rust structs for every schema defined in `components/schemas`
//! - A response enum per operation whose variants map to HTTP status codes
//! - A trait with one `async fn` per operation, identified by `operationId`
//! - A `router` function that wires a trait implementation to an HTTP router
//!
//! # Quick start
//!
//! ```rust,ignore
//! use openapi_trait::openapi_trait;
//!
//! #[openapi_trait("openapi/petstore.yaml")]
//! pub mod petstore {}
//!
//! struct MyServer;
//!
//! impl petstore::PetstoreApi for MyServer {
//!     async fn list_pets(
//!         &self,
//!         limit: Option<i32>,
//!     ) -> petstore::ListPetsResponse {
//!         petstore::ListPetsResponse::Ok200(vec![])
//!     }
//! }
//! ```
//!
//! See the [`openapi_trait`] macro documentation for the full list of
//! supported arguments and generated items.

/// The `openapi_trait` attribute macro.
///
/// Re-exported from [`openapi_trait_macros`]. See its documentation for
/// detailed usage.
#[doc(inline)]
pub use openapi_trait_macros::openapi_trait;
