# Changelog

All notable changes to this project will be documented in this file.
The format is based on Keep a Changelog,
and this project adheres to Semantic Versioning.

## [1.1.1](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v1.1.0...openapi-trait-shared-v1.1.1) - 2026-11-04

### Other

- *(deps)* bump syn to 3 and prettyplease to 0.3 together

## [1.1.0](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v1.0.0...openapi-trait-shared-v1.1.0) - 2026-11-04

### Added

- use the component type for parameters that reference an enum

## [1.0.0](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.9...openapi-trait-shared-v1.0.0) - 2026-11-04

### Added

- [**breaking**] support OpenAPI 3.1 by switching the model to `oas3`
- [**breaking**] type the `default` response and send its status
- [**breaking**] type header parameters by their schema
- re-export `serde` and `serde_json` for generated code
- derive `PartialEq` on generated types
- [**breaking**] pass handlers a single `Context` and drop axum state
- [**breaking**] move operation types into a module per operation
- [**breaking**] generate enums for the inline enum items of array query params
- [**breaking**] generate types for inline request and response body schemas
- support go to definition

### Fixed

- keep source_file names stable and temp files unique
- [**breaking**] report codegen warnings as compiler warnings
- keep string query values exactly as sent
- [**breaking**] keep a schema's own properties next to oneOf / allOf / anyOf
- [**breaking**] fail the build when an operation requires an unsupported security scheme
- give synthesized types a suffix when their name is taken
- box the references that make recursive schemas infinitely sized
- [**breaking**] support optional auth via an empty security requirement
- [**breaking**] require a bearer token for `oauth2` and `openIdConnect` operations
- type `+json` and charset JSON bodies, warn on dropped bodies
- leave out optional fields instead of sending `null`
- map `nullable: true` to `Option<T>`
- sanitize enum values and schema names into valid identifiers
- let operation-level parameters override path-level ones
- generate enum types for string-enum path parameters
- send declared status codes instead of falling back to 500

### Other

- share the macro expansion between backends
- say that validation needs serde_valid as a direct dependency
- don't link to a private item from collect_schemes
- thread a `CodegenCtx` through schema codegen

## [0.0.9](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.8...openapi-trait-shared-v0.0.9) - 2026-08-12

### Added

- body streaming for binary types

## [0.0.8](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.7...openapi-trait-shared-v0.0.8) - 2026-07-02

### Added

- support query styles

### Fixed

- dont emit validate for non Validate deriving types

### Other

- formatting

## [0.0.7](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.6...openapi-trait-shared-v0.0.7) - 2026-07-02

### Added

- add optional validation support

## [0.0.5](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.4...openapi-trait-shared-v0.0.5) - 2026-06-25

### Added

- improved inline string enum handling

## [0.0.4](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.3...openapi-trait-shared-v0.0.4) - 2026-06-25

### Added

- stronger types for string formats
- strong type for required headers
- handle invalid and rust keyword colliding idents
- improved diagnostics for unsupported or invalid openapi definitions
- functionality to dump generated code

## [0.0.3](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.2...openapi-trait-shared-v0.0.3) - 2026-06-24

### Added

- add support for security schemes in OpenAPI code generation
- handling of additional properties and inline ref definitions
- allof, oneof, anyof support

### Fixed

- normalize generated type identifiers to PascalCase

## [0.0.2](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-shared-v0.0.1...openapi-trait-shared-v0.0.2) - 2026-05-08

### Other

- init release-plz
- update

## [Unreleased]