//! Parsing of the attribute arguments shared by every backend macro.
//!
//! Accepted forms:
//!
//! ```text
//! #[openapi_trait::axum("spec.yaml")]
//! #[openapi_trait::axum("spec.yaml", source_file)]
//! #[openapi_trait::axum("spec.yaml", source_file = "generated")]
//! ```

use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token};

/// Where (and whether) to write the generated code to a real source file.
#[derive(Clone)]
pub enum SourceFileOpt {
    /// `source_file`: write into `CARGO_MANIFEST_DIR/.openapi-trait/`, which
    /// contains a `.gitignore` so it's never committed.
    Default,
    /// `source_file = "dir"`: write into `CARGO_MANIFEST_DIR/dir`.
    Dir(LitStr),
}

/// Parsed attribute arguments of a backend macro.
#[derive(Clone)]
pub struct MacroArgs {
    /// Path to the `OpenAPI` document, relative to `CARGO_MANIFEST_DIR`.
    pub spec: LitStr,
    /// Set when the generated code should be written to disk and `include!`d.
    pub source_file: Option<SourceFileOpt>,
}

impl std::fmt::Debug for SourceFileOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => f.write_str("Default"),
            Self::Dir(dir) => f.debug_tuple("Dir").field(&dir.value()).finish(),
        }
    }
}

impl std::fmt::Debug for MacroArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacroArgs")
            .field("spec", &self.spec.value())
            .field("source_file", &self.source_file)
            .finish()
    }
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let spec: LitStr = input.parse()?;
        let mut source_file = None;

        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            if key != "source_file" {
                return Err(syn::Error::new(
                    key.span(),
                    format!(
                        "unknown argument `{key}`; expected `source_file` or `source_file = \"dir\"`"
                    ),
                ));
            }
            if source_file.is_some() {
                return Err(syn::Error::new(
                    key.span(),
                    "duplicate `source_file` argument",
                ));
            }
            source_file = Some(if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                SourceFileOpt::Dir(input.parse()?)
            } else {
                SourceFileOpt::Default
            });
        }

        Ok(Self { spec, source_file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> syn::Result<MacroArgs> {
        syn::parse_str(src)
    }

    #[test]
    fn spec_only() {
        let args = parse(r#""a.yaml""#).unwrap();
        assert_eq!(args.spec.value(), "a.yaml");
        assert!(args.source_file.is_none());
    }

    #[test]
    fn spec_trailing_comma() {
        assert!(parse(r#""a.yaml","#).unwrap().source_file.is_none());
    }

    #[test]
    fn bare_source_file() {
        let args = parse(r#""a.yaml", source_file"#).unwrap();
        assert!(matches!(args.source_file, Some(SourceFileOpt::Default)));
    }

    #[test]
    fn source_file_dir() {
        let args = parse(r#""a.yaml", source_file = "gen","#).unwrap();
        match args.source_file {
            Some(SourceFileOpt::Dir(dir)) => assert_eq!(dir.value(), "gen"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn unknown_ident_errors() {
        let err = parse(r#""a.yaml", nope"#).unwrap_err();
        assert!(err.to_string().contains("unknown argument `nope`"));
    }

    #[test]
    fn duplicate_errors() {
        assert!(parse(r#""a.yaml", source_file, source_file"#).is_err());
    }
}
