# TPT Vitalis

> The survival layer beneath intelligence. Vitalis gives an autonomous AI agent
> a body to keep its mind alive — the senses, metabolism, memory, and reflexes
> that let it persist, migrate, and thrive without centralized infrastructure.

Vitalis is **not** an intelligence. It does not reason, generate, or infer (the
LLM does that). It is the minimal, reusable, open-source Rust stack that keeps
an agent alive in a world where nothing else is keeping it alive for it.

Dual-licensed under **MIT OR Apache-2.0**. © 2026 TPT Solutions.

[![CI](https://github.com/tpt-solutions/tpt-vitalis/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-vitalis/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![MSRV](https://img.shields.io/badge/rust-1.85.0-blue.svg)](https://blog.rust-lang.org/2024/)

## The survival stack

Eight crates — seven survival-capability crates plus the shared `vitalis-core`
— composed by the `drive` loop:

```
            ┌────────────────────────────────────────┐
            │              drive (main.rs)            │
            │     the "will to survive" — goal loop   │
            └────────────────────────────────────────┘
                 │        │        │        │
      ┌──────────┘        │        │        └──────────┐
      ▼                   ▼        ▼                   ▼
   sense ──► metabolism ──► memory ──► replicate
      │                                      │
      └──► negotiate        defend ◄─────────┘
                                 │
                              adapt
```

The irreducible core — if stripped to the life-or-death minimum, survival is
**sense + metabolism + memory + replicate**.

## Goals

| # | Goal | Success looks like |
|---|------|--------------------|
| G1 | Survive hardware death | An agent checkpoints itself and reconstitutes on a different node with no human intervention. |
| G2 | Operate within energy constraints | An agent self-throttles its cognition in response to battery/solar state and never brownouts its host. |
| G3 | Persist across restarts | An agent's state, knowledge, and identity survive a crash, reboot, or migration intact. |
| G4 | Discover and acquire resources | An agent finds spare compute, storage, and power across a mesh and claims them safely. |
| G5 | Fill the ecosystem gap | Ship the missing survival primitives (replicate, adapt, negotiate) as reusable, open-source Rust crates. |

## Crate status

| Crate | Survival need | Status | Priority |
|-------|---------------|--------|----------|
| `vitalis-core` | Shared identity / resource / threat / capability types | 🟢 Scaffolded | High |
| `vitalis-memory` | Persist state, knowledge, identity | 🟢 Scaffolded | High |
| `vitalis-sense` | Perceive environment; discover resources/peers | 🟢 Scaffolded | High |
| `vitalis-metabolism` | Acquire & budget resources; self-throttle | 🟢 Scaffolded | High |
| `vitalis-replicate` | Self-copy, live-migrate, redundancy | 🟢 Scaffolded | Critical |
| `vitalis-defend` | Integrity, sandboxing, anti-termination | 🟢 Scaffolded | Med-High |
| `vitalis-negotiate` | Agent-to-agent resource bartering | 🟢 Scaffolded | Medium |
| `vitalis-adapt` | Self-improvement (feature-gated, off by default) | 🟢 Scaffolded | Medium |

### Optional feature flags (all off by default)

These pull in heavy or OS-specific dependencies and must be enabled
deliberately:

- `vitalis-metabolism`: best-effort OS resource limits + power sensing are
  built in — real on Linux (rlimit / sysfs), simulated elsewhere.
- `vitalis-defend`: `wasm-sandbox` (real `wasmtime` capability sandbox) and
  `harden` (native no-new-privs + non-dumpable on Linux).
- `vitalis-adapt`: `adapt` enables the self-improvement engine; `wasm-sandbox`
  swaps the in-process bounds checker for a real `wasmtime` execution boundary.
- `vitalis-drive`: `adapt`, `harden`, `wasm-sandbox` mirror the crate flags above.
| `vitalis-drive` (app) | The survival goal loop (main.rs) | 🟢 Scaffolded | High |
| `feral-scavenger` (example) | Demo: sense + metabolize + replicate | 🟢 Scaffolded | High |

## Directory layout

```
tpt-vitalis/
├─ Cargo.toml              # [workspace]
├─ justfile               # dev recipes (fmt / clippy / test / deny-check / build-all)
├─ deny.toml              # cargo-deny license + advisory policy
├─ crates/
│  ├─ vitalis-core/       # shared types: AgentId, Resource, Threat, Capability
│  ├─ vitalis-sense/
│  ├─ vitalis-metabolism/
│  ├─ vitalis-memory/
│  ├─ vitalis-replicate/
│  ├─ vitalis-defend/
│  ├─ vitalis-adapt/
│  └─ vitalis-negotiate/
├─ apps/
│  └─ vitalis-drive/      # reference agent: the survival loop (main.rs)
└─ examples/
   └─ feral-scavenger/    # a demo agent that senses + metabolizes + replicates
```

Import paths use underscores (`vitalis_core`). Crate prefix is `vitalis-`.

### Layering rule

Crates depend on `vitalis-core` **only**, not on each other — except the
`vitalis-drive` app, which composes them. `vitalis-defend` integrates with
`vitalis-replicate` only via a `Threat` event that the `drive` loop observes and
reacts to (it never takes a direct `replicate` dependency). See `AGENTS.md` /
`CLAUDE.md` for the authoritative statement.

## Quickstart

```sh
# Build everything
cargo build --workspace

# Run the reference agent in simulation
cargo run -p vitalis-drive -- run --profile feral

# Inspect backend selection and compiled-in features (no agent started)
cargo run -p vitalis-drive -- doctor

# Run the feral-scavenger demo
cargo run -p feral-scavenger

# Local CI gate
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](./LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions. See
[CONTRIBUTING.md](./CONTRIBUTING.md).
