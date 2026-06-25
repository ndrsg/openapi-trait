//! Axum backend proc-macro for `openapi-trait`.
//!
//! This crate is not intended for direct use. Use the
//! [`openapi-trait`](https://docs.rs/openapi-trait) crate instead, which
//! re-exports the [`openapi_trait`] attribute macro from here as
//! `openapi_trait::axum`.

/// Code-generation modules for the axum backend.
mod codegen;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, ItemMod, LitStr};

/// Generates typed Rust code from an `OpenAPI` specification file.
///
/// Apply this attribute to a `mod` block. The macro reads the `OpenAPI`
/// document at the given path (resolved relative to `CARGO_MANIFEST_DIR`) at
/// compile time and replaces the module's contents with:
///
/// - Schema structs derived from `components/schemas`
/// - A `{OperationId}Request` struct per operation (bundles path, query,
///   header params and the request body)
/// - Per-operation `{OperationId}Response` enums implementing
///   [`axum::response::IntoResponse`](https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html)
/// - A `{ModName}Api<S = ()>` trait with one `async fn` per operation (keyed by
///   `operationId`). Trait methods have a default implementation that returns
///   `500 Internal Server Error`, so you only need to override the operations
///   your server handles.
/// - A `router` method on the trait that wires all operations to an
///   [`axum::Router`](https://docs.rs/axum/latest/axum/struct.Router.html)
///
/// The generated trait name is derived from the annotated module name, so
/// `mod petstore {}` produces `petstore::PetstoreApi`.
///
/// The crate recompiles automatically whenever the spec file changes.
///
/// # Arguments
///
/// First positional argument: path to the `OpenAPI` YAML or JSON file,
/// relative to the crate root (`CARGO_MANIFEST_DIR`).
///
/// # Debugging
///
/// Set the `OPENAPI_TRAIT_DEBUG` environment variable to dump a prettyprinted
/// copy of the code this macro generates (one level deep, without recursively
/// expanding nested derives). Use `1`/`true` to write to a default directory
/// (`$OUT_DIR/openapi-trait-debug`, or the system temp dir), or set it to a
/// directory path to choose the location. The resolved file path is printed to
/// stderr during the build.
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
    run_macro(&path_lit, item)
}

/// Run the core macro logic with the resolved spec path literal.
fn run_macro(path_lit: &LitStr, item: TokenStream) -> TokenStream {
    let module = parse_macro_input!(item as ItemMod);
    let mod_ident = &module.ident;
    let mod_vis = &module.vis;

    let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return syn::Error::new(
            Span::call_site(),
            "CARGO_MANIFEST_DIR is not set; cannot resolve spec path",
        )
        .to_compile_error()
        .into();
    };

    let spec_path = std::path::PathBuf::from(&manifest_dir).join(path_lit.value());
    let spec_path_str = spec_path.to_string_lossy().into_owned();

    let content = match std::fs::read_to_string(&spec_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("cannot read OpenAPI spec `{spec_path_str}`: {e}");
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let openapi: openapiv3::OpenAPI = match serde_yaml::from_str(&content) {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("cannot parse OpenAPI spec `{spec_path_str}`: {e}");
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let body = codegen::generate_axum(mod_ident, &openapi);

    let expanded = quote! {
        // Re-compile when the spec file changes.
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

    openapi_trait_shared::debug::write_debug_output(mod_ident, &expanded);

    expanded.into()
}
