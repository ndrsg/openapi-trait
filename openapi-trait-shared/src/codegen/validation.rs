//! Field-level `serde_valid` attribute generation.
//!
//! Given the schema of an object property, [`validation_attrs`] emits the
//! `#[validate(...)]` attributes that mirror the `OpenAPI` constraint keywords
//! (`minLength`, `maximum`, `minItems`, …), so a generated model deriving
//! `serde_valid::Validate` enforces the contract declared by the spec when
//! `model.validate()` is called.
//!
//! Two shapes are produced:
//!
//! - **Scalar/array constraints** become explicit attributes such as
//!   `#[validate(min_length = 1)]` or `#[validate(unique_items)]`.
//! - **Fields whose type is another generated model** (a `$ref` to an
//!   object/composition/enum, an inline object, or an inline
//!   `oneOf`/`anyOf`/`allOf`) get a bare `#[validate]`, which tells
//!   `serde_valid` to recurse into the nested value.
//!
//! A bare `#[validate]` on a `$ref` is only sound when the referenced schema
//! lowers to a type that derives `Validate`. A `$ref` can just as easily point
//! at a scalar alias (`pub type Foo = String;`) or a map alias
//! (`pub type Foo = HashMap<String, String>;`), and emitting `#[validate]` there
//! makes `serde_valid` try to call `.validate()` on a `String`/`HashMap`, which
//! does not compile. To avoid that, [`validatable_model_names`] pre-computes the
//! set of schema names that actually become `Validate`-deriving models, and the
//! `$ref` branch only emits `#[validate]` when the target is in that set.
//!
//! Numeric literals are emitted *unsuffixed* so they coerce to the field's
//! concrete type (`i32`/`i64`/`f32`/`f64`) regardless of the schema's declared
//! `format`.
//!
//! Note: constraints on a top-level schema that lowers to a type alias
//! (`pub type Foo = String;`) cannot be attached to that alias; they only take
//! effect where the schema is referenced as a struct field.

use std::collections::BTreeSet;

use oas3::spec::{ObjectSchema, Schema, Spec};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

use super::model::{self, Kind, SchemaRef};

/// Compute the set of `components/schemas` names whose generated Rust type
/// derives `serde_valid::Validate` and can therefore be recursed into via a bare
/// `#[validate]`.
///
/// This mirrors the lowering decisions made by [`super::schemas`]: object
/// structs, string enums, and `oneOf`/`anyOf`/`allOf` compositions derive
/// `Validate`; scalar/array aliases and pure-map (`additionalProperties`) aliases
/// do not. A schema that is itself a bare `$ref` is validatable exactly when its
/// target is.
#[must_use]
pub fn validatable_model_names(openapi: &Spec) -> BTreeSet<String> {
    model::component_schemas(openapi)
        .filter(|(_, schema)| is_validatable(schema, openapi, &mut Vec::new()))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Whether a component schema (possibly a bare `$ref`) lowers to a
/// `Validate`-deriving type. `visited` guards against `$ref` cycles.
fn is_validatable(schema: &Schema, openapi: &Spec, visited: &mut Vec<String>) -> bool {
    match model::view(schema) {
        SchemaRef::Ref(reference) => {
            let target = model::ref_name(reference);
            if visited.iter().any(|v| v == target) {
                return false;
            }
            visited.push(target.to_string());
            model::component_schemas(openapi)
                .find(|(name, _)| *name == target)
                .is_some_and(|(_, t)| is_validatable(t, openapi, visited))
        }
        SchemaRef::Item(obj) => schema_is_validatable(obj),
    }
}

/// Whether an inline schema lowers to a `Validate`-deriving type.
fn schema_is_validatable(schema: &ObjectSchema) -> bool {
    if model::is_string_enum(schema) {
        return true;
    }
    match model::kind(schema) {
        Kind::OneOf | Kind::AnyOf | Kind::AllOf => true,
        Kind::Object => {
            // An object with declared properties becomes a struct (validatable).
            // A pure map (no properties, but `additionalProperties` yields a
            // value type) becomes a `HashMap` alias (not validatable). An object
            // with neither becomes an empty struct (validatable).
            !schema.properties.is_empty()
                || !matches!(
                    &schema.additional_properties,
                    Some(Schema::Object(_) | Schema::Boolean(oas3::spec::BooleanSchema(true)))
                )
        }
        // Scalar / array / boolean top-level schemas lower to type aliases, and
        // unconstrained schemas lower to `serde_json::Value`; none derive
        // `Validate`.
        _ => false,
    }
}

/// Emit the `serde_valid` `#[validate(...)]` attributes for one object property.
///
/// `models` is the set from [`validatable_model_names`]; a `$ref` only gets a
/// bare `#[validate]` when its target is a `Validate`-deriving model.
///
/// Returns an empty stream when the property carries no expressible constraint.
#[must_use]
pub fn validation_attrs(prop: &Schema, models: &BTreeSet<String>) -> TokenStream {
    match model::view(prop) {
        // A reference to another named schema: recurse only when that schema
        // lowers to a `Validate`-deriving model, so a top-level `.validate()`
        // walks into it. Refs to scalar/map aliases get no attribute.
        SchemaRef::Ref(reference) => {
            if models.contains(model::ref_name(reference)) {
                quote! { #[validate] }
            } else {
                quote! {}
            }
        }
        SchemaRef::Item(schema) => schema_attrs(schema),
    }
}

/// Dispatch to the per-kind attribute builder for an inline schema.
fn schema_attrs(schema: &ObjectSchema) -> TokenStream {
    match model::kind(schema) {
        Kind::String => string_attrs(schema),
        Kind::Integer => bound_attrs(schema, |n| n.as_i64().map(Literal::i64_unsuffixed)),
        Kind::Number => bound_attrs(schema, |n| n.as_f64().map(Literal::f64_unsuffixed)),
        Kind::Array => array_attrs(schema),
        // An inline object property is synthesized into its own struct that also
        // derives `Validate`; recurse into it.
        Kind::Object if !schema.properties.is_empty() => quote! { #[validate] },
        // Inline compositions are synthesized into nested models too.
        Kind::OneOf | Kind::AnyOf | Kind::AllOf => quote! { #[validate] },
        _ => quote! {},
    }
}

/// `minLength` / `maxLength` / `pattern` for a string field.
fn string_attrs(s: &ObjectSchema) -> TokenStream {
    let mut attrs = Vec::new();
    if let Some(min) = s.min_length {
        let lit = Literal::u64_unsuffixed(min);
        attrs.push(quote! { #[validate(min_length = #lit)] });
    }
    if let Some(max) = s.max_length {
        let lit = Literal::u64_unsuffixed(max);
        attrs.push(quote! { #[validate(max_length = #lit)] });
    }
    if let Some(pattern) = &s.pattern {
        attrs.push(quote! { #[validate(pattern = #pattern)] });
    }
    quote! { #(#attrs)* }
}

/// `minimum` / `exclusiveMinimum` / `maximum` / `exclusiveMaximum` /
/// `multipleOf` for a numeric field. `literal` turns a bound into a literal of
/// the field's kind (integer or float), or `None` when it doesn't fit, in
/// which case the bound is skipped.
///
/// The exclusive bounds are numbers, as in OAS 3.1; a 3.0 document's boolean
/// flags were converted when it was loaded (see [`crate::spec`]).
fn bound_attrs(
    schema: &ObjectSchema,
    literal: impl Fn(&serde_json::Number) -> Option<Literal>,
) -> TokenStream {
    let bounds = [
        (&schema.minimum, quote!(minimum)),
        (&schema.exclusive_minimum, quote!(exclusive_minimum)),
        (&schema.maximum, quote!(maximum)),
        (&schema.exclusive_maximum, quote!(exclusive_maximum)),
        (&schema.multiple_of, quote!(multiple_of)),
    ];
    let attrs = bounds.into_iter().filter_map(|(bound, name)| {
        let lit = literal(bound.as_ref()?)?;
        Some(quote! { #[validate(#name = #lit)] })
    });
    quote! { #(#attrs)* }
}

/// `minItems` / `maxItems` / `uniqueItems` for an array field.
fn array_attrs(a: &ObjectSchema) -> TokenStream {
    let mut attrs = Vec::new();
    if let Some(min) = a.min_items {
        let lit = Literal::u64_unsuffixed(min);
        attrs.push(quote! { #[validate(min_items = #lit)] });
    }
    if let Some(max) = a.max_items {
        let lit = Literal::u64_unsuffixed(max);
        attrs.push(quote! { #[validate(max_items = #lit)] });
    }
    if a.unique_items == Some(true) {
        attrs.push(quote! { #[validate(unique_items)] });
    }
    quote! { #(#attrs)* }
}
