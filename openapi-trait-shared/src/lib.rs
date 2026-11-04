//! Framework-agnostic code-generation helpers for `openapi-trait`.
//!
//! This crate is not intended for direct use. It is an internal implementation
//! detail shared between `openapi-trait-axum`, `openapi-trait-client` and any
//! future backend crates.

#![allow(missing_docs)]

pub mod args;
pub mod codegen;
pub mod debug;
pub mod expand;
pub mod source_file;
pub mod spec;
