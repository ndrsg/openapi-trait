# Changelog

All notable changes to this project will be documented in this file.
The format is based on Keep a Changelog,
and this project adheres to Semantic Versioning.

## [1.1.0](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v1.0.0...openapi-trait-v1.1.0) - 2026-11-04

### Added

- use the component type for parameters that reference an enum

## [1.0.0](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.9...openapi-trait-v1.0.0) - 2026-11-04

### Added

- [**breaking**] report undecodable response bodies as a Decode error
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

- use full paths for format!, write! and Send in generated code
- check auth before parsing query parameters
- send a per-request cookie in the scheme's Cookie header
- [**breaking**] report codegen warnings as compiler warnings
- keep string query values exactly as sent
- [**breaking**] keep a schema's own properties next to oneOf / allOf / anyOf
- let the client read required string response headers
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
- allow omitting optional request bodies
- send declared status codes instead of falling back to 500
- bind path parameters in URL order

### Other

- release 1.0.0 through a release PR
- describe regression tests without referring to the issue list
- say that validation needs serde_valid as a direct dependency
- add mdBook guide on GitHub Pages and slim down README

## [0.0.9](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.8...openapi-trait-v0.0.9) - 2026-08-12

### Added

- body streaming for binary types

## [0.0.8](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.7...openapi-trait-v0.0.8) - 2026-07-02

### Added

- support query styles

### Fixed

- dont emit validate for non Validate deriving types

### Other

- formatting

## [0.0.7](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.6...openapi-trait-v0.0.7) - 2026-07-02

### Added

- add optional validation support

## [0.0.6](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.5...openapi-trait-v0.0.6) - 2026-06-26

### Fixed

- allow exclusive per requests credentials

## [0.0.5](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.4...openapi-trait-v0.0.5) - 2026-06-25

### Added

- improved inline string enum handling

## [0.0.4](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.3...openapi-trait-v0.0.4) - 2026-06-25

### Added

- stronger types for string formats
- handle invalid and rust keyword colliding idents
- functionality to dump generated code

## [0.0.3](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.2...openapi-trait-v0.0.3) - 2026-06-24

### Added

- per-request auth overrides scheme-level credentials
- add support for security schemes in OpenAPI code generation
- add per request headers to client
- handling of additional properties and inline ref definitions
- allof, oneof, anyof support

### Other

- add badges

## [0.0.2](https://github.com/ndrsg/openapi-trait/compare/openapi-trait-v0.0.1...openapi-trait-v0.0.2) - 2026-05-08

### Added

- add reqwest client
- add client trait

### Other

- init release-plz
- format code
- update
- update
- update to reqwest 0.13

## [Unreleased]