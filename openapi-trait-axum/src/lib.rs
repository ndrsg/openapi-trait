//! Axum integration for `openapi-trait`.
//!
//! This crate re-exports the [`openapi_trait`] attribute macro pre-configured
//! for use with the [`axum`](https://docs.rs/axum) web framework. The generated
//! `router` function returns an [`axum::Router`](https://docs.rs/axum/latest/axum/struct.Router.html).
//!
//! # Quick start
//!
//! ```rust,ignore
//! use openapi_trait_axum::openapi_trait;
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
//!
//! let app: axum::Router = petstore::router(MyServer);
//! ```

/// The `openapi_trait` attribute macro, re-exported for axum users.
///
/// Re-exported from [`openapi_trait`]. See its documentation for full usage.
#[doc(inline)]
pub use openapi_trait::openapi_trait;
