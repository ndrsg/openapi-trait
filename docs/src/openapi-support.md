# OpenAPI support

openapi-trait targets OpenAPI **3.0** documents in YAML or JSON.

## Schemas

| Feature | Status |
|---|---|
| `components/schemas` → structs | ✅ |
| Required vs. optional properties | ✅ optional properties become `Option<T>` |
| String enums | ✅ Rust enums, with `#[serde(rename)]` per variant |
| `additionalProperties` | ✅ `HashMap<String, T>` |
| String `format` | ✅ `date-time` → `chrono::DateTime<Utc>`, `date` → `chrono::NaiveDate`, `uuid` → `uuid::Uuid`, `binary` → `Vec<u8>`; other formats, including `email`, stay `String` (`chrono` and `uuid` are re-exported) |
| `oneOf` | ✅ a tagged enum if a `discriminator` is present, otherwise `#[serde(untagged)]` |
| `allOf` | ✅ one merged struct: `$ref` branches via `#[serde(flatten)]`, inline objects inlined |
| `anyOf` | ✅ `#[serde(untagged)]` enum |
| Inline compositions in properties | ✅ lifted to `{ParentStruct}{Property}` |
| `not`, unconstrained schemas | fall back to `serde_json::Value` |
| Constraints (`minLength`, `minimum`, `pattern`, …) | ✅ with the [`validation`](validation.md) feature |

## Operations

| Feature | Status |
|---|---|
| Path parameters | ✅ |
| Query parameters, including enums and array `style`/`explode` | ✅ |
| Header parameters | ✅ |
| JSON request bodies | ✅ |
| Binary request bodies | ✅ streamed: `axum::body::Body` on the server, `reqwest::Body` on the client |
| One response enum per operation | ✅ |
| `default` responses | ✅ `Default(...)` variant |
| Response headers (`Content-Disposition`, `ETag`, `Cache-Control`, …) | ✅ a payload struct holding the body and headers |
| Binary downloads | ✅ streamed: `axum::body::Body` on the server, `openapi_trait::ByteStream` on the client |

## Security

| Scheme | Status |
|---|---|
| `apiKey` in header, query or cookie | ✅ |
| `http` bearer | ✅ |
| `http` basic | ✅ |
| `oauth2`, `openIdConnect` | recognised, but no code is generated |

See [Security](security.md) for details.

## Not supported

If something you need isn't in these tables, please
[open an issue](https://github.com/ndrsg/openapi-trait/issues) and attach a
minimal spec.
