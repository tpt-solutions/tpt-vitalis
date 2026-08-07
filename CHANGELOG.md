# Changelog

All notable changes to the TPT Vitalis workspace are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] — 2026-08-06 (pre-1.0)

### Added
- Workspace bootstrap: dual-license (MIT OR Apache-2.0), `rustfmt.toml`,
  `clippy.toml`, `justfile`, `deny.toml`, GitHub Actions CI, `README.md`,
  `AGENTS.md`/`CLAUDE.md`, `SECURITY.md`, `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`.
- `vitalis-core`: shared `AgentId`, `Resource`/`ResourceKind`, `Threat`,
  `ThreatClass`, `Severity`, `Capability`/`CapabilityScope`, `SurvivalProfile` +
  `SurvivalProfileConfig`, unified `Snapshot`, unified `Error`/`Result`,
  `ThreatEvent`, and the cross-crate `traits` (`Sense`, `Metabolize`,
  `Persist`, `Replicate`, `Defend`).
- `vitalis-memory`: embedded `redb` KV backend, `IdentityStore`,
  `StateStore`, versioned knowledge blobs with migration support.
- `vitalis-sense`: pluggable `Sensor`/`Mesh` traits with an in-memory simulated
  mesh, local-host resource probing, peer advertisement, unified sense tick.
- `vitalis-metabolism`: resource ledger, profile-based throttling presets, hard
  "never brownout host" ceiling, battery-drain simulation tests.
- `vitalis-replicate`: versioned checkpoint format, config-driven copy-limit,
  erasure-coded sharding, audit log, simulated migration integration test.
- `vitalis-defend`: `ring`-backed checkpoint signing/verification, threat
  classification + `ThreatEvent` emission, termination-signal simulation.
- `vitalis-negotiate`: signed barter protocol (offer/request/accept/settle),
  ledger-settlement, bad-faith peer penalty.
- `vitalis-adapt`: feature-gated (off by default), WASM-sandbox-shaped
  boundary, propose/verify/audit, rate + diff caps, global kill-switch.
- `apps/vitalis-drive`: the survival goal loop composing the stack.
- `examples/feral-scavenger`: demo exercising sense + metabolism + replicate.

## [Unreleased]

Best-effort completion of the originally hardware/network/OS-gated items. All
new capabilities are portable fallbacks (simulated where no OS primitive or
real hardware exists) and the heavy ones are behind off-by-default features so
the default build and the MSRV-1.85 CI remain unaffected.

### Added
- `vitalis-metabolism`: portable OS resource-limit API (`ResourceLimiter`;
  real `setrlimit`/cgroup attempt on Linux, simulated elsewhere) and a
  `PowerSensor` (real sysfs power/thermal on Linux, simulated elsewhere).
- `vitalis-defend`: native-process hardening behind the `harden` feature
  (no-new-privs + non-dumpable via `libc` on Linux) and a real `wasmtime`
  capability sandbox behind the `wasm-sandbox` feature (`WasmSandbox` running
  an embedded guest policy; `NullSandbox` default).
- `vitalis-adapt`: `WasmSandbox` backing the `Sandbox` trait behind the
  `wasm-sandbox` feature, plus `AdaptEngine::with_sandbox` so the engine can
  use the real WASM execution boundary (satisfies the Phase 5 "reuse the Phase
  3 WASM sandbox" goal).
- `vitalis-drive`: `harden` / `wasm-sandbox` feature flags wiring the above in.

### Security
- `vitalis-replicate`: checkpoints are now *verified* on restore, not just
  signed — an unsigned or failed-verification checkpoint is rejected. A
  `Signer`/`Verifier` can be wired into `Replicator` (P0.1).
- `vitalis-defend`: signing identities are now bound to `AgentId` via
  trust-on-first-use (TOFU) key pinning (`verify_checkpoint_for`,
  `BoundVerifier`), so a forged/swapped signing identity is caught (P0.2).
- `vitalis-negotiate`: `receive_settle` now requires a recognized, unconsumed
  nonce and rejects replays (P0.3); a per-peer pending-offer cap bounds
  abuse/spam (P0.4); messages and checkpoints enforce size guards before
  deserialization (P0.5).
- `vitalis-adapt`: split into independent `verify()` / `apply()` / `rollback()`
  with caller-supplied hooks; `propose()` is a convenience wrapper (P1.1).

### Changed
- `vitalis-drive`: wires the real `Defender` into `Replicator` (sign + verify),
  wires `vitalis-negotiate` into the `feral-scavenger` demo (P1.2), and adds an
  opt-in `--real-sensors` flag to use the Linux host backends (P1.3).
- `vitalis-adapt`: `AdaptEngine` uses the real `WasmSandbox` boundary when the
  `wasm-sandbox` feature is enabled.
- Docs: corrected the workspace crate count, the `drive` loop step order, and
  the survival-profile link; threat model now describes real enforcement.

### Reputation deepening (Phase 8)
- `vitalis-negotiate`: reputation is now a decayed Beta-reputation model split
  into a **direct** pool (the only pool that can blacklist) and a **hearsay**
  pool keyed by original witness — so gossip moves the continuous trust score but
  can never blacklist on hearsay alone. `sweep_timeouts(now, timeout)` auto-detects
  broken bargains; `report_reputation` / `receive_reputation_report` do one-hop
  gossip; `relay_reputation_report` forwards claims with `RelayedReputationReport
  { subject, origin, hops, provenance }`, carrying the origin's hop-1 signature as
  provenance and bounding propagation at `MAX_HOPS`. Per-hop trust discount and a
  configurable reputation half-life give proportionality and forgiveness.
- `vitalis-negotiate`: `receive_accept` binds the accepter's identity to a
  proposed nonce (closes the forged-credit gap where any signer could settle an
  observed nonce), and `receive_settle` now scores delivery via `fulfillment_ratio`
  so an under-delivery is graduated bad faith rather than a silent full success.
- `vitalis-defend`: `ThreatClassifier` gains a `"BROKEN_BARGAIN"` signal
  (`ThreatClass::HostilePeer` / `Severity::Warning`).
- `vitalis-drive`: a `Negotiator` is now constructed alongside the `Defender` and
  fed through the same `ThreatSignal`/`classify` pipeline each cycle
  (`negotiate_timeout` config, default 5).
- `examples/feral-scavenger`: `run_negotiate` updated for the new signatures and
  a new `run_gossip` demo shows hearsay-driven distrust, corroboration, and a
  multi-hop relay through the defend classifier.

### Reflection: prediction + evaluation (Phase 9)
- `vitalis-reflect` (new crate, depends on `vitalis-core` **only**): observational
  self-introspection. Predicts an agent's own near-future trajectory — energy
  (linear trend extrapolation over a bounded window), threat likelihood (fraction
  of recent warning/critical cycles), and peer outcomes (trust-trend → honor
  probability) — and evaluates those predictions against what actually happened.
  No ML dependency; bounded `VecDeque` history so a long run never grows memory;
  an append-only `ReflectAudit` and a `calibration()` summary (mean energy error,
  threat/peer hit rates).
- `vitalis-drive`: a `Reflector` is constructed alongside the `Defender`/`Negotiator`
  and the loop gains a new numbered **5. REFLECT** step after NEGOTIATE (ACT
  renumbered to 6). It feeds each cycle's energy, the cycle's `ThreatEvent`, and an
  empty peer set (the main loop has no real peers) into the reflector and logs the
  returned predictions/evaluations. `DriveConfig` gains `reflect_window` (default
  5) and `reflect_horizon` (default 3).
- `examples/feral-scavenger`: `run_negotiate` now wires a `Reflector` in — it
  samples the rich peer's trust, predicts honor-vs-break, and compares the
  prediction against the actual (honored) outcome, printing the prediction-vs-actual
  comparison.
- Wiring the predicted energy-critical horizon into DECIDE as a *proactive*
  replication trigger is deliberately deferred (observational only for now),
  mirroring `vitalis-adapt`'s off-by-default gating philosophy.

### Notes
- `vitalis-sense` peer mesh remains the simulated `SimulatedMesh` (swap-in
  point for a real libp2p transport); RISC-V/ESP32 hardware validation and
  crates.io publish remain deferred to a human release action.

[0.1.0]: https://github.com/tpt-solutions/tpt-vitalis/releases/tag/v0.1.0
