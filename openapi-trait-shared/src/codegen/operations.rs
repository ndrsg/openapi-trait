use heck::ToPascalCase;
use openapiv3::{
    Header, MediaType, OpenAPI, Operation, Parameter, ParameterSchemaOrContent, PathItem,
    QueryStyle, ReferenceOr, RequestBody, Response, Responses, Schema, SchemaKind, StatusCode,
    StringFormat, Type, VariantOrUnknownOrEmpty,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::ctx::CodegenCtx;
use super::idents;
use super::schemas::doc_attr;
use super::security::{resolve_op_security, OpSecurity, SchemeInfo};
use super::types::{
    is_string_enum, non_null_schema_to_rust_type, schema_to_rust_type, string_enum_values,
};

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

/// How a query parameter's array (or object) value is delimited on the wire,
/// derived from the `OpenAPI` `style` keyword for `in: query` parameters.
///
/// Only meaningful for query params; other locations default to [`Self::Form`].
/// `DeepObject` is recognized but not yet serialized per-spec — it falls back to
/// default serialization with a build warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStyleKind {
    /// `style: form` — repeated keys when exploded, comma-joined otherwise.
    Form,
    /// `style: spaceDelimited` — space-joined array values (when not exploded).
    Space,
    /// `style: pipeDelimited` — pipe-joined array values (when not exploded).
    Pipe,
    /// `style: deepObject` — nested object rendering (unsupported; warns).
    DeepObject,
}

impl QueryStyleKind {
    /// The single-character delimiter used to join array values when the
    /// parameter is not exploded (`form` → `,`, space → ` `, pipe → `|`).
    #[must_use]
    pub const fn delimiter(self) -> &'static str {
        match self {
            Self::Space => " ",
            Self::Pipe => "|",
            // `form` uses comma; `deepObject` never reaches the join path but
            // comma is a harmless fallback.
            Self::Form | Self::DeepObject => ",",
        }
    }
}

/// How a query parameter is serialized on the wire, resolved from its `style`,
/// `explode`, and schema shape. Populated with defaults for non-query params.
#[derive(Debug)]
pub struct QuerySerialization {
    /// The `style` keyword; defaults to [`QueryStyleKind::Form`].
    pub style: QueryStyleKind,
    /// Resolved `explode` flag (spec default: `true` for `form`, `false` for the
    /// other styles).
    pub explode: bool,
    /// True when the schema is an array (the Rust type is `Vec<T>`).
    pub is_array: bool,
    /// The element type `T` when [`Self::is_array`] is true, so the server can
    /// parse array elements one at a time.
    pub array_item_type: Option<TokenStream>,
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
    /// Query serialization strategy (meaningful only for query params).
    pub query: QuerySerialization,
}

/// How a request/response body is encoded on the wire.
#[derive(Debug, Clone)]
pub enum BodyEncoding {
    /// JSON, serialized with `serde_json`. `content_type` is the declared media
    /// type (`application/json`, `application/merge-patch+json`, …), which is
    /// sent as the `Content-Type` header.
    Json {
        content_type: String,
    },
    Bytes {
        content_type: String,
    },
}

impl BodyEncoding {
    /// The declared `Content-Type` when it differs from plain
    /// `application/json`, which serde/axum/reqwest already send by default.
    #[must_use]
    pub fn custom_json_content_type(&self) -> Option<&str> {
        match self {
            Self::Json { content_type } if content_type != "application/json" => Some(content_type),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct BodyInfo {
    pub description: Option<String>,
    pub required: bool,
    pub rust_type: TokenStream,
    pub encoding: BodyEncoding,
}

/// How a declared response header value is formatted on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderValueKind {
    String,
    Integer,
    DateTime,
}

#[derive(Debug)]
pub struct ResponseHeaderInfo {
    pub name: String,
    pub field_ident: syn::Ident,
    pub description: Option<String>,
    pub required: bool,
    pub rust_type: TokenStream,
    pub value_kind: HeaderValueKind,
}

#[derive(Debug)]
pub struct ResponseInfo {
    pub status: ResponseStatus,
    pub description: String,
    pub rust_type: Option<TokenStream>,
    pub encoding: Option<BodyEncoding>,
    pub headers: Vec<ResponseHeaderInfo>,
}

/// Rust types used for binary request/response bodies in generated operation types.
///
/// Axum and the reqwest client pass different concrete stream/body types while
/// sharing the same `OpenAPI` metadata.
#[derive(Debug)]
pub struct BinaryBodyTypes {
    /// Type for binary request bodies (e.g. `axum::body::Body`).
    pub request: TokenStream,
    /// Type for binary response bodies (e.g. `axum::body::Body` or `ByteStream`).
    pub response: TokenStream,
}

/// True when the operation has a binary-encoded request body.
fn op_has_binary_request_body(op: &OperationInfo) -> bool {
    op.body
        .as_ref()
        .is_some_and(|b| matches!(b.encoding, BodyEncoding::Bytes { .. }))
}

/// True when any response variant carries a binary-encoded body.
fn op_has_binary_response_body(op: &OperationInfo) -> bool {
    op.responses
        .iter()
        .any(|r| r.rust_type.is_some() && matches!(r.encoding, Some(BodyEncoding::Bytes { .. })))
}

/// Resolve the Rust type for a request body field.
fn request_body_field_type(body: &BodyInfo, binary: &BinaryBodyTypes) -> TokenStream {
    let inner = match &body.encoding {
        BodyEncoding::Bytes { .. } => binary.request.clone(),
        BodyEncoding::Json { .. } => body.rust_type.clone(),
    };
    if body.required {
        inner
    } else {
        quote! { ::core::option::Option<#inner> }
    }
}

/// Resolve the Rust type for a binary or JSON response body.
fn response_body_type(r: &ResponseInfo, binary: &BinaryBodyTypes) -> Option<TokenStream> {
    r.rust_type.as_ref().map(|_| match &r.encoding {
        Some(BodyEncoding::Bytes { .. }) => binary.response.clone(),
        _ => r.rust_type.clone().unwrap(),
    })
}

/// Ident for a response payload struct when an operation declares response headers.
#[must_use]
pub fn response_payload_ident(operation_id: &str, status: u16) -> syn::Ident {
    format_ident!(
        "{}{}",
        operation_id.to_pascal_case(),
        format!("Status{status}")
    )
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
    ctx: &mut CodegenCtx,
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
            if let Some(info) = build_operation_info(
                (path, item),
                (&method, operation),
                openapi,
                schemes,
                ctx,
                &mut diag,
            ) {
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

/// Build an `OperationInfo` for the operation `method` of the path item at
/// `path`.
fn build_operation_info(
    (path, path_item): (&str, &PathItem),
    (method, operation): (&str, &Operation),
    openapi: &OpenAPI,
    schemes: &[SchemeInfo],
    ctx: &mut CodegenCtx,
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

    // Collect parameters: path-level then operation-level; `collect_params`
    // lets an operation-level param override a path-level one with the same
    // `(name, in)`.
    let mut all_params: Vec<&ReferenceOr<Parameter>> = Vec::new();
    all_params.extend(path_item.parameters.iter());
    all_params.extend(operation.parameters.iter());
    let (path_params, query_params, header_params) =
        collect_params(&all_params, &operation_id, method, path, openapi, ctx, diag)?;

    let body = operation
        .request_body
        .as_ref()
        .and_then(|rb| build_body_info(rb, openapi, &operation_id, diag));

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
    ctx: &mut CodegenCtx,
    diag: &mut Diagnostics,
) -> Option<(Vec<ParamInfo>, Vec<ParamInfo>, Vec<ParamInfo>)> {
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();

    let resolved = resolve_and_merge_params(all_params, operation_id, openapi, diag);
    for param in resolved {
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

        // Only path and query params get a dedicated enum type; header values
        // are carried as plain strings and cookie params are skipped.
        let enum_suffix = match param {
            Parameter::Path { .. } => Some("Path"),
            Parameter::Query { .. } => Some("Query"),
            Parameter::Header { .. } | Parameter::Cookie { .. } => None,
        };
        let (is_enum, enum_ident, enum_values) = if let (Some(suffix), Some(schema)) = (
            enum_suffix,
            param_schema.as_ref().filter(|s| is_string_enum(s)),
        ) {
            let name = ctx.reserve_type_name(&format!(
                "{}{}{suffix}",
                operation_id.to_pascal_case(),
                data.name.to_pascal_case()
            ));
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
            non_null_schema_to_rust_type(&ref_or, true)
        } else {
            quote! { ::std::string::String }
        };

        let query = query_serialization(param, param_schema.as_ref());

        // deepObject and object-valued query params are not yet serialized
        // per-spec; warn and fall back to default form serialization.
        if matches!(param, Parameter::Query { .. }) {
            let is_object = matches!(
                param_schema.as_ref().map(|s| &s.schema_kind),
                Some(SchemaKind::Type(Type::Object(_)))
            );
            if matches!(query.style, QueryStyleKind::DeepObject) || is_object {
                diag.warn(format!(
                    "operation `{operation_id}`: query parameter `{}` uses deepObject/object serialization, which is not yet supported; falling back to default form serialization",
                    data.name
                ));
            }
        }

        let info = ParamInfo {
            name: data.name.clone(),
            field_ident,
            description: data.description.clone(),
            required: data.required,
            rust_type,
            is_enum,
            enum_ident,
            enum_values,
            query,
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

/// Resolve a parameter's [`QuerySerialization`] from its `style`/`explode` and
/// schema shape. Only `in: query` params carry a `style`; other locations get
/// defaults (`form`, not exploded, not an array).
fn query_serialization(param: &Parameter, param_schema: Option<&Schema>) -> QuerySerialization {
    // Per the spec `explode` defaults to true for `form` and false otherwise.
    let (style, explode) = match param {
        Parameter::Query { style, .. } => {
            let kind = match style {
                QueryStyle::Form => QueryStyleKind::Form,
                QueryStyle::SpaceDelimited => QueryStyleKind::Space,
                QueryStyle::PipeDelimited => QueryStyleKind::Pipe,
                QueryStyle::DeepObject => QueryStyleKind::DeepObject,
            };
            let explode = param
                .parameter_data_ref()
                .explode
                .unwrap_or(matches!(kind, QueryStyleKind::Form));
            (kind, explode)
        }
        _ => (QueryStyleKind::Form, false),
    };

    // Detect array schemas so array query params can be serialized per style and
    // parsed element-by-element on the server.
    let (is_array, array_item_type) = match param_schema.map(|s| &s.schema_kind) {
        Some(SchemaKind::Type(Type::Array(arr))) => {
            let item_ty = arr.items.as_ref().map_or_else(
                || quote! { ::serde_json::Value },
                |items| non_null_schema_to_rust_type(&items.clone().unbox(), true),
            );
            (true, Some(item_ty))
        }
        _ => (false, None),
    };

    QuerySerialization {
        style,
        explode,
        is_array,
        array_item_type,
    }
}

/// Resolve parameter `$ref`s (warning about and skipping unresolvable ones),
/// then merge by `(name, in)`: per the `OpenAPI` spec a later (operation-level)
/// parameter overrides an earlier (path-level) one with the same key. The
/// overriding entry keeps the original's position.
fn resolve_and_merge_params<'a>(
    all_params: &[&'a ReferenceOr<Parameter>],
    operation_id: &str,
    openapi: &'a OpenAPI,
    diag: &mut Diagnostics,
) -> Vec<&'a Parameter> {
    let mut resolved: Vec<&Parameter> = Vec::new();
    for ref_or_param in all_params {
        let param = match ref_or_param {
            ReferenceOr::Item(p) => p,
            ReferenceOr::Reference { reference } => {
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
        let key = param_key(param);
        if let Some(slot) = resolved.iter_mut().find(|p| param_key(p) == key) {
            *slot = param;
        } else {
            resolved.push(param);
        }
    }
    resolved
}

/// The `(name, in)` pair that uniquely identifies a parameter within an
/// operation, per the `OpenAPI` spec.
fn param_key(param: &Parameter) -> (&str, &'static str) {
    let location = match param {
        Parameter::Path { .. } => "path",
        Parameter::Query { .. } => "query",
        Parameter::Header { .. } => "header",
        Parameter::Cookie { .. } => "cookie",
    };
    (param.parameter_data_ref().name.as_str(), location)
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
///
/// Warns when the body has content but none of its media types is supported,
/// because the operation is then generated without a `body` field.
fn build_body_info(
    ref_or_rb: &ReferenceOr<RequestBody>,
    openapi: &OpenAPI,
    op_id: &str,
    diag: &mut Diagnostics,
) -> Option<BodyInfo> {
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

    let Some(resolved) = media_type_to_rust(&rb.content, openapi) else {
        if !rb.content.is_empty() {
            let types: Vec<&str> = rb.content.keys().map(String::as_str).collect();
            diag.warn(format!(
                "operation `{op_id}`: request body media type(s) {} are not supported (only JSON and binary bodies are); the body was skipped",
                types.join(", ")
            ));
        }
        return None;
    };

    Some(BodyInfo {
        description: rb.description.clone(),
        required: rb.required,
        rust_type: resolved.rust_type,
        encoding: resolved.encoding,
    })
}

/// Resolved Rust type and wire encoding for a media-type content map entry.
struct MediaTypeResolved {
    /// Generated Rust type for the body payload.
    rust_type: TokenStream,
    /// How the body is encoded on the wire.
    encoding: BodyEncoding,
}

/// True when the content type represents a binary payload.
fn is_binary_media_type(content_type: &str) -> bool {
    content_type == "application/octet-stream"
        || content_type.starts_with("image/")
        || content_type.starts_with("audio/")
        || content_type.starts_with("video/")
        || content_type == "application/pdf"
        || content_type == "application/zip"
}

/// True when a schema is `type: string, format: binary`.
const fn schema_is_binary(ref_or: &ReferenceOr<Schema>) -> bool {
    match ref_or {
        ReferenceOr::Item(schema) => matches!(
            &schema.schema_kind,
            SchemaKind::Type(Type::String(s)) if matches!(
                s.format,
                VariantOrUnknownOrEmpty::Item(StringFormat::Binary)
            )
        ),
        ReferenceOr::Reference { .. } => false,
    }
}

/// The media type without parameters, trimmed and lowercased:
/// `"Application/JSON; charset=utf-8"` → `"application/json"`.
fn media_type_essence(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// True when the content type is JSON: `application/json` or any `+json`
/// structured-syntax suffix (`application/problem+json`, …), with or without
/// parameters such as `charset`.
fn is_json_media_type(content_type: &str) -> bool {
    let essence = media_type_essence(content_type);
    essence == "application/json" || essence.ends_with("+json")
}

/// Resolve the Rust type and wire encoding from a content map.
///
/// JSON media types win over binary ones, and plain `application/json` wins
/// over other JSON media types. Entries without a schema are skipped.
fn media_type_to_rust(
    content: &indexmap::IndexMap<String, MediaType>,
    _openapi: &OpenAPI,
) -> Option<MediaTypeResolved> {
    let with_schema = || {
        content
            .iter()
            .filter_map(|(ct, media)| media.schema.as_ref().map(|s| (ct, s)))
    };

    let json = with_schema()
        .find(|(ct, _)| media_type_essence(ct) == "application/json")
        .or_else(|| with_schema().find(|(ct, _)| is_json_media_type(ct)));
    if let Some((content_type, ref_or_schema)) = json {
        return Some(MediaTypeResolved {
            rust_type: schema_to_rust_type(ref_or_schema, true),
            encoding: BodyEncoding::Json {
                content_type: content_type.clone(),
            },
        });
    }

    with_schema()
        .find(|(ct, schema)| is_binary_media_type(ct) || schema_is_binary(schema))
        .map(|(content_type, _)| MediaTypeResolved {
            rust_type: quote! { ::std::vec::Vec<u8> },
            encoding: BodyEncoding::Bytes {
                content_type: content_type.clone(),
            },
        })
}

/// Resolve media type for a response, emitting a warning for unsupported types.
fn response_media_type_to_rust(
    content: &indexmap::IndexMap<String, MediaType>,
    openapi: &OpenAPI,
    op_id: &str,
    status_label: &str,
    diag: &mut Diagnostics,
) -> Option<MediaTypeResolved> {
    media_type_to_rust(content, openapi).or_else(|| {
        if !content.is_empty() {
            diag.warn(format!(
                "operation `{op_id}`: response {status_label} has unsupported non-JSON/non-binary content types; body typing skipped"
            ));
        }
        None
    })
}

/// Classify how a response header schema is serialized on the wire.
const fn classify_header_schema(ref_or: &ReferenceOr<Schema>) -> HeaderValueKind {
    match ref_or {
        ReferenceOr::Item(schema) => match &schema.schema_kind {
            SchemaKind::Type(Type::Integer(_)) => HeaderValueKind::Integer,
            SchemaKind::Type(Type::String(s)) => {
                if matches!(
                    s.format,
                    VariantOrUnknownOrEmpty::Item(StringFormat::DateTime)
                ) {
                    HeaderValueKind::DateTime
                } else {
                    HeaderValueKind::String
                }
            }
            _ => HeaderValueKind::String,
        },
        ReferenceOr::Reference { .. } => HeaderValueKind::String,
    }
}

/// Map a response header schema to its Rust type.
fn header_schema_to_rust(format: &ParameterSchemaOrContent, required: bool) -> TokenStream {
    match format {
        ParameterSchemaOrContent::Schema(ref_or) => non_null_schema_to_rust_type(ref_or, required),
        ParameterSchemaOrContent::Content(_) => {
            if required {
                quote! { ::std::string::String }
            } else {
                quote! { ::core::option::Option<::std::string::String> }
            }
        }
    }
}

/// Build response header metadata from a `Response` object.
fn build_response_headers(
    resp: &Response,
    op_id: &str,
    openapi: &OpenAPI,
    diag: &mut Diagnostics,
) -> Vec<ResponseHeaderInfo> {
    let mut out = Vec::new();

    for (name, ref_or_header) in &resp.headers {
        // Per OpenAPI, Content-Type declared under headers is ignored.
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }

        let header = match ref_or_header {
            ReferenceOr::Item(h) => h,
            ReferenceOr::Reference { reference } => {
                if let Some(h) = resolve_header_ref(reference, openapi) {
                    h
                } else {
                    diag.warn(format!(
                        "operation `{op_id}`: could not resolve response header $ref `{reference}`; header skipped"
                    ));
                    continue;
                }
            }
        };

        let field_ident = match idents::field_ident(name) {
            Ok(id) => id,
            Err(msg) => {
                diag.error(format!(
                    "operation `{op_id}`: response header `{name}`: {msg}"
                ));
                continue;
            }
        };

        let rust_type = header_schema_to_rust(&header.format, header.required);

        let value_kind = match &header.format {
            ParameterSchemaOrContent::Schema(ref_or) => classify_header_schema(ref_or),
            ParameterSchemaOrContent::Content(_) => HeaderValueKind::String,
        };

        out.push(ResponseHeaderInfo {
            name: name.clone(),
            field_ident,
            description: header.description.clone(),
            required: header.required,
            rust_type,
            value_kind,
        });
    }

    out
}

/// Resolve a `$ref` to a header from the components section.
fn resolve_header_ref<'a>(reference: &str, openapi: &'a OpenAPI) -> Option<&'a Header> {
    let name = reference.strip_prefix("#/components/headers/")?;
    openapi.components.as_ref()?.headers.get(name)?.as_item()
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

        let headers = build_response_headers(resp, op_id, openapi, diag);
        let media = response_media_type_to_rust(
            &resp.content,
            openapi,
            op_id,
            &format!("status `{status_code}`"),
            diag,
        );
        let (rust_type, encoding) =
            media.map_or((None, None), |m| (Some(m.rust_type), Some(m.encoding)));

        let status = match status_code {
            // openapiv3 already rejects codes outside 100..=999 at parse time;
            // the guard keeps the invariant the server codegen relies on explicit.
            StatusCode::Code(n) if (100..=999).contains(n) => ResponseStatus::Code(*n),
            StatusCode::Code(n) => {
                diag.warn(format!(
                    "operation `{op_id}`: response status `{n}` is not a valid HTTP status code (must be 100..=999) and was skipped"
                ));
                continue;
            }
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
            encoding,
            headers,
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
            rust_type: None,
            encoding: None,
            headers: Vec::new(),
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
pub fn generate_operation_types(ops: &[OperationInfo], binary: &BinaryBodyTypes) -> TokenStream {
    let items: Vec<TokenStream> = ops
        .iter()
        .map(|op| generate_single_operation_types(op, binary))
        .collect();
    quote! { #(#items)* }
}

/// Generate all types for a single operation.
fn generate_single_operation_types(op: &OperationInfo, binary: &BinaryBodyTypes) -> TokenStream {
    let param_enums = generate_param_enums(op);
    let request_struct = generate_request_struct(op, binary);
    let response_enum = generate_response_enum(op, binary);
    quote! {
        #param_enums
        #request_struct
        #response_enum
    }
}

/// Generate enum types for path and query parameters with string enum schemas.
///
/// Besides the serde derives, each enum gets a `Display` impl that renders the
/// wire value, so it can be interpolated into a URL path.
fn generate_param_enums(op: &OperationInfo) -> TokenStream {
    let enums: Vec<TokenStream> = op
        .path_params
        .iter()
        .chain(op.query_params.iter())
        .filter(|p| p.is_enum)
        .map(|p| {
            let ident = p.enum_ident.as_ref().unwrap();
            let doc = doc_attr(&p.description);
            let variant_idents = idents::enum_variant_idents(&p.enum_values);
            let display_arms = variant_idents
                .iter()
                .zip(&p.enum_values)
                .map(|(variant_ident, v)| quote! { Self::#variant_ident => #v, });
            let variants: Vec<TokenStream> = variant_idents
                .iter()
                .zip(&p.enum_values)
                .map(|(variant_ident, v)| {
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

                impl ::core::fmt::Display for #ident {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        f.write_str(match self {
                            #(#display_arms)*
                        })
                    }
                }
            }
        })
        .collect();

    quote! { #(#enums)* }
}

/// Generate the request struct for an operation.
fn generate_request_struct(op: &OperationInfo, binary: &BinaryBodyTypes) -> TokenStream {
    let ident = format_ident!("{}Request", op.operation_id.to_pascal_case());
    let doc = combined_doc(op.summary.as_ref(), op.description.as_ref());
    let clone_derive = if op_has_binary_request_body(op) {
        quote! { #[derive(::core::fmt::Debug)] }
    } else {
        quote! { #[derive(::core::fmt::Debug, ::core::clone::Clone)] }
    };

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
        let ftype = request_body_field_type(body, binary);
        let bdoc = doc_attr(&body.description);
        fields.push(quote! {
            #bdoc
            pub body: #ftype,
        });
    }

    quote! {
        #doc
        #clone_derive
        pub struct #ident {
            #(#fields)*
        }
    }
}

/// Generate the response enum and any payload structs for an operation.
fn generate_response_enum(op: &OperationInfo, binary: &BinaryBodyTypes) -> TokenStream {
    let ident = format_ident!("{}Response", op.operation_id.to_pascal_case());
    let doc = combined_doc(op.summary.as_ref(), op.description.as_ref());
    let clone_derive = if op_has_binary_response_body(op) {
        quote! { #[derive(::core::fmt::Debug)] }
    } else {
        quote! { #[derive(::core::fmt::Debug, ::core::clone::Clone)] }
    };

    let payload_structs: Vec<TokenStream> = op
        .responses
        .iter()
        .filter(|r| !r.headers.is_empty())
        .filter_map(|r| match r.status {
            ResponseStatus::Code(n) => Some(generate_response_payload_struct(op, r, n, binary)),
            ResponseStatus::Default => None,
        })
        .collect();

    let variants: Vec<TokenStream> = op
        .responses
        .iter()
        .map(|r| {
            let vdoc = doc_attr(&Some(r.description.clone()));
            match &r.status {
                ResponseStatus::Code(n) => {
                    let variant_ident = format_ident!("Status{}", n);
                    if r.headers.is_empty() {
                        response_body_type(r, binary).map_or_else(
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
                    } else {
                        let payload_ident = response_payload_ident(&op.operation_id, *n);
                        quote! {
                            #vdoc
                            #variant_ident(#payload_ident)
                        }
                    }
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
        #(#payload_structs)*
        #doc
        #clone_derive
        pub enum #ident {
            #(#variants,)*
        }
    }
}

/// Generate a payload struct for a response that declares headers.
fn generate_response_payload_struct(
    op: &OperationInfo,
    r: &ResponseInfo,
    status: u16,
    binary: &BinaryBodyTypes,
) -> TokenStream {
    let ident = response_payload_ident(&op.operation_id, status);
    let clone_derive = if matches!(r.encoding, Some(BodyEncoding::Bytes { .. })) {
        quote! { #[derive(::core::fmt::Debug)] }
    } else {
        quote! { #[derive(::core::fmt::Debug, ::core::clone::Clone)] }
    };
    let mut fields: Vec<TokenStream> = Vec::new();

    if let Some(ty) = response_body_type(r, binary) {
        fields.push(quote! {
            pub body: #ty,
        });
    }

    for h in &r.headers {
        let field_ident = &h.field_ident;
        let ftype = &h.rust_type;
        let fdoc = doc_attr(&h.description);
        fields.push(quote! {
            #fdoc
            pub #field_ident: #ftype,
        });
    }

    quote! {
        #clone_derive
        pub struct #ident {
            #(#fields)*
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
        collect_operations(&openapi, &[], &mut CodegenCtx::new(&openapi))
    }

    fn test_binary_types() -> BinaryBodyTypes {
        BinaryBodyTypes {
            request: quote!(::TestRequestBody),
            response: quote!(::TestResponseBody),
        }
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
    fn operation_param_overrides_path_item_param_with_same_name_and_location() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
components:
  parameters:
    Id: { name: id, in: path, required: true, schema: { type: integer } }
paths:
  /things/{id}:
    parameters:
      - { name: id, in: path, required: true, schema: { type: string } }
      - { name: id, in: query, schema: { type: string } }
    get:
      operationId: getThing
      parameters:
        - $ref: '#/components/parameters/Id'
      responses:
        '200': { description: ok }
"#,
        );
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        let op = &ops[0];
        assert_eq!(op.path_params.len(), 1);
        assert_eq!(op.path_params[0].rust_type.to_string(), "i64");
        // Same name, different location: kept as a separate param.
        assert_eq!(op.query_params.len(), 1);
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

        let struct_src = generate_request_struct(&ops[0], &test_binary_types()).to_string();
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
    fn query_style_and_explode_are_parsed_from_the_spec() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /items:
    get:
      operationId: searchItems
      parameters:
        - name: tag
          in: query
          style: form
          explode: true
          schema: { type: array, items: { type: string } }
        - name: csv
          in: query
          style: spaceDelimited
          schema: { type: array, items: { type: integer } }
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        let q = &ops[0].query_params;
        assert_eq!(q.len(), 2);

        let tag = &q[0];
        assert_eq!(tag.query.style, QueryStyleKind::Form);
        assert!(tag.query.explode, "explode: true is honored");
        assert!(tag.query.is_array);
        assert!(tag.query.array_item_type.is_some());

        let csv = &q[1];
        assert_eq!(csv.query.style, QueryStyleKind::Space);
        // spaceDelimited defaults explode to false.
        assert!(!csv.query.explode);
        assert!(csv.query.is_array);
    }

    #[test]
    fn explode_defaults_true_for_form_and_false_for_other_styles() {
        let (ops, _) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /items:
    get:
      operationId: searchItems
      parameters:
        - name: form_default
          in: query
          schema: { type: array, items: { type: string } }
        - name: pipe_default
          in: query
          style: pipeDelimited
          schema: { type: array, items: { type: string } }
      responses:
        '200': { description: ok }
"#,
        );
        let q = &ops[0].query_params;
        assert_eq!(q[0].query.style, QueryStyleKind::Form);
        assert!(q[0].query.explode, "form defaults explode to true");
        assert_eq!(q[1].query.style, QueryStyleKind::Pipe);
        assert!(
            !q[1].query.explode,
            "non-form styles default explode to false"
        );
    }

    #[test]
    fn deep_object_and_object_query_params_warn_but_keep_the_operation() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /items:
    get:
      operationId: searchItems
      parameters:
        - name: filter
          in: query
          style: deepObject
          explode: true
          schema:
            type: object
            additionalProperties: { type: string }
      responses:
        '200': { description: ok }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        assert_eq!(diag.warnings.len(), 1);
        assert!(
            diag.warnings[0].contains("deepObject/object"),
            "{:?}",
            diag.warnings
        );
    }

    #[test]
    fn binary_response_resolves_to_byte_vec_with_bytes_encoding() {
        let (ops, diag) = collect(
            r#"
openapi: 3.0.0
info: { title: t, version: "1.0" }
paths:
  /files/{id}:
    get:
      operationId: downloadFile
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses:
        '200':
          description: ok
          headers:
            Content-Disposition:
              schema: { type: string }
          content:
            application/octet-stream:
              schema: { type: string, format: binary }
"#,
        );
        assert_eq!(ops.len(), 1);
        assert!(diag.errors.is_empty(), "{:?}", diag.errors);
        let resp = &ops[0].responses[0];
        assert_eq!(resp.headers.len(), 1);
        assert!(resp.rust_type.is_some());
        assert!(matches!(resp.encoding, Some(BodyEncoding::Bytes { .. })));
        let enum_src = generate_response_enum(&ops[0], &test_binary_types()).to_string();
        assert!(
            enum_src.contains("DownloadFileStatus200"),
            "headers should produce a payload struct: {enum_src}"
        );
        assert!(
            enum_src.contains("TestResponseBody"),
            "binary response body should use configured stream type: {enum_src}"
        );
    }

    /// Spec with one `POST /x` operation whose request body has the given
    /// `content` map (YAML flow mapping).
    fn body_spec(content: &str) -> String {
        format!(
            r#"
openapi: 3.0.0
info: {{ title: t, version: "1.0" }}
paths:
  /x:
    post:
      operationId: postX
      requestBody:
        required: true
        content: {content}
      responses:
        '204': {{ description: ok }}
"#
        )
    }

    #[test]
    fn json_suffix_and_charset_bodies_are_typed_as_json() {
        for ct in [
            "application/merge-patch+json",
            "application/vnd.api+json",
            "application/json; charset=utf-8",
            "Application/JSON",
        ] {
            let (ops, diag) = collect(&body_spec(&format!(
                "{{ '{ct}': {{ schema: {{ type: integer }} }} }}"
            )));
            assert!(diag.warnings.is_empty(), "{ct}: {:?}", diag.warnings);
            let body = ops[0]
                .body
                .as_ref()
                .unwrap_or_else(|| panic!("{ct}: no body"));
            assert_eq!(body.rust_type.to_string(), "i64", "{ct}");
            match &body.encoding {
                BodyEncoding::Json { content_type } => assert_eq!(content_type, ct),
                other @ BodyEncoding::Bytes { .. } => panic!("{ct}: {other:?}"),
            }
        }
    }

    #[test]
    fn plain_json_is_preferred_over_other_json_media_types() {
        let (ops, _) = collect(&body_spec(
            "{ 'application/merge-patch+json': { schema: { type: integer } }, \
               'application/json': { schema: { type: string } } }",
        ));
        let body = ops[0].body.as_ref().unwrap();
        assert!(body.encoding.custom_json_content_type().is_none());
        assert_eq!(body.rust_type.to_string(), ":: std :: string :: String");
    }

    #[test]
    fn unsupported_body_media_type_warns() {
        let (ops, diag) = collect(&body_spec("{ 'text/csv': { schema: { type: string } } }"));
        assert_eq!(ops.len(), 1);
        assert!(ops[0].body.is_none());
        assert_eq!(diag.warnings.len(), 1, "{:?}", diag.warnings);
        assert!(
            diag.warnings[0].contains("postX") && diag.warnings[0].contains("text/csv"),
            "{:?}",
            diag.warnings
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
