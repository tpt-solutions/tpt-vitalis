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

### Notes
- `vitalis-sense` peer mesh remains the simulated `SimulatedMesh` (swap-in
  point for a real libp2p transport); RISC-V/ESP32 hardware validation and
  crates.io publish remain deferred to a human release action.

[0.1.0]: https://github.com/tpt-solutions/tpt-vitalis/releases/tag/v0.1.0
