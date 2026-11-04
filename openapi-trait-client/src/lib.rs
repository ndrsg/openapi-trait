//! Client backend proc-macro for `openapi-trait`.
//!
//! This crate is not intended for direct use. Use the
//! [`openapi-trait`](https://docs.rs/openapi-trait) crate instead, which
//! re-exports the [`openapi_trait`] attribute macro from here as
//! `openapi_trait::client`.

/// Code generation helpers for the attribute macro.
mod codegen;
/// Derive expansion helpers for `ReqwestClient`.
mod reqwest_derive;

use openapi_trait_shared::args::MacroArgs;
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

/// Generates transport-agnostic Rust client traits from an `OpenAPI` specification file.
///
/// Apply this attribute to a `mod` block. The macro reads the `OpenAPI`
/// document at the given path (resolved relative to `CARGO_MANIFEST_DIR`) at
/// compile time and replaces the module's contents with:
///
/// - Schema structs derived from `components/schemas`
/// - A module per operation (`get_pet`), holding a `Request` struct (bundles
///   path, query, header params and the request body) and a `Response` enum
///   whose variants map to HTTP status codes
/// - A `{ModName}Client` trait with one method per operation (keyed by
///   `operationId`)
/// - When the `reqwest-client` cargo feature is enabled through
///   `openapi-trait`, a blanket reqwest-backed implementation for any user
///   type deriving [`ReqwestClient`]
///
/// The generated trait name is derived from the annotated module name, so
/// `mod petstore {}` produces `petstore::PetstoreClient`.
///
/// # Go-to-definition
///
/// Add `source_file` to make IDE go-to-definition open the generated code
/// instead of this attribute:
///
/// ```text
/// #[openapi_trait::client("openapi/pets.yaml", source_file)]
/// #[openapi_trait::client("openapi/pets.yaml", source_file = "some/dir")]
/// ```
///
/// The macro writes the prettyprinted module body to
/// `<module>.client.<hash>.rs` and `include!`s it. With a bare `source_file`, the
/// file goes into `.openapi-trait/` in the crate root, which gets a
/// `.gitignore` containing `*`. With `source_file = "dir"`, it goes into `dir`
/// (relative to `CARGO_MANIFEST_DIR`), which you should gitignore yourself.
/// The file is rewritten only when its content changes. rust-analyzer may need
/// a server restart after the file is first created. If writing fails, the
/// macro emits a compiler warning and falls back to inline code.
///
/// # Debugging
///
/// Set the `OPENAPI_TRAIT_DEBUG` environment variable to dump a prettyprinted
/// copy of the code this macro generates (one level deep, without recursively
/// expanding nested derives). Use `1`/`true` to write to a default directory
/// (`$OUT_DIR/openapi-trait-debug`, or the system temp dir), or set it to a
/// directory path to choose the location. The resolved file path is printed to
/// stderr during the build.
#[proc_macro_attribute]
pub fn openapi_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    run_macro(&args, item, cfg!(feature = "reqwest-client"))
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

/// Expand `#[openapi_trait("...")]` into a generated client module.
fn run_macro(args: &MacroArgs, item: TokenStream, include_reqwest: bool) -> TokenStream {
    openapi_trait_shared::expand::expand(args, item.into(), "client", |mod_ident, openapi| {
        codegen::generate_client(mod_ident, openapi, include_reqwest)
    })
    .into()
}
