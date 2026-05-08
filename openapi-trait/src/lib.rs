//! Generate typed Rust traits from `OpenAPI` specifications.
//!
//! This crate exposes the [`axum`] and [`client`] attribute macros, which read
//! an `OpenAPI` specification file at compile time and generate inside the
//! annotated `mod`.

#[doc(inline)]
pub use openapi_trait_axum::openapi_trait as axum;

#[doc(inline)]
pub use openapi_trait_client::openapi_trait as client;

/// Derive support for user-owned reqwest client carrier structs.
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
