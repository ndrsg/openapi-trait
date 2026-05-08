# openapi-trait-shared

Framework-agnostic code generation helpers for `openapi-trait`.

This crate is an internal implementation detail shared by the proc-macro
backends and is not intended for direct use.

## What it contains

The shared crate holds logic used by multiple backends, including:

- OpenAPI operation collection and normalization
- schema-to-Rust type generation helpers
- reusable code generation utilities built on `syn`, `quote`, and `proc-macro2`

## Crate role

`openapi-trait-axum` and `openapi-trait-client` both depend on this crate so
they can share parsing and code generation behavior without duplicating logic.

It should be treated as an internal crate with no stability guarantees outside
the workspace.
