# Vitalis Architecture

Vitalis is an 8-crate Rust workspace — seven survival-capability crates plus the
shared `vitalis-core` — composed by a `drive` app with a `feral-scavenger`
example. It is the survival layer beneath an agent's intelligence: sense,
metabolize, remember, replicate, defend, adapt, negotiate.

## Layering (enforced)

```
vitalis-core        shared types, errors, traits, events  (depended on by ALL)
   │
   ├── vitalis-memory       depends on core only
   ├── vitalis-sense        depends on core only
   ├── vitalis-metabolism   depends on core only
   ├── vitalis-defend       depends on core only
   ├── vitalis-negotiate    depends on core only
   ├── vitalis-adapt         depends on core only (OFF by default; feature-gated)
   │
   └── vitalis-replicate    depends on core + memory + sense (the only extra deps)
        │
        └── vitalis-drive   app: composes everything (the "will to survive" loop)
              └── feral-scavenger  example using sense + metabolism + replicate
```

Rules (see `AGENTS.md` / `CLAUDE.md`):

1. Every crate depends on `vitalis-core` **only**, except `vitalis-replicate`,
   which additionally uses `vitalis-memory` (durable state) and `vitalis-sense`
   (discovery of migration targets).
2. The `vitalis-drive` **app** is the only place that composes multiple
   survival crates into a running agent.
3. `vitalis-defend` reaches `vitalis-replicate` *only* by emitting a
   `ThreatEvent` that the `drive` loop observes and reacts to. It never takes a
   direct `replicate` dependency. This keeps the immune response decoupled from
   reproduction.

## The survival loop (`drive`)

Each cycle the `drive` loop performs: **sense → metabolize → decide → defend →
act** (see `apps/vitalis-drive/src/loop_.rs`).

- **sense** (`vitalis-sense`): read host resources + discover peers/mesh.
- **metabolize** (`vitalis-metabolism`): update the ledger, derive an energy
  fraction, set the cognition throttle, enforce the host safety ceiling.
- **decide**: the energy fraction drives the cognition throttle; if energy is
  critically low the next step escalates to an escape-worthy threat.
- **defend** (`vitalis-defend`): classify threat signals into `ThreatEvent`s and
  sign/verify checkpoints for integrity.
- **act**: spend energy; when a `ThreatEvent` is escape-worthy (or energy is
  critically low), checkpoint + replicate (+ migrate) via `vitalis-replicate`.
- **persist** (`vitalis-memory`): durable identity/state/knowledge survives
  restart and migration (goals **G1–G3**); note persistence is *event-driven* —
  it happens when a checkpoint is captured under pressure or on migration, not
  unconditionally every cycle.

## Transport / backend choices

- **Mesh**: a `Mesh` trait with a network-free `SimulatedMesh` so the stack is
  testable in CI. Production would back this with libp2p.
- **Sandbox (adapt)**: a `Sandbox` trait shaped like a WASM/WASI runtime; the
  reference implementation is an in-process bounds checker. Production would use
  `wasmtime`/`wasmer`.
- **Crypto**: `ring` (Ed25519) for checkpoint signing and barter messages.
- **Storage**: `redb` embedded key/value.
- **Erasure coding**: `reed-solomon-erasure`.

## What is and isn't validated

| Goal | Status |
|------|--------|
| G1 survive hardware death | ✅ simulated migration integration test |
| G2 operate within energy constraints | ✅ throttle + safety-ceiling unit tests |
| G3 persist across restarts | ✅ redb reopen integration test |
| G4 discover/acquire resources | ✅ simulated mesh + scavenge tests |
| G5 ship reusable survival primitives | ✅ 7 crates + app + example |

**Not yet validated:** real RISC-V / ESP32 hardware runs, real libp2p mesh
networking, and real `wasmtime` sandboxing. The CI `cross-compile` job is a
non-blocking probe only.
