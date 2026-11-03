//! Shared state threaded through schema and operation codegen.

use std::collections::BTreeSet;

use openapiv3::OpenAPI;
use proc_macro2::TokenStream;

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
}

impl CodegenCtx {
    /// Build the context for `openapi`.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // not const with the `validation` feature
    pub fn new(openapi: &OpenAPI) -> Self {
        #[cfg(feature = "validation")]
        let models = super::validation::validatable_model_names(openapi);
        #[cfg(not(feature = "validation"))]
        let models = {
            let _ = openapi;
            BTreeSet::new()
        };
        Self {
            inline_types: Vec::new(),
            models,
        }
    }

    /// Take the inline types synthesized so far, leaving the sink empty.
    pub fn take_inline_types(&mut self) -> Vec<TokenStream> {
        std::mem::take(&mut self.inline_types)
    }
}
