//! Shared state threaded through schema and operation codegen.

use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{ObjectSchema, Schema, Spec};
use proc_macro2::TokenStream;
use quote::quote;

use super::model::{self, Kind, SchemaRef};

/// State shared by every codegen stage for one `OpenAPI` document.
///
/// Create one per macro expansion with [`CodegenCtx::new`] and pass it to
/// [`super::schemas::generate_schemas`] and
/// [`super::operations::collect_operations`].
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
    /// Type identifiers already taken in the module being generated (see
    /// [`CodegenCtx::reserve_type_name`]).
    type_names: BTreeSet<String>,
    /// The operation module being generated (see
    /// [`CodegenCtx::with_operation`]), or `None` at the macro module's top
    /// level.
    op_module: Option<syn::Ident>,
}

/// Name of the hidden module that re-exports the macro module's top level
/// (see [`root_module`]).
pub const ROOT_MODULE: &str = "__root";

/// The hidden `__root` module, which re-exports every public item of the
/// macro module.
///
/// Type paths that are used both at the top level and inside an operation
/// module are written through it (`__root::Pet`, `__root::create_pet::Body`),
/// so they resolve the same way in both places, and a schema named like an
/// operation type (`Request`) can't be shadowed by it.
#[must_use]
pub fn root_module() -> TokenStream {
    let root = quote::format_ident!("{ROOT_MODULE}");
    quote! {
        #[doc(hidden)]
        pub mod #root {
            #[allow(unused_imports)]
            pub use super::*;
        }
    }
}

impl CodegenCtx {
    /// Build the context for `openapi`.
    #[must_use]
    pub fn new(openapi: &Spec) -> Self {
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
            type_names: fixed_type_names(openapi),
            op_module: None,
        }
    }

    /// Run `f` while collecting the types of the operation whose module is
    /// `module`, and return its result with the types synthesized meanwhile.
    ///
    /// Inside, the operation module has its own type-name registry, starting
    /// with `reserved`, and type paths are qualified through [`ROOT_MODULE`]
    /// (see [`CodegenCtx::ref_type`] and [`CodegenCtx::local_type`]).
    pub fn with_operation<R>(
        &mut self,
        module: &syn::Ident,
        reserved: impl IntoIterator<Item = String>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> (R, Vec<TokenStream>) {
        let prev_names = std::mem::replace(&mut self.type_names, reserved.into_iter().collect());
        let prev_types = std::mem::take(&mut self.inline_types);
        let prev_module = self.op_module.replace(module.clone());
        let out = f(self);
        self.type_names = prev_names;
        self.op_module = prev_module;
        let types = std::mem::replace(&mut self.inline_types, prev_types);
        (out, types)
    }

    /// The path to a type synthesized at the current position: the bare
    /// `ident` at the top level, `__root::{module}::{ident}` inside an
    /// operation.
    #[must_use]
    pub fn local_type(&self, ident: &syn::Ident) -> TokenStream {
        self.op_module.as_ref().map_or_else(
            || quote! { #ident },
            |module| {
                let root = quote::format_ident!("{ROOT_MODULE}");
                quote! { #root::#module::#ident }
            },
        )
    }

    /// Reserve a type name for a type synthesized from `candidate`, such as
    /// an inline property schema (`PersonAddress`).
    ///
    /// Returns `candidate` when its identifier is still free, and otherwise
    /// the first free `{candidate}2`, `{candidate}3`, …, so a synthesized type
    /// never clashes with a component schema, an operation type or another
    /// synthesized type.
    pub fn reserve_type_name(&mut self, candidate: &str) -> String {
        let mut name = candidate.to_owned();
        let mut n = 1u32;
        while !self
            .type_names
            .insert(super::idents::schema_type_ident(&name).to_string())
        {
            n += 1;
            name = format!("{candidate}{n}");
        }
        name
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
            .get(model::ref_name(reference))
            .is_some_and(|reached| reached.contains(owner))
    }

    /// The Rust type for a by-value `$ref` at the current position: the
    /// referenced type, boxed when it closes a cycle, and qualified through
    /// [`ROOT_MODULE`] inside an operation.
    #[must_use]
    pub fn ref_type(&self, reference: &str) -> TokenStream {
        let mut ident = super::types::ref_to_ident(reference);
        if self.op_module.is_some() {
            let root = quote::format_ident!("{ROOT_MODULE}");
            ident = quote! { #root::#ident };
        }
        if self.needs_box(reference) {
            quote! { ::std::boxed::Box<#ident> }
        } else {
            ident
        }
    }
}

/// The type identifiers the macro module defines under fixed names:
/// component schemas and security schemes. Operation types live in their
/// own modules (see [`CodegenCtx::with_operation`]).
fn fixed_type_names(openapi: &Spec) -> BTreeSet<String> {
    let Some(components) = &openapi.components else {
        return BTreeSet::new();
    };
    components
        .schemas
        .keys()
        .chain(components.security_schemes.keys())
        .map(|name| super::idents::schema_type_ident(name).to_string())
        .collect()
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
fn by_value_edges(openapi: &Spec) -> BTreeMap<&str, Vec<ByValueRef<'_>>> {
    model::component_schemas(openapi)
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
fn by_value_refs<'a>(schema: &'a Schema, in_property: bool, out: &mut Vec<ByValueRef<'a>>) {
    match model::view(schema) {
        SchemaRef::Ref(reference) => out.push(ByValueRef {
            target: model::ref_name(reference),
            in_property,
        }),
        SchemaRef::Item(schema) => inline_by_value_refs(schema, in_property, out),
    }
}

/// [`by_value_refs`] for an inline schema.
fn inline_by_value_refs<'a>(
    schema: &'a ObjectSchema,
    in_property: bool,
    out: &mut Vec<ByValueRef<'a>>,
) {
    match model::kind(schema) {
        Kind::Object => {
            for prop in schema.properties.values() {
                by_value_refs(prop, true, out);
            }
        }
        Kind::OneOf | Kind::AnyOf | Kind::AllOf => {
            let branches = schema
                .one_of
                .iter()
                .chain(&schema.any_of)
                .chain(&schema.all_of);
            for branch in branches {
                by_value_refs(branch, in_property, out);
            }
        }
        _ => {}
    }
}
