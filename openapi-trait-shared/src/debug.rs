//! Optional, env-var-gated dump of generated code for local inspection.
//!
//! Unlike `cargo expand`, this writes only the tokens a backend macro produced
//! directly — it does not recursively expand the nested derives and macros
//! inside that output — which keeps the dump focused on the code generation
//! under inspection.

use std::path::PathBuf;

/// Environment variable that enables and (optionally) locates the dump.
const ENV_VAR: &str = "OPENAPI_TRAIT_DEBUG";

/// Write a prettyprinted copy of `expanded` to disk when debug output is
/// enabled.
///
/// Whether anything is written is decided at macro-expansion time from the
/// `OPENAPI_TRAIT_DEBUG` environment variable:
///
/// - unset, empty, `0`, or `false` — disabled (no-op).
/// - `1` or `true` — write to the default directory.
/// - any other value — used verbatim as the target directory path.
///
/// The default directory is `$OUT_DIR/openapi-trait-debug` when `OUT_DIR` is
/// set (i.e. the consuming crate has a build script), otherwise
/// `<system temp dir>/openapi-trait-debug`. The file is named after the
/// generated module (`<mod_ident>.rs`), and the resolved path is printed to
/// stderr.
///
/// Failures are reported to stderr but never abort compilation: debug output
/// is a convenience and must not turn a buildable crate into a broken one
/// (e.g. on read-only filesystems).
pub fn write_debug_output(mod_ident: &syn::Ident, expanded: &proc_macro2::TokenStream) {
    let Some(dir) = resolve_dir() else {
        return;
    };

    let path = dir.join(format!("{mod_ident}.rs"));

    let formatted = syn::parse2::<syn::File>(expanded.clone())
        .map_or_else(|_| expanded.to_string(), |file| prettyplease::unparse(&file));

    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, formatted)) {
        Ok(()) => eprintln!("openapi-trait: wrote debug output to {}", path.display()),
        Err(error) => eprintln!(
            "openapi-trait: failed to write debug output to {}: {error}",
            path.display()
        ),
    }
}

/// Resolve the target directory, or `None` when debug output is disabled.
fn resolve_dir() -> Option<PathBuf> {
    let value = std::env::var(ENV_VAR).ok()?;

    match value.trim() {
        "" | "0" | "false" => None,
        "1" | "true" => Some(default_dir()),
        path => Some(PathBuf::from(path)),
    }
}

/// Default dump directory when the env var is a plain on/off toggle.
fn default_dir() -> PathBuf {
    std::env::var_os("OUT_DIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("openapi-trait-debug")
}
