use heck::ToPascalCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::ctx::ROOT_MODULE;
use openapi_trait_shared::codegen::operations::{OperationInfo, AUTH_TYPE};
use openapi_trait_shared::codegen::security::{resolve_alternatives, SchemeInfo};

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

        pub trait #trait_name: ::core::marker::Send + ::core::marker::Sync {
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
            ///
            /// The routes don't use axum state; keep your application state
            /// in `self`. To merge them into a router with state `S`, convert
            /// with `.with_state(())`, which yields a `Router<S>`.
            fn router(self) -> ::axum::Router
            where
                Self: Sized + 'static,
            {
                make_router(::std::sync::Arc::new(self))
            }
        }
    }
}

/// The type of the operation's `auth` value as written inside its module:
/// the scheme's credential type for one scheme, the module's `Auth` enum for
/// several, wrapped in `Option` when auth is optional. `None` when the
/// operation isn't secured.
fn auth_type_in_module(op: &OperationInfo, schemes: &[SchemeInfo]) -> Option<TokenStream> {
    let alts = resolve_alternatives(&op.auth, schemes);
    let ty = match alts.as_slice() {
        [] => return None,
        [scheme] => {
            let root = format_ident!("{ROOT_MODULE}");
            let ident = &scheme.ident;
            quote! { #root::#ident }
        }
        _ => {
            let ident = format_ident!("{AUTH_TYPE}");
            quote! { #ident }
        }
    };
    // With optional auth (`security: [{}, …]`) the handler gets `None` for an
    // anonymous request.
    Some(if op.auth.optional {
        quote! { ::core::option::Option<#ty> }
    } else {
        ty
    })
}

/// Generate the operation's `Context`, the single argument its trait method
/// receives.
///
/// It is emitted into the operation's module. A private field keeps code
/// outside the module from building it with a struct literal or matching it
/// without `..`, so fields can be added later without breaking anyone; the
/// router and tests use `Context::new`.
pub fn generate_context(op: &OperationInfo, schemes: &[SchemeInfo]) -> TokenStream {
    let auth = auth_type_in_module(op, schemes);
    let (auth_field, auth_param, auth_init) = auth.map_or_else(
        || (quote! {}, quote! {}, quote! {}),
        |ty| {
            (
                quote! {
                    /// The credentials the request was authenticated with.
                    pub auth: #ty,
                },
                quote! { auth: #ty, },
                quote! { auth, },
            )
        },
    );
    quote! {
        /// Everything a handler receives for one request.
        ///
        /// Destructure it with `..`, for example
        /// `let Context { req, .. } = ctx;`.
        #[derive(::core::fmt::Debug)]
        pub struct Context {
            /// The typed parameters and body.
            pub req: Request,
            #auth_field
            /// All request headers, for anything the spec doesn't model.
            pub headers: ::axum::http::HeaderMap,
            /// Keeps the struct open for new fields.
            _private: (),
        }

        impl Context {
            /// Build a context, for example to call a handler in a test.
            #[must_use]
            pub fn new(req: Request, #auth_param headers: ::axum::http::HeaderMap) -> Self {
                Self { req, #auth_init headers, _private: () }
            }
        }
    }
}

/// Generate a single trait method for one operation.
fn generate_trait_method(op: &OperationInfo) -> TokenStream {
    let method_ident = &op.method_ident;
    let ctx_ty = op.type_path("Context");
    let resp_ident = op.response_path();

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
            ctx: #ctx_ty,
        ) -> impl ::std::future::Future<Output = ::core::result::Result<#resp_ident, Self::Error>> + Send {
            let _ = ctx;
            async {
                ::core::result::Result::Err(
                    <Self::Error as ::core::convert::From<NotImplemented>>::from(NotImplemented),
                )
            }
        }
    }
}
