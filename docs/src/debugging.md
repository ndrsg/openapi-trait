# Debugging generated code

To see what the macro produces, set `OPENAPI_TRAIT_DEBUG` when you build. Each
annotated module is written as a pretty-printed `<module_name>.rs` file:

```bash
OPENAPI_TRAIT_DEBUG=1 cargo build
```

| `OPENAPI_TRAIT_DEBUG` | Behaviour |
|---|---|
| unset, empty, `0`, `false` | disabled (default) |
| `1`, `true` | write to `$OUT_DIR/openapi-trait-debug`, or to `<temp dir>/openapi-trait-debug` if the crate has no build script |
| any other value | used as the output directory, e.g. `OPENAPI_TRAIT_DEBUG=./gen` |

The build prints the path of each file to stderr. Write failures are reported,
but they never stop the build.

## Things to know

- **Only this macro's output is expanded.** Unlike
  [`cargo expand`](https://github.com/dtolnay/cargo-expand), nested derives
  such as `Serialize` stay as `#[derive(...)]`, so the output shows only what
  openapi-trait generated.
- **Dumps are written when the crate compiles.** Run `cargo clean -p <crate>`
  or touch the spec to get a fresh dump.
- **Files are named after the module.** Two `mod petstore {}` blocks writing to
  the same directory overwrite each other. Give each build its own directory to
  keep both.
