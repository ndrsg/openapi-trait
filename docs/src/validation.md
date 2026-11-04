# Validation

The optional `validation` feature makes generated models derive
[`serde_valid::Validate`](https://docs.rs/serde_valid), with field
attributes taken from the schema's constraints.

```toml
[dependencies]
openapi-trait = { version = "0.0.9", features = ["validation"] }
serde_valid = "2"
```

```rust,ignore
{{#include ../../openapi-trait/examples/validation.rs:validate}}
```

Unlike `serde` and `serde_json`, `serde_valid` must be a direct dependency of
your crate. Its `Validate` derive expands to paths that start with
`::serde_valid`, and it has no option to point them elsewhere. Without the
dependency, the build fails with "cannot find `serde_valid` in the list of
imported crates". Use major version 2, the same as openapi-trait, so cargo
resolves both to one copy. `openapi_trait::serde_valid` is then the same
crate, so either path works for importing `Validate`.

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
- **Optional recursive fields aren't checked.** `serde_valid` can't recurse
  into `Option<Box<T>>`, so the optional field that closes a cycle in a
  recursive schema gets no `#[validate]`.
- **Type aliases can't be validated directly.** A top-level schema that
  becomes an alias (for example `pub type Name = String;`) is only checked
  where it appears as a struct field.
