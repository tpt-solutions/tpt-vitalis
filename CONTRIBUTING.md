# Contributing to TPT Vitalis

Thank you for considering a contribution. Vitalis is a community-governed,
safety-sensitive survival stack, so we keep a lightweight but firm process.

## Workflow: issues first

1. **Open or comment on an issue before a non-trivial change.** This is a
   safety-critical codebase (replication limits, self-modification, threat
   detection). A short design note up front avoids wasted work and dangerous
   surprises.
2. Fork, branch (`fix/...`, `feat/...`, `rfc/...`), and open a PR against
   `main`.
3. Ensure the **CI gate is green** before requesting review:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo deny check`
4. At least one maintainer reviews. RFC-gated changes (notably anything
   touching `vitalis-adapt` or the replication copy-limit) need explicit
   sign-off noted in the PR.

## License agreement

By contributing, you agree that your contributions are dual-licensed under
**MIT OR Apache-2.0**, matching the project license, and that the
Apache-2.0 `NOTICE`/attribution terms apply. You do not need to add a
per-file license header — the repo-level license covers it.

## Engineering norms

- Rust 2021, MSRV **1.85**. Code must compile on 1.85.
- `max_width = 100`; `reorder_imports = true` (see `rustfmt.toml`).
- Clippy warnings are denied (`-D warnings`) in CI — keep it clean locally.
- Public types in `vitalis-core` need unit tests or doctests.
- Use the shared `vitalis_core::error::Error` / `Result` for fallible APIs.
- Respect the **layering rule** (see `AGENTS.md`): crates depend on
  `vitalis-core` only; the `drive` app composes them; `defend` reaches
  `replicate` only via an event.

## Adding a crate

Follow the "How to add a crate" steps in `AGENTS.md`, then update the crate
status table in `README.md` and this file's scope if relevant.

## Code of Conduct

We adopt the Rust Code of Conduct for all project spaces. See
[`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).
