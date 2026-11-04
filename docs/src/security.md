# Security

Every scheme in `components.securitySchemes` gets a typed credential struct,
named after the scheme key in PascalCase:

| Scheme | Generated type |
|---|---|
| `apiKey` (in `header`, `query` or `cookie`) | `pub struct ApiKey(pub String)` |
| `http` with `scheme: bearer` | `pub struct BearerAuth(pub String)` |
| `http` with `scheme: basic` | `pub struct BasicAuth { pub username: String, pub password: String }` |
| `oauth2`, `openIdConnect` | `pub struct OAuth(pub String)`, the access token sent as `Authorization: Bearer <token>` |

These rules decide which operations require credentials:

- Document-level `security` is the default for every operation.
- An operation's own `security` replaces that default.
- `security: []` turns auth off for an operation (`listPets` in the example
  spec).
- If an operation lists several alternatives (an OR), the macro generates an
  `{operation}::Auth` enum with one variant per scheme.
- An empty requirement (`security: [{}, {bearerAuth: []}]`) makes auth
  optional: the server passes `Option<…>` to the handler, with `None` for an
  anonymous request, instead of returning 401, and the client sends a
  credential if one is configured but doesn't fail without one.
- An operation that requires an unsupported scheme (such as `http` with
  `scheme: digest`, or `mutualTLS`) or a scheme that isn't declared in
  `components.securitySchemes` is a compile error, so it is never generated
  without auth. Unsupported schemes that no operation requires are ignored.

## Server side

Secured handlers find the credentials in `ctx.auth`. The macro
extracts the raw credential and returns `401 Unauthorized` if it is missing,
**before** your handler runs. Checking that the credential is *valid* is your
job. For `oauth2` and `openIdConnect` that includes checking the token's
scopes against the ones the operation lists:

```rust,ignore
{{#include ../../openapi-trait/examples/server.rs:impl}}
```

With alternatives, match on the generated enum:

```rust,ignore
async fn get_flex(
    &self,
    ctx: api::get_flex::Context,
) -> Result<api::get_flex::Response, Self::Error> {
    let who = match ctx.auth {
        api::get_flex::Auth::BearerAuth(token) => token.0,
        api::get_flex::Auth::BasicAuth(basic) => basic.username,
    };
    // ...
}
```

## Client side

Clients keep credentials for the whole session. The reqwest carrier holds a
`{Mod}AuthState`, and the `{Mod}ClientAuth` extension trait adds a
`with_<scheme>(…)` setter for each scheme:

```rust,ignore
{{#include ../../openapi-trait/examples/reqwest_client.rs:carrier}}
```

```rust,ignore
use pets::PetsClientAuth as _;

let client = PetsHttpClient { /* … */ }.with_bearer_auth("secret");
```

The client attaches the right credential to every secured call. If none of an
operation's schemes has a credential, the call fails with
`MissingCredential` before anything is sent. To use different credentials for
one call, see
[per-request headers and auth](client.md#per-request-headers-and-auth).

## Why the server and client differ

On the server, credentials arrive with each request, so authorization can
depend on the caller. On the client, credentials normally belong to the
session, so they are set once on the client.
