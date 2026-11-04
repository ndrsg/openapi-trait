//! The expansion of `#[openapi_trait::axum(...)]` and
//! `#[openapi_trait::client(...)]`, shared by both backends.

use oas3::Spec;
use proc_macro2::{Span, TokenStream};
use syn::ItemMod;

use crate::args::MacroArgs;
use crate::source_file::{module_contents, warning_items, wrap_module};

/// Expand the macro on the module `item`: load the spec named in `args`, let
/// `generate` produce the module body and its warnings, and wrap the result
/// in the module.
///
/// `backend` (`"axum"`, `"client"`) names the backend in the generated
/// source file's name. Problems with the spec file are reported as compile
/// errors on the spec path literal.
pub fn expand(
    args: &MacroArgs,
    item: TokenStream,
    backend: &str,
    generate: impl FnOnce(&syn::Ident, &Spec) -> (TokenStream, Vec<String>),
) -> TokenStream {
    let module = match syn::parse2::<ItemMod>(item) {
        Ok(module) => module,
        Err(error) => return error.to_compile_error(),
    };
    let path_lit = &args.spec;
    let mod_ident = &module.ident;

    let (spec_path, openapi) = match load_spec(path_lit) {
        Ok(loaded) => loaded,
        Err(error) => return error.to_compile_error(),
    };

    let (body, mut warnings) = generate(mod_ident, &openapi);

    let contents = module_contents(
        args.source_file.as_ref(),
        backend,
        mod_ident,
        &path_lit.value(),
        &body,
        &mut warnings,
    );
    let warnings = warning_items(&warnings, path_lit.span());
    let expanded = wrap_module(&module.vis, mod_ident, &spec_path, &warnings, &contents);

    crate::debug::write_debug_output(mod_ident, &expanded);

    expanded
}

/// Read and parse the spec at `path_lit` (relative to `CARGO_MANIFEST_DIR`).
///
/// Returns the spec's absolute path and the parsed document.
///
/// # Errors
///
/// Returns an error spanned at `path_lit` if the file can't be read or
/// parsed, or at the call site if `CARGO_MANIFEST_DIR` isn't set.
pub fn load_spec(path_lit: &syn::LitStr) -> Result<(String, Spec), syn::Error> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "CARGO_MANIFEST_DIR is not set; cannot resolve spec path",
        )
    })?;

    let spec_path = std::path::PathBuf::from(&manifest_dir).join(path_lit.value());
    let spec_path_str = spec_path.to_string_lossy().into_owned();

    let content = std::fs::read_to_string(&spec_path).map_err(|error| {
        let msg = format!("cannot read OpenAPI spec `{spec_path_str}`: {error}");
        syn::Error::new(path_lit.span(), msg)
    })?;

    let openapi = crate::spec::load_spec(&content).map_err(|error| {
        let msg = format!("cannot parse OpenAPI spec `{spec_path_str}`: {error}");
        syn::Error::new(path_lit.span(), msg)
    })?;

    Ok((spec_path_str, openapi))
}
