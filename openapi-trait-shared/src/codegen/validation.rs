//! Field-level `serde_valid` attribute generation.
//!
//! Compiled only under the `validation` feature. Given the schema of an object
//! property, [`validation_attrs`] emits the `#[validate(...)]` attributes that
//! mirror the `OpenAPI` constraint keywords (`minLength`, `maximum`, `minItems`,
//! …), so a generated model deriving `serde_valid::Validate` enforces the
//! contract declared by the spec when `model.validate()` is called.
//!
//! Two shapes are produced:
//!
//! - **Scalar/array constraints** become explicit attributes such as
//!   `#[validate(min_length = 1)]` or `#[validate(unique_items)]`.
//! - **Fields whose type is another generated model** (a `$ref`, an inline
//!   object, or a `oneOf`/`anyOf`/`allOf` composition) get a bare
//!   `#[validate]`, which tells `serde_valid` to recurse into the nested value.
//!   This is safe because every generated model derives `Validate` under this
//!   feature.
//!
//! Numeric literals are emitted *unsuffixed* so they coerce to the field's
//! concrete type (`i32`/`i64`/`f32`/`f64`) regardless of the schema's declared
//! `format`.
//!
//! Note: constraints on a top-level schema that lowers to a type alias
//! (`pub type Foo = String;`) cannot be attached to that alias; they only take
//! effect where the schema is referenced as a struct field.

use openapiv3::{
    ArrayType, IntegerType, NumberType, ReferenceOr, Schema, SchemaKind, StringType, Type,
};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

/// Emit the `serde_valid` `#[validate(...)]` attributes for one object property.
///
/// Returns an empty stream when the property carries no expressible constraint.
#[must_use]
pub fn validation_attrs(prop: &ReferenceOr<Schema>) -> TokenStream {
    match prop {
        // A reference to another named schema: recurse so a top-level
        // `.validate()` walks into the referenced model.
        ReferenceOr::Reference { .. } => quote! { #[validate] },
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
