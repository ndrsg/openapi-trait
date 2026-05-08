/// Generated trait definitions for client APIs.
mod client_trait;
/// Generated reqwest-backed client implementations.
mod reqwest_impl;

use openapiv3::OpenAPI;
use proc_macro2::TokenStream;
use quote::quote;

use openapi_trait_shared::codegen::{
    operations::{collect_operations, generate_operation_types},
    schemas::generate_schemas,
};

use self::{client_trait::generate_trait, reqwest_impl::generate_reqwest_impl};

/// Generate schemas + operation types + transport-agnostic client trait.
pub fn generate_client(
    mod_ident: &syn::Ident,
    openapi: &OpenAPI,
    include_reqwest: bool,
) -> TokenStream {
    let schemas = generate_schemas(openapi);
    let ops = collect_operations(openapi);
    let op_types = generate_operation_types(&ops);
    let client_trait = generate_trait(mod_ident, &ops);
    let reqwest_impl = if include_reqwest {
        generate_reqwest_impl(mod_ident, &ops)
    } else {
        TokenStream::default()
    };

    quote! {
        use super::*;
        #schemas
        #op_types
        #client_trait
        #reqwest_impl
    }
}
