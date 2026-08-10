/// API trait code generation.
pub mod api_trait;
/// Axum router code generation.
pub mod router;

use openapiv3::OpenAPI;
use proc_macro2::TokenStream;
use quote::quote;

use api_trait::generate_trait;
use openapi_trait_shared::codegen::{
    operations::{
        collect_operations, generate_operation_errors, generate_operation_types, BinaryBodyTypes,
    },
    schemas::generate_schemas,
    security::{
        collect_schemes, generate_op_auth_enum, generate_scheme_types, resolve_alternatives,
    },
};
use router::generate_router;

/// Generate schemas + operation types + trait + axum router.
pub fn generate_axum(mod_ident: &syn::Ident, openapi: &OpenAPI) -> TokenStream {
    let auth_schemes = collect_schemes(openapi);
    let schema_types = generate_schemas(openapi);
    let (ops, diagnostics) = collect_operations(openapi, &auth_schemes);
    diagnostics.emit_warnings();
    let op_errors = generate_operation_errors(&diagnostics.errors);
    let op_types = generate_operation_types(
        &ops,
        &BinaryBodyTypes {
            request: quote!(::axum::body::Body),
            response: quote!(::axum::body::Body),
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

    let api_trait = generate_trait(mod_ident, &ops, &auth_schemes);
    let router = generate_router(mod_ident, &ops, &auth_schemes);

    quote! {
        use super::*;
        #op_errors
        #unsupported_and
        #schema_types
        #auth_types
        #(#op_auth_enums)*
        #op_types
        #api_trait
        #router
    }
}
