# AGENTS.md — Workspace orientation for contributors & agents

This file is the authoritative guide to the *layout and layering rules* of the
tpt-vitalis workspace. `CLAUDE.md` mirrors it for Claude-based agents.

## What this project is

A portable AI survival stack — the minimal set of capabilities any autonomous
agent needs to persist, migrate, and thrive without centralized infrastructure.
It is the survival layer beneath intelligence, not intelligence itself.

Design doc: [`spec.txt`](./spec.txt). Project checklist: [`todo.md`](./todo.md).

## Layout

```
crates/vitalis-core        shared types: AgentId, Resource, Threat, Capability, SurvivalProfile
crates/vitalis-sense       perceive environment; discover compute/energy/storage/peers
crates/vitalis-metabolism  acquire & budget resources; self-throttle cognition
crates/vitalis-memory      persist state, knowledge, identity across restarts/migrations
crates/vitalis-replicate   self-copy, live-migrate, erasure-coded redundancy
crates/vitalis-defend      integrity, sandboxing, anti-termination
crates/vitalis-adapt       self-improvement — FEATURE-GATED, OFF BY DEFAULT
crates/vitalis-negotiate   agent-to-agent resource bartering
apps/vitalis-drive         the survival goal loop (main.rs) — composes the crates
examples/feral-scavenger   demo agent exercising sense + metabolism + replicate
```

Import paths use underscores (`vitalis_core`). Crate prefix is `vitalis-`.

## Layering rule (ENFORCED)

1. Every crate depends on `vitalis-core` **only**, never on another survival
   crate — with one exception: `vitalis-replicate` depends on `vitalis-memory`
   and `vitalis-sense` (it needs to persist state and discover targets).
2. The `vitalis-drive` **app** is the only place that composes multiple
   survival crates.
3. `vitalis-defend` integrates with `vitalis-replicate` **only via an event**
   (`vitalis_core::events::ThreatEvent`) that the `drive` loop observes and
   reacts to. Defend must NOT take a direct `vitalis-replicate` dependency.
   This keeps the immune response decoupled from the reproduction machinery.

If you find yourself wanting a direct cross-crate dependency that breaks these
rules, surface it as a question first — it usually means the shared type belongs
in `vitalis-core` instead.

## Engineering conventions

- Rust 2021, rust-version 1.85 (MSRV). Keep code compiling on 1.85.
- `cargo fmt` (max_width 100) and `clippy -D warnings` must pass.
- Every public type in `vitalis-core` gets unit tests and/or doctests.
- Failures use the shared `vitalis_core::error::Error` / `Result` alias.
- Dual-licensed MIT OR Apache-2.0; every new crate file should not need a
  per-file license header (repo-level license applies).

## How to add a crate

1. Create `crates/<name>/Cargo.toml` inheriting `.workspace = true` package
   fields; only add crate-specific `description`/`keywords`/`categories`.
2. Add the path to `workspace.dependencies` and to `members` in root
   `Cargo.toml`.
3. Update the crate status table in `README.md`.
4. Keep the layering rule above.

## Things that are NOT real dependencies

`spec.txt` references "TPT Cambium" and "the TPT constitution." Neither exists
yet — both are aspirational forward references in vitalis's own design doc. Do
not search the repo for them; vitalis is fully standalone.
