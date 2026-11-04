use heck::{ToPascalCase, ToSnakeCase};
use oas3::spec::{ObjectSchema, Schema, Spec};
use proc_macro2::TokenStream;
use quote::quote;

use super::compositions::generate_composition;
use super::ctx::CodegenCtx;
use super::model::{self, Kind, SchemaRef};
use super::types::{additional_properties_value_type, ref_to_ident, schema_to_rust_type_ctx};

/// Generate all schema structs and enums from `components/schemas`.
///
/// Any inline `oneOf` / `allOf` / `anyOf` encountered inside an object property
/// is hoisted to a synthesized top-level type and emitted alongside the named
/// schemas, so the resulting module is self-contained.
#[must_use]
pub fn generate_schemas(openapi: &Spec, ctx: &mut CodegenCtx) -> TokenStream {
    let items: Vec<TokenStream> = model::component_schemas(openapi)
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
fn generate_schema_item(name: &str, ref_or: &Schema, ctx: &mut CodegenCtx) -> TokenStream {
    ctx.with_owner(name, |ctx| generate_schema_item_inner(name, ref_or, ctx))
}

/// [`generate_schema_item`] with the owner already set.
fn generate_schema_item_inner(name: &str, ref_or: &Schema, ctx: &mut CodegenCtx) -> TokenStream {
    let schema = match model::view(ref_or) {
        SchemaRef::Item(s) => s,
        SchemaRef::Ref(reference) => {
            // Unusual: a component schema that is itself a $ref; just emit a type alias.
            let ident = super::idents::schema_type_ident(name);
            let target = ref_to_ident(reference);
            return quote! { pub type #ident = #target; };
        }
    };

    if model::is_string_enum(schema) {
        return generate_string_enum(name, schema);
    }

    match model::kind(schema) {
        Kind::OneOf | Kind::AnyOf | Kind::AllOf => generate_composition(name, schema, ctx),
        Kind::Object => generate_object_struct(name, schema, ctx),
        _ => {
            // Array, integer, etc. at top level: emit a newtype alias.
            let ident = super::idents::schema_type_ident(name);
            let inner = schema_to_rust_type_ctx(ref_or, true, Some(name), ctx);
            let doc = doc_attr(&schema.description);
            quote! {
                #doc
                pub type #ident = #inner;
            }
        }
    }
}

/// Generate a string enum type from a schema.
///
/// Besides the derives, the enum gets a `Display` impl that writes the wire
/// value, like the enums generated for parameters.
pub(crate) fn generate_string_enum(name: &str, schema: &ObjectSchema) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&schema.description);
    let values = model::string_enum_values(schema).unwrap_or_default();
    let variant_idents = super::idents::enum_variant_idents(&values);
    let display_arms = variant_idents
        .iter()
        .zip(&values)
        .map(|(variant_ident, v)| quote! { Self::#variant_ident => #v, })
        .collect::<Vec<_>>();
    let variants = variant_idents
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

    let derives = string_enum_derives();
    quote! {
        #doc
        #derives

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
}

/// Generate a struct from an object schema.
///
/// Declared properties become fields; when the schema also carries an
/// `additionalProperties` entry, a flattened `HashMap` catch-all field is added.
/// A schema with no declared properties (a pure map) instead becomes a
/// `HashMap` type alias.
#[must_use]
pub fn generate_object_struct(name: &str, obj: &ObjectSchema, ctx: &mut CodegenCtx) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&obj.description);

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
            object_field_tokens(prop_name, prop_ref_or, is_required, name, ctx)
        })
        .collect();

    let additional_field = additional_properties_field(name, obj, ctx);

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

/// The flattened `HashMap` catch-all field that collects the entries of
/// `obj`'s `additionalProperties`, if it allows any.
///
/// serde hands a flattened map whatever the fields before it didn't consume,
/// so this must be the struct's last field.
pub(crate) fn additional_properties_field(
    name: &str,
    obj: &ObjectSchema,
    ctx: &mut CodegenCtx,
) -> Option<TokenStream> {
    obj.additional_properties.as_ref().and_then(|ap| {
        let synth_name = format!("{name}AdditionalProperties");
        additional_properties_value_type(ap, Some(&synth_name), ctx).map(|value_ty| {
            quote! {
                #[serde(flatten)]
                pub additional_properties:
                    ::std::collections::HashMap<::std::string::String, #value_ty>,
            }
        })
    })
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
    prop_ref_or: &Schema,
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

    let field_doc = doc_attr(&model::description(prop_ref_or).cloned());

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
    let validate_attrs = match model::view(prop_ref_or) {
        SchemaRef::Ref(reference)
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

/// The `#[derive(...)]` and `#[serde(crate)]` attributes applied to every generated model struct and
/// enum.
///
/// `serde` is reached through the facade's re-export, and
/// `#[serde(crate = "...")]` tells the derives to do the same, so users don't
/// need `serde` as a dependency.
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
            ::core::cmp::PartialEq,
            ::openapi_trait::serde::Serialize,
            ::openapi_trait::serde::Deserialize
            #extra
        )]
        #[serde(crate = "::openapi_trait::serde")]
    }
}

/// The derive attributes applied to every generated string enum.
///
/// Like [`model_derives`], plus `Eq`, `Hash` and `Copy`: a string enum has only
/// unit variants, so it can be compared exactly, used as a map key and copied.
/// Structs don't get `Eq`, because their fields may be floats.
#[must_use]
pub fn string_enum_derives() -> TokenStream {
    #[cfg(feature = "validation")]
    let extra = quote! { , ::openapi_trait::serde_valid::Validate };
    #[cfg(not(feature = "validation"))]
    let extra = quote! {};

    quote! {
        #[derive(
            ::core::fmt::Debug,
            ::core::clone::Clone,
            ::core::marker::Copy,
            ::core::cmp::PartialEq,
            ::core::cmp::Eq,
            ::core::hash::Hash,
            ::openapi_trait::serde::Serialize,
            ::openapi_trait::serde::Deserialize
            #extra
        )]
        #[serde(crate = "::openapi_trait::serde")]
    }
}
