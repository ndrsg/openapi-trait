# Writing a client

`#[openapi_trait::client(...)]` generates the same models as the server macro,
plus a **transport-agnostic** `{Mod}Client` trait:

```rust,ignore
{{#include ../../openapi-trait/examples/reqwest_client.rs:macro}}
```

You can implement that trait yourself, or use the generated
[reqwest implementation](#the-reqwest-client).

## Implementing the trait yourself

Every method takes the operation's request plus an
`Option<openapi_trait::RequestOptions>` and returns the operation's response
enum:

```rust,ignore
{{#include ../../openapi-trait/examples/client.rs:impl}}
```

Code that depends on the trait doesn't care which implementation it gets, so
fakes like this one make good test doubles:

```rust,ignore
{{#include ../../openapi-trait/examples/client.rs:generic}}
```

Unlike the server trait, client methods have no default bodies. You implement
every operation.

## The reqwest client

With the default `reqwest-client` feature, the macro also generates a blanket
`{Mod}Client` implementation. Any type can use it by deriving
`openapi_trait::ReqwestClient`:

```rust,ignore
{{#include ../../openapi-trait/examples/reqwest_client.rs:carrier}}
```

| Field attribute | Field type | Required |
|---|---|---|
| `#[openapi_trait(client)]` | `openapi_trait::reqwest::Client` | yes |
| `#[openapi_trait(base_url)]` | `String` | yes |
| `#[openapi_trait(auth)]` | `{Mod}AuthState` | when the spec declares security schemes |

A field named `auth` is picked up without the attribute.

Then call operations through the trait:

```rust,ignore
{{#include ../../openapi-trait/examples/reqwest_client.rs:usage}}
```

The generated implementation returns `Reqwest{Mod}ClientError`, an enum
covering transport errors, missing credentials, malformed response headers,
and status codes the spec doesn't declare (`UnexpectedStatus`, which includes
the body). When the operation has a `default` response, an undeclared status
isn't an error: it's decoded into `Response::Default { status, body, … }`.

## Per-request headers and auth

Pass `Some(RequestOptions)` to add headers or override auth for a single call
without changing the client:

```rust,ignore
{{#include ../../openapi-trait/examples/reqwest_client.rs:options}}
```

`RequestOptions` provides `header(name, value)`, `bearer_auth(token)` and
`basic_auth(username, Option<password>)`. Options are applied after the
operation's declared headers and credentials:

- `bearer_auth` and `basic_auth` **replace** any `Authorization` header set by
  the client's security scheme. They also satisfy the credential check, so a
  call can succeed even if the client has no stored credentials.
- `header` **appends** to the request.

## Binary downloads

Operations that return a binary body give you an `openapi_trait::ByteStream`,
a `futures_core::Stream` of `bytes::Bytes` chunks. Read it incrementally instead
of buffering the whole response.

## Without reqwest

To generate only the transport-agnostic trait and drop the `reqwest`
dependency:

```toml
[dependencies]
openapi-trait = { version = "0.0.9", default-features = false }
```
