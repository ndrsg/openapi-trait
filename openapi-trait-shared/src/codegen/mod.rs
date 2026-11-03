pub mod compositions;
pub mod ctx;
pub mod idents;
pub mod model;
pub mod operations;
pub mod schemas;
pub mod security;
pub mod types;
#[cfg(feature = "validation")]
pub mod validation;

use oas3::Spec as OpenAPI;
use proc_macro2::TokenStream;
use quote::quote;

use schemas::generate_schemas;

/// Generate schema structs and enums only (framework-agnostic).
#[must_use]
pub fn generate_models(_mod_ident: &syn::Ident, openapi: &OpenAPI) -> TokenStream {
    let schemas = generate_schemas(openapi, &mut ctx::CodegenCtx::new(openapi));
    quote! {
        use super::*;
        #schemas
    }
}
