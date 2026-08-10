/// Generated trait definitions for client APIs.
mod client_trait;
/// Generated reqwest-backed client implementations.
mod reqwest_impl;

use openapiv3::OpenAPI;
use proc_macro2::TokenStream;
use quote::quote;

use openapi_trait_shared::codegen::{
    operations::{
        collect_operations, generate_operation_errors, generate_operation_types, BinaryBodyTypes,
    },
    schemas::generate_schemas,
    security::{
        collect_schemes, generate_op_auth_enum, generate_scheme_types, resolve_alternatives,
    },
};

use self::{client_trait::generate_trait, reqwest_impl::generate_reqwest_impl};

/// Generate schemas + operation types + transport-agnostic client trait.
pub fn generate_client(
    mod_ident: &syn::Ident,
    openapi: &OpenAPI,
    include_reqwest: bool,
) -> TokenStream {
    let auth_schemes = collect_schemes(openapi);
    let schema_types = generate_schemas(openapi);
    let (ops, diagnostics) = collect_operations(openapi, &auth_schemes);
    diagnostics.emit_warnings();
    let op_errors = generate_operation_errors(&diagnostics.errors);
    let op_types = generate_operation_types(
        &ops,
        &BinaryBodyTypes {
            request: quote!(::openapi_trait::reqwest::Body),
            response: quote!(::openapi_trait::ByteStream),
        },
    );

    let auth_types = generate_scheme_types(&auth_schemes);
    let op_auth_enums: Vec<TokenStream> = ops
        .iter()
        .filter_map(|op| {
            let alts = resolve_alternatives(&op.auth, &auth_schemes);
            generate_op_auth_enum(&op.operation_id, &alts)
        })
        .collect();
    let unsupported_and = openapi_trait_shared::codegen::security::generate_unsupported_and_errors(
        &ops.iter()
            .filter(|op| op.auth.had_unsupported_and)
            .map(|op| op.operation_id.clone())
            .collect::<Vec<_>>(),
    );

    let client_trait = generate_trait(mod_ident, &ops);
    let reqwest_impl = if include_reqwest {
        generate_reqwest_impl(mod_ident, &ops, &auth_schemes)
    } else {
        TokenStream::default()
    };

    quote! {
        use super::*;
        #op_errors
        #unsupported_and
        #schema_types
        #auth_types
        #(#op_auth_enums)*
        #op_types
        #client_trait
        #reqwest_impl
    }
}
