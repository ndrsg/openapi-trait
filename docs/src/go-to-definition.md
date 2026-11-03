# Go-to-definition in your IDE

By default, the generated code exists only inside the compiler. Go to
definition (F12 in VS Code) on `pets::Pet` or `PetsApi::list_pets` therefore
takes you to the `#[openapi_trait::axum(...)]` attribute, not to the struct or
method.

Add `source_file` to the attribute to fix this:

```rust,ignore
#[openapi_trait::axum("openapi/pets.yaml", source_file)]
pub mod pets {}

#[openapi_trait::client("openapi/pets.yaml", source_file)]
pub mod pets_client {}
```

The macro writes the generated module body to a pretty-printed `.rs` file and
`include!`s it. rust-analyzer then treats that file as the source of every
generated item, so go-to-definition, find-references and hover open readable
code.

## Where the files go

| Argument | Directory |
|---|---|
| `source_file` | `.openapi-trait/` in your crate root (next to `Cargo.toml`) |
| `source_file = "dir"` | `dir`, relative to your crate root |

The default directory gets a `.gitignore` containing `*` the first time it is
created, so Git ignores it without any setup. A directory you choose with
`source_file = "dir"` gets no `.gitignore`; add it to your own `.gitignore`.

Files are named `<module>.<backend>.<hash>.rs`, for example
`pets.axum.c1bb924c.rs`. The hash comes from the spec path, so modules with the
same name but different specs don't overwrite each other. Each file starts with
an `// @generated` header. Editing the file has no effect because it is
rewritten on every build. Change the spec instead.

The file is rewritten only when its content changes. Builds don't touch it
otherwise, so neither Cargo nor rust-analyzer re-checks without reason.

> **Why not `target/`?** rust-analyzer does not load files under `target/`
> (except a build script's `OUT_DIR`). An `include!` pointing there resolves to
> nothing in the IDE, and the whole module looks empty. The default directory is
> inside the crate so the IDE can always see it.

## rust-analyzer notes

- **First use.** rust-analyzer creates the file while it expands the macro, so
  it may not see the file on that first pass. It normally picks the file up a
  moment later. If definitions still don't resolve, run
  **rust-analyzer: Restart server** once.
- **Proc-macro expansion must be on**, which is the default
  (`rust-analyzer.procMacro.enable`).
- **Excluded folders.** If you exclude dot-folders in your settings
  (`rust-analyzer.files.exclude`), keep `.openapi-trait` visible, or use
  `source_file = "dir"` to pick a directory that isn't excluded.

## Things to know

- **Behaviour is unchanged otherwise.** The generated code is identical, and
  without `source_file` nothing is written to disk.
- **Write failures don't stop the build.** If the file can't be written, for
  example on a read-only file system, the macro prints a warning and falls back
  to inline code. Go-to-definition then lands on the attribute again.
- **Compared with `OPENAPI_TRAIT_DEBUG`:** the [debug dump](debugging.md) is a
  one-off copy for reading. The compiler never uses it. `source_file` produces
  the code that is actually compiled, and it stays up to date.
