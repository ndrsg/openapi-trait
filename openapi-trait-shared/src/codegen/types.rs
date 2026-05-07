use openapiv3::{IntegerFormat, NumberFormat, ReferenceOr, Schema, SchemaKind, StringFormat, Type};
use proc_macro2::TokenStream;
use quote::quote;

/// Map an `OpenAPI` `Schema` (or `$ref`) to a Rust type `TokenStream`.
///
/// `required` controls whether the result is wrapped in `Option<T>`.
#[must_use]
pub fn schema_to_rust_type(ref_or: &ReferenceOr<Schema>, required: bool) -> TokenStream {
    let inner = ref_or_to_inner_type(ref_or);
    if required {
        inner
    } else {
        quote! { ::core::option::Option<#inner> }
    }
}

/// Convert a reference-or-schema to a Rust type token stream.
fn ref_or_to_inner_type(ref_or: &ReferenceOr<Schema>) -> TokenStream {
    match ref_or {
        ReferenceOr::Reference { reference } => ref_to_ident(reference),
        ReferenceOr::Item(schema) => schema_kind_to_type(&schema.schema_kind),
    }
}

#[must_use]
pub fn ref_to_ident(reference: &str) -> TokenStream {
    // "#/components/schemas/Foo" -> Foo
    let name = reference.rsplit('/').next().unwrap_or(reference);
    let ident = quote::format_ident!("{}", name);
    quote! { #ident }
}

/// Convert a schema kind to a Rust type token stream.
fn schema_kind_to_type(kind: &SchemaKind) -> TokenStream {
    match kind {
        SchemaKind::Type(t) => primitive_type_to_rust(t),
        // For compositions, fall back to serde_json::Value
        SchemaKind::OneOf { .. }
        | SchemaKind::AllOf { .. }
        | SchemaKind::AnyOf { .. }
        | SchemaKind::Not { .. }
        | SchemaKind::Any(_) => {
            quote! { ::serde_json::Value }
        }
    }
}

/// Convert a primitive `OpenAPI` type to a Rust type token stream.
fn primitive_type_to_rust(t: &Type) -> TokenStream {
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
                |items| ref_or_to_inner_type(&items.clone().unbox()),
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
