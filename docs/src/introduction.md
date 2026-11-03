# Introduction

**openapi-trait** reads an OpenAPI specification at **compile time** and
generates Rust *traits* from it. You write the business logic by implementing
the trait; the macro handles the types, routing, (de)serialization and
HTTP plumbing.

```rust,ignore
#[openapi_trait::axum("openapi/pets.yaml")]
pub mod pets {}   // ← filled with models, request/response types and `PetsApi`

#[openapi_trait::client("openapi/pets.yaml")]
pub mod pets_client {}   // ← models plus a `PetsClientClient` trait
```

## Why traits?

Most OpenAPI generators emit a pile of source files you check in and
regenerate by hand. openapi-trait takes a different approach:

- **The spec is the source of truth.** Code is generated on every build, so it
  can never drift from the YAML. Edit the spec and the compiler tells you
  exactly which handlers need updating.
- **The compiler checks the contract.** Every operation becomes a trait method
  with typed request and response values. You cannot return a status code or
  body that the spec does not declare.
- **Nothing to check in.** No `build.rs`, no generated directory. The macro is
  a single attribute on an empty `mod`.
- **Clients are swappable.** The client side is a plain trait too, so the same
  calling code works against the generated `reqwest` implementation, a test
  double, or any transport you write yourself.

## Where to go next

- [Getting started](getting-started.md) walks through adding the crate and
  generating your first module.
- [Implementing a server](server.md) and [Writing a client](client.md) cover
  the two sides in detail.
- The [API reference on docs.rs](https://docs.rs/openapi-trait) documents the
  runtime types such as `RequestOptions` and `ByteStream`.

All code in this guide is taken from the
[`openapi-trait/examples`](https://github.com/ndrsg/openapi-trait/tree/main/openapi-trait/examples)
directory, which is compiled in CI, so it matches the current release.
