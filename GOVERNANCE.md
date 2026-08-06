# Governance

TPT Vitalis is community-governed and **standalone** — it does not depend on any
TPT-wide constitution (none exists; references to one in `spec.txt` are
aspirational). This note describes how the project is run today.

## Principles

- **Safety first.** The survival layer must never become a maximizer (spec §10).
  `vitalis-adapt` is feature-gated and off by default; any change to its
  activation path requires explicit, documented human action.
- **Bounded survival.** Replication is copy-limited and audited; negotiation is
  cryptographic. Unbounded self-replication or self-improvement is out of scope.
- **Composable.** Each crate is independently usable; the `drive` app is the only
  place that composes them.

## Decision process

1. **Issues first.** Non-trivial changes start with an issue (see
   `CONTRIBUTING.md`).
2. **RFC for danger-zone changes.** Anything touching `vitalis-adapt` or the
   replication copy-limit needs a written, reviewed RFC before code.
3. **CI is the gate.** `cargo fmt --check`, `cargo clippy -- -D warnings`,
   `cargo test --workspace`, and `cargo deny check` must all be green.
4. **Maintainer sign-off** on releases and on RFC-gated changes.

## Becoming a maintainer

Propose it in an issue. Maintainers are added by existing maintainers per the
process above; there is no formal charter beyond this note.

## Code of Conduct

We adopt the [Rust Code of Conduct](./CODE_OF_CONDUCT.md).
