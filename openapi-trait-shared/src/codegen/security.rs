//! `OpenAPI` security-scheme support.
//!
//! Models the subset of `OpenAPI` 3 security used by v0.1 codegen:
//!
//! - `apiKey` schemes (`in: header | query | cookie`)
//! - `http` schemes with `scheme: bearer` or `scheme: basic`
//! - `oauth2` and `openIdConnect`, which carry an `Authorization: Bearer`
//!   token and are handled like `http` bearer. Scopes are not checked.
//!
//! An operation that requires any other scheme (`http` digest, `mutualTLS`,
//! an `apiKey` with an invalid `in`, a `$ref`) or an undeclared scheme is a
//! `compile_error!`, so it is never generated without auth.
//!
//! Multi-scheme `AND`
//! requirements (a single requirement object naming more than one scheme) emit
//! a `compile_error!` because v0.1 only supports `OR` of single-scheme
//! alternatives.

use heck::{ToPascalCase, ToSnakeCase};
use oas3::spec::{ObjectOrReference, Operation, SecurityScheme, Spec};
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
    /// True if one of the requirements is empty (`{}`), which makes auth
    /// optional: the request may also be anonymous.
    pub optional: bool,
}

/// Read `components.securitySchemes` and keep only schemes supported in v0.1.
///
/// Unsupported schemes are left out here. An operation that requires one is
/// reported by [`resolve_op_security`].
#[must_use]
pub fn collect_schemes(openapi: &Spec) -> Vec<SchemeInfo> {
    let Some(components) = openapi.components.as_ref() else {
        return Vec::new();
    };
    components
        .security_schemes
        .iter()
        .filter_map(|(name, scheme)| {
            let kind = classify_scheme(scheme).ok()?;
            Some(SchemeInfo {
                name: name.clone(),
                ident: super::idents::schema_type_ident(name),
                snake: name.to_snake_case(),
                kind,
            })
        })
        .collect()
}

/// Map a declared scheme to a supported [`SchemeKind`], or explain why it is
/// not supported.
fn classify_scheme(scheme: &ObjectOrReference<SecurityScheme>) -> Result<SchemeKind, String> {
    let scheme = match scheme {
        ObjectOrReference::Object(s) => s,
        ObjectOrReference::Ref { ref_path, .. } => {
            return Err(format!(
                "is a `$ref` (`{ref_path}`), which is not supported"
            ));
        }
    };
    match scheme {
        SecurityScheme::ApiKey { location, name, .. } => {
            let location = match location.as_str() {
                "header" => ApiKeyIn::Header,
                "query" => ApiKeyIn::Query,
                "cookie" => ApiKeyIn::Cookie,
                other => {
                    return Err(format!(
                        "is an `apiKey` with `in: {other}`; only `header`, `query` and `cookie` are valid"
                    ));
                }
            };
            Ok(SchemeKind::ApiKey {
                key: name.clone(),
                location,
            })
        }
        SecurityScheme::Http { scheme, .. } => match scheme.to_ascii_lowercase().as_str() {
            "bearer" => Ok(SchemeKind::HttpBearer),
            "basic" => Ok(SchemeKind::HttpBasic),
            _ => Err(format!(
                "is an `http` scheme with `scheme: {scheme}`; only `bearer` and `basic` are supported"
            )),
        },
        // Both send the access token as `Authorization: Bearer <token>`.
        // Validating the token, including its scopes, is the handler's job.
        SecurityScheme::OAuth2 { .. } | SecurityScheme::OpenIdConnect { .. } => {
            Ok(SchemeKind::HttpBearer)
        }
        SecurityScheme::MutualTls { .. } => {
            Err("is a `mutualTLS` scheme, which is not supported".to_owned())
        }
    }
}

/// Explain why a requirement names a scheme missing from `schemes`.
fn unusable_scheme_reason(openapi: &Spec, name: &str) -> String {
    let declared = openapi
        .components
        .as_ref()
        .and_then(|c| c.security_schemes.get(name));
    declared.map_or_else(
        || "is not declared in `components.securitySchemes`".to_owned(),
        |scheme| {
            classify_scheme(scheme)
                .err()
                .unwrap_or_else(|| "was not collected".to_owned())
        },
    )
}

/// Resolve the effective security requirements for an operation.
///
/// Semantics:
/// - `security: []` on the operation → explicit disable (no auth).
/// - non-empty `security` on the operation → use op-level requirements.
/// - no `security` on the operation → inherit `openapi.security` (doc-level
///   default). [`oas3`] can't tell this case from `security: []`, so the
///   loader marks operations that declare it (see
///   [`crate::spec::OPERATION_HAS_SECURITY`]).
///
/// Each requirement object must reference exactly one supported scheme name to
/// be accepted. A requirement naming a scheme that is unsupported or not
/// declared is a fatal error in `diag`: dropping it could leave the operation
/// without auth. Multi-key requirements (AND) are flagged via
/// `had_unsupported_and` and otherwise dropped. An empty requirement (`{}`)
/// allows anonymous requests and sets `optional`.
pub(crate) fn resolve_op_security(
    op: &Operation,
    operation_id: &str,
    openapi: &Spec,
    schemes: &[SchemeInfo],
    diag: &mut super::operations::Diagnostics,
) -> OpSecurity {
    let requirements = if op
        .extensions
        .contains_key(crate::spec::OPERATION_HAS_SECURITY)
    {
        &op.security
    } else {
        &openapi.security
    };

    let mut out = OpSecurity::default();
    for oas3::spec::SecurityRequirement(req) in requirements {
        if req.len() > 1 {
            out.had_unsupported_and = true;
            continue;
        }
        let Some((name, _scopes)) = req.iter().next() else {
            out.optional = true;
            continue;
        };
        if scheme_by_name(schemes, name).is_some() {
            out.alternatives.push(name.clone());
        } else {
            diag.error(format!(
                "operation `{operation_id}` requires security scheme `{name}`, which {}",
                unusable_scheme_reason(openapi, name)
            ));
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

/// Generate the `Auth` enum of an operation that accepts more than one
/// scheme. It is emitted into the operation's module, so the scheme types are
/// referenced through [`super::ctx::ROOT_MODULE`].
#[must_use]
pub fn generate_op_auth_enum(alternatives: &[&SchemeInfo]) -> Option<TokenStream> {
    if alternatives.len() < 2 {
        return None;
    }
    let ident = format_ident!("{}", super::operations::AUTH_TYPE);
    let root = format_ident!("{}", super::ctx::ROOT_MODULE);
    let variants: Vec<TokenStream> = alternatives
        .iter()
        .map(|s| {
            let variant = &s.ident;
            quote! { #variant(#root::#variant), }
        })
        .collect();
    Some(quote! {
        #[derive(::core::fmt::Debug, ::core::clone::Clone)]
        pub enum #ident {
            #(#variants)*
        }
    })
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
