use heck::ToPascalCase;
use openapiv3::{
    MediaType, OpenAPI, Operation, Parameter, PathItem, ReferenceOr, RequestBody, Response,
    Responses, Schema, StatusCode,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::idents;
use super::schemas::doc_attr;
use super::security::{resolve_op_security, OpSecurity, SchemeInfo};
use super::types::{is_string_enum, schema_to_rust_type, string_enum_values};

/// All information about a single API operation needed by later codegen stages.
#[derive(Debug)]
pub struct OperationInfo {
    pub operation_id: String,
    /// Keyword-safe Rust method identifier derived from `operation_id`
    /// (snake-cased, raw-escaped for keywords, e.g. `r#type`).
    pub method_ident: syn::Ident,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub path_params: Vec<ParamInfo>,
    pub query_params: Vec<ParamInfo>,
    pub header_params: Vec<ParamInfo>,
    pub body: Option<BodyInfo>,
    pub responses: Vec<ResponseInfo>,
    pub auth: OpSecurity,
}

#[derive(Debug)]
pub struct ParamInfo {
    pub name: String,
    /// Keyword-safe Rust struct-field identifier derived from `name`
    /// (snake-cased, raw-escaped for keywords, e.g. `r#type`).
    pub field_ident: syn::Ident,
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

/// Collected codegen diagnostics, separated by severity.
///
/// Errors are turned into `compile_error!` tokens (the build fails); warnings
/// are printed to stderr during macro expansion (the build proceeds). This
/// keeps unsupported `OpenAPI` constructs from being dropped silently.
#[derive(Debug, Default)]
pub struct Diagnostics {
    /// Fatal problems, emitted as `compile_error!` tokens.
    pub errors: Vec<String>,
    /// Non-fatal skips, emitted as `eprintln!` warnings; the build proceeds.
    pub warnings: Vec<String>,
}

impl Diagnostics {
    /// Record a fatal diagnostic.
    fn error(&mut self, msg: String) {
        self.errors.push(msg);
    }

    /// Record a non-fatal diagnostic.
    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Print all collected warnings to stderr during macro expansion.
    pub fn emit_warnings(&self) {
        for warning in &self.warnings {
            eprintln!("openapi-trait: warning: {warning}");
        }
    }
}

/// Collect all operations from the `OpenAPI` document.
///
/// Returns the operations along with any [`Diagnostics`] gathered while walking
/// the spec (unsupported constructs, unresolved `$ref`s, missing
/// `operationId`s).
#[must_use]
pub fn collect_operations(
    openapi: &OpenAPI,
    schemes: &[SchemeInfo],
) -> (Vec<OperationInfo>, Diagnostics) {
    let mut ops = Vec::new();
    let mut diag = Diagnostics::default();
    for (path, ref_or_item) in &openapi.paths.paths {
        let item = match ref_or_item {
            ReferenceOr::Item(i) => i,
            ReferenceOr::Reference { .. } => {
                diag.warn(format!(
                    "path `{path}` is a $ref to a path item, which is not supported; all its operations were skipped"
                ));
                continue;
            }
        };
        for (method, operation) in path_item_operations(item) {
            if let Some(info) =
                build_operation_info(path, &method, operation, item, openapi, schemes, &mut diag)
            {
                ops.push(info);
            }
        }
    }
    (ops, diag)
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
    schemes: &[SchemeInfo],
    diag: &mut Diagnostics,
) -> Option<OperationInfo> {
    let Some(operation_id) = operation.operation_id.clone() else {
        diag.error(format!(
            "operation `{method} {path}` is missing an `operationId`; one is required to name the generated Rust method"
        ));
        return None;
    };

    // Validate the operationId yields usable Rust identifiers up front, so that
    // keyword/invalid names surface a clear diagnostic instead of a downstream
    // proc-macro panic. Keywords become raw identifiers (`type` -> `r#type`);
    // truly invalid names drop the operation with an error.
    let method_ident = match idents::method_ident(&operation_id) {
        Ok(id) => id,
        Err(msg) => {
            diag.error(format!("operation `{method} {path}`: {msg}"));
            return None;
        }
    };
    if let Err(msg) = idents::validate_type_base(&operation_id) {
        diag.error(format!("operation `{method} {path}`: {msg}"));
        return None;
    }

    // Collect parameters: path-level then operation-level (operation wins on name clash)
    let mut all_params: Vec<&ReferenceOr<Parameter>> = Vec::new();
    all_params.extend(path_item.parameters.iter());
    all_params.extend(operation.parameters.iter());
    let (path_params, query_params, header_params) =
        collect_params(&all_params, &operation_id, method, path, openapi, diag)?;

    let body = operation
        .request_body
        .as_ref()
        .and_then(|rb| build_body_info(rb, openapi));

    let responses = build_responses(&operation.responses, openapi, &operation_id, diag);
    let auth = resolve_op_security(operation, openapi, schemes);

    Some(OperationInfo {
        operation_id,
        method_ident,
        method: method.to_owned(),
        path: path.to_owned(),
        summary: operation.summary.clone(),
        description: operation.description.clone(),
        path_params,
        query_params,
        header_params,
        body,
        responses,
        auth,
    })
}

/// Collect and classify an operation's parameters into `(path, query, header)`
/// buckets.
///
/// Returns `None` (after recording a fatal diagnostic) when a parameter name
/// cannot become a valid Rust identifier; cookie params and unresolved `$ref`s
/// are non-fatal and merely warn.
fn collect_params(
    all_params: &[&ReferenceOr<Parameter>],
    operation_id: &str,
    method: &str,
    path: &str,
    openapi: &OpenAPI,
    diag: &mut Diagnostics,
) -> Option<(Vec<ParamInfo>, Vec<ParamInfo>, Vec<ParamInfo>)> {
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();

    for ref_or_param in all_params {
        let param = match ref_or_param {
            ReferenceOr::Item(p) => p,
            ReferenceOr::Reference { reference } => {
                // Try to resolve from components
                if let Some(resolved) = resolve_param_ref(reference, openapi) {
                    resolved
                } else {
                    diag.warn(format!(
                        "operation `{operation_id}`: could not resolve parameter $ref `{reference}`; parameter skipped"
                    ));
                    continue;
                }
            }
        };

        let data = param.parameter_data_ref();
        let param_schema = param_schema(param, openapi);

        // Keyword-safe field identifier for this parameter (shared by the request
        // struct and every transport that constructs it).
        let field_ident = match idents::field_ident(&data.name) {
            Ok(id) => id,
            Err(msg) => {
                diag.error(format!("operation `{method} {path}`: {msg}"));
                return None;
            }
        };

        let (is_enum, enum_ident, enum_values) =
            if param_schema.as_ref().is_some_and(is_string_enum) {
                let schema = param_schema.as_ref().expect("checked is_some_and above");
                let name = format!(
                    "{}{}Query",
                    operation_id.to_pascal_case(),
                    data.name.to_pascal_case()
                );
                let ident = match idents::type_ident(&name, operation_id) {
                    Ok(id) => id,
                    Err(msg) => {
                        diag.error(format!("operation `{method} {path}`: {msg}"));
                        return None;
                    }
                };
                let vals = string_enum_values(schema);
                (true, Some(ident), vals)
            } else {
                (false, None, vec![])
            };

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
            field_ident,
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
            Parameter::Cookie { .. } => diag.warn(format!(
                "operation `{operation_id}`: cookie parameter `{}` is not supported and was skipped",
                data.name
            )),
        }
    }

    Some((path_params, query_params, header_params))
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
fn build_responses(
    responses: &Responses,
    openapi: &OpenAPI,
    op_id: &str,
    diag: &mut Diagnostics,
) -> Vec<ResponseInfo> {
    let mut out = Vec::new();

    for (status_code, ref_or_resp) in &responses.responses {
        let resp = match ref_or_resp {
            ReferenceOr::Item(r) => r,
            ReferenceOr::Reference { reference } => {
                if let Some(r) = resolve_response_ref(reference, openapi) {
                    r
                } else {
                    diag.warn(format!(
                        "operation `{op_id}`: could not resolve response $ref `{reference}`; response skipped"
                    ));
                    continue;
                }
            }
        };

        let rust_type = json_media_type_to_rust(&resp.content, openapi);

        let status = match status_code {
            StatusCode::Code(n) => ResponseStatus::Code(*n),
            StatusCode::Range(n) => {
                diag.warn(format!(
                    "operation `{op_id}`: response status range `{n}XX` is not supported and was skipped"
                ));
                continue;
            }
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
                    diag.warn(format!(
                        "operation `{op_id}`: could not resolve default response $ref `{reference}`; default response skipped"
                    ));
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

/// Emit a `compile_error!` token for each fatal operation diagnostic.
#[must_use]
pub fn generate_operation_errors(errors: &[String]) -> TokenStream {
    if errors.is_empty() {
        return TokenStream::new();
    }
    let msgs: Vec<TokenStream> = errors
        .iter()
        .map(|err| {
            let msg = format!("openapi-trait: {err}");
            quote! { ::core::compile_error!(#msg); }
        })
        .collect();
    quote! { #(#msgs)* }
}

/// Generate query-param enum types + request structs + response enums for all operations.
#[must_use]
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
        let field_ident = &p.field_ident;
        let ftype = &p.rust_type;
        let fdoc = doc_attr(&p.description);
        fields.push(quote! {
            #fdoc
            pub #field_ident: #ftype,
        });
    }

    for p in &op.query_params {
        let field_ident = &p.field_ident;
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
        let field_ident = &p.field_ident;
        let fdoc = doc_attr(&p.description);
        // Header values arrive as strings over the wire. A required header is a
        // plain `String` so the type system guarantees it is present; an optional
        // one is `Option<String>` since extraction from a `HeaderMap` can fail.
        let ftype = if p.required {
            quote! { ::std::string::String }
        } else {
            quote! { ::core::option::Option<::std::string::String> }
        };
        fields.push(quote! {
            #fdoc
            pub #field_ident: #ftype,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a spec and collect its operations + diagnostics.
    fn collect(spec: &str) -> (Vec<OperationInfo>, Diagnostics) {
        let openapi: OpenAPI = serde_yaml::from_str(spec).expect("spec parses");
        collect_operations(&openapi, &[])
    }

    #[test]
    fn missing_operation_id_is_a_fatal_error_and_drops_the_operation() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      responses:
        '200': { description: ok }
"#,
        );
        assert!(
            ops.is_empty(),
            "operation without operationId must be dropped"
        );
        assert!(diag.warnings.is_empty());
        assert_eq!(diag.errors.len(), 1);
        assert!(diag.errors[0].contains("missing an `operationId`"));
        assert!(diag.errors[0].contains("get /pets"));
    }

    #[test]
    fn cookie_param_warns_but_keeps_the_operation() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - { name: session, in: cookie, schema: { type: string } }
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1, "operation must still be generated");
        assert!(diag.errors.is_empty());
        assert_eq!(diag.warnings.len(), 1);
        assert!(diag.warnings[0].contains("cookie parameter `session`"));
    }

    #[test]
    fn status_range_warns_but_keeps_the_operation() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: listPets
      responses:
        '2XX': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty());
        assert_eq!(diag.warnings.len(), 1);
        assert!(diag.warnings[0].contains("status range `2XX`"));
    }

    #[test]
    fn keyword_operation_id_becomes_a_raw_method_ident() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /things:
    get:
      operationId: type
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1, "keyword operationId must still generate");
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        assert_eq!(ops[0].method_ident.to_string(), "r#type");
    }

    #[test]
    fn hyphenated_operation_id_is_snake_cased() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: list-pets
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty());
        assert_eq!(ops[0].method_ident.to_string(), "list_pets");
    }

    #[test]
    fn operation_id_with_leading_digit_is_a_fatal_error() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: 1pet
      responses:
        '200': { description: ok }
"#,
        );
        assert!(ops.is_empty(), "invalid operationId must be dropped");
        assert_eq!(diag.errors.len(), 1);
        assert!(diag.errors[0].contains("1pet"), "{:?}", diag.errors);
    }

    #[test]
    fn non_raw_keyword_operation_id_is_a_fatal_error() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /me:
    get:
      operationId: self
      responses:
        '200': { description: ok }
"#,
        );
        assert!(ops.is_empty());
        assert_eq!(diag.errors.len(), 1);
        assert!(
            diag.errors[0].contains("reserved Rust keyword"),
            "{:?}",
            diag.errors
        );
    }

    #[test]
    fn keyword_parameter_becomes_a_raw_field_ident() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - { name: type, in: query, schema: { type: string } }
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        assert_eq!(ops[0].query_params.len(), 1);
        assert_eq!(ops[0].query_params[0].field_ident.to_string(), "r#type");
    }

    #[test]
    fn parameter_with_leading_digit_is_a_fatal_error() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - { name: 1abc, in: query, schema: { type: string } }
      responses:
        '200': { description: ok }
"#,
        );
        assert!(ops.is_empty(), "invalid parameter name must drop the op");
        assert_eq!(diag.errors.len(), 1);
        assert!(diag.errors[0].contains("1abc"), "{:?}", diag.errors);
    }

    #[test]
    fn required_header_is_non_optional_string_optional_one_is_option() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - { name: X-Required, in: header, required: true, schema: { type: string } }
        - { name: X-Optional, in: header, schema: { type: string } }
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        assert_eq!(ops[0].header_params.len(), 2);

        let struct_src = generate_request_struct(&ops[0]).to_string();
        // The required header is a plain `String`; the optional one is wrapped in
        // `Option`. Normalize whitespace so the token spacing doesn't matter.
        let normalized: String = struct_src.split_whitespace().collect();
        assert!(
            normalized.contains("x_required:::std::string::String,"),
            "required header must be a non-optional String: {struct_src}"
        );
        assert!(
            normalized.contains("x_optional:::core::option::Option<::std::string::String>"),
            "optional header must stay an Option: {struct_src}"
        );
    }

    #[test]
    fn specific_status_codes_are_handled_without_diagnostics() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /pets:
    post:
      operationId: createPet
      responses:
        '201': { description: created }
        '202': { description: accepted }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].responses.len(), 2, "201 and 202 both generated");
        assert!(diag.errors.is_empty(), "no errors for a clean operation");
        assert!(
            diag.warnings.is_empty(),
            "no warnings for a clean operation"
        );
    }
}
