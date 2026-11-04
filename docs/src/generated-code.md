# Generated code

For a module `mod pets {}`, each macro fills the module with the following
items:

| Item | Generated from | `axum` | `client` |
|---|---|:-:|:-:|
| Structs and enums with `serde` derives | `components/schemas` | ✅ | ✅ |
| A module per operation, named like its method (`get_pet`) | operations | ✅ | ✅ |
| `{operation}::Request` struct | path, query and header parameters, plus `body` | ✅ | ✅ |
| `{operation}::Response` enum, one `Status{code}` variant per response, plus `Default { status, … }` for `default` | `responses` | ✅ | ✅ |
| `impl IntoResponse` for each response enum | | ✅ | |
| Credential structs (`BearerAuth`, `ApiKey`, …) | `securitySchemes` | ✅ | ✅ |
| `{operation}::Auth` enum, for operations that accept several schemes | `security` | ✅ | ✅ |
| `PetsApi` trait with a `router()` method | operations | ✅ | |
| `NotImplemented` marker error | | ✅ | |
| `PetsClient` trait | operations | | ✅ |
| `PetsAuthState`, `PetsClientAuth`, `ReqwestPetsClientError`, and the blanket reqwest impl | security schemes, operations | | ✅ (with `reqwest-client`) |

Each operation's types live in its own module, so they can't clash with
schemas or with each other: operation `createPet` gets
`pets::create_pet::Request` even if the spec also has a `CreatePetRequest`
schema. The module holds:

| Item | When |
|---|---|
| `Request`, `Response` | always |
| `Context` (the handler's argument: `req`, `auth`, `headers`) | always, server only |
| `Auth` | the operation accepts more than one security scheme |
| `Status{code}` payload struct (body plus headers) | the response declares headers |
| `Body`, `Status{code}Body`, `DefaultBody` | the request or response body schema is inline |
| `{Param}Path`, `{Param}Query`, `{Param}Header` enums | a path, query or header parameter (or the items of an array query or header parameter) is an inline enum. A parameter that `$ref`s a component enum uses the component's type instead |

Schema types derive `Debug`, `Clone`, `PartialEq`, `Serialize` and
`Deserialize`. String enums (component schemas and parameter enums) also
derive `Copy`, `Eq` and `Hash`, so they can be used as map keys, and implement
`Display`, which writes the wire value (`"rounded-square"`). Structs don't
derive `Eq`, because a field may be a float.

## Naming rules

- **Types** use PascalCase of the schema key (`pet_status` → `PetStatus`).
- **Methods and fields** use snake_case (`getPet` → `get_pet`, `petId` →
  `pet_id`). The original name is kept with `#[serde(rename = "…")]`.
- **Reserved words and invalid identifiers** are escaped, so `type` becomes
  `r#type`. Type names that can't be Rust identifiers get a `_` prefix
  (`2FAConfig` → `_2faConfig`, `Self` → `_Self`).
- **Enum variants** are the PascalCase of the value, with a `V` prefix when the
  value starts with a digit or is a keyword (`"1.0"` → `V1_0`, `"self"` →
  `VSelf`) and `Empty` for `""`. Values that map to the same name get a numeric
  suffix (`created_at`, `createdAt` → `CreatedAt`, `CreatedAt2`). The wire value
  is always kept with `#[serde(rename = "…")]`.
- **Inline schemas** get a type named after where they appear. A nested
  object or composition in a property becomes `{ParentStruct}{Property}`, at
  the top level. Inline bodies and parameter enums go into the operation's
  module (see above), and their nested types are named the same way
  (`create_pet::BodyOwner`).

  If that name is already taken in the same module, a numeric suffix is
  added: with a `PersonAddress` schema, `Person.address` becomes
  `PersonAddress2`.

The best way to see what a spec produces is to
[dump the generated code](debugging.md), or to add
[`source_file`](go-to-definition.md) and jump into the code from your IDE.
