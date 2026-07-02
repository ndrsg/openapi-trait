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
    let route_calls: Vec<TokenStream> =
        ops.iter().map(|op| generate_route(op, schemes)).collect();

    let auth_helpers = generate_auth_helpers(schemes);
    let query_helpers = generate_query_helpers(ops, schemes);

    quote! {
        #(#into_response_impls)*

        #auth_helpers
        #query_helpers

        fn make_router<T, S>(__api: ::std::sync::Arc<T>) -> ::axum::Router<S>
        where
            T: #trait_name<S> + ::core::marker::Send + ::core::marker::Sync + 'static,
            S: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static,
        {
            ::axum::Router::new()
                #(#route_calls)*
        }
    }
}

/// Collect the query-string keys used by an operation's active auth schemes
/// (API keys carried `in: query`), so the router can read them out of the parsed
/// query pairs.
fn collect_auth_query_keys<'a>(op: &'a OperationInfo, schemes: &'a [SchemeInfo]) -> Vec<&'a str> {
    resolve_alternatives(&op.auth, schemes)
        .iter()
        .filter_map(|s| match &s.kind {
            SchemeKind::ApiKey {
                key,
                location: ApiKeyIn::Query,
            } => Some(key.as_str()),
            _ => None,
        })
        .collect()
}

/// Whether an operation's route needs to parse the raw query string at all
/// (it has declared query params or reads an auth credential from the query).
fn route_uses_query(op: &OperationInfo, schemes: &[SchemeInfo]) -> bool {
    !op.query_params.is_empty() || !collect_auth_query_keys(op, schemes).is_empty()
}

/// Emit the shared query-parsing helpers, but only the ones some route uses.
///
/// `__parse_query` decodes the raw query into `(name, value)` pairs;
/// `__query_de` converts a single decoded string into a typed value, trying a
/// bare JSON parse first (numbers/bools) and falling back to a JSON string
/// (plain strings, enums, dates, uuids).
fn generate_query_helpers(ops: &[OperationInfo], schemes: &[SchemeInfo]) -> TokenStream {
    let needs_parse = ops.iter().any(|op| route_uses_query(op, schemes));
    if !needs_parse {
        return quote! {};
    }
    let needs_de = ops.iter().any(|op| !op.query_params.is_empty());

    let parse = quote! {
        fn __parse_query(
            raw: &::core::option::Option<::std::string::String>,
        ) -> ::std::vec::Vec<(::std::string::String, ::std::string::String)> {
            match raw {
                ::core::option::Option::Some(q) => {
                    ::openapi_trait::form_urlencoded::parse(q.as_bytes())
                        .map(|(k, v)| (k.into_owned(), v.into_owned()))
                        .collect()
                }
                ::core::option::Option::None => ::std::vec::Vec::new(),
            }
        }
    };

    let de = if needs_de {
        quote! {
            fn __query_de<T: ::serde::de::DeserializeOwned>(raw: &str) -> ::core::option::Option<T> {
                if let ::core::result::Result::Ok(value) = ::serde_json::from_str::<T>(raw) {
                    return ::core::option::Option::Some(value);
                }
                let quoted = ::serde_json::Value::String(raw.to_string()).to_string();
                ::serde_json::from_str::<T>(&quoted).ok()
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #parse
        #de
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

/// Generate the route call for one operation.
fn generate_route(op: &OperationInfo, schemes: &[SchemeInfo]) -> TokenStream {
    let method_ident = &op.method_ident;
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let path = &op.path;
    let routing_method = format_ident!("{}", op.method);

    let alts = resolve_alternatives(&op.auth, schemes);
    let auth_query_keys = collect_auth_query_keys(op, schemes);

    let (path_extractor, path_fields) = build_path_extractor(&op.path_params);
    let (query_extractor, query_stmts, query_fields) = build_query_extractor(op, &auth_query_keys);
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

    quote! {
        .route(#path, ::axum::routing::#routing_method({
            let __api = __api.clone();
            move |#(#closure_params),*| {
                let __api = __api.clone();
                async move {
                    use ::axum::response::IntoResponse as _;
                    #(#query_stmts)*
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
    }
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
            // Bound as a local `Option<String>` by the query extractor.
            let field = format_ident!("__auth_query_{}", key.to_snake_case());
            quote! {
                #field.clone().map(#ident)
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

/// Build the query handling for an operation.
///
/// Returns `(extractor param, binding statements, request-field inits)`. The
/// statements parse the raw query into typed locals (honoring each param's
/// `style`/`explode`) and bind any auth-query credentials; the inits move those
/// locals into the request struct via field shorthand. Auth fields are consumed
/// by the auth extractor, so they are not part of the inits.
fn build_query_extractor(
    op: &OperationInfo,
    auth_query_keys: &[&str],
) -> (Option<TokenStream>, Vec<TokenStream>, Vec<TokenStream>) {
    if op.query_params.is_empty() && auth_query_keys.is_empty() {
        return (None, vec![], vec![]);
    }

    let extractor = quote! {
        ::axum::extract::RawQuery(__raw_query): ::axum::extract::RawQuery
    };

    let mut stmts: Vec<TokenStream> = vec![quote! {
        let __pairs = __parse_query(&__raw_query);
    }];
    stmts.extend(op.query_params.iter().map(query_param_binding));

    for key in auth_query_keys {
        let field_ident = format_ident!("__auth_query_{}", key.to_snake_case());
        let raw = *key;
        stmts.push(quote! {
            let #field_ident: ::core::option::Option<::std::string::String> = __pairs
                .iter()
                .rev()
                .find(|(k, _)| k == #raw)
                .map(|(_, v)| v.clone());
        });
    }

    let inits: Vec<TokenStream> = op
        .query_params
        .iter()
        .map(|p| {
            let field_ident = &p.field_ident;
            quote! { #field_ident, }
        })
        .collect();

    (Some(extractor), stmts, inits)
}

/// Emit the statement binding one query parameter to a typed local, parsing it
/// out of `__pairs` per the param's `style`/`explode`. Required params that are
/// missing or fail to parse return `400 Bad Request`.
fn query_param_binding(param: &ParamInfo) -> TokenStream {
    if param.query.is_array {
        array_query_binding(param)
    } else {
        scalar_query_binding(param)
    }
}

/// An expression yielding `Option<&str>` for the last query pair named `name`.
fn last_query_value(name: &str) -> TokenStream {
    quote! {
        __pairs.iter().rev().find(|(k, _)| k == #name).map(|(_, v)| v.as_str())
    }
}

/// A `return 400` statement for a malformed value of the named parameter.
fn invalid_query_param(name: &str) -> TokenStream {
    quote! {
        let msg = ::std::format!("invalid query parameter `{}`", #name);
        return (::axum::http::StatusCode::BAD_REQUEST, msg).into_response();
    }
}

/// Bind an array query parameter, honoring `explode` (repeated keys) vs. the
/// delimited single-value form.
fn array_query_binding(param: &ParamInfo) -> TokenStream {
    let field_ident = &param.field_ident;
    let name = &param.name;
    let item_ty = param
        .query
        .array_item_type
        .clone()
        .unwrap_or_else(|| quote! { ::serde_json::Value });
    let bad_element = invalid_query_param(name);

    if param.query.explode {
        // One repeated key per element.
        let build = quote! {
            let mut __acc: ::std::vec::Vec<#item_ty> = ::std::vec::Vec::new();
            for (__k, __v) in &__pairs {
                if __k == #name {
                    match __query_de::<#item_ty>(__v) {
                        ::core::option::Option::Some(__el) => __acc.push(__el),
                        ::core::option::Option::None => { #bad_element }
                    }
                }
            }
        };
        if param.required {
            quote! { let #field_ident = { #build __acc }; }
        } else {
            quote! {
                let #field_ident = {
                    #build
                    if __acc.is_empty() {
                        ::core::option::Option::None
                    } else {
                        ::core::option::Option::Some(__acc)
                    }
                };
            }
        }
    } else {
        // A single delimited value.
        let delim = param.query.style.delimiter();
        let parse_value = quote! {
            let mut __acc: ::std::vec::Vec<#item_ty> = ::std::vec::Vec::new();
            if !__raw.is_empty() {
                for __part in __raw.split(#delim) {
                    match __query_de::<#item_ty>(__part) {
                        ::core::option::Option::Some(__el) => __acc.push(__el),
                        ::core::option::Option::None => { #bad_element }
                    }
                }
            }
        };
        let found = last_query_value(name);
        if param.required {
            quote! {
                let #field_ident = match #found {
                    ::core::option::Option::Some(__raw) => { #parse_value __acc },
                    ::core::option::Option::None => ::std::vec::Vec::new(),
                };
            }
        } else {
            quote! {
                let #field_ident = match #found {
                    ::core::option::Option::Some(__raw) => { #parse_value ::core::option::Option::Some(__acc) },
                    ::core::option::Option::None => ::core::option::Option::None,
                };
            }
        }
    }
}

/// Bind a scalar query parameter (also the object/deepObject fallback): a single
/// value round-tripped through `__query_de`.
fn scalar_query_binding(param: &ParamInfo) -> TokenStream {
    let field_ident = &param.field_ident;
    let name = &param.name;
    let ty = &param.rust_type;
    let found = last_query_value(name);
    let invalid = invalid_query_param(name);

    if param.required {
        quote! {
            let #field_ident = match #found {
                ::core::option::Option::Some(__raw) => match __query_de::<#ty>(__raw) {
                    ::core::option::Option::Some(__v) => __v,
                    ::core::option::Option::None => { #invalid }
                },
                ::core::option::Option::None => {
                    let msg = ::std::format!("missing required query parameter `{}`", #name);
                    return (::axum::http::StatusCode::BAD_REQUEST, msg).into_response();
                }
            };
        }
    } else {
        quote! {
            let #field_ident = match #found {
                ::core::option::Option::Some(__raw) => match __query_de::<#ty>(__raw) {
                    ::core::option::Option::Some(__v) => ::core::option::Option::Some(__v),
                    ::core::option::Option::None => { #invalid }
                },
                ::core::option::Option::None => ::core::option::Option::None,
            };
        }
    }
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
