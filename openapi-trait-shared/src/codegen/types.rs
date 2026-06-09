use openapiv3::{IntegerFormat, NumberFormat, ReferenceOr, Schema, SchemaKind, StringFormat, Type};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Map an `OpenAPI` `Schema` (or `$ref`) to a Rust type `TokenStream`.
///
/// `required` controls whether the result is wrapped in `Option<T>`.
///
/// This is the context-free entry point: any inline `oneOf` / `allOf` / `anyOf`
/// encountered along the way falls back to `serde_json::Value`. Use
/// [`schema_to_rust_type_ctx`] when a parent name is available so that inline
/// compositions can be synthesized into named top-level types.
#[must_use]
pub fn schema_to_rust_type(ref_or: &ReferenceOr<Schema>, required: bool) -> TokenStream {
    let mut sink: Vec<TokenStream> = Vec::new();
    schema_to_rust_type_ctx(ref_or, required, None, &mut sink)
    // sink is discarded — by definition no parent name means no synthesis.
}

/// Context-aware variant of [`schema_to_rust_type`].
///
/// When `parent_name` is `Some` and an inline composition is encountered, a
/// top-level type definition is appended to `inline_types` (`parent_name` is
/// used verbatim as the type ident) and the returned token stream references
/// that ident.
#[must_use]
pub fn schema_to_rust_type_ctx(
    ref_or: &ReferenceOr<Schema>,
    required: bool,
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    let inner = ref_or_to_inner_type_ctx(ref_or, parent_name, inline_types);
    if required {
        inner
    } else {
        quote! { ::core::option::Option<#inner> }
    }
}

/// Resolve a `$ref` or inline schema to its Rust type, threading inline-type
/// synthesis context through.
fn ref_or_to_inner_type_ctx(
    ref_or: &ReferenceOr<Schema>,
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    match ref_or {
        ReferenceOr::Reference { reference } => ref_to_ident(reference),
        ReferenceOr::Item(schema) => schema_kind_to_type(schema, parent_name, inline_types),
    }
}

#[must_use]
pub fn ref_to_ident(reference: &str) -> TokenStream {
    // "#/components/schemas/Foo" -> Foo
    let name = reference.rsplit('/').next().unwrap_or(reference);
    let ident = format_ident!("{}", name);
    quote! { #ident }
}

/// Convert a schema to a Rust type, synthesizing a top-level composition type
/// when `parent_name` is provided and the schema is a composition.
fn schema_kind_to_type(
    schema: &Schema,
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    match &schema.schema_kind {
        SchemaKind::Type(t) => primitive_type_to_rust(t, parent_name, inline_types),
        SchemaKind::OneOf { one_of } => {
            synthesize_inline_composition(parent_name, inline_types, |name, sink| {
                super::compositions::generate_one_of(
                    name,
                    one_of,
                    schema.schema_data.discriminator.as_ref(),
                    schema.schema_data.description.as_ref(),
                    sink,
                )
            })
        }
        SchemaKind::AnyOf { any_of } => {
            synthesize_inline_composition(parent_name, inline_types, |name, sink| {
                super::compositions::generate_any_of(
                    name,
                    any_of,
                    schema.schema_data.description.as_ref(),
                    sink,
                )
            })
        }
        SchemaKind::AllOf { all_of } => {
            synthesize_inline_composition(parent_name, inline_types, |name, sink| {
                super::compositions::generate_all_of(
                    name,
                    all_of,
                    schema.schema_data.description.as_ref(),
                    sink,
                )
            })
        }
        SchemaKind::Not { .. } | SchemaKind::Any(_) => {
            // Intentionally unsupported: emit untyped JSON.
            quote! { ::serde_json::Value }
        }
    }
}

/// Either synthesize a top-level composition type (when a parent name is
/// available) and return a reference to it, or fall back to
/// `serde_json::Value`.
fn synthesize_inline_composition(
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
    generate: impl FnOnce(&str, &mut Vec<TokenStream>) -> TokenStream,
) -> TokenStream {
    parent_name.map_or_else(
        || quote! { ::serde_json::Value },
        |name| {
            let tokens = generate(name, inline_types);
            inline_types.push(tokens);
            let ident = format_ident!("{}", name);
            quote! { #ident }
        },
    )
}

/// Convert a primitive `OpenAPI` type to a Rust type token stream.
fn primitive_type_to_rust(
    t: &Type,
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    match t {
        Type::Integer(i) => {
            if i.format == openapiv3::VariantOrUnknownOrEmpty::Item(IntegerFormat::Int32) {
                quote! { i32 }
            } else {
                quote! { i64 }
            }
        }
        Type::Number(n) => {
            if n.format == openapiv3::VariantOrUnknownOrEmpty::Item(NumberFormat::Float) {
                quote! { f32 }
            } else {
                quote! { f64 }
            }
        }
        Type::String(s) => {
            // For string enums, the caller handles the dedicated enum type;
            // here we just return String as a fallback.
            if s.enumeration.is_empty() {
                if matches!(
                    &s.format,
                    openapiv3::VariantOrUnknownOrEmpty::Item(StringFormat::Binary)
                ) {
                    quote! { ::std::vec::Vec<u8> }
                } else {
                    quote! { ::std::string::String }
                }
            } else {
                quote! { ::std::string::String }
            }
        }
        Type::Boolean(_) => quote! { bool },
        Type::Array(a) => {
            let item_ty = a.items.as_ref().map_or_else(
                || quote! { ::serde_json::Value },
                |items| ref_or_to_inner_type_ctx(&items.clone().unbox(), parent_name, inline_types),
            );
            quote! { ::std::vec::Vec<#item_ty> }
        }
        Type::Object(_) => quote! { ::serde_json::Value },
    }
}

/// Returns true when the schema is a string with enumeration values.
#[must_use]
pub fn is_string_enum(schema: &Schema) -> bool {
    if let SchemaKind::Type(Type::String(s)) = &schema.schema_kind {
        !s.enumeration.is_empty()
    } else {
        false
    }
}

/// Extract enum values from a string schema (skipping None entries).
#[must_use]
pub fn string_enum_values(schema: &Schema) -> Vec<String> {
    if let SchemaKind::Type(Type::String(s)) = &schema.schema_kind {
        s.enumeration.iter().filter_map(Clone::clone).collect()
    } else {
        vec![]
    }
}
