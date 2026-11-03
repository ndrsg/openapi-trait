# Validation

The optional `validation` feature makes generated models derive
[`serde_valid::Validate`](https://docs.rs/serde_valid), with field
attributes taken from the schema's constraints.

```toml
[dependencies]
openapi-trait = { version = "0.0.9", features = ["validation"] }
```

```rust,ignore
{{#include ../../openapi-trait/examples/validation.rs:validate}}
```

`serde_valid` is re-exported as `openapi_trait::serde_valid`, so you don't need
to depend on it yourself.

## Constraint mapping

| OpenAPI keyword | `serde_valid` attribute |
|---|---|
| `minLength` / `maxLength` / `pattern` | `min_length` / `max_length` / `pattern` |
| `minimum` / `maximum` (+ `exclusiveMinimum` / `exclusiveMaximum`) | `minimum` / `maximum` (or `exclusive_minimum` / `exclusive_maximum`) |
| `multipleOf` | `multiple_of` |
| `minItems` / `maxItems` / `uniqueItems` | `min_items` / `max_items` / `unique_items` |
| `$ref` / inline object / `oneOf`·`anyOf`·`allOf` field | bare `#[validate]` (recurses into the nested model) |

## Opt-in and non-enforcing

- **Nothing calls `.validate()` for you.** Neither the server nor the client
  rejects invalid values automatically. Call it in your handlers or before
  sending.
- **Feature off, no change.** Without the feature, the generated code is
  exactly the same as before.
- **Type aliases can't be validated directly.** A top-level schema that
  becomes an alias (for example `pub type Name = String;`) is only checked
  where it appears as a struct field.
