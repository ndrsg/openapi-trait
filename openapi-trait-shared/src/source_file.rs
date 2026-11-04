//! Opt-in emission of the generated code as a real source file.
//!
//! Tokens produced by a proc macro carry the span of the macro invocation, so
//! IDE go-to-definition on a generated item lands on the attribute. When the
//! user passes `source_file`, the backend writes the prettyprinted module body
//! to disk and expands to `include!("<that file>")` instead. rustc and
//! rust-analyzer then attribute every generated token to that file, so
//! go-to-definition opens readable generated code.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::args::SourceFileOpt;

/// Directory, relative to `CARGO_MANIFEST_DIR`, used by a bare `source_file`.
pub const DEFAULT_DIR: &str = ".openapi-trait";

/// Makes each temp file name of [`write_source_file`] unique within the
/// process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Prettyprint `tokens` as a Rust file, falling back to the raw token string
/// if they don't parse as one.
#[must_use]
pub fn pretty(tokens: &TokenStream) -> String {
    syn::parse2::<syn::File>(tokens.clone())
        .map_or_else(|_| tokens.to_string(), |file| prettyplease::unparse(&file))
}

/// Wrap the generated `inner` tokens in the annotated module, with the lint
/// allowances and the spec-file rebuild trigger shared by every backend.
///
/// `warnings` (from [`warning_items`]) go next to the module rather than
/// into it, so the module's `#[allow]`s don't apply to them and they keep
/// their span when the body is `include!`d from a source file.
#[must_use]
pub fn wrap_module(
    vis: &syn::Visibility,
    ident: &syn::Ident,
    spec_path: &str,
    warnings: &TokenStream,
    inner: &TokenStream,
) -> TokenStream {
    quote! {
        // Re-compile when the spec file changes.
        const _: &str = ::core::include_str!(#spec_path);

        #warnings

        #[allow(
            missing_docs,
            missing_debug_implementations,
            dead_code,
            unused_imports,
            // Code `include!`d from a real file loses the macro-span lint
            // suppression rustc applies to proc-macro output.
            unused,
            clippy::all,
            clippy::nursery,
            clippy::pedantic,
        )]
        #vis mod #ident {
            #inner
        }
    }
}

/// Turn codegen warnings into compiler warnings that point at `span` (the
/// spec path literal).
///
/// Stable Rust has no API for a proc macro to emit a warning, and anything
/// printed to stderr is mostly invisible: cargo hides it for dependencies,
/// rust-analyzer never shows it, and a cached build doesn't run the macro.
/// So each warning becomes a `#[deprecated]` constant that is used right
/// away, with the use spanned at `span`. rustc then reports the warning as a
/// `deprecated` lint, on every build that compiles the crate.
#[must_use]
pub fn warning_items(warnings: &[String], span: Span) -> TokenStream {
    let items = warnings.iter().map(|warning| {
        let note = format!("openapi-trait: {warning}");
        let def = syn::Ident::new("openapi_trait_warning", Span::call_site());
        let used = syn::Ident::new("openapi_trait_warning", span);
        quote! {
            const _: () = {
                #[deprecated(note = #note)]
                #[allow(non_upper_case_globals)]
                const #def: () = ();
                let _ = #used;
            };
        }
    });
    quote! { #(#items)* }
}

/// Produce the module contents: either the `body` itself, or (when
/// `source_file` is requested and writing succeeds) an `include!` of a file
/// holding the prettyprinted body.
///
/// A write failure is added to `warnings` and falls back to the inline body —
/// like the debug dump, this convenience must never break a build.
#[must_use]
pub fn module_contents(
    opt: Option<&SourceFileOpt>,
    backend: &str,
    mod_ident: &syn::Ident,
    spec_rel: &str,
    body: &TokenStream,
    warnings: &mut Vec<String>,
) -> TokenStream {
    let Some(opt) = opt else {
        return body.clone();
    };
    match write_source_file(opt, backend, mod_ident, spec_rel, body) {
        Ok(path) => {
            let path = path.to_string_lossy().into_owned();
            quote! { ::core::include!(#path); }
        }
        Err(error) => {
            warnings.push(format!(
                "cannot write generated source file, inlining instead: {error}"
            ));
            body.clone()
        }
    }
}

/// Write the prettyprinted `body` to its source file (only if the content
/// changed) and return the file's absolute path.
///
/// # Errors
///
/// Returns a description of the failure if the directory can't be resolved or
/// the file can't be written.
pub fn write_source_file(
    opt: &SourceFileOpt,
    backend: &str,
    mod_ident: &syn::Ident,
    spec_rel: &str,
    body: &TokenStream,
) -> Result<PathBuf, String> {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or("CARGO_MANIFEST_DIR is not set")?;

    // rust-analyzer does not load files under `target/` (except a build
    // script's `OUT_DIR`), so the default lives in the crate and keeps itself
    // out of version control.
    let (dir, self_ignoring) = match opt {
        SourceFileOpt::Dir(lit) => (manifest_dir.join(lit.value()), false),
        SourceFileOpt::Default => (manifest_dir.join(DEFAULT_DIR), true),
    };
    let dir = absolutize(&dir);

    let spec_abs = absolutize(&manifest_dir.join(spec_rel));
    let path = dir.join(source_file_name(mod_ident, backend, &spec_abs, &dir));

    let contents = format!(
        "// @generated by openapi-trait from `{spec_rel}`. Do not edit: changes are overwritten.\n\n{}",
        pretty(body)
    );

    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == contents) {
        return Ok(path);
    }

    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if self_ignoring {
        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            std::fs::write(&gitignore, "# Generated by openapi-trait\n*\n")
                .map_err(|e| format!("{}: {e}", gitignore.display()))?;
        }
    }
    // Write atomically so a concurrent expansion (rustc and rust-analyzer)
    // never observes a half-written file. The temp name is unique per
    // process and per call, because rust-analyzer expands macros in parallel
    // in one process.
    let tmp = dir.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&tmp, &contents).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })?;

    Ok(path)
}

/// The name of the generated file: `<module>.<backend>.<hash>.rs`.
///
/// The hash is taken from the spec's path relative to the output directory
/// `dir` (both absolute), so modules with the same name but different specs
/// get different files, and the name doesn't change when the whole project
/// moves (another checkout, a renamed parent directory).
fn source_file_name(mod_ident: &syn::Ident, backend: &str, spec_abs: &Path, dir: &Path) -> String {
    let spec = relative_path(spec_abs, dir);
    // Keep the low 32 bits: 8 hex digits are plenty to disambiguate.
    let hash = fnv1a(spec.as_bytes()) & 0xffff_ffff;
    format!("{mod_ident}.{backend}.{hash:08x}.rs")
}

/// `path` relative to `base` (both absolute and normalized), with `/` as the
/// separator on every platform, so it hashes the same everywhere.
fn relative_path(path: &Path, base: &Path) -> String {
    let path: Vec<_> = path.components().collect();
    let base: Vec<_> = base.components().collect();
    let common = path.iter().zip(&base).take_while(|(a, b)| a == b).count();
    let ups = std::iter::repeat_n("..".to_owned(), base.len() - common);
    let downs = path[common..]
        .iter()
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    ups.chain(downs).collect::<Vec<_>>().join("/")
}

/// Make `path` absolute relative to the current directory and lexically
/// resolve `.`/`..`, so diagnostics and IDEs show a clean path.
fn absolutize(path: &Path) -> PathBuf {
    use std::path::Component;

    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    };
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// 64-bit FNV-1a: a tiny hash that is stable across Rust versions (unlike
/// `DefaultHasher`), so generated file names don't change on toolchain bumps.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_walks_up_and_down() {
        let rel = |p: &str, b: &str| relative_path(Path::new(p), Path::new(b));
        assert_eq!(rel("/w/c/spec.yaml", "/w/c/.openapi-trait"), "../spec.yaml");
        assert_eq!(rel("/w/c/a/spec.yaml", "/w/c/gen"), "../a/spec.yaml");
        assert_eq!(rel("/w/c/gen/spec.yaml", "/w/c/gen"), "spec.yaml");
        assert_eq!(rel("/w/c/spec.yaml", "/w/shared/gen"), "../../c/spec.yaml");
    }

    #[test]
    fn source_file_name_survives_moving_the_project() {
        let ident = syn::Ident::new("pets", Span::call_site());
        let name = |root: &str| {
            let root = Path::new(root);
            source_file_name(
                &ident,
                "axum",
                &root.join("c/api/pets.yaml"),
                &root.join("c/.openapi-trait"),
            )
        };
        assert_eq!(name("/home/a/work"), name("/tmp/other-checkout"));
        assert!(name("/x").starts_with("pets.axum."), "{}", name("/x"));
    }

    #[test]
    fn source_file_name_tells_crates_sharing_a_dir_apart() {
        let ident = syn::Ident::new("pets", Span::call_site());
        let dir = Path::new("/w/gen");
        let a = source_file_name(&ident, "axum", Path::new("/w/a/pets.yaml"), dir);
        let b = source_file_name(&ident, "axum", Path::new("/w/b/pets.yaml"), dir);
        assert_ne!(a, b);
    }

    #[test]
    fn concurrent_writes_of_one_file_all_succeed() {
        let dir =
            std::env::temp_dir().join(format!("openapi-trait-concurrent-{}", std::process::id()));
        let dir_str = dir.to_string_lossy().into_owned();
        std::thread::scope(|scope| {
            for thread in 0..8_u32 {
                let dir_str = &dir_str;
                scope.spawn(move || {
                    // `syn` values aren't `Send`, so each thread makes its own.
                    let opt = SourceFileOpt::Dir(syn::LitStr::new(dir_str, Span::call_site()));
                    let ident = syn::Ident::new("m", Span::call_site());
                    for round in 0..50_u32 {
                        // A different body each time, so every call writes.
                        let body = quote! { const _: (u32, u32) = (#thread, #round); };
                        write_source_file(&opt, "axum", &ident, "spec.yaml", &body)
                            .unwrap_or_else(|e| panic!("thread {thread}, round {round}: {e}"));
                    }
                });
            }
        });
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n.to_string_lossy().ends_with(".tmp"))
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn warning_items_use_one_deprecated_constant_per_warning() {
        let tokens = warning_items(
            &["first".to_owned(), "second \"quoted\"".to_owned()],
            Span::call_site(),
        )
        .to_string();
        assert_eq!(tokens.matches("deprecated").count(), 2, "{tokens}");
        assert!(
            tokens.contains(r#"note = "openapi-trait: first""#),
            "{tokens}"
        );
        assert!(
            tokens.contains(r#"note = "openapi-trait: second \"quoted\"""#),
            "{tokens}"
        );
        assert!(tokens.contains("let _ = openapi_trait_warning"), "{tokens}");
    }

    #[test]
    fn no_warnings_emit_nothing() {
        assert!(warning_items(&[], Span::call_site()).is_empty());
    }

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
