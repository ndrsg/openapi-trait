use heck::ToPascalCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::{OperationInfo, ParamInfo, ResponseStatus};
use openapi_trait_shared::codegen::security::{
    auth_state_ident, client_auth_trait_ident, resolve_alternatives, scheme_field_ident, ApiKeyIn,
    SchemeInfo, SchemeKind,
};

/// Generate a reqwest-backed implementation of the generated client trait.
pub fn generate_reqwest_impl(
    mod_ident: &syn::Ident,
    ops: &[OperationInfo],
    schemes: &[SchemeInfo],
) -> TokenStream {
    let module_name = mod_ident.to_string().to_pascal_case();
    let trait_name = format_ident!("{}Client", module_name);
    let error_name = format_ident!("Reqwest{}ClientError", module_name);
    let auth_state = auth_state_ident(mod_ident);
    let auth_trait = client_auth_trait_ident(mod_ident);
    let has_auth = !schemes.is_empty();

    let methods: Vec<TokenStream> = ops
        .iter()
        .map(|op| generate_impl_method(op, &error_name, schemes, has_auth))
        .collect();

    let auth_state_def = generate_auth_state_struct(&auth_state, schemes);
    let auth_trait_def = generate_auth_ext_trait(&auth_trait, &auth_state, schemes);
    let impl_bound = generate_impl_bound(&auth_state, has_auth);
    let error_type_def = generate_error_type(&error_name);

    quote! {
        #auth_state_def
        #auth_trait_def
        #error_type_def

        fn encode_path_param(value: &impl ::core::fmt::Display) -> ::std::string::String {
            ::openapi_trait::percent_encoding::utf8_percent_encode(
                &value.to_string(),
                ::openapi_trait::percent_encoding::NON_ALPHANUMERIC,
            )
            .to_string()
        }

        // Render a single query-parameter value to its raw (unencoded) wire
        // string. Serializing through `serde_json` honors `#[serde(rename)]` on
        // enum variants and the string encodings of dates/uuids; reqwest then
        // percent-encodes as it assembles the URL.
        fn query_value(value: &impl ::serde::Serialize) -> ::std::string::String {
            match ::serde_json::to_value(value) {
                ::core::result::Result::Ok(::serde_json::Value::String(s)) => s,
                ::core::result::Result::Ok(::serde_json::Value::Null) => ::std::string::String::new(),
                ::core::result::Result::Ok(other) => other.to_string(),
                ::core::result::Result::Err(_) => ::std::string::String::new(),
            }
        }

        impl<T> #trait_name for T
        where
            #impl_bound
        {
            type Error = #error_name;

            #(#methods)*
        }
    }
}

/// Generate the where-clause bound on the blanket `Client` impl.
fn generate_impl_bound(auth_state: &syn::Ident, has_auth: bool) -> TokenStream {
    if has_auth {
        quote! {
            T: ::openapi_trait::ReqwestClientCore
                + ::openapi_trait::ReqwestClientAuth<#auth_state>
                + ::core::marker::Send
                + ::core::marker::Sync,
        }
    } else {
        quote! {
            T: ::openapi_trait::ReqwestClientCore + ::core::marker::Send + ::core::marker::Sync,
        }
    }
}

/// Generate the `Reqwest{Mod}ClientError` enum and its trait impls.
fn generate_error_type(error_name: &syn::Ident) -> TokenStream {
    quote! {
        #[derive(::core::fmt::Debug)]
        pub enum #error_name {
            Transport(::openapi_trait::reqwest::Error),
            MissingCredential {
                operation: &'static str,
                scheme: &'static str,
            },
            UnexpectedStatus {
                operation: &'static str,
                status: ::openapi_trait::reqwest::StatusCode,
                body: ::std::string::String,
            },
        }

        impl ::core::fmt::Display for #error_name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    Self::Transport(error) => write!(formatter, "reqwest transport error: {error}"),
                    Self::MissingCredential { operation, scheme } => {
                        write!(formatter, "missing credentials for scheme `{scheme}` on `{operation}`")
                    }
                    Self::UnexpectedStatus { operation, status, body } => write!(
                        formatter,
                        "unexpected status {status} for `{operation}`: {body}"
                    ),
                }
            }
        }

        impl ::std::error::Error for #error_name {
            fn source(&self) -> ::core::option::Option<&(dyn ::std::error::Error + 'static)> {
                match self {
                    Self::Transport(error) => ::core::option::Option::Some(error),
                    Self::MissingCredential { .. }
                    | Self::UnexpectedStatus { .. } => ::core::option::Option::None,
                }
            }
        }

        impl ::core::convert::From<::openapi_trait::reqwest::Error> for #error_name {
            fn from(error: ::openapi_trait::reqwest::Error) -> Self {
                Self::Transport(error)
            }
        }
    }
}

/// Generate the `{Mod}AuthState` struct holding configured credentials.
fn generate_auth_state_struct(ident: &syn::Ident, schemes: &[SchemeInfo]) -> TokenStream {
    let fields: Vec<TokenStream> = schemes
        .iter()
        .map(|s| {
            let field = scheme_field_ident(s);
            match &s.kind {
                SchemeKind::ApiKey { .. } | SchemeKind::HttpBearer => quote! {
                    pub #field: ::core::option::Option<::std::string::String>,
                },
                SchemeKind::HttpBasic => quote! {
                    pub #field: ::core::option::Option<(::std::string::String, ::std::string::String)>,
                },
            }
        })
        .collect();

    quote! {
        #[derive(::core::fmt::Debug, ::core::clone::Clone, ::core::default::Default)]
        pub struct #ident {
            #(#fields)*
        }
    }
}

/// Generate the `{Mod}ClientAuth` extension trait + blanket impl.
fn generate_auth_ext_trait(
    trait_ident: &syn::Ident,
    state_ident: &syn::Ident,
    schemes: &[SchemeInfo],
) -> TokenStream {
    if schemes.is_empty() {
        return quote! {};
    }
    let trait_methods: Vec<TokenStream> = schemes
        .iter()
        .map(|s| {
            let setter = format_ident!("with_{}", s.snake);
            if matches!(s.kind, SchemeKind::HttpBasic) {
                quote! {
                    fn #setter(
                        self,
                        username: impl ::core::convert::Into<::std::string::String>,
                        password: impl ::core::convert::Into<::std::string::String>,
                    ) -> Self;
                }
            } else {
                quote! {
                    fn #setter(self, value: impl ::core::convert::Into<::std::string::String>) -> Self;
                }
            }
        })
        .collect();

    let impl_methods: Vec<TokenStream> = schemes
        .iter()
        .map(|s| {
            let setter = format_ident!("with_{}", s.snake);
            let field = scheme_field_ident(s);
            if matches!(s.kind, SchemeKind::HttpBasic) {
                quote! {
                    fn #setter(
                        mut self,
                        username: impl ::core::convert::Into<::std::string::String>,
                        password: impl ::core::convert::Into<::std::string::String>,
                    ) -> Self {
                        self.as_mut().#field = ::core::option::Option::Some((username.into(), password.into()));
                        self
                    }
                }
            } else {
                quote! {
                    fn #setter(mut self, value: impl ::core::convert::Into<::std::string::String>) -> Self {
                        self.as_mut().#field = ::core::option::Option::Some(value.into());
                        self
                    }
                }
            }
        })
        .collect();

    quote! {
        pub trait #trait_ident: ::core::marker::Sized {
            #(#trait_methods)*
        }

        impl<T> #trait_ident for T
        where
            T: ::core::convert::AsMut<#state_ident> + ::core::marker::Sized,
        {
            #(#impl_methods)*
        }
    }
}

/// Generate one reqwest-backed trait method for a single `OpenAPI` operation.
fn generate_impl_method(
    op: &OperationInfo,
    error_name: &proc_macro2::Ident,
    schemes: &[SchemeInfo],
    has_auth: bool,
) -> TokenStream {
    let method_ident = &op.method_ident;
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let resp_ident = format_ident!("{}Response", op.operation_id.to_pascal_case());
    let http_method = format_ident!("{}", op.method);
    let operation_name = &op.operation_id;
    let path = &op.path;

    let request_fields: Vec<proc_macro2::Ident> = op
        .path_params
        .iter()
        .chain(op.query_params.iter())
        .chain(op.header_params.iter())
        .map(|param| param.field_ident.clone())
        .chain(op.body.iter().map(|_| format_ident!("body")))
        .collect();

    let path_replacements: Vec<TokenStream> = op
        .path_params
        .iter()
        .map(|param| {
            let field_ident = &param.field_ident;
            let placeholder = format!("{{{}}}", param.name);
            quote! {
                path = path.replace(#placeholder, &encode_path_param(&#field_ident));
            }
        })
        .collect();

    let query_builder = generate_query_builder(op);
    let header_builder = generate_header_builder(op);
    let body_builder = generate_body_builder(op);
    let (response_arms, fallback) =
        generate_response_match(op, error_name, &resp_ident, operation_name);

    let alts = resolve_alternatives(&op.auth, schemes);
    let auth_setup = if has_auth && !alts.is_empty() {
        quote! {
            let auth_state = ::openapi_trait::ReqwestClientAuth::auth_state(self).clone();
        }
    } else {
        quote! {}
    };
    let auth_inject = generate_auth_inject(op, &alts, error_name, operation_name);

    quote! {
        fn #method_ident(
            &self,
            req: #req_ident,
            options: ::core::option::Option<::openapi_trait::RequestOptions>,
        ) -> impl ::std::future::Future<Output = ::core::result::Result<#resp_ident, Self::Error>> + Send {
            let client = ::openapi_trait::ReqwestClientCore::reqwest_client(self).clone();
            let base_url = ::openapi_trait::ReqwestClientCore::base_url(self).to_owned();
            #auth_setup

            async move {
                let #req_ident { #(#request_fields),* } = req;
                let mut path = ::std::string::String::from(#path);
                #(#path_replacements)*

                let url = format!("{}{}", base_url.trim_end_matches('/'), path);
                let mut request = client.#http_method(url);

                #query_builder
                #header_builder
                #body_builder
                #auth_inject

                let request = match options {
                    ::core::option::Option::Some(options) => options.apply(request),
                    ::core::option::Option::None => request,
                };

                let response = request.send().await.map_err(#error_name::Transport)?;
                let status = response.status();

                match status.as_u16() {
                    #(#response_arms)*
                    #fallback
                }
            }
        }
    }
}

/// Emit credential injection code for one operation.
fn generate_auth_inject(
    op: &OperationInfo,
    alts: &[&SchemeInfo],
    error_name: &proc_macro2::Ident,
    operation_name: &str,
) -> TokenStream {
    if alts.is_empty() {
        return quote! {};
    }

    let scheme_label = op.auth.alternatives.join(",");

    // A request that supplies its own credentials via `RequestOptions`
    // (`bearer_auth`/`basic_auth`) is valid even when the client carries no
    // configured scheme credential, so skip the missing-credential pre-flight
    // check in that case. The options themselves are applied after this block
    // and replace any scheme-injected `Authorization` header.
    let options_provide_auth = quote! {
        let __options_provides_auth = match &options {
            ::core::option::Option::Some(options) => options.provides_auth(),
            ::core::option::Option::None => false,
        };
    };

    if alts.len() == 1 {
        let inject = inject_scheme_expr(alts[0]);
        return quote! {
            #options_provide_auth
            let __injected = #inject;
            if !__injected && !__options_provides_auth {
                return ::core::result::Result::Err(#error_name::MissingCredential {
                    operation: #operation_name,
                    scheme: #scheme_label,
                });
            }
        };
    }

    let attempts: Vec<TokenStream> = alts
        .iter()
        .map(|s| {
            let inject = inject_scheme_expr(s);
            quote! {
                if !__injected {
                    __injected = #inject;
                }
            }
        })
        .collect();

    quote! {
        #options_provide_auth
        let mut __injected = false;
        #(#attempts)*
        if !__injected && !__options_provides_auth {
            return ::core::result::Result::Err(#error_name::MissingCredential {
                operation: #operation_name,
                scheme: #scheme_label,
            });
        }
    }
}

/// Produce a boolean expression: attempts to inject a single scheme into `request`,
/// returning whether the credential was set.
fn inject_scheme_expr(scheme: &SchemeInfo) -> TokenStream {
    let field = scheme_field_ident(scheme);
    match &scheme.kind {
        SchemeKind::ApiKey {
            key,
            location: ApiKeyIn::Header,
        } => quote! {
            match &auth_state.#field {
                ::core::option::Option::Some(v) => {
                    request = request.header(#key, v);
                    true
                }
                ::core::option::Option::None => false,
            }
        },
        SchemeKind::ApiKey {
            key,
            location: ApiKeyIn::Query,
        } => quote! {
            match &auth_state.#field {
                ::core::option::Option::Some(v) => {
                    request = request.query(&[(#key, v.as_str())]);
                    true
                }
                ::core::option::Option::None => false,
            }
        },
        SchemeKind::ApiKey {
            key,
            location: ApiKeyIn::Cookie,
        } => quote! {
            match &auth_state.#field {
                ::core::option::Option::Some(v) => {
                    request = request.header(
                        ::openapi_trait::reqwest::header::COOKIE,
                        ::std::format!("{}={}", #key, v),
                    );
                    true
                }
                ::core::option::Option::None => false,
            }
        },
        SchemeKind::HttpBearer => quote! {
            match &auth_state.#field {
                ::core::option::Option::Some(v) => {
                    request = request.bearer_auth(v);
                    true
                }
                ::core::option::Option::None => false,
            }
        },
        SchemeKind::HttpBasic => quote! {
            match &auth_state.#field {
                ::core::option::Option::Some((u, p)) => {
                    request = request.basic_auth(u, ::core::option::Option::Some(p));
                    true
                }
                ::core::option::Option::None => false,
            }
        },
    }
}

/// Generate the match arms used to translate reqwest responses into operation response enums.
fn generate_response_match(
    op: &OperationInfo,
    error_name: &proc_macro2::Ident,
    resp_ident: &proc_macro2::Ident,
    operation_name: &str,
) -> (Vec<TokenStream>, TokenStream) {
    let response_arms: Vec<TokenStream> = op
        .responses
        .iter()
        .filter_map(|response| match response.status {
            ResponseStatus::Code(code) => {
                let variant_ident = format_ident!("Status{}", code);
                Some(response.rust_type.as_ref().map_or_else(
                    || {
                        quote! {
                            #code => ::core::result::Result::Ok(#resp_ident::#variant_ident),
                        }
                    },
                    |_| {
                        quote! {
                            #code => {
                                let body = response.json().await.map_err(#error_name::Transport)?;
                                ::core::result::Result::Ok(#resp_ident::#variant_ident(body))
                            }
                        }
                    },
                ))
            }
            ResponseStatus::Default => None,
        })
        .collect();

    let fallback = if op
        .responses
        .iter()
        .any(|response| matches!(response.status, ResponseStatus::Default))
    {
        quote! {
            _ => {
                let body = response.text().await.map_err(#error_name::Transport)?;
                ::core::result::Result::Ok(#resp_ident::Default(body))
            }
        }
    } else {
        quote! {
            _ => {
                let body = response.text().await.map_err(#error_name::Transport)?;
                ::core::result::Result::Err(#error_name::UnexpectedStatus {
                    operation: #operation_name,
                    status,
                    body,
                })
            }
        }
    };

    (response_arms, fallback)
}

/// Generate the reqwest query population code for an operation.
///
/// Builds a `Vec<(String, String)>` of raw query pairs, honoring each param's
/// `style`/`explode`, then hands it to reqwest (which percent-encodes as it
/// assembles the URL).
fn generate_query_builder(op: &OperationInfo) -> TokenStream {
    if op.query_params.is_empty() {
        return quote! {};
    }

    let pushes: Vec<TokenStream> = op.query_params.iter().map(query_param_push).collect();

    quote! {
        let mut __query: ::std::vec::Vec<(::std::string::String, ::std::string::String)> =
            ::std::vec::Vec::new();
        #(#pushes)*
        request = request.query(&__query);
    }
}

/// Emit the code appending one query parameter's pairs to `__query`.
///
/// Scalars become a single `name=value`. Exploded arrays repeat the key once per
/// element; non-exploded arrays join their elements with the style delimiter
/// (`,`/` `/`|`). Object/`deepObject` params fall through the scalar branch and
/// serialize to a single JSON value (the warned-about fallback).
fn query_param_push(param: &ParamInfo) -> TokenStream {
    let field_ident = &param.field_ident;
    let name = &param.name;

    if param.query.is_array {
        let delim = param.query.style.delimiter();
        let append = if param.query.explode {
            quote! {
                for __item in __items {
                    __query.push((#name.to_string(), query_value(__item)));
                }
            }
        } else {
            quote! {
                if !__items.is_empty() {
                    let __joined = __items
                        .iter()
                        .map(query_value)
                        .collect::<::std::vec::Vec<_>>()
                        .join(#delim);
                    __query.push((#name.to_string(), __joined));
                }
            }
        };
        if param.required {
            quote! {
                { let __items = &#field_ident; #append }
            }
        } else {
            quote! {
                if let ::core::option::Option::Some(__items) = &#field_ident { #append }
            }
        }
    } else if param.required {
        quote! {
            __query.push((#name.to_string(), query_value(&#field_ident)));
        }
    } else {
        quote! {
            if let ::core::option::Option::Some(__value) = &#field_ident {
                __query.push((#name.to_string(), query_value(__value)));
            }
        }
    }
}

/// Generate the reqwest header population code for an operation.
fn generate_header_builder(op: &OperationInfo) -> TokenStream {
    let header_updates: Vec<TokenStream> = op
        .header_params
        .iter()
        .map(|param| {
            let field_ident = &param.field_ident;
            let header_name = &param.name;

            if param.required {
                // Required headers are non-optional `String`s on the request
                // struct, so they can be set unconditionally.
                quote! {
                    request = request.header(#header_name, #field_ident);
                }
            } else {
                quote! {
                    if let ::core::option::Option::Some(value) = #field_ident {
                        request = request.header(#header_name, value);
                    }
                }
            }
        })
        .collect();

    quote! { #(#header_updates)* }
}

/// Generate the reqwest request body population code for an operation.
fn generate_body_builder(op: &OperationInfo) -> TokenStream {
    match op.body {
        Some(ref body) if body.required => quote! {
            request = request.json(&body);
        },
        Some(_) => quote! {
            if let ::core::option::Option::Some(body) = body {
                request = request.json(&body);
            }
        },
        None => quote! {},
    }
}
