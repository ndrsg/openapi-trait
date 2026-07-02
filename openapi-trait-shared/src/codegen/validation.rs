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

use openapiv3::{
    AdditionalProperties, ArrayType, Components, IntegerType, NumberType, OpenAPI, ReferenceOr,
    Schema, SchemaKind, StringType, Type,
};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

use super::types::is_string_enum;

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
pub fn validatable_model_names(openapi: &OpenAPI) -> BTreeSet<String> {
    let Some(components) = &openapi.components else {
        return BTreeSet::new();
    };
    components
        .schemas
        .iter()
        .filter(|(_, ref_or)| is_validatable(ref_or, components, &mut Vec::new()))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Whether a component schema (possibly a bare `$ref`) lowers to a
/// `Validate`-deriving type. `visited` guards against `$ref` cycles.
fn is_validatable(
    ref_or: &ReferenceOr<Schema>,
    components: &Components,
    visited: &mut Vec<String>,
) -> bool {
    match ref_or {
        ReferenceOr::Reference { reference } => {
            let target = reference.rsplit('/').next().unwrap_or(reference);
            if visited.iter().any(|v| v == target) {
                return false;
            }
            visited.push(target.to_string());
            components
                .schemas
                .get(target)
                .is_some_and(|t| is_validatable(t, components, visited))
        }
        ReferenceOr::Item(schema) => schema_is_validatable(schema),
    }
}

/// Whether an inline schema lowers to a `Validate`-deriving type.
fn schema_is_validatable(schema: &Schema) -> bool {
    if is_string_enum(schema) {
        return true;
    }
    match &schema.schema_kind {
        SchemaKind::OneOf { .. } | SchemaKind::AnyOf { .. } | SchemaKind::AllOf { .. } => true,
        SchemaKind::Type(Type::Object(obj)) => {
            // An object with declared properties becomes a struct (validatable).
            // A pure map (no properties, but `additionalProperties` yields a
            // value type) becomes a `HashMap` alias (not validatable). An object
            // with neither becomes an empty struct (validatable).
            !obj.properties.is_empty()
                || !matches!(
                    &obj.additional_properties,
                    Some(AdditionalProperties::Any(true) | AdditionalProperties::Schema(_))
                )
        }
        // Scalar / array / boolean top-level schemas lower to type aliases, and
        // `Not`/`Any` lower to `serde_json::Value`; none derive `Validate`.
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
pub fn validation_attrs(prop: &ReferenceOr<Schema>, models: &BTreeSet<String>) -> TokenStream {
    match prop {
        // A reference to another named schema: recurse only when that schema
        // lowers to a `Validate`-deriving model, so a top-level `.validate()`
        // walks into it. Refs to scalar/map aliases get no attribute.
        ReferenceOr::Reference { reference } => {
            let target = reference.rsplit('/').next().unwrap_or(reference);
            if models.contains(target) {
                quote! { #[validate] }
            } else {
                quote! {}
            }
        }
        ReferenceOr::Item(schema) => schema_attrs(schema),
    }
}

/// Dispatch to the per-kind attribute builder for an inline schema.
fn schema_attrs(schema: &Schema) -> TokenStream {
    match &schema.schema_kind {
        SchemaKind::Type(Type::String(s)) => string_attrs(s),
        SchemaKind::Type(Type::Integer(i)) => integer_attrs(i),
        SchemaKind::Type(Type::Number(n)) => number_attrs(n),
        SchemaKind::Type(Type::Array(a)) => array_attrs(a),
        // An inline object property is synthesized into its own struct that also
        // derives `Validate`; recurse into it.
        SchemaKind::Type(Type::Object(obj)) if !obj.properties.is_empty() => quote! { #[validate] },
        // Inline compositions are synthesized into nested models too.
        SchemaKind::OneOf { .. } | SchemaKind::AnyOf { .. } | SchemaKind::AllOf { .. } => {
            quote! { #[validate] }
        }
        _ => quote! {},
    }
}

/// `minLength` / `maxLength` / `pattern` for a string field.
fn string_attrs(s: &StringType) -> TokenStream {
    let mut attrs = Vec::new();
    if let Some(min) = s.min_length {
        let lit = Literal::usize_unsuffixed(min);
        attrs.push(quote! { #[validate(min_length = #lit)] });
    }
    if let Some(max) = s.max_length {
        let lit = Literal::usize_unsuffixed(max);
        attrs.push(quote! { #[validate(max_length = #lit)] });
    }
    if let Some(pattern) = &s.pattern {
        attrs.push(quote! { #[validate(pattern = #pattern)] });
    }
    quote! { #(#attrs)* }
}

/// `minimum` / `maximum` / `multipleOf` for an integer field, honoring the
/// `exclusiveMinimum` / `exclusiveMaximum` boolean flags.
fn integer_attrs(i: &IntegerType) -> TokenStream {
    let min = i.minimum.map(Literal::i64_unsuffixed);
    let max = i.maximum.map(Literal::i64_unsuffixed);
    let mult = i.multiple_of.map(Literal::i64_unsuffixed);
    bound_attrs(min, i.exclusive_minimum, max, i.exclusive_maximum, mult)
}

/// `minimum` / `maximum` / `multipleOf` for a floating-point field, honoring the
/// `exclusiveMinimum` / `exclusiveMaximum` boolean flags.
fn number_attrs(n: &NumberType) -> TokenStream {
    let min = n.minimum.map(Literal::f64_unsuffixed);
    let max = n.maximum.map(Literal::f64_unsuffixed);
    let mult = n.multiple_of.map(Literal::f64_unsuffixed);
    bound_attrs(min, n.exclusive_minimum, max, n.exclusive_maximum, mult)
}

/// Shared numeric-bound emitter for integers and numbers.
///
/// An `exclusive_*` flag turns the corresponding bound into
/// `exclusive_minimum` / `exclusive_maximum`; otherwise the inclusive
/// `minimum` / `maximum` attribute is used.
fn bound_attrs(
    min: Option<Literal>,
    exclusive_min: bool,
    max: Option<Literal>,
    exclusive_max: bool,
    multiple_of: Option<Literal>,
) -> TokenStream {
    let mut attrs = Vec::new();
    if let Some(min) = min {
        if exclusive_min {
            attrs.push(quote! { #[validate(exclusive_minimum = #min)] });
        } else {
            attrs.push(quote! { #[validate(minimum = #min)] });
        }
    }
    if let Some(max) = max {
        if exclusive_max {
            attrs.push(quote! { #[validate(exclusive_maximum = #max)] });
        } else {
            attrs.push(quote! { #[validate(maximum = #max)] });
        }
    }
    if let Some(mult) = multiple_of {
        attrs.push(quote! { #[validate(multiple_of = #mult)] });
    }
    quote! { #(#attrs)* }
}

/// `minItems` / `maxItems` / `uniqueItems` for an array field.
fn array_attrs(a: &ArrayType) -> TokenStream {
    let mut attrs = Vec::new();
    if let Some(min) = a.min_items {
        let lit = Literal::usize_unsuffixed(min);
        attrs.push(quote! { #[validate(min_items = #lit)] });
    }
    if let Some(max) = a.max_items {
        let lit = Literal::usize_unsuffixed(max);
        attrs.push(quote! { #[validate(max_items = #lit)] });
    }
    if a.unique_items {
        attrs.push(quote! { #[validate(unique_items)] });
    }
    quote! { #(#attrs)* }
}
