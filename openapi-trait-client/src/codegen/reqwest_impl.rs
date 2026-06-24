use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use openapi_trait_shared::codegen::operations::{OperationInfo, ParamInfo, ResponseStatus};

/// Generate a reqwest-backed implementation of the generated client trait.
pub fn generate_reqwest_impl(mod_ident: &syn::Ident, ops: &[OperationInfo]) -> TokenStream {
    let module_name = mod_ident.to_string().to_pascal_case();
    let trait_name = format_ident!("{}Client", module_name);
    let error_name = format_ident!("Reqwest{}ClientError", module_name);

    let methods: Vec<TokenStream> = ops
        .iter()
        .map(|op| generate_impl_method(op, &error_name))
        .collect();

    quote! {
        #[derive(::core::fmt::Debug)]
        pub enum #error_name {
            Transport(::openapi_trait::reqwest::Error),
            MissingRequiredHeader {
                operation: &'static str,
                header: &'static str,
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
                    Self::MissingRequiredHeader { operation, header } => {
                        write!(formatter, "missing required header `{header}` for `{operation}`")
                    }
                    Self::UnexpectedStatus {
                        operation,
                        status,
                        body,
                    } => write!(
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
                    Self::MissingRequiredHeader { .. } | Self::UnexpectedStatus { .. } => {
                        ::core::option::Option::None
                    }
                }
            }
        }

        impl ::core::convert::From<::openapi_trait::reqwest::Error> for #error_name {
            fn from(error: ::openapi_trait::reqwest::Error) -> Self {
                Self::Transport(error)
            }
        }

        fn encode_path_param(value: &impl ::core::fmt::Display) -> ::std::string::String {
            ::openapi_trait::percent_encoding::utf8_percent_encode(
                &value.to_string(),
                ::openapi_trait::percent_encoding::NON_ALPHANUMERIC,
            )
            .to_string()
        }

        impl<T> #trait_name for T
        where
            T: ::openapi_trait::ReqwestClientCore + ::core::marker::Send + ::core::marker::Sync,
        {
            type Error = #error_name;

            #(#methods)*
        }
    }
}

/// Generate one reqwest-backed trait method for a single `OpenAPI` operation.
fn generate_impl_method(op: &OperationInfo, error_name: &proc_macro2::Ident) -> TokenStream {
    let method_ident = format_ident!("{}", op.operation_id.to_snake_case());
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
        .map(|param| format_ident!("{}", param.name.to_snake_case()))
        .chain(op.body.iter().map(|_| format_ident!("body")))
        .collect();

    let path_replacements: Vec<TokenStream> = op
        .path_params
        .iter()
        .map(|param| {
            let field_ident = format_ident!("{}", param.name.to_snake_case());
            let placeholder = format!("{{{}}}", param.name);
            quote! {
                path = path.replace(#placeholder, &encode_path_param(&#field_ident));
            }
        })
        .collect();

    let query_struct = generate_query_struct(op);
    let query_builder = generate_query_builder(op);
    let header_builder = generate_header_builder(op, error_name, operation_name);
    let body_builder = generate_body_builder(op);
    let (response_arms, fallback) =
        generate_response_match(op, error_name, &resp_ident, operation_name);

    quote! {
        fn #method_ident(
            &self,
            req: #req_ident,
            options: ::core::option::Option<::openapi_trait::RequestOptions>,
        ) -> impl ::std::future::Future<Output = ::core::result::Result<#resp_ident, Self::Error>> + Send {
            let client = ::openapi_trait::ReqwestClientCore::reqwest_client(self).clone();
            let base_url = ::openapi_trait::ReqwestClientCore::base_url(self).to_owned();

            async move {
                let #req_ident { #(#request_fields),* } = req;
                let mut path = ::std::string::String::from(#path);
                #(#path_replacements)*

                let url = format!("{}{}", base_url.trim_end_matches('/'), path);
                let mut request = client.#http_method(url);

                #query_struct
                #query_builder
                #header_builder
                #body_builder

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

/// Generate a serializable helper struct for query parameters.
fn generate_query_struct(op: &OperationInfo) -> TokenStream {
    if op.query_params.is_empty() {
        return quote! {};
    }

    let struct_ident = format_ident!("{}ReqwestQuery", op.operation_id.to_pascal_case());
    let fields: Vec<TokenStream> = op
        .query_params
        .iter()
        .map(generate_query_struct_field)
        .collect();

    quote! {
        #[derive(::serde::Serialize)]
        struct #struct_ident<'a> {
            #(#fields)*
        }
    }
}

/// Generate one field for the reqwest query helper struct.
fn generate_query_struct_field(param: &ParamInfo) -> TokenStream {
    let field_ident = format_ident!("{}", param.name.to_snake_case());
    let ty = &param.rust_type;
    let field_type = if param.required {
        quote! { &'a #ty }
    } else {
        quote! { &'a ::core::option::Option<#ty> }
    };
    let rename = &param.name;
    let skip_attr = if param.required {
        quote! {}
    } else {
        quote! { #[serde(skip_serializing_if = "::core::option::Option::is_none")] }
    };

    quote! {
        #[serde(rename = #rename)]
        #skip_attr
        #field_ident: #field_type,
    }
}

/// Generate the reqwest query population code for an operation.
fn generate_query_builder(op: &OperationInfo) -> TokenStream {
    if op.query_params.is_empty() {
        return quote! {};
    }

    let struct_ident = format_ident!("{}ReqwestQuery", op.operation_id.to_pascal_case());
    let fields: Vec<TokenStream> = op
        .query_params
        .iter()
        .map(|param| {
            let field_ident = format_ident!("{}", param.name.to_snake_case());
            quote! { #field_ident: &#field_ident, }
        })
        .collect();

    quote! {
        let query = #struct_ident { #(#fields)* };
        request = request.query(&query);
    }
}

/// Generate the reqwest header population code for an operation.
fn generate_header_builder(
    op: &OperationInfo,
    error_name: &proc_macro2::Ident,
    operation_name: &str,
) -> TokenStream {
    let header_updates: Vec<TokenStream> = op
        .header_params
        .iter()
        .map(|param| {
            let field_ident = format_ident!("{}", param.name.to_snake_case());
            let header_name = &param.name;

            if param.required {
                quote! {
                    let #field_ident = match #field_ident {
                        ::core::option::Option::Some(value) => value,
                        ::core::option::Option::None => {
                            return ::core::result::Result::Err(#error_name::MissingRequiredHeader {
                                operation: #operation_name,
                                header: #header_name,
                            });
                        }
                    };
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
