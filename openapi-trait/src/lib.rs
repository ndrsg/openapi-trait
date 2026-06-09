//! Generate typed Rust traits from `OpenAPI` specifications.
//!
//! This crate exposes the [`axum`] and [`client`] attribute macros, which read
//! an `OpenAPI` specification file at compile time and generate inside the
//! annotated `mod`.
//!
//! # Examples
//!
//! ```rust
//! #[openapi_trait::axum("assets/testdata/petstore.openapi.yaml")]
//! pub mod petstore {}
//!
//! use petstore::PetstoreApi as _;
//!
//! #[derive(Clone)]
//! struct MyServer;
//!
//! #[derive(Clone)]
//! struct AppState;
//!
//! impl petstore::PetstoreApi<AppState> for MyServer {
//!     type Error = petstore::NotImplemented;
//!
//!     async fn get_pet_by_id(
//!         &self,
//!         req: petstore::GetPetByIdRequest,
//!         _state: axum::extract::State<AppState>,
//!         _headers: axum::http::HeaderMap,
//!     ) -> Result<petstore::GetPetByIdResponse, Self::Error> {
//!         Ok(petstore::GetPetByIdResponse::Status200(petstore::Pet {
//!             id: Some(req.pet_id),
//!             name: "doggie".into(),
//!             photo_urls: vec![],
//!             category: None,
//!             tags: None,
//!             status: None,
//!         }))
//!     }
//! }
//!
//! let app: axum::Router = MyServer.router().with_state(AppState);
//! ```
//!
//! The generated trait names come from the annotated module name, so `mod petstore {}`
//! produces `petstore::PetstoreApi` and `petstore::PetstoreClient`.
//!
//! The `reqwest-client` feature is enabled by default. It adds [`ReqwestClient`],
//! [`ReqwestClientCore`], and the [`reqwest`] re-export used by the generated blanket
//! client implementation.

#[doc(inline)]
pub use openapi_trait_axum::openapi_trait as axum;

#[doc(inline)]
pub use openapi_trait_client::openapi_trait as client;

/// Derive support for user-owned reqwest client carrier structs.
///
/// The derive looks for fields named `client` and `base_url` by default.
/// Override those conventions with `#[openapi_trait(client)]` and
/// `#[openapi_trait(base_url)]` on the corresponding fields.
#[cfg(feature = "reqwest-client")]
#[doc(inline)]
pub use openapi_trait_client::ReqwestClient;

/// Shared accessors used by generated reqwest client implementations.
#[cfg(feature = "reqwest-client")]
pub trait ReqwestClientCore {
    /// Return the reqwest client used for outbound requests.
    fn reqwest_client(&self) -> &reqwest::Client;

    /// Return the base URL prepended to generated operation paths.
    fn base_url(&self) -> &str;
}

#[cfg(feature = "reqwest-client")]
pub use percent_encoding;
#[cfg(feature = "reqwest-client")]
pub use reqwest;
