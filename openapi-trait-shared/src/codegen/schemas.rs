use heck::{ToPascalCase, ToSnakeCase};
use openapiv3::{OpenAPI, ReferenceOr, Schema, SchemaKind, Type};
use proc_macro2::TokenStream;
use quote::quote;

use super::compositions::{generate_all_of, generate_any_of, generate_one_of};
use super::ctx::CodegenCtx;
use super::types::{
    additional_properties_value_type, is_string_enum, ref_to_ident, schema_to_rust_type_ctx,
    string_enum_values,
};

/// Generate all schema structs and enums from `components/schemas`.
///
/// Any inline `oneOf` / `allOf` / `anyOf` encountered inside an object property
/// is hoisted to a synthesized top-level type and emitted alongside the named
/// schemas, so the resulting module is self-contained.
#[must_use]
pub fn generate_schemas(openapi: &OpenAPI, ctx: &mut CodegenCtx) -> TokenStream {
    let Some(components) = &openapi.components else {
        return quote! {};
    };

    let items: Vec<TokenStream> = components
        .schemas
        .iter()
        .map(|(name, ref_or)| generate_schema_item(name, ref_or, ctx))
        .collect();
    let inline_types = ctx.take_inline_types();

    quote! {
        #(#items)*
        #(#inline_types)*
    }
}

/// Generate a single schema item (enum, struct, or type alias).
///
/// By-value references that lead back to `name` are boxed (see
/// [`CodegenCtx::ref_type`]), so recursive schemas get a finite size.
fn generate_schema_item(
    name: &str,
    ref_or: &ReferenceOr<Schema>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    ctx.with_owner(name, |ctx| generate_schema_item_inner(name, ref_or, ctx))
}

/// [`generate_schema_item`] with the owner already set.
fn generate_schema_item_inner(
    name: &str,
    ref_or: &ReferenceOr<Schema>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let schema = match ref_or {
        ReferenceOr::Item(s) => s,
        ReferenceOr::Reference { reference } => {
            // Unusual: a component schema that is itself a $ref; just emit a type alias.
            let ident = super::idents::schema_type_ident(name);
            let target = ref_to_ident(reference);
            return quote! { pub type #ident = #target; };
        }
    };

    if is_string_enum(schema) {
        return generate_string_enum(name, schema);
    }

    match &schema.schema_kind {
        SchemaKind::OneOf { one_of } => generate_one_of(
            name,
            one_of,
            schema.schema_data.discriminator.as_ref(),
            schema.schema_data.description.as_ref(),
            ctx,
        ),
        SchemaKind::AnyOf { any_of } => {
            generate_any_of(name, any_of, schema.schema_data.description.as_ref(), ctx)
        }
        SchemaKind::AllOf { all_of } => {
            generate_all_of(name, all_of, schema.schema_data.description.as_ref(), ctx)
        }
        SchemaKind::Type(Type::Object(obj)) => generate_object_struct(name, schema, obj, ctx),
        _ => {
            // Array, integer, etc. at top level: emit a newtype alias.
            let ident = super::idents::schema_type_ident(name);
            let inner = schema_to_rust_type_ctx(ref_or, true, Some(name), ctx);
            let doc = doc_attr(&schema.schema_data.description);
            quote! {
                #doc
                pub type #ident = #inner;
            }
        }
    }
}

/// Generate a string enum type from a schema.
pub(crate) fn generate_string_enum(name: &str, schema: &Schema) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&schema.schema_data.description);
    let values = string_enum_values(schema);
    let variants = super::idents::enum_variant_idents(&values)
        .into_iter()
        .zip(&values)
        .map(|(variant_ident, v)| {
            if variant_ident == v {
                quote! { #variant_ident }
            } else {
                quote! {
                    #[serde(rename = #v)]
                    #variant_ident
                }
            }
        })
        .collect::<Vec<_>>();

    let derives = model_derives();
    quote! {
        #doc
        #derives

        pub enum #ident {
            #(#variants,)*
        }
    }
}

/// Generate a struct from an object schema.
///
/// Declared properties become fields; when the schema also carries an
/// `additionalProperties` entry, a flattened `HashMap` catch-all field is added.
/// A schema with no declared properties (a pure map) instead becomes a
/// `HashMap` type alias.
#[must_use]
pub fn generate_object_struct(
    name: &str,
    schema: &Schema,
    obj: &openapiv3::ObjectType,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&schema.schema_data.description);

    // A pure-map object (no declared properties, only `additionalProperties`)
    // is emitted as a `HashMap` type alias rather than an empty struct.
    if obj.properties.is_empty() {
        if let Some(ap) = &obj.additional_properties {
            if let Some(value_ty) = additional_properties_value_type(ap, Some(name), ctx) {
                return quote! {
                    #doc
                    pub type #ident =
                        ::std::collections::HashMap<::std::string::String, #value_ty>;
                };
            }
        }
    }

    let fields: Vec<TokenStream> = obj
        .properties
        .iter()
        .map(|(prop_name, prop_ref_or)| {
            let is_required = obj.required.iter().any(|r| r == prop_name);
            object_field_tokens(
                prop_name,
                &prop_ref_or.clone().unbox(),
                is_required,
                name,
                ctx,
            )
        })
        .collect();

    // When declared properties coexist with `additionalProperties`, collect the
    // extra entries into a flattened `HashMap` catch-all field.
    let additional_field = obj.additional_properties.as_ref().and_then(|ap| {
        let synth_name = format!("{name}AdditionalProperties");
        additional_properties_value_type(ap, Some(&synth_name), ctx).map(|value_ty| {
            quote! {
                #[serde(flatten)]
                pub additional_properties:
                    ::std::collections::HashMap<::std::string::String, #value_ty>,
            }
        })
    });

    let derives = model_derives();
    quote! {
        #doc
        #derives

        pub struct #ident {
            #(#fields)*
            #additional_field
        }
    }
}

/// Emit a single struct field for an object property. Shared between
/// [`generate_object_struct`] and the `allOf` merger in
/// [`super::compositions`].
///
/// `parent_struct_name` is used as the prefix for any inline composition
/// encountered in this property, so that hoisted types get a stable, readable
/// name like `PersonAddress`.
#[must_use]
pub fn object_field_tokens(
    prop_name: &str,
    prop_ref_or: &ReferenceOr<Schema>,
    is_required: bool,
    parent_struct_name: &str,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let snake = prop_name.to_snake_case();
    let field_ident = super::idents::keyword_safe_ident(&snake);
    let rename_attr = {
        let n = prop_name;
        quote! { #[serde(rename = #n)] }
    };

    let field_doc = match prop_ref_or {
        ReferenceOr::Item(s) => doc_attr(&s.schema_data.description),
        ReferenceOr::Reference { .. } => quote! {},
    };

    // An optional field is left out when `None` instead of being sent as
    // `null`, which the spec only allows for nullable fields. A required
    // nullable field is still sent as an explicit `null`.
    let skip_attr = if is_required {
        quote! {}
    } else {
        quote! { #[serde(default, skip_serializing_if = "::core::option::Option::is_none")] }
    };

    let synth_name = format!("{parent_struct_name}{}", prop_name.to_pascal_case());
    let field_type = ctx.in_property(|ctx| {
        schema_to_rust_type_ctx(prop_ref_or, is_required, Some(&synth_name), ctx)
    });

    // `serde_valid` validation attributes derived from the schema's constraints
    // (`minLength`, `minimum`, `minItems`, …). Emitted only under the
    // `validation` feature; otherwise the field is byte-identical to before.
    // `serde_valid` has no `Validate` impl for `Box<T>`, so an optional boxed
    // field (`Option<Box<T>>`) can't recurse. A required one can, because the
    // derive's `field.validate()` call auto-derefs through the `Box`.
    #[cfg(feature = "validation")]
    let validate_attrs = match prop_ref_or {
        ReferenceOr::Reference { reference }
            if !is_required && ctx.in_property(|ctx| ctx.needs_box(reference)) =>
        {
            quote! {}
        }
        _ => super::validation::validation_attrs(prop_ref_or, &ctx.models),
    };
    #[cfg(not(feature = "validation"))]
    let validate_attrs = quote! {};

    quote! {
        #field_doc
        #rename_attr
        #skip_attr
        #validate_attrs
        pub #field_ident: #field_type,
    }
}

/// Emit `#[doc = "..."]` if the description is `Some`, otherwise nothing.
#[must_use]
pub fn doc_attr(description: &Option<String>) -> TokenStream {
    description
        .as_ref()
        .map_or_else(|| quote! {}, |d| quote! { #[doc = #d] })
}

/// The `#[derive(...)]` attribute applied to every generated model struct and
/// enum.
///
/// With the `validation` feature enabled, `serde_valid::Validate` is appended so
/// the generated field-level `#[validate(...)]` attributes take effect and
/// callers can run `model.validate()`. The derive is referenced through the
/// facade re-export (`::openapi_trait::serde_valid`), matching the convention
/// used for `chrono`/`uuid`.
#[must_use]
pub fn model_derives() -> TokenStream {
    #[cfg(feature = "validation")]
    let extra = quote! { , ::openapi_trait::serde_valid::Validate };
    #[cfg(not(feature = "validation"))]
    let extra = quote! {};

    quote! {
        #[derive(
            ::core::fmt::Debug,
            ::core::clone::Clone,
            ::serde::Serialize,
            ::serde::Deserialize
            #extra
        )]
    }
}
