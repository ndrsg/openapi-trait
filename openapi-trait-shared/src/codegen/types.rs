use oas3::spec::{ObjectSchema, Schema};
use proc_macro2::TokenStream;
use quote::quote;

use super::ctx::CodegenCtx;
use super::model::{self, Kind, SchemaRef};

/// Like [`schema_to_rust_type_ctx`], but ignores whether the schema allows
/// `null`.
///
/// Used for parameters and headers: they can't carry a JSON `null` on the
/// wire, so `null` must not change their type.
#[must_use]
pub fn non_null_schema_to_rust_type_ctx(
    schema: &Schema,
    required: bool,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let inner = ref_or_to_inner_type_ctx(schema, parent_name, ctx);
    optional_if(!required, inner)
}

/// Map an `OpenAPI` `Schema` (or `$ref`) to a Rust type `TokenStream`.
///
/// The result is wrapped in `Option<T>` when `required` is false or the schema
/// allows `null` (see [`model::is_nullable`]). References go through
/// [`CodegenCtx::ref_type`], so they are boxed or qualified as the current
/// position needs.
///
/// When `parent_name` is `Some` and an inline composition is encountered, a
/// top-level type definition is appended to `ctx.inline_types` and the
/// returned token stream references it. The type is named `parent_name`, or
/// `parent_name` with a numeric suffix if that name is already taken.
#[must_use]
pub fn schema_to_rust_type_ctx(
    schema: &Schema,
    required: bool,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let inner = ref_or_to_inner_type_ctx(schema, parent_name, ctx);
    optional_if(!required || model::is_nullable(schema), inner)
}

/// `Option<inner>` when `optional`, otherwise `inner`.
fn optional_if(optional: bool, inner: TokenStream) -> TokenStream {
    if optional {
        quote! { ::core::option::Option<#inner> }
    } else {
        inner
    }
}

/// Like [`ref_or_to_inner_type_ctx`], but wraps the type in `Option<T>` when
/// the schema is nullable. Used for array items and map values, where `null`
/// entries are allowed only if the item schema is nullable. The container
/// already adds indirection, so references in here are never boxed.
fn nullable_inner_type_ctx(
    schema: &Schema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let inner = ctx.behind_indirection(|ctx| ref_or_to_inner_type_ctx(schema, parent_name, ctx));
    optional_if(model::is_nullable(schema), inner)
}

/// Resolve a `$ref` or inline schema to its Rust type, threading inline-type
/// synthesis context through.
fn ref_or_to_inner_type_ctx(
    schema: &Schema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    match model::view(schema) {
        SchemaRef::Ref(reference) => ctx.ref_type(reference),
        SchemaRef::Item(obj) => schema_kind_to_type(obj, parent_name, ctx),
    }
}

#[must_use]
pub fn ref_to_ident(reference: &str) -> TokenStream {
    // "#/components/schemas/Foo" -> Foo
    let ident = super::idents::schema_type_ident(model::ref_name(reference));
    quote! { #ident }
}

/// Convert a schema to a Rust type, synthesizing a top-level composition type
/// when `parent_name` is provided and the schema is a composition.
fn schema_kind_to_type(
    schema: &ObjectSchema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    match model::kind(schema) {
        Kind::String if model::is_string_enum(schema) => {
            synthesize_inline_string_enum(schema, parent_name, ctx)
        }
        Kind::Object => object_schema_to_type(schema, parent_name, ctx),
        Kind::OneOf => synthesize_inline_composition(parent_name, ctx, |name, ctx| {
            super::compositions::generate_one_of(
                name,
                &schema.one_of,
                schema.discriminator.as_ref(),
                schema.description.as_ref(),
                ctx,
            )
        }),
        Kind::AnyOf => synthesize_inline_composition(parent_name, ctx, |name, ctx| {
            super::compositions::generate_any_of(
                name,
                &schema.any_of,
                schema.description.as_ref(),
                ctx,
            )
        }),
        Kind::AllOf => synthesize_inline_composition(parent_name, ctx, |name, ctx| {
            super::compositions::generate_all_of(
                name,
                &schema.all_of,
                schema.description.as_ref(),
                ctx,
            )
        }),
        Kind::Any => {
            // Intentionally unsupported: emit untyped JSON.
            quote! { ::openapi_trait::serde_json::Value }
        }
        kind => primitive_type_to_rust(kind, schema, parent_name, ctx),
    }
}

/// Generate a named enum for an inline string enum when a parent name is
/// available; otherwise fall back to `String`.
fn synthesize_inline_string_enum(
    schema: &ObjectSchema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    parent_name.map_or_else(
        || quote! { ::std::string::String },
        |name| {
            let name = ctx.reserve_type_name(name);
            let tokens = super::schemas::generate_string_enum(&name, schema);
            ctx.inline_types.push(tokens);
            ctx.local_type(&super::idents::schema_type_ident(&name))
        },
    )
}

/// Either synthesize a top-level composition type (when a parent name is
/// available) and return a reference to it, or fall back to
/// `serde_json::Value`.
///
/// The parent name is reserved in [`CodegenCtx`] first, so a clash with an
/// existing type gets a numeric suffix (`PersonAddress2`).
fn synthesize_inline_composition(
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
    generate: impl FnOnce(&str, &mut CodegenCtx) -> TokenStream,
) -> TokenStream {
    parent_name.map_or_else(
        || quote! { ::openapi_trait::serde_json::Value },
        |name| {
            let name = ctx.reserve_type_name(name);
            let tokens = generate(&name, ctx);
            ctx.inline_types.push(tokens);
            ctx.local_type(&super::idents::schema_type_ident(&name))
        },
    )
}

/// Convert a scalar or array schema of kind `kind` to a Rust type.
fn primitive_type_to_rust(
    kind: Kind,
    schema: &ObjectSchema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    match kind {
        Kind::Integer => {
            if schema.format.as_deref() == Some("int32") {
                quote! { i32 }
            } else {
                quote! { i64 }
            }
        }
        Kind::Number => {
            if schema.format.as_deref() == Some("float") {
                quote! { f32 }
            } else {
                quote! { f64 }
            }
        }
        Kind::String => string_type_to_rust(schema),
        Kind::Boolean => quote! { bool },
        Kind::Array => {
            let item_ty = schema.items.as_deref().map_or_else(
                || quote! { ::openapi_trait::serde_json::Value },
                |items| nullable_inner_type_ctx(items, parent_name, ctx),
            );
            quote! { ::std::vec::Vec<#item_ty> }
        }
        // Objects, compositions and `Any` are handled in
        // `schema_kind_to_type`.
        Kind::Object | Kind::OneOf | Kind::AllOf | Kind::AnyOf | Kind::Any => {
            quote! { ::openapi_trait::serde_json::Value }
        }
    }
}

/// Map a string schema to its Rust type, specializing known `format` values.
///
/// `date-time`/`date`/`uuid` map to typed `chrono`/`uuid` types (re-exported
/// through the facade so generated code can reference them as
/// `::openapi_trait::…`); `binary` maps to `Vec<u8>`. Every other format —
/// including `email` and unknown formats — falls back to `String`.
fn string_type_to_rust(s: &ObjectSchema) -> TokenStream {
    // String enums are handled by the caller via a dedicated enum type; here we
    // only emit the scalar fallback.
    if !s.enum_values.is_empty() {
        return quote! { ::std::string::String };
    }
    match s.format.as_deref() {
        Some("binary") => quote! { ::std::vec::Vec<u8> },
        Some("date-time") => {
            quote! { ::openapi_trait::chrono::DateTime<::openapi_trait::chrono::Utc> }
        }
        Some("date") => quote! { ::openapi_trait::chrono::NaiveDate },
        Some("uuid") => quote! { ::openapi_trait::uuid::Uuid },
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
    schema: &ObjectSchema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    if !schema.properties.is_empty() {
        return synthesize_inline_composition(parent_name, ctx, |name, ctx| {
            super::schemas::generate_object_struct(name, schema, ctx)
        });
    }
    if let Some(ap) = &schema.additional_properties {
        if let Some(value_ty) = additional_properties_value_type(ap, parent_name, ctx) {
            return quote! {
                ::std::collections::HashMap<::std::string::String, #value_ty>
            };
        }
    }
    quote! { ::openapi_trait::serde_json::Value }
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
    ap: &Schema,
    parent_name: Option<&str>,
    ctx: &mut CodegenCtx,
) -> Option<TokenStream> {
    match ap {
        Schema::Boolean(b) if !b.0 => None,
        Schema::Boolean(_) => Some(quote! { ::openapi_trait::serde_json::Value }),
        Schema::Object(_) => Some(nullable_inner_type_ctx(ap, parent_name, ctx)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a string schema with the given `format`.
    fn string_with_format(format: Option<&str>) -> ObjectSchema {
        ObjectSchema {
            schema_type: Some(oas3::spec::SchemaTypeSet::Single(
                oas3::spec::SchemaType::String,
            )),
            format: format.map(str::to_owned),
            ..Default::default()
        }
    }

    fn emitted(s: &ObjectSchema) -> String {
        string_type_to_rust(s).to_string()
    }

    #[test]
    fn date_time_maps_to_chrono_datetime() {
        let s = string_with_format(Some("date-time"));
        assert_eq!(
            emitted(&s),
            quote! { ::openapi_trait::chrono::DateTime<::openapi_trait::chrono::Utc> }.to_string()
        );
    }

    #[test]
    fn date_maps_to_chrono_naive_date() {
        let s = string_with_format(Some("date"));
        assert_eq!(
            emitted(&s),
            quote! { ::openapi_trait::chrono::NaiveDate }.to_string()
        );
    }

    #[test]
    fn uuid_format_maps_to_uuid() {
        let s = string_with_format(Some("uuid"));
        assert_eq!(
            emitted(&s),
            quote! { ::openapi_trait::uuid::Uuid }.to_string()
        );
    }

    #[test]
    fn binary_still_maps_to_byte_vec() {
        let s = string_with_format(Some("binary"));
        assert_eq!(emitted(&s), quote! { ::std::vec::Vec<u8> }.to_string());
    }

    #[test]
    fn email_format_stays_string() {
        let s = string_with_format(Some("email"));
        assert_eq!(emitted(&s), quote! { ::std::string::String }.to_string());
    }

    #[test]
    fn no_format_stays_string() {
        let s = string_with_format(None);
        assert_eq!(emitted(&s), quote! { ::std::string::String }.to_string());
    }

    #[test]
    fn string_enum_stays_string_even_with_format() {
        // Enums are emitted as dedicated enum types by the caller; the scalar
        // mapping must not specialize them on `format`.
        let s = ObjectSchema {
            enum_values: vec!["a".into(), "b".into()],
            ..string_with_format(Some("uuid"))
        };
        assert_eq!(emitted(&s), quote! { ::std::string::String }.to_string());
    }
}
