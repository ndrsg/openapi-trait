//! Generated code names everything by its full path, so it compiles even when
//! the surrounding module shadows prelude names. The generated module starts
//! with `use super::*;`, which brings these into scope ahead of the prelude.
#![cfg(feature = "reqwest-client")]
#![allow(dead_code, unused_imports, unused_macros, non_camel_case_types)]

mod shadow {
    macro_rules! format {
        ($($t:tt)*) => {
            compile_error!("generated code used a bare `format!`")
        };
    }
    macro_rules! write {
        ($($t:tt)*) => {
            compile_error!("generated code used a bare `write!`")
        };
    }
    macro_rules! vec {
        ($($t:tt)*) => {
            compile_error!("generated code used a bare `vec!`")
        };
    }
    pub(crate) use {format, write};
    // `serde_valid`'s `Validate` derive expands to a bare `vec!`, which this
    // crate can't change.
    #[cfg(not(feature = "validation"))]
    pub(crate) use vec;

    pub trait Send {}
    pub trait Sync {}
    pub trait Clone {}
    pub trait Default {}
    pub trait Into<T> {}
    pub trait From<T> {}
    pub trait ToString {}
    pub trait Iterator {}
    pub trait IntoIterator {}
    pub struct String;
    pub struct Vec;
    pub struct Box;
    pub struct Option;
    pub struct Result;
}
use shadow::*;

#[openapi_trait::axum("assets/testdata/petstore.openapi.yaml")]
pub mod server {}

#[openapi_trait::client("assets/testdata/petstore.openapi.yaml")]
pub mod client {}

#[openapi_trait::axum("assets/testdata/security.openapi.yaml")]
pub mod sec_server {}

#[openapi_trait::client("assets/testdata/security.openapi.yaml")]
pub mod sec_client {}

#[openapi_trait::axum("assets/testdata/downloads.openapi.yaml")]
pub mod downloads_server {}

#[openapi_trait::client("assets/testdata/downloads.openapi.yaml")]
pub mod downloads_client {}

#[openapi_trait::axum("assets/testdata/header_params.openapi.yaml")]
pub mod header_params_server {}

#[openapi_trait::client("assets/testdata/header_params.openapi.yaml")]
pub mod header_params_client {}

#[openapi_trait::axum("assets/testdata/default_response.openapi.yaml")]
pub mod default_response_server {}

#[openapi_trait::client("assets/testdata/default_response.openapi.yaml")]
pub mod default_response_client {}

#[openapi_trait::axum("assets/testdata/query_styles.openapi.yaml")]
pub mod query_styles_server {}

#[openapi_trait::client("assets/testdata/query_styles.openapi.yaml")]
pub mod query_styles_client {}

#[openapi_trait::axum("assets/testdata/composition_properties.openapi.yaml")]
pub mod composition_properties_server {}

#[openapi_trait::client("assets/testdata/composition_properties.openapi.yaml")]
pub mod composition_properties_client {}

#[openapi_trait::axum("assets/testdata/status_and_body.openapi.yaml")]
pub mod status_and_body_server {}

#[openapi_trait::client("assets/testdata/status_and_body.openapi.yaml")]
pub mod status_and_body_client {}

#[openapi_trait::axum("assets/testdata/recursive.openapi.yaml")]
pub mod recursive_server {}

#[openapi_trait::client("assets/testdata/recursive.openapi.yaml")]
pub mod recursive_client {}

#[test]
fn compiles() {}
