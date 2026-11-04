# Changelog

All notable changes to this project will be documented in this file.
The format is based on Keep a Changelog,
and this project adheres to Semantic Versioning.

## [1.1.1](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v1.1.0...openapi-trait-axum-v1.1.1) - 2026-11-04

### Other

- *(deps)* bump syn to 3 and prettyplease to 0.3 together

## [1.0.0](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.9...openapi-trait-axum-v1.0.0) - 2026-11-04

### Added

- [**breaking**] support OpenAPI 3.1 by switching the model to `oas3`
- [**breaking**] type the `default` response and send its status
- [**breaking**] type header parameters by their schema
- re-export `serde` and `serde_json` for generated code
- [**breaking**] pass handlers a single `Context` and drop axum state
- [**breaking**] move operation types into a module per operation
- [**breaking**] generate types for inline request and response body schemas
- support go to definition

### Fixed

- use full paths for format!, write! and Send in generated code
- check auth before parsing query parameters
- [**breaking**] report codegen warnings as compiler warnings
- keep string query values exactly as sent
- give synthesized types a suffix when their name is taken
- [**breaking**] support optional auth via an empty security requirement
- type `+json` and charset JSON bodies, warn on dropped bodies
- allow omitting optional request bodies
- send declared status codes instead of falling back to 500
- bind path parameters in URL order

### Other

- release 1.0.0 through a release PR
- share the macro expansion between backends
- thread a `CodegenCtx` through schema codegen

## [0.0.9](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.8...openapi-trait-axum-v0.0.9) - 2026-08-12

### Added

- body streaming for binary types

## [0.0.8](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.7...openapi-trait-axum-v0.0.8) - 2026-07-02

### Added

- support query styles

### Other

- formatting

## [0.0.7](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.6...openapi-trait-axum-v0.0.7) - 2026-07-02

### Added

- add optional validation support

## [0.0.4](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.3...openapi-trait-axum-v0.0.4) - 2026-06-25

### Added

- strong type for required headers
- handle invalid and rust keyword colliding idents
- improved diagnostics for unsupported or invalid openapi definitions
- functionality to dump generated code

## [0.0.3](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.2...openapi-trait-axum-v0.0.3) - 2026-06-24

### Added

- add support for security schemes in OpenAPI code generation
- allof, oneof, anyof support

## [0.0.2](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-axum-v0.0.1...openapi-trait-axum-v0.0.2) - 2026-05-08

### Other

- init release-plz
- update
- update

## [Unreleased]