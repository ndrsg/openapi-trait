//! Client backend proc-macro for `openapi-trait`.
//! 
//! This crate is not intended for direct use. Use the
//! [`openapi-trait`](https://docs.rs/openapi-trait) crate instead, which
//! re-exports the [`openapi_trait`] attribute macro from here as
//! `openapi_trait::client`.

mod codegen;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, ItemMod, LitStr};

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
/// - A `{Title}Client` trait with one method per operation (keyed by
///   `operationId`)
///
/// The crate recompiles automatically whenever the spec file changes.
///
/// # Arguments
///
/// First positional argument: path to the `OpenAPI` YAML or JSON file,
/// relative to the crate root (`CARGO_MANIFEST_DIR`).
///
/// # Examples
///
/// ```rust,ignore
/// #[openapi_trait::client("openapi/petstore.yaml")]
/// pub mod petstore {}
///
/// struct MyClient;
///
/// impl petstore::PetstoreClient for MyClient {
///     type Error = std::convert::Infallible;
///
///     async fn get_pet_by_id(
///         &self,
///         req: petstore::GetPetByIdRequest,
///     ) -> Result<petstore::GetPetByIdResponse, Self::Error> {
///         Ok(petstore::GetPetByIdResponse::Status200(petstore::Pet {
///             id: Some(req.pet_id),
///             name: "doggie".into(),
///             photo_urls: vec![],
///             category: None,
///             tags: None,
///             status: None,
///         }))
///     }
/// }
/// ```
///
/// # Errors
///
/// The macro emits a compile error if:
///
/// - The file cannot be found or read.
/// - The `OpenAPI` document is malformed or cannot be parsed.
/// - An operation is missing an `operationId`.
#[proc_macro_attribute]
pub fn openapi_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(attr as LitStr);
    run_macro(path_lit, item)
}

fn run_macro(path_lit: LitStr, item: TokenStream) -> TokenStream {
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

    let body = codegen::generate_client(mod_ident, &openapi);

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