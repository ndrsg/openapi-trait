use heck::ToPascalCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::OperationInfo;
use openapi_trait_shared::codegen::security::{auth_enum_ident, resolve_alternatives, SchemeInfo};

/// Generate the `{ModName}Api` trait with one `async fn` per operation,
/// preceded by the per-module `NotImplemented` marker that the trait's default
/// method bodies use to signal "this operation was not overridden".
pub fn generate_trait(
    mod_ident: &syn::Ident,
    ops: &[OperationInfo],
    schemes: &[SchemeInfo],
) -> TokenStream {
    let trait_name = format_ident!("{}Api", mod_ident.to_string().to_pascal_case());

    let methods: Vec<TokenStream> = ops
        .iter()
        .map(|op| generate_trait_method(op, schemes))
        .collect();

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
fn generate_trait_method(op: &OperationInfo, schemes: &[SchemeInfo]) -> TokenStream {
    let method_ident = &op.method_ident;
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let resp_ident = format_ident!("{}Response", op.operation_id.to_pascal_case());

    let doc = match (&op.summary, &op.description) {
        (Some(s), Some(d)) if s != d => quote! { #[doc = #s] #[doc = ""] #[doc = #d] },
        (Some(s), _) => quote! { #[doc = #s] },
        (None, Some(d)) => quote! { #[doc = #d] },
        (None, None) => quote! {},
    };

    let alts = resolve_alternatives(&op.auth, schemes);
    let (auth_param, auth_discard) = match alts.len() {
        0 => (quote! {}, quote! {}),
        1 => {
            let ty = &alts[0].ident;
            (quote! { auth: #ty, }, quote! { let _ = auth; })
        }
        _ => {
            let ty = auth_enum_ident(&op.operation_id);
            (quote! { auth: #ty, }, quote! { let _ = auth; })
        }
    };

    quote! {
        #doc
        fn #method_ident(
            &self,
            req: #req_ident,
            #auth_param
            state: ::axum::extract::State<S>,
            headers: ::axum::http::HeaderMap,
        ) -> impl ::std::future::Future<Output = ::core::result::Result<#resp_ident, Self::Error>> + Send {
            let _ = req;
            #auth_discard
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
