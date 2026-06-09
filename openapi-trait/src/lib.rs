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
//!         _auth: petstore::ApiKey,
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

/// Per-request transport options applied on top of the operation's own
/// parameters.
///
/// Every generated client method takes a `RequestOptions` argument, letting you
/// attach extra HTTP headers or authentication to a single request without
/// re-instantiating the underlying client. Pass [`RequestOptions::default`]
/// (or [`RequestOptions::new`]) when you have nothing to add.
///
/// The builder methods are chainable:
///
/// ```rust
/// # #[cfg(feature = "reqwest-client")] {
/// let options = openapi_trait::RequestOptions::new()
///     .bearer_auth("token-123")
///     .header("X-Request-Id", "abc");
/// # let _ = options;
/// # }
/// ```
///
/// Options are applied after the operation's declared headers, so a header set
/// here is sent in addition to (and after) any same-named operation header.
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    /// Extra headers, applied in insertion order.
    headers: Vec<(String, String)>,
    /// `Authorization: Bearer <token>` to attach, if any.
    bearer_token: Option<String>,
    /// `Authorization: Basic` credentials (username, optional password).
    basic_auth: Option<(String, Option<String>)>,
}

impl RequestOptions {
    /// Create an empty set of request options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an extra header to the request.
    ///
    /// Invalid header names or values are surfaced by reqwest when the request
    /// is sent, matching `reqwest::RequestBuilder::header` semantics.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Attach an `Authorization: Bearer <token>` header to the request.
    #[must_use]
    pub fn bearer_auth(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    /// Attach `Authorization: Basic` credentials to the request.
    #[must_use]
    pub fn basic_auth(mut self, username: impl Into<String>, password: Option<String>) -> Self {
        self.basic_auth = Some((username.into(), password));
        self
    }

    /// Apply these options to a reqwest request builder.
    ///
    /// Used by the generated reqwest-backed client implementation; you should
    /// not normally need to call it directly.
    #[cfg(feature = "reqwest-client")]
    pub fn apply(self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (name, value) in self.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if let Some(token) = self.bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some((username, password)) = self.basic_auth {
            request = request.basic_auth(username, password);
        }
        request
    }
}

/// Sibling of [`ReqwestClientCore`] for clients that carry credentials.
///
/// Implemented automatically by [`ReqwestClient`] when the carrier struct has
/// a field annotated `#[openapi_trait(auth)]` (or named `auth`). The generic
/// `A` is the generated `{Mod}AuthState` struct for the spec.
#[cfg(feature = "reqwest-client")]
pub trait ReqwestClientAuth<A> {
    /// Borrow the auth-state struct holding configured credentials.
    fn auth_state(&self) -> &A;
}

#[cfg(feature = "reqwest-client")]
pub use percent_encoding;
#[cfg(feature = "reqwest-client")]
pub use reqwest;

pub use base64;
