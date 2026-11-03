# Getting started

## Install

```toml
[dependencies]
openapi-trait = "0.0.9"

# For servers:
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

The `reqwest-client` feature is on by default. See
[Crates and features](crates.md) for the full list.

## The example spec

The chapters in this guide use a small spec with three operations:

| Operation | Method and path | Auth |
|---|---|---|
| `listPets` | `GET /pets?limit=` | none |
| `createPet` | `POST /pets` | bearer |
| `getPet` | `GET /pets/{petId}` | bearer |

<details>
<summary>Show <code>pets.openapi.yaml</code></summary>

```yaml
{{#include ../../openapi-trait/examples/pets.openapi.yaml}}
```

</details>

## Apply the macro

Put the attribute on an empty module:

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:macro}}
```

- **The path is relative to your crate root** (`CARGO_MANIFEST_DIR`), not to
  the source file.
- **The spec is tracked by Cargo.** Editing the YAML recompiles the crate.
- **YAML and JSON** specs are both accepted.
- **Every operation needs an `operationId`.** It becomes the method name
  (`getPet` → `get_pet`).
- **Want F12 to open the generated code?** Add `source_file`:
  `#[openapi_trait::axum("pets.openapi.yaml", source_file)]`. See
  [Go-to-definition in your IDE](go-to-definition.md).

## Names come from the module

Trait names come from the module name in PascalCase:

| Module | Server trait | Client trait | Client auth state |
|---|---|---|---|
| `mod pets {}` | `pets::PetsApi` | `pets::PetsClient` | `pets::PetsAuthState` |
| `mod petstore {}` | `petstore::PetstoreApi` | `petstore::PetstoreClient` | `petstore::PetstoreAuthState` |

Types inside the module are named after the spec: `components/schemas/Pet`
becomes `pets::Pet`, and operation `getPet` becomes `pets::GetPetRequest` and
`pets::GetPetResponse`.

## Next steps

- [Implement a server](server.md) with axum.
- [Write a client](client.md), either by hand or with the generated reqwest
  implementation.
