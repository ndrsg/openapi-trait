/// API trait code generation.
pub mod api_trait;
/// Axum router code generation.
pub mod router;

use openapiv3::OpenAPI;
use proc_macro2::TokenStream;
use quote::quote;

use api_trait::generate_trait;
use openapi_trait_shared::codegen::{
    operations::{collect_operations, generate_operation_types},
    schemas::generate_schemas,
};
use router::generate_router;

/// Generate schemas + operation types + trait + axum router.
pub fn generate_axum(mod_ident: &syn::Ident, openapi: &OpenAPI) -> TokenStream {
    let schemas = generate_schemas(openapi);
    let ops = collect_operations(openapi);
    let op_types = generate_operation_types(&ops);
    let api_trait = generate_trait(mod_ident, &ops);
    let router = generate_router(mod_ident, &ops);

    quote! {
        use super::*;
        #schemas
        #op_types
        #api_trait
        #router
    }
}
