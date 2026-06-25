use heck::{ToPascalCase, ToSnakeCase};
use openapiv3::{OpenAPI, ReferenceOr, Schema, SchemaKind, Type};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::compositions::{generate_all_of, generate_any_of, generate_one_of};
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
pub fn generate_schemas(openapi: &OpenAPI) -> TokenStream {
    let Some(components) = &openapi.components else {
        return quote! {};
    };

    let mut inline_types: Vec<TokenStream> = Vec::new();
    let items: Vec<TokenStream> = components
        .schemas
        .iter()
        .map(|(name, ref_or)| generate_schema_item(name, ref_or, &mut inline_types))
        .collect();

    quote! {
        #(#items)*
        #(#inline_types)*
    }
}

/// Generate a single schema item (enum, struct, or type alias).
fn generate_schema_item(
    name: &str,
    ref_or: &ReferenceOr<Schema>,
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    let schema = match ref_or {
        ReferenceOr::Item(s) => s,
        ReferenceOr::Reference { reference } => {
            // Unusual: a component schema that is itself a $ref; just emit a type alias.
            let ident = format_ident!("{}", name.to_pascal_case());
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
            inline_types,
        ),
        SchemaKind::AnyOf { any_of } => generate_any_of(
            name,
            any_of,
            schema.schema_data.description.as_ref(),
            inline_types,
        ),
        SchemaKind::AllOf { all_of } => generate_all_of(
            name,
            all_of,
            schema.schema_data.description.as_ref(),
            inline_types,
        ),
        SchemaKind::Type(Type::Object(obj)) => {
            generate_object_struct(name, schema, obj, inline_types)
        }
        _ => {
            // Array, integer, etc. at top level: emit a newtype alias.
            let ident = format_ident!("{}", name.to_pascal_case());
            let inner = schema_to_rust_type_ctx(ref_or, true, Some(name), inline_types);
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
    let ident = format_ident!("{}", name.to_pascal_case());
    let doc = doc_attr(&schema.schema_data.description);
    let variants = string_enum_values(schema)
        .into_iter()
        .map(|v| {
            let variant_ident = format_ident!("{}", v.to_pascal_case());
            if variant_ident == v {
                quote! { #variant_ident }
            } else {
                let rename = &v;
                quote! {
                    #[serde(rename = #rename)]
                    #variant_ident
                }
            }
        })
        .collect::<Vec<_>>();

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
    inline_types: &mut Vec<TokenStream>,
) -> TokenStream {
    let ident = format_ident!("{}", name.to_pascal_case());
    let doc = doc_attr(&schema.schema_data.description);

    // A pure-map object (no declared properties, only `additionalProperties`)
    // is emitted as a `HashMap` type alias rather than an empty struct.
    if obj.properties.is_empty() {
        if let Some(ap) = &obj.additional_properties {
            if let Some(value_ty) = additional_properties_value_type(ap, Some(name), inline_types) {
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
                inline_types,
            )
        })
        .collect();

    // When declared properties coexist with `additionalProperties`, collect the
    // extra entries into a flattened `HashMap` catch-all field.
    let additional_field = obj.additional_properties.as_ref().and_then(|ap| {
        let synth_name = format!("{name}AdditionalProperties");
        additional_properties_value_type(ap, Some(&synth_name), inline_types).map(|value_ty| {
            quote! {
                #[serde(flatten)]
                pub additional_properties:
                    ::std::collections::HashMap<::std::string::String, #value_ty>,
            }
        })
    });

    quote! {
        #doc
        #[derive(
            ::core::fmt::Debug,
            ::core::clone::Clone,
            ::serde::Serialize,
            ::serde::Deserialize,
        )]

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
    inline_types: &mut Vec<TokenStream>,
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

    let synth_name = format!("{parent_struct_name}{}", prop_name.to_pascal_case());
    let field_type =
        schema_to_rust_type_ctx(prop_ref_or, is_required, Some(&synth_name), inline_types);

    quote! {
        #field_doc
        #rename_attr
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
