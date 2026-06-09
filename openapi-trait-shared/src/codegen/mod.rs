pub mod compositions;
pub mod operations;
pub mod schemas;
pub mod security;
pub mod types;

use openapiv3::OpenAPI;
use proc_macro2::TokenStream;
use quote::quote;

use schemas::generate_schemas;

/// Generate schema structs and enums only (framework-agnostic).
#[must_use]
pub fn generate_models(_mod_ident: &syn::Ident, openapi: &OpenAPI) -> TokenStream {
    let schemas = generate_schemas(openapi);
    quote! {
        use super::*;
        #schemas
    }
}
