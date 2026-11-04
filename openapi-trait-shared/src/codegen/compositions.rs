//! Generators for `oneOf` / `allOf` / `anyOf` schema compositions.
//!
//! [`generate_composition`] picks the shape:
//!
//! - Only `oneOf` → Rust enum. Tagged via `#[serde(tag = "...")]` when the
//!   schema carries a `discriminator`; otherwise `#[serde(untagged)]`.
//! - Only `anyOf` → Rust enum, always `#[serde(untagged)]` (`oneOf` semantics
//!   without the exclusivity guarantee is rarely useful in typed Rust).
//! - Anything else (`allOf`, the schema's own `properties` next to a
//!   composition, or several composition keywords) → Rust struct holding, in
//!   this order:
//!   1. the schema's own `properties`,
//!   2. the `allOf` branches: `$ref` branches become `#[serde(flatten)]`
//!      fields; inline object branches inline their properties directly,
//!   3. a `#[serde(flatten)]` `one_of` / `any_of` field holding an enum of
//!      the branches (`{Name}OneOf` / `{Name}AnyOf`),
//!   4. a flattened catch-all map for `additionalProperties` (`allOf` only;
//!      `oneOf` / `anyOf` next to `additionalProperties` is untyped, see
//!      [`super::model::kind`]).
//!
//! A property that also appears in a branch is consumed by the struct field
//! and doesn't reach the branch. The exception is a `oneOf` discriminator's
//! tag property: the tagged enum needs it, so it isn't a struct field.
//!
//! A `{type: "null"}` branch adds no variant or field: it makes the
//! composition nullable instead (see [`super::model::is_nullable`]).
//!
//! `not` and unconstrained schemas remain unsupported and continue to fall
//! back to `serde_json::Value` in [`super::types`].
//!
//! Inline (non-top-level) compositions are synthesized into top-level types
//! using a deterministic name derived from the enclosing context (object name +
//! property, operation id + role). The accumulated definitions are emitted
//! alongside the explicit `components/schemas` items so all generated types
//! live at module scope.

use heck::{ToPascalCase, ToSnakeCase};
use oas3::spec::{Discriminator, ObjectSchema, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::ctx::CodegenCtx;
use super::model::{self, Kind, SchemaRef};
use super::schemas::{additional_properties_field, doc_attr, model_derives, object_field_tokens};
use super::types::schema_to_rust_type_ctx;

/// Generate the Rust type named `name` for a composition schema (one whose
/// [`model::kind`] is `OneOf`, `AnyOf` or `AllOf`). See the module docs for
/// the shapes.
#[must_use]
pub fn generate_composition(
    name: &str,
    schema: &ObjectSchema,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let enum_only = schema.properties.is_empty()
        && schema.all_of.is_empty()
        && !model::allows_additional_properties(schema)
        && schema.one_of.is_empty() != schema.any_of.is_empty();
    if !enum_only {
        return generate_struct(name, schema, ctx);
    }
    let description = schema.description.as_ref();
    if schema.one_of.is_empty() {
        generate_enum(name, &schema.any_of, None, description, ctx)
    } else {
        let discriminator = schema.discriminator.as_ref();
        generate_enum(name, &schema.one_of, discriminator, description, ctx)
    }
}

/// Generate the struct for a composition that isn't a plain enum (see the
/// module docs).
fn generate_struct(name: &str, schema: &ObjectSchema, ctx: &mut CodegenCtx) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&schema.description);

    // The tagged `oneOf` enum reads its tag itself, so the struct must not
    // consume it.
    let tag = schema
        .discriminator
        .as_ref()
        .filter(|_| !schema.one_of.is_empty())
        .map(|d| d.property_name.as_str());
    let mut fields: Vec<TokenStream> = schema
        .properties
        .iter()
        .filter(|(prop_name, _)| Some(prop_name.as_str()) != tag)
        .map(|(prop_name, prop_ref)| {
            let is_required = schema.required.iter().any(|r| r == prop_name);
            object_field_tokens(prop_name, prop_ref, is_required, name, ctx)
        })
        .collect();
    fields.extend(all_of_fields(name, &schema.all_of, ctx));
    let discriminator = schema.discriminator.as_ref();
    fields.extend(flattened_enum_field(
        name,
        "oneOf",
        &schema.one_of,
        discriminator,
        ctx,
    ));
    fields.extend(flattened_enum_field(
        name,
        "anyOf",
        &schema.any_of,
        None,
        ctx,
    ));
    // `kind` makes `oneOf` / `anyOf` next to `additionalProperties` untyped,
    // so this is only reached for `allOf`, where every field before the map
    // is a struct that consumes its own keys.
    fields.extend(additional_properties_field(name, schema, ctx));

    let derives = model_derives();
    quote! {
        #doc
        #derives
        pub struct #ident {
            #(#fields)*
        }
    }
}

/// The struct fields that merge the `allOf` branches of the composition
/// `name`.
///
/// `$ref` branches become `#[serde(flatten)]` fields of the referenced type.
/// Inline object branches contribute their properties directly. Any other
/// inline branch becomes a flattened field of a synthesized type, or
/// `serde_json::Value` (see [`schema_to_rust_type_ctx`]).
fn all_of_fields(name: &str, branches: &[Schema], ctx: &mut CodegenCtx) -> Vec<TokenStream> {
    let mut fields: Vec<TokenStream> = Vec::new();
    let mut ref_field_counter = 0usize;

    for branch in model::non_null_branches(branches) {
        match model::view(branch) {
            SchemaRef::Ref(reference) => {
                ref_field_counter += 1;
                let field_ident = format_ident!("inner_{}", ref_field_counter);
                let ty = ctx.ref_type(reference);
                fields.push(quote! {
                    #[serde(flatten)]
                    pub #field_ident: #ty,
                });
            }
            SchemaRef::Item(obj) => {
                if model::kind(obj) == Kind::Object {
                    for (prop_name, prop_ref) in &obj.properties {
                        let is_required = obj.required.iter().any(|r| r == prop_name);
                        fields.push(object_field_tokens(
                            prop_name,
                            prop_ref,
                            is_required,
                            name,
                            ctx,
                        ));
                    }
                } else {
                    // Nested composition or non-object inline branch: flatten a
                    // synthesized type when possible, otherwise fall back.
                    ref_field_counter += 1;
                    let field_ident = format_ident!("inner_{}", ref_field_counter);
                    let parent = format!("{name}Inner{ref_field_counter}");
                    let ty = schema_to_rust_type_ctx(branch, true, Some(&parent), ctx);
                    fields.push(quote! {
                        #[serde(flatten)]
                        pub #field_ident: #ty,
                    });
                }
            }
        }
    }
    fields
}

/// The `#[serde(flatten)]` field (`one_of` / `any_of`) that holds an enum of
/// the `keyword` branches of the composition `parent`, or `None` if there are
/// no branches. The enum is synthesized as `{parent}OneOf` /
/// `{parent}AnyOf`.
fn flattened_enum_field(
    parent: &str,
    keyword: &str,
    branches: &[Schema],
    discriminator: Option<&Discriminator>,
    ctx: &mut CodegenCtx,
) -> Option<TokenStream> {
    if branches.is_empty() {
        return None;
    }
    let enum_name = ctx.reserve_type_name(&format!("{parent}{}", keyword.to_pascal_case()));
    let parent_ident = super::idents::schema_type_ident(parent);
    let description = format!("The `{keyword}` branches of `{parent_ident}`.");
    let tokens = generate_enum(&enum_name, branches, discriminator, Some(&description), ctx);
    ctx.inline_types.push(tokens);
    let ty = ctx.local_type(&super::idents::schema_type_ident(&enum_name));
    let field = format_ident!("{}", keyword.to_snake_case());
    Some(quote! {
        #[serde(flatten)]
        pub #field: #ty,
    })
}

/// Generate an enum with one variant per `oneOf` / `anyOf` branch.
///
/// When a `discriminator` is present we emit an internally tagged enum
/// (`#[serde(tag = "...")]`); otherwise an untagged enum.
fn generate_enum(
    name: &str,
    variants: &[Schema],
    discriminator: Option<&Discriminator>,
    description: Option<&String>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&description.cloned());

    let serde_attr = discriminator.map_or_else(
        || quote! { #[serde(untagged)] },
        |d| {
            let tag = &d.property_name;
            quote! { #[serde(tag = #tag)] }
        },
    );

    let variant_tokens: Vec<TokenStream> = model::non_null_branches(variants)
        .enumerate()
        .map(|(idx, branch)| build_enum_variant(name, idx, branch, discriminator, ctx))
        .collect();

    let derives = model_derives();
    quote! {
        #doc
        #derives
        #serde_attr
        pub enum #ident {
            #(#variant_tokens,)*
        }
    }
}

/// Build a single enum variant for a `oneOf` / `anyOf` branch.
fn build_enum_variant(
    parent: &str,
    idx: usize,
    branch: &Schema,
    discriminator: Option<&Discriminator>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    match model::view(branch) {
        SchemaRef::Ref(reference) => {
            let target_name = model::ref_name(reference);
            let variant_ident = super::idents::schema_type_ident(target_name);
            let ty = ctx.ref_type(reference);
            // For tagged enums the rename governs the discriminator value on
            // the wire; for untagged enums the variant name is structural and
            // the rename is a no-op (but harmless).
            let rename_attr = discriminator
                .and_then(|d| discriminator_key_for_ref(d, reference))
                .map_or_else(|| quote! {}, |k| quote! { #[serde(rename = #k)] });
            quote! {
                #rename_attr
                #variant_ident(#ty)
            }
        }
        SchemaRef::Item(_) => {
            let variant_ident = format_ident!("Variant{}", idx + 1);
            let parent_for_synth = format!("{parent}Variant{}", idx + 1);
            let ty = schema_to_rust_type_ctx(branch, true, Some(&parent_for_synth), ctx);
            quote! {
                #variant_ident(#ty)
            }
        }
    }
}

/// Look up the discriminator mapping key (the serde tag value) for a given
/// `$ref`. Matches both fully qualified refs and bare component names, mirroring
/// the `OpenAPI` spec which permits either form in `discriminator.mapping`.
fn discriminator_key_for_ref(d: &Discriminator, reference: &str) -> Option<String> {
    let bare = model::ref_name(reference);
    d.mapping
        .iter()
        .flatten()
        .find(|(_, v)| *v == reference || *v == bare)
        .map(|(k, _)| k.clone())
}
