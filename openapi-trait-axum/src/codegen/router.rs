use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::{OperationInfo, ParamInfo, ResponseStatus};

/// Generate `IntoResponse` impls and the private `make_router` free function.
pub fn generate_router(mod_ident: &syn::Ident, ops: &[OperationInfo]) -> TokenStream {
    let trait_name = format_ident!("{}Api", mod_ident.to_string().to_pascal_case());

    let into_response_impls: Vec<TokenStream> =
        ops.iter().map(generate_into_response_impl).collect();
    let (query_structs, route_calls): (Vec<_>, Vec<_>) = ops.iter().map(generate_route).unzip();

    quote! {
        #(#into_response_impls)*

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
fn generate_route(op: &OperationInfo) -> (TokenStream, TokenStream) {
    let method_ident = format_ident!("{}", op.operation_id.to_snake_case());
    let req_ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let path = &op.path;
    let routing_method = format_ident!("{}", op.method);

    let (path_extractor, path_fields) = build_path_extractor(&op.path_params);
    let (query_struct, query_extractor, query_fields) = build_query_extractor(op);
    let (body_extractor, body_field) = build_body_extractor(op);

    // Extract spec-defined header params from the HeaderMap
    let header_fields: Vec<TokenStream> = op
        .header_params
        .iter()
        .map(|p| {
            let field_ident = format_ident!("{}", p.name.to_snake_case());
            let header_name = &p.name;
            quote! {
                #field_ident: headers
                    .get(#header_name)
                    .and_then(|v| v.to_str().ok())
                    .map(::std::string::String::from),
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

    let route_call = quote! {
        .route(#path, ::axum::routing::#routing_method({
            let __api = __api.clone();
            move |#(#closure_params),*| {
                let __api = __api.clone();
                async move {
                    use ::axum::response::IntoResponse as _;
                    let req = #req_ident { #(#req_fields)* };
                    match __api.#method_ident(req, state, headers).await {
                        ::core::result::Result::Ok(r)  => r.into_response(),
                        ::core::result::Result::Err(e) => e.into_response(),
                    }
                }
            }
        }))
    };

    (query_struct, route_call)
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
    let field_idents: Vec<proc_macro2::Ident> = params
        .iter()
        .map(|p| format_ident!("{}", p.name.to_snake_case()))
        .collect();

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

/// Returns (optional query struct definition, optional extractor param, vec of field init tokens).
fn build_query_extractor(
    op: &OperationInfo,
) -> (TokenStream, Option<TokenStream>, Vec<TokenStream>) {
    if op.query_params.is_empty() {
        return (quote! {}, None, vec![]);
    }

    let struct_ident = format_ident!("{}QueryParams", op.operation_id.to_pascal_case());

    let struct_fields: Vec<TokenStream> = op
        .query_params
        .iter()
        .map(|p| {
            let field_ident = format_ident!("{}", p.name.to_snake_case());
            let rename_attr = if field_ident == p.name.as_str() {
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
            let field_ident = format_ident!("{}", p.name.to_snake_case());
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
