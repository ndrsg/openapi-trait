//! Generators for `oneOf` / `allOf` / `anyOf` schema compositions.
//!
//! - `oneOf` → Rust enum. Tagged via `#[serde(tag = "...")]` when the schema
//!   carries a `discriminator`; otherwise `#[serde(untagged)]`.
//! - `anyOf` → Rust enum, always `#[serde(untagged)]` (`oneOf` semantics without
//!   the exclusivity guarantee is rarely useful in typed Rust).
//! - `allOf` → Rust struct that merges its branches: `$ref` branches become
//!   `#[serde(flatten)]` fields; inline object branches inline their properties
//!   directly.
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

use oas3::spec::{Discriminator, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::ctx::CodegenCtx;
use super::model::{self, Kind, SchemaRef};
use super::schemas::{doc_attr, model_derives, object_field_tokens};
use super::types::schema_to_rust_type_ctx;

/// Generate a Rust enum type for a `oneOf` composition.
///
/// When a `discriminator` is present we emit an internally tagged enum
/// (`#[serde(tag = "...")]`); otherwise an untagged enum.
#[must_use]
pub fn generate_one_of(
    name: &str,
    variants: &[Schema],
    discriminator: Option<&Discriminator>,
    description: Option<&String>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    generate_enum(name, variants, discriminator, description, ctx)
}

/// Generate a Rust enum type for an `anyOf` composition.
///
/// Always untagged — we treat `anyOf` like a non-discriminated `oneOf` because
/// strict `anyOf` semantics (multiple branches may match) do not have a clean
/// representation in typed Rust.
#[must_use]
pub fn generate_any_of(
    name: &str,
    variants: &[Schema],
    description: Option<&String>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    generate_enum(name, variants, None, description, ctx)
}

/// Generate a Rust struct type for an `allOf` composition.
///
/// `$ref` branches become `#[serde(flatten)]` fields of the referenced type.
/// Inline object branches contribute their properties directly. Any other inline
/// branch falls back to `serde_json::Value` (we do not synthesize nested types
/// here — those are handled by the enum path via [`schema_to_rust_type_ctx`]).
#[must_use]
pub fn generate_all_of(
    name: &str,
    variants: &[Schema],
    description: Option<&String>,
    ctx: &mut CodegenCtx,
) -> TokenStream {
    let ident = super::idents::schema_type_ident(name);
    let doc = doc_attr(&description.cloned());

    let mut fields: Vec<TokenStream> = Vec::new();
    let mut ref_field_counter = 0usize;

    for branch in model::non_null_branches(variants) {
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

    let derives = model_derives();
    quote! {
        #doc
        #derives
        pub struct #ident {
            #(#fields)*
        }
    }
}

/// Shared helper backing [`generate_one_of`] and [`generate_any_of`].
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
