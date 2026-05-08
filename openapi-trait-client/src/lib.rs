//! Client backend proc-macro for `openapi-trait`.
//!
//! This crate is not intended for direct use. Use the
//! [`openapi-trait`](https://docs.rs/openapi-trait) crate instead, which
//! re-exports the [`openapi_trait`] attribute macro from here as
//! `openapi_trait::client`.

mod codegen;
mod reqwest_derive;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, ItemMod, LitStr};

/// Generates transport-agnostic Rust client traits from an `OpenAPI` specification file.
///
/// Apply this attribute to a `mod` block. The macro reads the `OpenAPI`
/// document at the given path (resolved relative to `CARGO_MANIFEST_DIR`) at
/// compile time and replaces the module's contents with:
///
/// - Schema structs derived from `components/schemas`
/// - A `{OperationId}Request` struct per operation (bundles path, query,
///   header params and the request body)
/// - Per-operation `{OperationId}Response` enums whose variants map to HTTP
///   status codes
/// - A `{ModName}Client` trait with one method per operation (keyed by
///   `operationId`)
/// - When the `reqwest-client` cargo feature is enabled through
///   `openapi-trait`, a blanket reqwest-backed implementation for any user
///   type deriving [`ReqwestClient`]
///
/// The generated trait name is derived from the annotated module name, so
/// `mod petstore {}` produces `petstore::PetstoreClient`.
#[proc_macro_attribute]
pub fn openapi_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(attr as LitStr);
    run_macro(path_lit, item, cfg!(feature = "reqwest-client"))
}

/// Derive the reqwest client carrier trait for a user-owned struct.
///
/// By default, the derive expects named fields called `client` and `base_url`.
/// You can override those conventions with field attributes:
/// `#[openapi_trait(client)]` and `#[openapi_trait(base_url)]`.
#[proc_macro_derive(ReqwestClient, attributes(openapi_trait))]
pub fn derive_reqwest_client(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    match reqwest_derive::expand_reqwest_client(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn run_macro(path_lit: LitStr, item: TokenStream, include_reqwest: bool) -> TokenStream {
    let module = parse_macro_input!(item as ItemMod);
    let mod_ident = &module.ident;
    let mod_vis = &module.vis;

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(value) => value,
        Err(_) => {
            return syn::Error::new(
                Span::call_site(),
                "CARGO_MANIFEST_DIR is not set; cannot resolve spec path",
            )
            .to_compile_error()
            .into();
        }
    };

    let spec_path = std::path::PathBuf::from(&manifest_dir).join(path_lit.value());
    let spec_path_str = spec_path.to_string_lossy().into_owned();

    let content = match std::fs::read_to_string(&spec_path) {
        Ok(value) => value,
        Err(error) => {
            let msg = format!("cannot read OpenAPI spec `{}`: {}", spec_path_str, error);
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let openapi: openapiv3::OpenAPI = match serde_yaml::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            let msg = format!("cannot parse OpenAPI spec `{}`: {}", spec_path_str, error);
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let body = codegen::generate_client(mod_ident, &openapi, include_reqwest);

    let expanded = quote! {
        const _: &str = ::core::include_str!(#spec_path_str);

        #[allow(
            missing_docs,
            missing_debug_implementations,
            dead_code,
            unused_imports,
            clippy::all,
            clippy::nursery,
            clippy::pedantic,
        )]
        #mod_vis mod #mod_ident {
            #body
        }
    };

    expanded.into()
}
