use heck::{ToPascalCase, ToSnakeCase};
use openapiv3::{Components, OpenAPI, ReferenceOr, Schema, SchemaKind, Type};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::types::{is_string_enum, ref_to_ident, schema_to_rust_type, string_enum_values};

/// Generate all schema structs and enums from `components/schemas`.
#[must_use]
pub fn generate_schemas(openapi: &OpenAPI) -> TokenStream {
    let Some(components) = &openapi.components else {
        return quote! {};
    };

    let items: Vec<TokenStream> = components
        .schemas
        .iter()
        .map(|(name, ref_or)| generate_schema_item(name, ref_or, components))
        .collect();

    quote! { #(#items)* }
}

/// Generate a single schema item (enum, struct, or type alias).
fn generate_schema_item(
    name: &str,
    ref_or: &ReferenceOr<Schema>,
    components: &Components,
) -> TokenStream {
    let schema = match ref_or {
        ReferenceOr::Item(s) => s,
        ReferenceOr::Reference { reference } => {
            // Unusual: a component schema that is itself a $ref; just emit a type alias.
            let ident = format_ident!("{}", name);
            let target = ref_to_ident(reference);
            return quote! { pub type #ident = #target; };
        }
    };

    if is_string_enum(schema) {
        generate_string_enum(name, schema)
    } else if let SchemaKind::Type(Type::Object(obj)) = &schema.schema_kind {
        generate_object_struct(name, schema, obj, components)
    } else {
        // Array, integer, etc. at top level: emit a newtype alias.
        let ident = format_ident!("{}", name);
        let inner = schema_to_rust_type(ref_or, true);
        let doc = doc_attr(&schema.schema_data.description);
        quote! {
            #doc
            pub type #ident = #inner;
        }
    }
}

/// Generate a string enum type from a schema.
fn generate_string_enum(name: &str, schema: &Schema) -> TokenStream {
    let ident = format_ident!("{}", name);
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
fn generate_object_struct(
    name: &str,
    schema: &Schema,
    obj: &openapiv3::ObjectType,
    _components: &Components,
) -> TokenStream {
    let ident = format_ident!("{}", name);
    let doc = doc_attr(&schema.schema_data.description);

    let fields: Vec<TokenStream> = obj
        .properties
        .iter()
        .map(|(prop_name, prop_ref_or)| {
            // Check actual required array on the object
            let is_required = obj.required.iter().any(|r| r == prop_name);

            let snake = prop_name.to_snake_case();
            // Escape Rust keywords (e.g. `type` -> `r#type`)
            let field_ident = keyword_safe_ident(&snake);
            // Always emit rename if either the snake conversion or keyword escaping changed the name
            let rename_attr = {
                let n = prop_name.as_str();
                quote! { #[serde(rename = #n)] }
            };

            // Get description from the property schema if it's inline
            let field_doc = match prop_ref_or {
                ReferenceOr::Item(s) => doc_attr(&s.schema_data.description),
                ReferenceOr::Reference { .. } => quote! {},
            };

            let field_type = schema_to_rust_type(&prop_ref_or.clone().unbox(), is_required);

            quote! {
                #field_doc
                #rename_attr
                pub #field_ident: #field_type,
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

        pub struct #ident {
            #(#fields)*
        }
    }
}

/// Turn a `snake_case` name into a keyword-safe `syn::Ident`, using raw identifier
/// syntax (`r#type`) when the name clashes with a Rust keyword.
fn keyword_safe_ident(name: &str) -> proc_macro2::Ident {
    // Keywords that are valid raw identifiers but not plain identifiers:
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "union", "unsafe", "use", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&name) {
        proc_macro2::Ident::new_raw(name, proc_macro2::Span::call_site())
    } else {
        format_ident!("{}", name)
    }
}

/// Emit `#[doc = "..."]` if the description is `Some`, otherwise nothing.
#[must_use]
pub fn doc_attr(description: &Option<String>) -> TokenStream {
    description
        .as_ref()
        .map_or_else(|| quote! {}, |d| quote! { #[doc = #d] })
}
