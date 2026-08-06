# Publishing to crates.io

Publish order must respect internal path dependencies. `vitalis-core` has no
internal deps; everything else depends (directly or transitively) on it, and
`vitalis-replicate` additionally depends on `vitalis-memory` + `vitalis-sense`.
The `drive` app and `feral-scavenger` example are binaries that compose the
crates and are not published as libraries.

## Recommended order

1. `vitalis-core`
2. `vitalis-memory`
3. `vitalis-sense`
4. `vitalis-metabolism`
5. `vitalis-replicate`
6. `vitalis-defend`
7. `vitalis-negotiate`
8. `vitalis-adapt` (remember: the `adapt` feature stays **off by default** in the
   published crate)

The app (`apps/vitalis-drive`) and example (`examples/feral-scavenger`) are
workspace members but are not published to crates.io (they are binaries / demos).

## Notes

- Each crate's `Cargo.toml` inherits package fields from the workspace
  (`version`, `license`, `authors`, `repository`, ...). Bump the workspace
  `version` in lockstep before publishing.
- Verify with `cargo publish --dry-run -p <crate>` for each, in the order above.
- Do **not** enable `vitalis-adapt`'s `adapt` feature in any published default.
