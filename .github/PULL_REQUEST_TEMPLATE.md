## Summary

<!-- What does this PR change, and why? -->

## Crate / scope

<!-- Which crate(s) or tooling does this touch? -->

## Layering check

- [ ] No new cross-crate dependency was added (or it was approved per the
      layering rule in `AGENTS.md`).
- [ ] `vitalis-defend` still integrates with `vitalis-replicate` only via a
      `Threat` event observed by the `drive` loop.

## Safety / security

- [ ] `vitalis-adapt` changes keep it feature-gated **off by default**.
- [ ] Checkpoint/negotiation crypto and verification are preserved or improved.

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo deny check` passes
- [ ] `CHANGELOG.md` updated (if user-facing)
