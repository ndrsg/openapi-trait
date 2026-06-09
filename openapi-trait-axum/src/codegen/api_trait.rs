use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::OperationInfo;

/// Generate the `{ModName}Api` trait with one `async fn` per operation,
/// preceded by the per-module `NotImplemented` marker that the trait's default
/// method bodies use to signal "this operation was not overridden".
pub fn generate_trait(mod_ident: &syn::Ident, ops: &[OperationInfo]) -> TokenStream {
    let trait_name = format_ident!("{}Api", mod_ident.to_string().to_pascal_case());

    let methods: Vec<TokenStream> = ops.iter().map(generate_trait_method).collect();

    quote! {
        /// Marker error returned by default trait method implementations.
        ///
        /// Each generated `*Api` trait requires `Self::Error: From<NotImplemented>`
        /// so that overrides do not have to opt in to anything special, while
        /// unoverridden methods can still surface a typed "not implemented"
        /// signal. The included `IntoResponse` impl turns it into a plain
        /// `500 Internal Server Error` for routes that the user has not yet
        /// implemented.
        #[derive(::core::fmt::Debug, ::core::clone::Clone, ::core::marker::Copy)]
        pub struct NotImplemented;

        impl ::axum::response::IntoResponse for NotImplemented {
            fn into_response(self) -> ::axum::response::Response {
                (
                    ::axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "not implemented",
                )
                    .into_response()
            }
        }

        pub trait #trait_name<S = ()>: ::core::marker::Send + ::core::marker::Sync {
            /// The error type returned by all operations.
            ///
            /// Must be convertible from [`NotImplemented`] so that default
            /// method bodies have a way to signal "not overridden" without
            /// constraining the user's choice of error representation.
            type Error: ::axum::response::IntoResponse
                + ::core::convert::From<NotImplemented>
                + ::core::marker::Send;

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
            async {
                ::core::result::Result::Err(
                    <Self::Error as ::core::convert::From<NotImplemented>>::from(NotImplemented),
                )
            }
        }
    }
}
