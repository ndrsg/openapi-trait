use heck::{ToPascalCase, ToSnakeCase};
use openapiv3::{
    MediaType, OpenAPI, Operation, Parameter, PathItem, ReferenceOr, RequestBody, Response,
    Responses, Schema, StatusCode,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::schemas::doc_attr;
use super::types::{is_string_enum, schema_to_rust_type, string_enum_values};

/// All information about a single API operation needed by later codegen stages.
#[derive(Debug)]
pub struct OperationInfo {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub path_params: Vec<ParamInfo>,
    pub query_params: Vec<ParamInfo>,
    pub header_params: Vec<ParamInfo>,
    pub body: Option<BodyInfo>,
    pub responses: Vec<ResponseInfo>,
}

#[derive(Debug)]
pub struct ParamInfo {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    /// The Rust type for this parameter (e.g. `i64`, `String`, or an enum ident).
    pub rust_type: TokenStream,
    /// True when this param's schema is a string enum (so we emit a dedicated enum type).
    pub is_enum: bool,
    /// The enum ident when `is_enum` is true, e.g. `FindPetsByStatusStatusQuery`.
    pub enum_ident: Option<syn::Ident>,
    /// Enum values when `is_enum` is true.
    pub enum_values: Vec<String>,
}

#[derive(Debug)]
pub struct BodyInfo {
    pub description: Option<String>,
    pub required: bool,
    pub rust_type: TokenStream,
}

#[derive(Debug)]
pub struct ResponseInfo {
    pub status: ResponseStatus,
    pub description: String,
    pub rust_type: Option<TokenStream>,
}

#[derive(Debug)]
pub enum ResponseStatus {
    Code(u16),
    Default,
}

/// Collect all operations from the `OpenAPI` document.
#[must_use]
pub fn collect_operations(openapi: &OpenAPI) -> Vec<OperationInfo> {
    let mut ops = Vec::new();
    for (path, ref_or_item) in &openapi.paths.paths {
        let item = match ref_or_item {
            ReferenceOr::Item(i) => i,
            ReferenceOr::Reference { .. } => continue,
        };
        for (method, operation) in path_item_operations(item) {
            if let Some(info) = build_operation_info(path, &method, operation, item, openapi) {
                ops.push(info);
            }
        }
    }
    ops
}

/// Returns all operations in a path item with their HTTP methods.
fn path_item_operations(item: &PathItem) -> Vec<(String, &Operation)> {
    let mut out = Vec::new();
    if let Some(op) = &item.get {
        out.push(("get".into(), op));
    }
    if let Some(op) = &item.post {
        out.push(("post".into(), op));
    }
    if let Some(op) = &item.put {
        out.push(("put".into(), op));
    }
    if let Some(op) = &item.delete {
        out.push(("delete".into(), op));
    }
    if let Some(op) = &item.patch {
        out.push(("patch".into(), op));
    }
    if let Some(op) = &item.head {
        out.push(("head".into(), op));
    }
    if let Some(op) = &item.options {
        out.push(("options".into(), op));
    }
    if let Some(op) = &item.trace {
        out.push(("trace".into(), op));
    }
    out
}

/// Build an `OperationInfo` for a single operation in the path item.
fn build_operation_info(
    path: &str,
    method: &str,
    operation: &Operation,
    path_item: &PathItem,
    openapi: &OpenAPI,
) -> Option<OperationInfo> {
    let operation_id = operation.operation_id.clone()?;

    // Collect parameters: path-level then operation-level (operation wins on name clash)
    let mut all_params: Vec<&ReferenceOr<Parameter>> = Vec::new();
    all_params.extend(path_item.parameters.iter());
    all_params.extend(operation.parameters.iter());

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();

    for ref_or_param in &all_params {
        let param = match ref_or_param {
            ReferenceOr::Item(p) => p,
            ReferenceOr::Reference { reference } => {
                // Try to resolve from components
                if let Some(resolved) = resolve_param_ref(reference, openapi) {
                    resolved
                } else {
                    continue;
                }
            }
        };

        let data = param.parameter_data_ref();
        let param_schema = param_schema(param, openapi);

        let (is_enum, enum_ident, enum_values) = param_schema.as_ref().map_or_else(
            || (false, None, vec![]),
            |schema| {
                if is_string_enum(schema) {
                    let ident = format_ident!(
                        "{}{}Query",
                        operation_id.to_pascal_case(),
                        data.name.to_pascal_case()
                    );
                    let vals = string_enum_values(schema);
                    (true, Some(ident), vals)
                } else {
                    (false, None, vec![])
                }
            },
        );

        let rust_type = if is_enum {
            let ei = enum_ident.as_ref().unwrap();
            quote! { #ei }
        } else if let Some(schema) = &param_schema {
            let ref_or = ReferenceOr::Item(schema.clone());
            schema_to_rust_type(&ref_or, true)
        } else {
            quote! { ::std::string::String }
        };

        let info = ParamInfo {
            name: data.name.clone(),
            description: data.description.clone(),
            required: data.required,
            rust_type,
            is_enum,
            enum_ident,
            enum_values,
        };

        match param {
            Parameter::Path { .. } => path_params.push(info),
            Parameter::Query { .. } => query_params.push(info),
            Parameter::Header { .. } => header_params.push(info),
            Parameter::Cookie { .. } => {}
        }
    }

    let body = operation
        .request_body
        .as_ref()
        .and_then(|rb| build_body_info(rb, openapi));

    let responses = build_responses(&operation.responses, openapi);

    Some(OperationInfo {
        operation_id,
        method: method.to_owned(),
        path: path.to_owned(),
        summary: operation.summary.clone(),
        description: operation.description.clone(),
        path_params,
        query_params,
        header_params,
        body,
        responses,
    })
}

/// Resolve a `$ref` to a parameter from the components section.
fn resolve_param_ref<'a>(reference: &str, openapi: &'a OpenAPI) -> Option<&'a Parameter> {
    let name = reference.strip_prefix("#/components/parameters/")?;
    openapi.components.as_ref()?.parameters.get(name)?.as_item()
}

/// Extract the schema for a parameter, resolving component refs if needed.
fn param_schema(param: &Parameter, openapi: &OpenAPI) -> Option<Schema> {
    use openapiv3::ParameterSchemaOrContent;
    let data = param.parameter_data_ref();
    match &data.format {
        ParameterSchemaOrContent::Schema(ref_or) => match ref_or {
            ReferenceOr::Item(s) => Some(s.clone()),
            ReferenceOr::Reference { reference } => {
                let name = reference.strip_prefix("#/components/schemas/")?;
                openapi
                    .components
                    .as_ref()?
                    .schemas
                    .get(name)?
                    .as_item()
                    .cloned()
            }
        },
        ParameterSchemaOrContent::Content(_) => None,
    }
}

/// Build body info from a request body reference.
fn build_body_info(ref_or_rb: &ReferenceOr<RequestBody>, openapi: &OpenAPI) -> Option<BodyInfo> {
    let rb = match ref_or_rb {
        ReferenceOr::Item(r) => r,
        ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/requestBodies/")?;
            openapi
                .components
                .as_ref()?
                .request_bodies
                .get(name)?
                .as_item()?
        }
    };

    let rust_type = json_media_type_to_rust(&rb.content, openapi)?;

    Some(BodyInfo {
        description: rb.description.clone(),
        required: rb.required,
        rust_type,
    })
}

/// Extract the Rust type from a JSON media type content map.
fn json_media_type_to_rust(
    content: &indexmap::IndexMap<String, MediaType>,
    _openapi: &OpenAPI,
) -> Option<TokenStream> {
    let media = content
        .get("application/json")
        .or_else(|| content.values().next())?;
    let ref_or_schema = media.schema.as_ref()?;
    Some(schema_to_rust_type(ref_or_schema, true))
}

/// Build response info list from an `OpenAPI` responses object.
fn build_responses(responses: &Responses, openapi: &OpenAPI) -> Vec<ResponseInfo> {
    let mut out = Vec::new();

    for (status_code, ref_or_resp) in &responses.responses {
        let resp = match ref_or_resp {
            ReferenceOr::Item(r) => r,
            ReferenceOr::Reference { reference } => {
                if let Some(r) = resolve_response_ref(reference, openapi) {
                    r
                } else {
                    continue;
                }
            }
        };

        let rust_type = json_media_type_to_rust(&resp.content, openapi);

        let status = match status_code {
            StatusCode::Code(n) => ResponseStatus::Code(*n),
            StatusCode::Range(_) => continue, // skip range codes
        };

        out.push(ResponseInfo {
            status,
            description: resp.description.clone(),
            rust_type,
        });
    }

    // Handle default response
    if let Some(ref_or_default) = &responses.default {
        let resp = match ref_or_default {
            ReferenceOr::Item(r) => r,
            ReferenceOr::Reference { reference } => {
                if let Some(r) = resolve_response_ref(reference, openapi) {
                    r
                } else {
                    return out;
                }
            }
        };
        out.push(ResponseInfo {
            status: ResponseStatus::Default,
            description: resp.description.clone(),
            rust_type: None, // Default carries a String message
        });
    }

    out
}

/// Resolve a `$ref` to a response from the components section.
fn resolve_response_ref<'a>(reference: &str, openapi: &'a OpenAPI) -> Option<&'a Response> {
    let name = reference.strip_prefix("#/components/responses/")?;
    openapi.components.as_ref()?.responses.get(name)?.as_item()
}

/// Generate query-param enum types + request structs + response enums for all operations.
pub fn generate_operation_types(ops: &[OperationInfo]) -> TokenStream {
    let items: Vec<TokenStream> = ops.iter().map(generate_single_operation_types).collect();
    quote! { #(#items)* }
}

/// Generate all types for a single operation.
fn generate_single_operation_types(op: &OperationInfo) -> TokenStream {
    let query_enums = generate_query_enums(op);
    let request_struct = generate_request_struct(op);
    let response_enum = generate_response_enum(op);
    quote! {
        #query_enums
        #request_struct
        #response_enum
    }
}

/// Generate enum types for query parameters with string enum schemas.
fn generate_query_enums(op: &OperationInfo) -> TokenStream {
    let enums: Vec<TokenStream> = op
        .query_params
        .iter()
        .filter(|p| p.is_enum)
        .map(|p| {
            let ident = p.enum_ident.as_ref().unwrap();
            let doc = doc_attr(&p.description);
            let variants: Vec<TokenStream> = p
                .enum_values
                .iter()
                .map(|v| {
                    let variant_ident = format_ident!("{}", v.to_pascal_case());
                    if variant_ident == v.as_str() {
                        quote! { #variant_ident }
                    } else {
                        quote! {
                            #[serde(rename = #v)]
                            #variant_ident
                        }
                    }
                })
                .collect();

            quote! {
                #doc
                #[derive(
                    ::core::fmt::Debug,
                    ::core::clone::Clone,
                    ::serde::Serialize,
                    ::serde::Deserialize,
                )]

                pub enum #ident {
                    #(#variants,)*
                }
            }
        })
        .collect();

    quote! { #(#enums)* }
}

/// Generate the request struct for an operation.
fn generate_request_struct(op: &OperationInfo) -> TokenStream {
    let ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let doc = combined_doc(op.summary.as_ref(), op.description.as_ref());

    let mut fields: Vec<TokenStream> = Vec::new();

    for p in &op.path_params {
        let field_ident = format_ident!("{}", p.name.to_snake_case());
        let ftype = &p.rust_type;
        let fdoc = doc_attr(&p.description);
        fields.push(quote! {
            #fdoc
            pub #field_ident: #ftype,
        });
    }

    for p in &op.query_params {
        let field_ident = format_ident!("{}", p.name.to_snake_case());
        let inner = &p.rust_type;
        let ftype = if p.required {
            quote! { #inner }
        } else {
            quote! { ::core::option::Option<#inner> }
        };
        let fdoc = doc_attr(&p.description);
        fields.push(quote! {
            #fdoc
            pub #field_ident: #ftype,
        });
    }

    for p in &op.header_params {
        let field_ident = format_ident!("{}", p.name.to_snake_case());
        let fdoc = doc_attr(&p.description);
        // Header params are always Option<String> since extraction from HeaderMap can fail
        fields.push(quote! {
            #fdoc
            pub #field_ident: ::core::option::Option<::std::string::String>,
        });
    }

    if let Some(body) = &op.body {
        let inner = &body.rust_type;
        let ftype = if body.required {
            quote! { #inner }
        } else {
            quote! { ::core::option::Option<#inner> }
        };
        let bdoc = doc_attr(&body.description);
        fields.push(quote! {
            #bdoc
            pub body: #ftype,
        });
    }

    quote! {
        #doc
        #[derive(::core::fmt::Debug, ::core::clone::Clone)]
        pub struct #ident {
            #(#fields)*
        }
    }
}

/// Generate the response enum for an operation.
fn generate_response_enum(op: &OperationInfo) -> TokenStream {
    let ident = format_ident!("{}Response", op.operation_id.to_pascal_case());
    let doc = combined_doc(op.summary.as_ref(), op.description.as_ref());

    let variants: Vec<TokenStream> = op
        .responses
        .iter()
        .map(|r| {
            let vdoc = doc_attr(&Some(r.description.clone()));
            match &r.status {
                ResponseStatus::Code(n) => {
                    let variant_ident = format_ident!("Status{}", n);
                    r.rust_type.as_ref().map_or_else(
                        || {
                            quote! {
                                #vdoc
                                #variant_ident
                            }
                        },
                        |ty| {
                            quote! {
                                #vdoc
                                #variant_ident(#ty)
                            }
                        },
                    )
                }
                ResponseStatus::Default => {
                    quote! {
                        #vdoc
                        Default(::std::string::String)
                    }
                }
            }
        })
        .collect();

    quote! {
        #doc
        #[derive(::core::fmt::Debug, ::core::clone::Clone)]
        pub enum #ident {
            #(#variants,)*
        }
    }
}

/// Combine summary and description into doc attributes.
fn combined_doc(summary: Option<&String>, description: Option<&String>) -> TokenStream {
    match (summary, description) {
        (Some(s), Some(d)) if s != d => quote! { #[doc = #s] #[doc = ""] #[doc = #d] },
        (Some(s), _) => quote! { #[doc = #s] },
        (None, Some(d)) => quote! { #[doc = #d] },
        (None, None) => quote! {},
    }
}
