//! `OpenAPI` security-scheme support.
//!
//! Models the subset of `OpenAPI` 3 security used by v0.1 codegen:
//!
//! - `apiKey` schemes (`in: header | query | cookie`)
//! - `http` schemes with `scheme: bearer` or `scheme: basic`
//!
//! `oauth2` and `openIdConnect` are recognised at parse time and silently
//! skipped — operations that reference them behave as if they had no security
//! requirement so the rest of the spec still compiles. Multi-scheme `AND`
//! requirements (a single requirement object naming more than one scheme) emit
//! a `compile_error!` because v0.1 only supports `OR` of single-scheme
//! alternatives.

use heck::{ToPascalCase, ToSnakeCase};
use openapiv3::{APIKeyLocation, OpenAPI, Operation, ReferenceOr, SecurityScheme};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Where an `apiKey` credential is carried.
#[derive(Debug, Clone, Copy)]
pub enum ApiKeyIn {
    Header,
    Query,
    Cookie,
}

/// Concrete v0.1-supported scheme variants.
#[derive(Debug, Clone)]
pub enum SchemeKind {
    ApiKey { key: String, location: ApiKeyIn },
    HttpBearer,
    HttpBasic,
}

/// One declared scheme from `components.securitySchemes`.
#[derive(Debug, Clone)]
pub struct SchemeInfo {
    /// Raw key from `components.securitySchemes`.
    pub name: String,
    /// `PascalCase` Rust ident used for the generated type.
    pub ident: syn::Ident,
    /// Snake-cased fragment used to derive builder method / state-field names.
    pub snake: String,
    pub kind: SchemeKind,
}

/// Resolved security requirements for one operation.
#[derive(Debug, Clone, Default)]
pub struct OpSecurity {
    /// One scheme name per alternative (OR). Empty = no auth required.
    pub alternatives: Vec<String>,
    /// True if a multi-scheme AND requirement was rejected for this op.
    pub had_unsupported_and: bool,
}

/// Read `components.securitySchemes` and keep only schemes supported in v0.1.
#[must_use]
pub fn collect_schemes(openapi: &OpenAPI) -> Vec<SchemeInfo> {
    let Some(components) = openapi.components.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, ref_or) in &components.security_schemes {
        let scheme = match ref_or {
            ReferenceOr::Item(s) => s,
            ReferenceOr::Reference { .. } => continue,
        };
        let kind = match scheme {
            SecurityScheme::APIKey { location, name, .. } => {
                let loc = match location {
                    APIKeyLocation::Header => ApiKeyIn::Header,
                    APIKeyLocation::Query => ApiKeyIn::Query,
                    APIKeyLocation::Cookie => ApiKeyIn::Cookie,
                };
                SchemeKind::ApiKey {
                    key: name.clone(),
                    location: loc,
                }
            }
            SecurityScheme::HTTP { scheme, .. } => match scheme.to_ascii_lowercase().as_str() {
                "bearer" => SchemeKind::HttpBearer,
                "basic" => SchemeKind::HttpBasic,
                _ => continue,
            },
            SecurityScheme::OAuth2 { .. } | SecurityScheme::OpenIDConnect { .. } => continue,
        };
        let ident = format_ident!("{}", name.to_pascal_case());
        let snake = name.to_snake_case();
        out.push(SchemeInfo {
            name: name.clone(),
            ident,
            snake,
            kind,
        });
    }
    out
}

/// Resolve the effective security requirements for an operation.
///
/// Semantics:
/// - `operation.security == Some(vec![])` → explicit disable (no auth).
/// - `operation.security == Some(non-empty)` → use op-level requirements.
/// - `operation.security == None` → inherit `openapi.security` (doc-level default).
///
/// Each requirement object must reference exactly one supported scheme name to
/// be accepted; requirements naming unknown schemes are dropped, and multi-key
/// requirements (AND) are flagged via `had_unsupported_and` and otherwise dropped.
#[must_use]
pub fn resolve_op_security(
    op: &Operation,
    openapi: &OpenAPI,
    schemes: &[SchemeInfo],
) -> OpSecurity {
    let requirements = match op.security.as_ref() {
        Some(v) => v,
        None => match openapi.security.as_ref() {
            Some(v) => v,
            None => return OpSecurity::default(),
        },
    };

    let mut out = OpSecurity::default();
    for req in requirements {
        if req.len() > 1 {
            out.had_unsupported_and = true;
            continue;
        }
        let Some((name, _scopes)) = req.iter().next() else {
            continue;
        };
        if scheme_by_name(schemes, name).is_some() {
            out.alternatives.push(name.clone());
        }
    }
    out
}

/// Look up a scheme by its raw name.
#[must_use]
pub fn scheme_by_name<'a>(schemes: &'a [SchemeInfo], name: &str) -> Option<&'a SchemeInfo> {
    schemes.iter().find(|s| s.name == name)
}

/// Generate the top-level type definition for every declared scheme.
#[must_use]
pub fn generate_scheme_types(schemes: &[SchemeInfo]) -> TokenStream {
    let items: Vec<TokenStream> = schemes
        .iter()
        .map(|s| {
            let ident = &s.ident;
            match &s.kind {
                SchemeKind::ApiKey { .. } | SchemeKind::HttpBearer => quote! {
                    #[derive(::core::fmt::Debug, ::core::clone::Clone)]
                    pub struct #ident(pub ::std::string::String);
                },
                SchemeKind::HttpBasic => quote! {
                    #[derive(::core::fmt::Debug, ::core::clone::Clone)]
                    pub struct #ident {
                        pub username: ::std::string::String,
                        pub password: ::std::string::String,
                    }
                },
            }
        })
        .collect();
    quote! { #(#items)* }
}

/// Generate one `{Op}Auth` enum per operation that has more than one alternative.
#[must_use]
pub fn generate_op_auth_enum(op_id: &str, alternatives: &[&SchemeInfo]) -> Option<TokenStream> {
    if alternatives.len() < 2 {
        return None;
    }
    let ident = auth_enum_ident(op_id);
    let variants: Vec<TokenStream> = alternatives
        .iter()
        .map(|s| {
            let variant = &s.ident;
            let ty = &s.ident;
            quote! { #variant(#ty), }
        })
        .collect();
    Some(quote! {
        #[derive(::core::fmt::Debug, ::core::clone::Clone)]
        pub enum #ident {
            #(#variants)*
        }
    })
}

/// Pascal-cased ident for an operation's auth enum.
#[must_use]
pub fn auth_enum_ident(op_id: &str) -> syn::Ident {
    format_ident!("{}Auth", op_id.to_pascal_case())
}

/// Compute the Rust type used as the trait method's `auth` parameter.
///
/// Returns `None` when the operation has no security requirement.
#[must_use]
pub fn auth_param_type(op_id: &str, op_security: &OpSecurity) -> Option<TokenStream> {
    if op_security.alternatives.is_empty() {
        return None;
    }
    if op_security.alternatives.len() == 1 {
        return Some(TokenStream::new()); // sentinel: caller looks up the scheme ident
    }
    let ident = auth_enum_ident(op_id);
    Some(quote! { #ident })
}

/// Returns the scheme infos referenced by an op's alternatives, in declaration order.
#[must_use]
pub fn resolve_alternatives<'a>(
    op_security: &'a OpSecurity,
    schemes: &'a [SchemeInfo],
) -> Vec<&'a SchemeInfo> {
    op_security
        .alternatives
        .iter()
        .filter_map(|name| scheme_by_name(schemes, name))
        .collect()
}

/// Module-level identifier for the generated client auth-state struct.
#[must_use]
pub fn auth_state_ident(mod_ident: &syn::Ident) -> syn::Ident {
    format_ident!("{}AuthState", mod_ident.to_string().to_pascal_case())
}

/// Module-level identifier for the generated client auth extension trait.
#[must_use]
pub fn client_auth_trait_ident(mod_ident: &syn::Ident) -> syn::Ident {
    format_ident!("{}ClientAuth", mod_ident.to_string().to_pascal_case())
}

/// Per-scheme snake-case suffix for builder methods / state fields.
#[must_use]
pub fn scheme_field_ident(scheme: &SchemeInfo) -> syn::Ident {
    format_ident!("{}", scheme.snake)
}

/// Emit a `compile_error!` token if any operation referenced a multi-scheme AND
/// requirement that v0.1 cannot represent.
#[must_use]
pub fn generate_unsupported_and_errors(ops_with_and: &[String]) -> TokenStream {
    if ops_with_and.is_empty() {
        return TokenStream::new();
    }
    let msgs: Vec<TokenStream> = ops_with_and
        .iter()
        .map(|op_id| {
            let msg = format!(
                "openapi-trait: operation `{op_id}` requires multiple security schemes simultaneously (AND); v0.1 only supports OR of single-scheme alternatives"
            );
            quote! { ::core::compile_error!(#msg); }
        })
        .collect();
    quote! { #(#msgs)* }
}
