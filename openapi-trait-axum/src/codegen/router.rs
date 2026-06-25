use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::{OperationInfo, ParamInfo, ResponseStatus};
use openapi_trait_shared::codegen::security::{
    auth_enum_ident, resolve_alternatives, ApiKeyIn, SchemeInfo, SchemeKind,
};

/// Generate `IntoResponse` impls and the private `make_router` free function.
pub fn generate_router(
    mod_ident: &syn::Ident,
    ops: &[OperationInfo],
    schemes: &[SchemeInfo],
) -> TokenStream {
    let trait_name = format_ident!("{}Api", mod_ident.to_string().to_pascal_case());

    let into_response_impls: Vec<TokenStream> =
        ops.iter().map(generate_into_response_impl).collect();
    let (query_structs, route_calls): (Vec<_>, Vec<_>) =
        ops.iter().map(|op| generate_route(op, schemes)).unzip();

    let auth_helpers = generate_auth_helpers(schemes);

    quote! {
        #(#into_response_impls)*

        #auth_helpers

        fn make_router<T, S>(__api: ::std::sync::Arc<T>) -> ::axum::Router<S>
        where
            T: #trait_name<S> + ::core::marker::Send + ::core::marker::Sync + 'static,
            S: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static,
        {
            #(#query_structs)*
            ::axum::Router::new()
                #(#route_calls)*
        }
    }
}

/// Emit one-off helpers (basic-auth decode, cookie lookup) only when needed.
fn generate_auth_helpers(schemes: &[SchemeInfo]) -> TokenStream {
    let needs_basic = schemes
        .iter()
        .any(|s| matches!(s.kind, SchemeKind::HttpBasic));
    let needs_cookie = schemes.iter().any(|s| {
        matches!(
            s.kind,
            SchemeKind::ApiKey {
                location: ApiKeyIn::Cookie,
                ..
            }
        )
    });

    let basic = if needs_basic {
        quote! {
            fn __decode_basic_auth(b64: &str) -> ::core::option::Option<(::std::string::String, ::std::string::String)> {
                use ::openapi_trait::base64::Engine as _;
                let bytes = ::openapi_trait::base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
                let s = ::std::string::String::from_utf8(bytes).ok()?;
                let idx = s.find(':')?;
                ::core::option::Option::Some((s[..idx].to_string(), s[idx + 1..].to_string()))
            }
        }
    } else {
        quote! {}
    };

    let cookie = if needs_cookie {
        quote! {
            fn __lookup_cookie(headers: &::axum::http::HeaderMap, name: &str) -> ::core::option::Option<::std::string::String> {
                for h in headers.get_all(::axum::http::header::COOKIE).iter() {
                    let Ok(raw) = h.to_str() else { continue };
                    for kv in raw.split(';') {
                        let kv = kv.trim();
                        if let ::core::option::Option::Some(value) = kv.strip_prefix(name).and_then(|rest| rest.strip_prefix('=')) {
                            return ::core::option::Option::Some(value.to_string());
                        }
                    }
                }
                ::core::option::Option::None
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #basic
        #cookie
    }
}

/// Generate `IntoResponse` impl for a single operation's response enum.
fn generate_into_response_impl(op: &OperationInfo) -> TokenStream {
    let resp_ident = format_ident!("{}Response", op.operation_id.to_pascal_case());

    let arms: Vec<TokenStream> = op
        .responses
        .iter()
        .map(|r| match &r.status {
            ResponseStatus::Code(n) => {
                let variant_ident = format_ident!("Status{}", n);
                let status_ident = status_code_ident(*n);
                if r.rust_type.is_some() {
                    quote! {
                        Self::#variant_ident(body) => (
                            ::axum::http::StatusCode::#status_ident,
                            ::axum::Json(body),
                        ).into_response(),
                    }
                } else {
                    quote! {
                        Self::#variant_ident => {
                            ::axum::http::StatusCode::#status_ident
                                .into_response()
                        },
                    }
                }
            }
            ResponseStatus::Default => {
                quote! {
                    Self::Default(msg) => (
                        ::axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        msg,
                    ).into_response(),
                }
            }
        })
        .collect();

    quote! {
        impl ::axum::response::IntoResponse for #resp_ident {
            fn into_response(self) -> ::axum::response::Response {
                use ::axum::response::IntoResponse as _;
                match self {
                    #(#arms)*
                }
            }
        }
    }
}

/// Map a numeric HTTP status code to an `axum::http::StatusCode` constant ident.
fn status_code_ident(n: u16) -> proc_macro2::Ident {
    // Map common status codes to axum's StatusCode constants
    let name = match n {
        200 => "OK",
        201 => "CREATED",
        202 => "ACCEPTED",
        204 => "NO_CONTENT",
        301 => "MOVED_PERMANENTLY",
        302 => "FOUND",
        304 => "NOT_MODIFIED",
        400 => "BAD_REQUEST",
        401 => "UNAUTHORIZED",
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        405 => "METHOD_NOT_ALLOWED",
        409 => "CONFLICT",
        410 => "GONE",
        422 => "UNPROCESSABLE_ENTITY",
        429 => "TOO_MANY_REQUESTS",
        501 => "NOT_IMPLEMENTED",
        502 => "BAD_GATEWAY",
        503 => "SERVICE_UNAVAILABLE",
        _ => "INTERNAL_SERVER_ERROR",
    };
    format_ident!("{}", name)
}

/// Generate the query-params struct and route call for one operation.
fn generate_route(op: &OperationInfo, schemes: &[SchemeInfo]) -> (TokenStream, TokenStream) {
    let method_ident = &op.method_ident;
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let path = &op.path;
    let routing_method = format_ident!("{}", op.method);

    let alts = resolve_alternatives(&op.auth, schemes);
    let auth_query_keys: Vec<&str> = alts
        .iter()
        .filter_map(|s| match &s.kind {
            SchemeKind::ApiKey {
                key,
                location: ApiKeyIn::Query,
            } => Some(key.as_str()),
            _ => None,
        })
        .collect();

    let (path_extractor, path_fields) = build_path_extractor(&op.path_params);
    let (query_struct, query_extractor, query_fields) = build_query_extractor(op, &auth_query_keys);
    let (body_extractor, body_field) = build_body_extractor(op);

    // Extract spec-defined header params from the HeaderMap. Required headers are
    // bound up front so a missing one returns 400 before the handler is called;
    // optional headers map straight into the request struct as `Option<String>`.
    let mut header_stmts: Vec<TokenStream> = Vec::new();
    let header_fields: Vec<TokenStream> = op
        .header_params
        .iter()
        .map(|p| {
            let field_ident = &p.field_ident;
            let header_name = &p.name;
            if p.required {
                header_stmts.push(quote! {
                    let #field_ident = match headers
                        .get(#header_name)
                        .and_then(|v| v.to_str().ok())
                    {
                        ::core::option::Option::Some(v) => ::std::string::String::from(v),
                        ::core::option::Option::None => {
                            let msg = ::std::format!("missing required header `{}`", #header_name);
                            return (::axum::http::StatusCode::BAD_REQUEST, msg).into_response();
                        }
                    };
                });
                quote! { #field_ident, }
            } else {
                quote! {
                    #field_ident: headers
                        .get(#header_name)
                        .and_then(|v| v.to_str().ok())
                        .map(::std::string::String::from),
                }
            }
        })
        .collect();

    let mut closure_params: Vec<TokenStream> = vec![
        quote! { state: ::axum::extract::State<S> },
        quote! { headers: ::axum::http::HeaderMap },
    ];
    if let Some(p) = path_extractor {
        closure_params.push(p);
    }
    if let Some(p) = query_extractor {
        closure_params.push(p);
    }
    if let Some(p) = body_extractor {
        closure_params.push(p);
    }

    let mut req_fields: Vec<TokenStream> = path_fields;
    req_fields.extend(query_fields);
    req_fields.extend(header_fields);
    if let Some(f) = body_field {
        req_fields.push(f);
    }

    let (auth_extract, auth_call_arg) = build_auth_extractor(op, &alts);

    let route_call = quote! {
        .route(#path, ::axum::routing::#routing_method({
            let __api = __api.clone();
            move |#(#closure_params),*| {
                let __api = __api.clone();
                async move {
                    use ::axum::response::IntoResponse as _;
                    #auth_extract
                    #(#header_stmts)*
                    let req = #req_ident { #(#req_fields)* };
                    match __api.#method_ident(req, #auth_call_arg state, headers).await {
                        ::core::result::Result::Ok(r)  => r.into_response(),
                        ::core::result::Result::Err(e) => e.into_response(),
                    }
                }
            }
        }))
    };

    (query_struct, route_call)
}

/// Build the auth extraction block + the call-site argument for an operation.
///
/// Returns `(extract_stmts, call_arg)` where `call_arg` is either empty or
/// `auth,` (with the trailing comma) ready to splice between `req` and `state`.
fn build_auth_extractor(op: &OperationInfo, alts: &[&SchemeInfo]) -> (TokenStream, TokenStream) {
    if alts.is_empty() {
        return (quote! {}, quote! {});
    }

    let scheme_names: Vec<&str> = op.auth.alternatives.iter().map(String::as_str).collect();
    let scheme_label = scheme_names.join(",");

    if alts.len() == 1 {
        let scheme = alts[0];
        let extract = extract_scheme_expr(scheme);
        let ty = &scheme.ident;
        let stmts = quote! {
            let __extracted: ::core::option::Option<#ty> = #extract;
            let auth = match __extracted {
                ::core::option::Option::Some(a) => a,
                ::core::option::Option::None => {
                    let msg = ::std::format!("missing credentials for scheme `{}`", #scheme_label);
                    return (::axum::http::StatusCode::UNAUTHORIZED, msg).into_response();
                }
            };
        };
        return (stmts, quote! { auth, });
    }

    // Multiple alternatives — try each in order; wrap in op's auth enum.
    let enum_ident = auth_enum_ident(&op.operation_id);
    let try_arms: Vec<TokenStream> = alts
        .iter()
        .map(|s| {
            let variant = &s.ident;
            let extract = extract_scheme_expr(s);
            quote! {
                if __auth.is_none() {
                    if let ::core::option::Option::Some(v) = #extract {
                        __auth = ::core::option::Option::Some(#enum_ident::#variant(v));
                    }
                }
            }
        })
        .collect();

    let stmts = quote! {
        let mut __auth: ::core::option::Option<#enum_ident> = ::core::option::Option::None;
        #(#try_arms)*
        let auth = match __auth {
            ::core::option::Option::Some(a) => a,
            ::core::option::Option::None => {
                let msg = ::std::format!("missing credentials for scheme `{}`", #scheme_label);
                return (::axum::http::StatusCode::UNAUTHORIZED, msg).into_response();
            }
        };
    };
    (stmts, quote! { auth, })
}

/// Produce an expression that yields `Option<SchemeIdent>` for a single scheme.
fn extract_scheme_expr(scheme: &SchemeInfo) -> TokenStream {
    let ident = &scheme.ident;
    match &scheme.kind {
        SchemeKind::ApiKey {
            key,
            location: ApiKeyIn::Header,
        } => quote! {
            headers
                .get(#key)
                .and_then(|v| v.to_str().ok())
                .map(|s| #ident(s.to_string()))
        },
        SchemeKind::ApiKey {
            key,
            location: ApiKeyIn::Cookie,
        } => quote! {
            __lookup_cookie(&headers, #key).map(#ident)
        },
        SchemeKind::ApiKey {
            key,
            location: ApiKeyIn::Query,
        } => {
            let field = format_ident!("__auth_query_{}", key.to_snake_case());
            quote! {
                query_params.#field.clone().map(#ident)
            }
        }
        SchemeKind::HttpBearer => quote! {
            headers
                .get(::axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.strip_prefix("Bearer ")
                        .or_else(|| s.strip_prefix("bearer "))
                })
                .map(|t| #ident(t.to_string()))
        },
        SchemeKind::HttpBasic => quote! {
            headers
                .get(::axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.strip_prefix("Basic ")
                        .or_else(|| s.strip_prefix("basic "))
                })
                .and_then(__decode_basic_auth)
                .map(|(u, p)| #ident { username: u, password: p })
        },
    }
}

/// Returns (extractor param token, vec of `field_name: var` init tokens).
fn build_path_extractor(params: &[ParamInfo]) -> (Option<TokenStream>, Vec<TokenStream>) {
    if params.is_empty() {
        return (None, vec![]);
    }

    let types: Vec<TokenStream> = params.iter().map(|p| p.rust_type.clone()).collect();
    let var_idents: Vec<proc_macro2::Ident> = params
        .iter()
        .map(|p| format_ident!("path_{}", p.name.to_snake_case()))
        .collect();
    let field_idents: Vec<proc_macro2::Ident> =
        params.iter().map(|p| p.field_ident.clone()).collect();

    let extractor = if params.len() == 1 {
        let v = &var_idents[0];
        let t = &types[0];
        quote! {
            ::axum::extract::Path(#v):
                ::axum::extract::Path<#t>
        }
    } else {
        quote! {
            ::axum::extract::Path((#(#var_idents),*)):
                ::axum::extract::Path<(#(#types),*)>
        }
    };

    let inits: Vec<TokenStream> = field_idents
        .iter()
        .zip(var_idents.iter())
        .map(|(f, v)| quote! { #f: #v, })
        .collect();

    (Some(extractor), inits)
}

/// Returns (optional query struct definition, optional extractor param, vec of field init tokens
/// for the Request struct — auth fields are NOT included here since they're consumed by the
/// auth extractor).
fn build_query_extractor(
    op: &OperationInfo,
    auth_query_keys: &[&str],
) -> (TokenStream, Option<TokenStream>, Vec<TokenStream>) {
    if op.query_params.is_empty() && auth_query_keys.is_empty() {
        return (quote! {}, None, vec![]);
    }

    let struct_ident = format_ident!("{}QueryParams", op.operation_id.to_pascal_case());

    let mut struct_fields: Vec<TokenStream> = op
        .query_params
        .iter()
        .map(|p| {
            let field_ident = &p.field_ident;
            let rename_attr = if *field_ident == p.name.as_str() {
                quote! {}
            } else {
                let n = &p.name;
                quote! { #[serde(rename = #n)] }
            };
            let inner = &p.rust_type;
            let ftype = if p.required {
                quote! { #inner }
            } else {
                quote! { ::core::option::Option<#inner> }
            };
            quote! {
                #rename_attr
                pub #field_ident: #ftype,
            }
        })
        .collect();

    for key in auth_query_keys {
        let field_ident = format_ident!("__auth_query_{}", key.to_snake_case());
        let raw = *key;
        struct_fields.push(quote! {
            #[serde(rename = #raw, default)]
            pub #field_ident: ::core::option::Option<::std::string::String>,
        });
    }

    let query_struct = quote! {
        #[derive(::serde::Deserialize)]

        struct #struct_ident {
            #(#struct_fields)*
        }
    };

    let extractor = quote! {
        ::axum::extract::Query(query_params):
            ::axum::extract::Query<#struct_ident>
    };

    let inits: Vec<TokenStream> = op
        .query_params
        .iter()
        .map(|p| {
            let field_ident = &p.field_ident;
            quote! { #field_ident: query_params.#field_ident, }
        })
        .collect();

    (query_struct, Some(extractor), inits)
}

/// Build the body extractor param and field init for an operation.
fn build_body_extractor(op: &OperationInfo) -> (Option<TokenStream>, Option<TokenStream>) {
    op.body.as_ref().map_or_else(
        || (None, None),
        |body| {
            let ty = &body.rust_type;
            let extractor = quote! {
                ::axum::extract::Json(body):
                    ::axum::extract::Json<#ty>
            };
            let field_init = if body.required {
                quote! { body, }
            } else {
                quote! { body: ::core::option::Option::Some(body), }
            };
            (Some(extractor), Some(field_init))
        },
    )
}
