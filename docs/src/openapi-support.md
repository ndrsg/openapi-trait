# OpenAPI support

openapi-trait reads OpenAPI **3.0** and **3.1** documents in YAML or JSON.
Swagger (OpenAPI 2.0) documents are rejected with a compile error; convert them
to OpenAPI 3 first.

## Schemas

| Feature | Status |
|---|---|
| `components/schemas` → structs | ✅ |
| Required vs. optional properties | ✅ optional properties become `Option<T>` and are left out of the JSON when `None` |
| `nullable: true` (3.0), `type: [T, "null"]` (3.1) | ✅ `Option<T>`, also for array items and map values; ignored for parameters and headers |
| `oneOf` / `anyOf` with a `{type: "null"}` branch | ✅ `Option<T>` of the other branch; with several other branches, an `Option` of the enum |
| String enums | ✅ Rust enums, with `#[serde(rename)]` per variant |
| `const` (3.1) on a string | ✅ a one-variant enum |
| `additionalProperties` | ✅ `HashMap<String, T>` |
| String `format` | ✅ `date-time` → `chrono::DateTime<Utc>`, `date` → `chrono::NaiveDate`, `uuid` → `uuid::Uuid`, `binary` → `Vec<u8>`; other formats, including `email`, stay `String` (`chrono` and `uuid` are re-exported) |
| `oneOf` | ✅ a tagged enum if a `discriminator` is present, otherwise `#[serde(untagged)]` |
| `allOf` | ✅ one merged struct: `$ref` branches via `#[serde(flatten)]`, inline objects inlined |
| `anyOf` | ✅ `#[serde(untagged)]` enum |
| Inline objects, enums and compositions in properties | ✅ lifted to `{ParentStruct}{Property}` |
| Inline request and response body schemas | ✅ `Body` and `Status{code}Body` in the operation's module |
| Recursive schemas | ✅ the field that closes a cycle becomes `Box<T>` (`Option<Box<T>>` when optional); references inside arrays and maps aren't boxed |
| `not`, unconstrained schemas, several types (`type: [string, integer]`) | fall back to `serde_json::Value` |
| Constraints (`minLength`, `minimum`, `pattern`, …) | ✅ with the [`validation`](validation.md) feature; `exclusiveMinimum` / `exclusiveMaximum` in both their 3.0 (boolean) and 3.1 (numeric) form |

## Operations

| Feature | Status |
|---|---|
| Path parameters, including enums | ✅ |
| Query parameters, including enums and array `style`/`explode` | ✅ |
| Header parameters, including enums | ✅ typed like query parameters; arrays use `style: simple` (`a,b,c`). The server returns 400 for a value that doesn't parse |
| JSON request bodies | ✅ an optional body (`required: false`) is `None` when no JSON `Content-Type` is sent |
| JSON media types (`application/*+json`, `charset` parameters) | ✅ typed like `application/json`; the declared media type is sent as the `Content-Type` |
| Other request body media types (`text/plain`, forms, multipart) | not supported: the body is skipped with a warning |
| Binary request bodies | ✅ streamed: `axum::body::Body` on the server, `reqwest::Body` on the client |
| One response enum per operation | ✅ |
| `default` responses | ✅ a `Default { status, body, … }` variant, typed like a numbered response (body and headers); `status` is any status the operation doesn't declare |
| Response headers (`Content-Disposition`, `ETag`, `Cache-Control`, …) | ✅ a payload struct holding the body and headers |
| Binary downloads | ✅ streamed: `axum::body::Body` on the server, `openapi_trait::ByteStream` on the client |

## Security

| Scheme | Status |
|---|---|
| `apiKey` in header, query or cookie | ✅ |
| `http` bearer | ✅ |
| `http` basic | ✅ |
| `oauth2`, `openIdConnect` | ✅ handled like `http` bearer; scopes are not checked |

See [Security](security.md) for details.

## OpenAPI 3.1

A 3.0 document is converted to 3.1 when it's read, so both versions produce the
same code. These 3.1 features are ignored:

- `prefixItems`, `if` / `then` / `else`, `dependentSchemas`,
  `patternProperties`, `unevaluatedProperties` and `$dynamicRef`
- keywords next to a `$ref`
- `contentEncoding` and `contentMediaType`; use `format: binary` or a binary
  media type for binary bodies
- `webhooks` and `components/pathItems`

## Not supported

If something you need isn't in these tables, please
[open an issue](https://github.com/ndrsg/openapi-trait/issues) and attach a
minimal spec.
