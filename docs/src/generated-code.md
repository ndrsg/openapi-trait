# Generated code

For a module `mod pets {}`, each macro fills the module with the following
items:

| Item | Generated from | `axum` | `client` |
|---|---|:-:|:-:|
| Structs and enums with `serde` derives | `components/schemas` | ✅ | ✅ |
| `{OperationId}Request` struct | path, query and header parameters, plus `body` | ✅ | ✅ |
| `{OperationId}Response` enum, one `Status{code}` variant per response | `responses` | ✅ | ✅ |
| `impl IntoResponse` for each response enum | | ✅ | |
| Credential structs (`BearerAuth`, `ApiKey`, …) and `{OperationId}Auth` enums | `securitySchemes`, `security` | ✅ | ✅ |
| `PetsApi<S = ()>` trait with a `router()` method | operations | ✅ | |
| `NotImplemented` marker error | | ✅ | |
| `PetsClient` trait | operations | | ✅ |
| `PetsAuthState`, `PetsClientAuth`, `ReqwestPetsClientError`, and the blanket reqwest impl | security schemes, operations | | ✅ (with `reqwest-client`) |

## Naming rules

- **Types** use PascalCase of the schema key (`pet_status` → `PetStatus`).
- **Methods and fields** use snake_case (`getPet` → `get_pet`, `petId` →
  `pet_id`). The original name is kept with `#[serde(rename = "…")]`.
- **Reserved words and invalid identifiers** are escaped, so `type` becomes
  `r#type`.
- **Inline schemas** are lifted to the top level and named after where they
  appear:
  - a nested object or composition in a property becomes
    `{ParentStruct}{Property}`;
  - an inline query enum becomes `{OperationId}{Param}Query`, and an inline
    path enum becomes `{OperationId}{Param}Path`.

The best way to see what a spec produces is to
[dump the generated code](debugging.md), or to add
[`source_file`](go-to-definition.md) and jump into the code from your IDE.
