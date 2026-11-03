//! Shared state threaded through schema and operation codegen.

use std::collections::{BTreeMap, BTreeSet};

use openapiv3::{OpenAPI, ReferenceOr, Schema, SchemaKind, Type};
use proc_macro2::TokenStream;
use quote::quote;

/// State shared by every codegen stage for one `OpenAPI` document.
///
/// Create one per macro expansion with [`CodegenCtx::new`], pass it to
/// [`super::schemas::generate_schemas`] and
/// [`super::operations::collect_operations`], and emit
/// [`CodegenCtx::take_inline_types`] alongside the generated items.
#[derive(Debug, Default)]
pub struct CodegenCtx {
    /// Top-level types synthesized from inline schemas (inline objects,
    /// enums and compositions), waiting to be emitted at module scope.
    pub inline_types: Vec<TokenStream>,
    /// Names of component schemas whose generated type derives `Validate`, so
    /// a `$ref` field to one can carry a bare `#[validate]`. Empty without the
    /// `validation` feature.
    pub models: BTreeSet<String>,
    /// For each component schema, the component schemas it embeds by value,
    /// directly or transitively (see [`by_value_refs`]).
    reaches: BTreeMap<String, BTreeSet<String>>,
    /// Like `reaches`, but only following references that aren't below an
    /// object property: `oneOf` / `anyOf` variants and `allOf` branches.
    reaches_via_compositions: BTreeMap<String, BTreeSet<String>>,
    /// The component schema whose by-value layout is being generated, or
    /// `None` outside a component or below a `Vec` / `HashMap`, where a
    /// reference can't make a type infinitely sized.
    owner: Option<String>,
    /// Whether the current position is below an object property of `owner`
    /// (a struct field), rather than directly in a composition.
    in_property: bool,
}

impl CodegenCtx {
    /// Build the context for `openapi`.
    #[must_use]
    pub fn new(openapi: &OpenAPI) -> Self {
        #[cfg(feature = "validation")]
        let models = super::validation::validatable_model_names(openapi);
        #[cfg(not(feature = "validation"))]
        let models = BTreeSet::new();
        let edges = by_value_edges(openapi);
        let compositions_only = edges
            .iter()
            .map(|(&name, refs)| {
                let refs = refs.iter().filter(|r| !r.in_property).copied().collect();
                (name, refs)
            })
            .collect();
        Self {
            inline_types: Vec::new(),
            models,
            reaches: reachability(&edges),
            reaches_via_compositions: reachability(&compositions_only),
            owner: None,
            in_property: false,
        }
    }

    /// Take the inline types synthesized so far, leaving the sink empty.
    pub fn take_inline_types(&mut self) -> Vec<TokenStream> {
        std::mem::take(&mut self.inline_types)
    }

    /// Run `f` while generating the by-value layout of component `name`.
    pub fn with_owner<R>(&mut self, name: &str, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = (self.owner.replace(name.to_owned()), self.in_property);
        self.in_property = false;
        let out = f(self);
        (self.owner, self.in_property) = prev;
        out
    }

    /// Run `f` while generating the type of an object property (a struct
    /// field).
    pub fn in_property<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = std::mem::replace(&mut self.in_property, true);
        let out = f(self);
        self.in_property = prev;
        out
    }

    /// Run `f` below an indirection (`Vec`, `HashMap`), where references
    /// never need a `Box`.
    pub fn behind_indirection<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = self.owner.take();
        let out = f(self);
        self.owner = prev;
        out
    }

    /// Whether a by-value `$ref` to `reference` at the current position closes
    /// a cycle, so it must be emitted as `Box<T>` to give the type a finite
    /// size.
    ///
    /// Struct fields are boxed in preference to enum variants and `allOf`
    /// branches: a field is boxed when its target leads back to the owner at
    /// all, a variant only when it leads back through compositions alone.
    /// Every cycle is broken either way: one with a field in it at that
    /// field, one without at each of its variants.
    #[must_use]
    pub fn needs_box(&self, reference: &str) -> bool {
        let Some(owner) = &self.owner else {
            return false;
        };
        let reaches = if self.in_property {
            &self.reaches
        } else {
            &self.reaches_via_compositions
        };
        reaches
            .get(ref_name(reference))
            .is_some_and(|reached| reached.contains(owner))
    }

    /// The Rust type for a by-value `$ref` at the current position: the
    /// referenced type, boxed when it closes a cycle.
    #[must_use]
    pub fn ref_type(&self, reference: &str) -> TokenStream {
        let ident = super::types::ref_to_ident(reference);
        if self.needs_box(reference) {
            quote! { ::std::boxed::Box<#ident> }
        } else {
            ident
        }
    }
}

/// The component name a `$ref` points at: `#/components/schemas/Foo` → `Foo`.
fn ref_name(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}

/// A by-value reference from one component schema to another.
#[derive(Debug, Clone, Copy)]
struct ByValueRef<'a> {
    /// The referenced component name.
    target: &'a str,
    /// Whether the reference is below an object property.
    in_property: bool,
}

/// For each component schema, its by-value references (see [`by_value_refs`]).
fn by_value_edges(openapi: &OpenAPI) -> BTreeMap<&str, Vec<ByValueRef<'_>>> {
    let Some(components) = &openapi.components else {
        return BTreeMap::new();
    };
    components
        .schemas
        .iter()
        .map(|(name, schema)| {
            let mut out = Vec::new();
            by_value_refs(schema, false, &mut out);
            (name.as_str(), out)
        })
        .collect()
}

/// For each component schema, the set of component schemas reachable through
/// chains of `edges`.
fn reachability(edges: &BTreeMap<&str, Vec<ByValueRef<'_>>>) -> BTreeMap<String, BTreeSet<String>> {
    edges
        .iter()
        .map(|(&start, refs)| {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut stack: Vec<&str> = refs.iter().map(|r| r.target).collect();
            while let Some(next) = stack.pop() {
                if seen.insert(next.to_owned()) {
                    if let Some(more) = edges.get(next) {
                        stack.extend(more.iter().map(|r| r.target));
                    }
                }
            }
            (start.to_owned(), seen)
        })
        .collect()
}

/// Collect the component names that `schema` embeds by value: `$ref`s in
/// object properties and in `allOf` / `oneOf` / `anyOf` branches, including
/// those nested in inline schemas. References behind an array or a map are
/// skipped, because `Vec` and `HashMap` already add indirection.
fn by_value_refs<'a>(
    schema: &'a ReferenceOr<Schema>,
    in_property: bool,
    out: &mut Vec<ByValueRef<'a>>,
) {
    match schema {
        ReferenceOr::Reference { reference } => out.push(ByValueRef {
            target: ref_name(reference),
            in_property,
        }),
        ReferenceOr::Item(schema) => inline_by_value_refs(schema, in_property, out),
    }
}

/// [`by_value_refs`] for an inline schema.
fn inline_by_value_refs<'a>(schema: &'a Schema, in_property: bool, out: &mut Vec<ByValueRef<'a>>) {
    match &schema.schema_kind {
        SchemaKind::Type(Type::Object(obj)) => {
            for prop in obj.properties.values() {
                match prop {
                    ReferenceOr::Reference { reference } => out.push(ByValueRef {
                        target: ref_name(reference),
                        in_property: true,
                    }),
                    ReferenceOr::Item(inner) => inline_by_value_refs(inner, true, out),
                }
            }
        }
        SchemaKind::OneOf { one_of: branches }
        | SchemaKind::AnyOf { any_of: branches }
        | SchemaKind::AllOf { all_of: branches } => {
            for branch in branches {
                by_value_refs(branch, in_property, out);
            }
        }
        _ => {}
    }
}
