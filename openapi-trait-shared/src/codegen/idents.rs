//! Keyword-safe, validated construction of Rust identifiers from `OpenAPI` names.
//!
//! `OpenAPI` `operationId`s and parameter names are free-form strings, but they
//! are used to name generated Rust methods, struct fields, and types. Feeding an
//! arbitrary string into [`format_ident!`](quote::format_ident) (i.e.
//! [`proc_macro2::Ident::new`]) *panics* when the string is not a legal
//! identifier — for example an op named `type`/`async` (a keyword) or one
//! starting with a digit. These helpers turn that panic into either a raw
//! identifier (`type` → `r#type`) or a clear diagnostic the caller can route
//! through [`super::operations::Diagnostics`].

use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::{Ident, Span};

/// Rust keywords that are legal as *raw* identifiers (`r#keyword`).
///
/// Covers strict and reserved keywords. Deliberately excludes the four in
/// [`NON_RAW_KEYWORDS`] (which cannot be raw) and the contextual keyword
/// `union` (already a valid plain identifier).
const RAW_SAFE_KEYWORDS: &[&str] = &[
    // strict keywords
    "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "static", "struct", "trait", "true", "type", "unsafe", "use", "where", "while",
    // reserved for future use
    "abstract", "become", "box", "do", "final", "macro", "override", "priv", "try", "typeof",
    "unsized", "virtual", "yield",
];

/// Keywords that cannot be expressed as raw identifiers; using one requires
/// renaming the source `OpenAPI` construct.
const NON_RAW_KEYWORDS: &[&str] = &["crate", "self", "super", "Self"];

/// True when `s` is a legal *plain* Rust identifier (ASCII rule).
///
/// `OpenAPI` names are realistically ASCII once `heck` has normalised them, so a
/// simple ASCII check keeps us dependency-free. Keywords are intentionally *not*
/// rejected here — keyword handling is the caller's job via [`safe_ident`].
fn is_plain_ident(s: &str) -> bool {
    if s.is_empty() || s == "_" {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().expect("non-empty checked above");
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Turn an already case-converted name into a keyword-safe [`Ident`].
///
/// Keywords become raw identifiers; names that cannot be a Rust identifier at
/// all (empty, leading digit, illegal characters, or a non-raw-able keyword)
/// return `Err` with a human-readable reason. `original` and `role` are used
/// only to build that message (e.g. `role = "operationId"`).
///
/// # Errors
///
/// Returns `Err(reason)` when `converted` cannot name a Rust item.
pub fn safe_ident(converted: &str, original: &str, role: &str) -> Result<Ident, String> {
    if converted.is_empty() {
        return Err(format!(
            "{role} `{original}` produces an empty Rust identifier; rename it"
        ));
    }
    if NON_RAW_KEYWORDS.contains(&converted) {
        return Err(format!(
            "{role} `{original}` maps to the reserved Rust keyword `{converted}`, which cannot be used as an identifier (not even as a raw identifier); rename it"
        ));
    }
    if RAW_SAFE_KEYWORDS.contains(&converted) {
        return Ok(Ident::new_raw(converted, Span::call_site()));
    }
    if !is_plain_ident(converted) {
        return Err(format!(
            "{role} `{original}` produces the invalid Rust identifier `{converted}`; identifiers must start with a letter or underscore and contain only letters, digits, and underscores"
        ));
    }
    Ok(Ident::new(converted, Span::call_site()))
}

/// Keyword-safe method identifier for an `operationId` (snake-cased).
///
/// # Errors
///
/// See [`safe_ident`].
pub fn method_ident(operation_id: &str) -> Result<Ident, String> {
    safe_ident(&operation_id.to_snake_case(), operation_id, "operationId")
}

/// Keyword-safe struct-field identifier for a parameter name (snake-cased).
///
/// # Errors
///
/// See [`safe_ident`].
pub fn field_ident(param_name: &str) -> Result<Ident, String> {
    safe_ident(&param_name.to_snake_case(), param_name, "parameter")
}

/// Validate that an `operationId` yields a usable `PascalCase` base for generated
/// type names (`{Base}Request`, `{Base}Response`, `{Base}Auth`, …).
///
/// A non-empty suffix is always appended by callers, so a keyword base is fine
/// (`type` → `TypeRequest`); only genuinely invalid characters or an empty base
/// are rejected.
///
/// # Errors
///
/// Returns `Err(reason)` when the `PascalCase` base is not a legal identifier.
pub fn validate_type_base(operation_id: &str) -> Result<(), String> {
    let pascal = operation_id.to_pascal_case();
    if !is_plain_ident(&pascal) {
        return Err(format!(
            "operationId `{operation_id}` produces the invalid Rust type-name base `{pascal}`; it must start with a letter or underscore and contain only letters, digits, and underscores"
        ));
    }
    Ok(())
}

/// Validate an already-assembled type name (e.g. a synthesized query-param enum
/// like `ListPetsStatusQuery`) and turn it into an [`Ident`].
///
/// # Errors
///
/// Returns `Err(reason)` when `name` is not a legal identifier.
pub fn type_ident(name: &str, source: &str) -> Result<Ident, String> {
    if !is_plain_ident(name) {
        return Err(format!(
            "synthesized type name `{name}` (from `{source}`) is not a valid Rust identifier"
        ));
    }
    Ok(Ident::new(name, Span::call_site()))
}

/// Infallible keyword-safe identifier for contexts without a diagnostics
/// channel (schema property field names).
///
/// Keywords become raw identifiers; the non-raw-able keywords get a trailing
/// underscore (`self` → `self_`). Any other string is passed through unchanged,
/// matching the prior behaviour of the per-module helper this replaces.
#[must_use]
pub fn keyword_safe_ident(name: &str) -> Ident {
    if NON_RAW_KEYWORDS.contains(&name) {
        return Ident::new(&format!("{name}_"), Span::call_site());
    }
    if RAW_SAFE_KEYWORDS.contains(&name) {
        return Ident::new_raw(name, Span::call_site());
    }
    Ident::new(name, Span::call_site())
}

/// Make `candidate` (already case-converted) a legal non-keyword identifier.
///
/// - an empty candidate becomes `empty`;
/// - a leading digit or a keyword (in practice `Self`, the only one reachable
///   through `PascalCase`, and one that can't be raw) gets `prefix` prepended;
/// - anything `syn` still rejects (exotic Unicode) is reduced to its ASCII
///   alphanumerics, falling back to `empty`.
///
/// The results never put `_` next to a letter, so they don't trip the
/// `non_camel_case_types` lint in user crates.
fn sanitize_pascal(candidate: String, prefix: &str, empty: &str) -> Ident {
    let fixup = |mut s: String| -> String {
        if s.is_empty() {
            return empty.to_owned();
        }
        if s.starts_with(|c: char| c.is_ascii_digit())
            || NON_RAW_KEYWORDS.contains(&s.as_str())
            || RAW_SAFE_KEYWORDS.contains(&s.as_str())
        {
            s.insert_str(0, prefix);
        }
        s
    };
    let s = fixup(candidate);
    if let Ok(id) = syn::parse_str::<Ident>(&s) {
        return id;
    }
    let ascii: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    syn::parse_str::<Ident>(&fixup(ascii)).unwrap_or_else(|_| Ident::new(empty, Span::call_site()))
}

/// Infallible `PascalCase` type identifier for a schema / security-scheme name
/// or a type name synthesized from one (`{Parent}{Prop}`, …).
///
/// Ordinary names map exactly as `heck`'s `to_pascal_case` does; names that
/// would not be legal identifiers are repaired instead of panicking
/// (`2FAConfig` → `_2faConfig`, `Self` → `_Self`). Every definition *and*
/// reference to a schema type must go through this so they agree.
#[must_use]
pub fn schema_type_ident(name: &str) -> Ident {
    sanitize_pascal(name.to_pascal_case(), "_", "Unnamed")
}

/// `PascalCase` an enum wire value, keeping digit runs on either side of a
/// separator apart (`1.0` → `1_0`, not `10`) so distinct versions stay
/// distinct.
fn pascal_enum_value(value: &str) -> String {
    let mut out = String::new();
    for segment in value
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        let pascal = segment.to_pascal_case();
        if out.ends_with(|c: char| c.is_ascii_digit())
            && pascal.starts_with(|c: char| c.is_ascii_digit())
        {
            out.push('_');
        }
        out.push_str(&pascal);
    }
    out
}

/// Infallible, collision-free variant identifiers for a string enum's values,
/// in the same order as `values`.
///
/// Values that don't make a legal identifier are repaired (`"1.0"` → `V1_0`,
/// `""` → `Empty`, `"self"` → `VSelf`), and values that map to the same
/// identifier get a numeric suffix (`created_at`, `createdAt` → `CreatedAt`,
/// `CreatedAt2`). Callers keep a `#[serde(rename = "...")]` with the original
/// value whenever the identifier differs from it.
#[must_use]
pub fn enum_variant_idents(values: &[String]) -> Vec<Ident> {
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|v| {
            let base = sanitize_pascal(pascal_enum_value(v), "V", "Empty").to_string();
            let mut candidate = base.clone();
            let mut n = 2usize;
            while !used.insert(candidate.clone()) {
                candidate = format!("{base}{n}");
                n += 1;
            }
            Ident::new(&candidate, Span::call_site())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variants(values: &[&str]) -> Vec<String> {
        let owned: Vec<String> = values.iter().map(|s| (*s).to_owned()).collect();
        enum_variant_idents(&owned)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn enum_variants_are_sanitized() {
        assert_eq!(
            variants(&["1.0", "2.0", "+1", "", "Self", "self", "404", "super", "crate"]),
            ["V1_0", "V2_0", "V1", "Empty", "VSelf", "VSelf2", "V404", "Super", "Crate"]
        );
    }

    #[test]
    fn enum_variants_keep_existing_names() {
        assert_eq!(
            variants(&["available", "in-stock", "snake_case_value", "SHOUTY"]),
            ["Available", "InStock", "SnakeCaseValue", "Shouty"]
        );
    }

    #[test]
    fn colliding_enum_variants_get_a_suffix() {
        assert_eq!(
            variants(&["created_at", "createdAt", "-created_at"]),
            ["CreatedAt", "CreatedAt2", "CreatedAt3"]
        );
    }

    #[test]
    fn schema_type_idents_are_sanitized() {
        assert_eq!(schema_type_ident("Pet").to_string(), "Pet");
        assert_eq!(schema_type_ident("pet_store").to_string(), "PetStore");
        assert_eq!(schema_type_ident("2FAConfig").to_string(), "_2faConfig");
        assert_eq!(schema_type_ident("Self").to_string(), "_Self");
        assert_eq!(schema_type_ident("self").to_string(), "_Self");
        assert_eq!(schema_type_ident("type").to_string(), "Type");
        assert_eq!(schema_type_ident("").to_string(), "Unnamed");
    }

    #[test]
    fn keyword_method_becomes_raw_ident() {
        let id = method_ident("type").expect("keyword is raw-safe");
        assert_eq!(id.to_string(), "r#type");
        assert_eq!(method_ident("async").unwrap().to_string(), "r#async");
    }

    #[test]
    fn hyphenated_and_clean_names_pass_through() {
        assert_eq!(method_ident("list-pets").unwrap().to_string(), "list_pets");
        assert_eq!(
            method_ident("getPetById").unwrap().to_string(),
            "get_pet_by_id"
        );
    }

    #[test]
    fn leading_digit_is_an_error() {
        let err = method_ident("1pet").unwrap_err();
        assert!(err.contains("operationId `1pet`"), "{err}");
    }

    #[test]
    fn empty_after_conversion_is_an_error() {
        assert!(method_ident("___").is_err());
    }

    #[test]
    fn non_raw_keyword_is_an_error() {
        let err = method_ident("self").unwrap_err();
        assert!(err.contains("reserved Rust keyword"), "{err}");
    }

    #[test]
    fn keyword_param_field_becomes_raw_ident() {
        assert_eq!(field_ident("type").unwrap().to_string(), "r#type");
    }

    #[test]
    fn type_base_keyword_is_fine_but_digit_is_not() {
        // `type` -> `Type`, a valid base (a suffix is appended by callers).
        assert!(validate_type_base("type").is_ok());
        assert!(validate_type_base("1pet").is_err());
    }

    #[test]
    fn infallible_helper_handles_non_raw_keyword() {
        assert_eq!(keyword_safe_ident("self").to_string(), "self_");
        assert_eq!(keyword_safe_ident("type").to_string(), "r#type");
        assert_eq!(keyword_safe_ident("name").to_string(), "name");
    }
}
