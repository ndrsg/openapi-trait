use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::OperationInfo;

/// Generate the `{ModName}Client` trait with one method per operation.
pub fn generate_trait(mod_ident: &syn::Ident, ops: &[OperationInfo]) -> TokenStream {
    let trait_name = format_ident!("{}Client", mod_ident.to_string().to_pascal_case());

    let methods: Vec<TokenStream> = ops.iter().map(generate_trait_method).collect();

    quote! {
        pub trait #trait_name: ::core::marker::Send + ::core::marker::Sync {
            /// The error type returned by all operations.
            type Error;

            #(#methods)*
        }
    }
}

/// Generate a single trait method for one operation.
fn generate_trait_method(op: &OperationInfo) -> TokenStream {
    let method_ident = format_ident!("{}", op.operation_id.to_snake_case());
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let resp_ident = format_ident!("{}Response", op.operation_id.to_pascal_case());

    let doc = match (&op.summary, &op.description) {
        (Some(summary), Some(description)) if summary != description => {
            quote! { #[doc = #summary] #[doc = ""] #[doc = #description] }
        }
        (Some(summary), _) => quote! { #[doc = #summary] },
        (None, Some(description)) => quote! { #[doc = #description] },
        (None, None) => quote! {},
    };

    quote! {
        #doc
        fn #method_ident(
            &self,
            req: #req_ident,
            options: ::core::option::Option<::openapi_trait::RequestOptions>,
        ) -> impl ::std::future::Future<Output = ::core::result::Result<#resp_ident, Self::Error>> + Send;
    }
}
