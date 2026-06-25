use heck::ToPascalCase;
use openapiv3::{
    AdditionalProperties, IntegerFormat, NumberFormat, ObjectType, ReferenceOr, Schema, SchemaKind,
    StringFormat, StringType, Type, VariantOrUnknownOrEmpty,
};
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
    let ident = format_ident!("{}", name.to_pascal_case());
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
        SchemaKind::Type(Type::Object(obj)) => {
            object_schema_to_type(schema, obj, parent_name, inline_types)
        }
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
            let ident = format_ident!("{}", name.to_pascal_case());
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
        Type::String(s) => string_type_to_rust(s),
        Type::Boolean(_) => quote! { bool },
        Type::Array(a) => {
            let item_ty = a.items.as_ref().map_or_else(
                || quote! { ::serde_json::Value },
                |items| ref_or_to_inner_type_ctx(&items.clone().unbox(), parent_name, inline_types),
            );
            quote! { ::std::vec::Vec<#item_ty> }
        }
        // Objects are handled in `schema_kind_to_type`, which has the full
        // schema (description, synthesis context) available.
        Type::Object(_) => quote! { ::serde_json::Value },
    }
}

/// Map a string schema to its Rust type, specializing known `format` values.
///
/// `date-time`/`date`/`uuid` map to typed `chrono`/`uuid` types (re-exported
/// through the facade so generated code can reference them as
/// `::openapi_trait::…`); `binary` maps to `Vec<u8>`. Every other format —
/// including `email` and unknown formats — falls back to `String`.
///
/// Note that `openapiv3::StringFormat` only models `Date`/`DateTime`/`Binary`/
/// `Byte`/`Password`; `uuid` (and other non-standard formats) arrive as
/// `VariantOrUnknownOrEmpty::Unknown`.
fn string_type_to_rust(s: &StringType) -> TokenStream {
    // String enums are handled by the caller via a dedicated enum type; here we
    // only emit the scalar fallback.
    if !s.enumeration.is_empty() {
        return quote! { ::std::string::String };
    }
    match &s.format {
        VariantOrUnknownOrEmpty::Item(StringFormat::Binary) => quote! { ::std::vec::Vec<u8> },
        VariantOrUnknownOrEmpty::Item(StringFormat::DateTime) => {
            quote! { ::openapi_trait::chrono::DateTime<::openapi_trait::chrono::Utc> }
        }
        VariantOrUnknownOrEmpty::Item(StringFormat::Date) => {
            quote! { ::openapi_trait::chrono::NaiveDate }
        }
        VariantOrUnknownOrEmpty::Unknown(name) if name == "uuid" => {
            quote! { ::openapi_trait::uuid::Uuid }
        }
        _ => quote! { ::std::string::String },
    }
}

/// Convert an object schema to a Rust type.
///
/// - An object that declares `properties` is synthesized into a named top-level
///   struct (via [`super::schemas::generate_object_struct`]) when a
///   `parent_name` is available; the returned token stream references it.
/// - An object with no declared `properties` but an `additionalProperties`
///   entry is a map and becomes `HashMap<String, T>`.
/// - Anything else (e.g. a free-form object with no schema info, or no parent
///   name to synthesize against) falls back to untyped JSON.
fn object_schema_to_type(
    schema: &Schema,
    obj: &ObjectType,
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    if !obj.properties.is_empty() {
        return synthesize_inline_composition(parent_name, inline_types, |name, sink| {
            super::schemas::generate_object_struct(name, schema, obj, sink)
        });
    }
    if let Some(ap) = &obj.additional_properties {
        if let Some(value_ty) = additional_properties_value_type(ap, parent_name, inline_types) {
            return quote! {
                ::std::collections::HashMap<::std::string::String, #value_ty>
            };
        }
    }
    quote! { ::serde_json::Value }
}

/// Resolve an `additionalProperties` declaration to the value type `T` of the
/// resulting `HashMap<String, T>`.
///
/// - `additionalProperties: true` → `serde_json::Value` (any value allowed).
/// - `additionalProperties: false` → `None` (no extra properties; the caller
///   should not emit a map).
/// - `additionalProperties: <schema>` → the mapped type of that schema.
#[must_use]
pub fn additional_properties_value_type(
    ap: &AdditionalProperties,
    parent_name: Option<&str>,
    inline_types: &mut Vec<TokenStream>,
) -> Option<TokenStream> {
    match ap {
        AdditionalProperties::Any(false) => None,
        AdditionalProperties::Any(true) => Some(quote! { ::serde_json::Value }),
        AdditionalProperties::Schema(schema) => {
            Some(ref_or_to_inner_type_ctx(schema, parent_name, inline_types))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `StringType` with the given `format`, treating `None` as no
    /// format and a known marker for the unknown variants.
    fn string_with_format(format: VariantOrUnknownOrEmpty<StringFormat>) -> StringType {
        StringType {
            format,
            ..Default::default()
        }
    }

    fn emitted(s: &StringType) -> String {
        string_type_to_rust(s).to_string()
    }

    #[test]
    fn date_time_maps_to_chrono_datetime() {
        let s = string_with_format(VariantOrUnknownOrEmpty::Item(StringFormat::DateTime));
        assert_eq!(
            emitted(&s),
            quote! { ::openapi_trait::chrono::DateTime<::openapi_trait::chrono::Utc> }.to_string()
        );
    }

    #[test]
    fn date_maps_to_chrono_naive_date() {
        let s = string_with_format(VariantOrUnknownOrEmpty::Item(StringFormat::Date));
        assert_eq!(
            emitted(&s),
            quote! { ::openapi_trait::chrono::NaiveDate }.to_string()
        );
    }

    #[test]
    fn uuid_unknown_format_maps_to_uuid() {
        let s = string_with_format(VariantOrUnknownOrEmpty::Unknown("uuid".to_string()));
        assert_eq!(
            emitted(&s),
            quote! { ::openapi_trait::uuid::Uuid }.to_string()
        );
    }

    #[test]
    fn binary_still_maps_to_byte_vec() {
        let s = string_with_format(VariantOrUnknownOrEmpty::Item(StringFormat::Binary));
        assert_eq!(emitted(&s), quote! { ::std::vec::Vec<u8> }.to_string());
    }

    #[test]
    fn email_unknown_format_stays_string() {
        let s = string_with_format(VariantOrUnknownOrEmpty::Unknown("email".to_string()));
        assert_eq!(emitted(&s), quote! { ::std::string::String }.to_string());
    }

    #[test]
    fn no_format_stays_string() {
        let s = string_with_format(VariantOrUnknownOrEmpty::Empty);
        assert_eq!(emitted(&s), quote! { ::std::string::String }.to_string());
    }

    #[test]
    fn string_enum_stays_string_even_with_format() {
        // Enums are emitted as dedicated enum types by the caller; the scalar
        // mapping must not specialize them on `format`.
        let s = StringType {
            format: VariantOrUnknownOrEmpty::Unknown("uuid".to_string()),
            enumeration: vec![Some("a".to_string()), Some("b".to_string())],
            ..Default::default()
        };
        assert_eq!(emitted(&s), quote! { ::std::string::String }.to_string());
    }
}
