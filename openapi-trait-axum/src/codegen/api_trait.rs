use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::OperationInfo;

/// Generate the `{ModName}Api` trait with one `async fn` per operation.
pub fn generate_trait(mod_ident: &syn::Ident, ops: &[OperationInfo]) -> TokenStream {
    let trait_name = format_ident!("{}Api", mod_ident.to_string().to_pascal_case());

    let methods: Vec<TokenStream> = ops.iter().map(generate_trait_method).collect();

    quote! {
        pub trait #trait_name<S = ()>: ::core::marker::Send + ::core::marker::Sync {
            /// The error type returned by all operations.
            type Error: ::axum::response::IntoResponse + ::core::marker::Send;

            #(#methods)*

            /// Build an [`axum::Router`] wired to `self`.
            fn router(self) -> ::axum::Router<S>
            where
                Self: Sized + 'static,
                S: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                make_router(::std::sync::Arc::new(self))
            }
        }
    }
}

/// Generate a single trait method for one operation.
fn generate_trait_method(op: &OperationInfo) -> TokenStream {
    let method_ident = format_ident!("{}", op.operation_id.to_snake_case());
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let resp_ident = format_ident!("{}Response", op.operation_id.to_pascal_case());

    let doc = match (&op.summary, &op.description) {
        (Some(s), Some(d)) if s != d => quote! { #[doc = #s] #[doc = ""] #[doc = #d] },
        (Some(s), _) => quote! { #[doc = #s] },
        (None, Some(d)) => quote! { #[doc = #d] },
        (None, None) => quote! {},
    };

    quote! {
        #doc
        fn #method_ident(
            &self,
            req: #req_ident,
            state: ::axum::extract::State<S>,
            headers: ::axum::http::HeaderMap,
        ) -> impl ::std::future::Future<Output = ::core::result::Result<#resp_ident, Self::Error>> + Send {
            let _ = req;
            let _ = state;
            let _ = headers;
            async { ::core::result::Result::Ok(#resp_ident::Default("not implemented".into())) }
        }
    }
}
