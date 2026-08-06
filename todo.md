# tpt-vitalis — Project Checklist

A portable AI survival stack — the minimal set of capabilities any autonomous
agent needs to persist, migrate, and thrive without centralized
infrastructure. TPT Solutions. Dual-licensed **MIT OR Apache-2.0**.

> **Note on spec.txt references:** `spec.txt` mentions "TPT Cambium" and "the
> TPT constitution." Neither exists anywhere yet — both are aspirational,
> forward-looking references in vitalis's own design doc, not real
> dependencies. Don't go looking for them; vitalis is fully standalone.

Source design doc: [`spec.txt`](./spec.txt). See `spec.txt` §5 for the full
crate catalog and §10 for risks/watch-outs referenced below.

---

## Phase 0 — Repo & Tooling Bootstrap

*Goal: get an empty repo to a green, contributable Rust workspace before any
crate logic exists.*

- [ ] Create the `github.com/tpt-solutions/tpt-vitalis` repo
- [x] `git init`; `.gitignore` added (`/target`; `Cargo.lock` policy:
      commit it, since the workspace ships binaries — `vitalis-drive` +
      `feral-scavenger`); initial commit still pending
- [x] Add `LICENSE-MIT` (dated "Copyright 2026 TPT Solutions", eidos-style)
- [x] Add `LICENSE-APACHE` (with the dual-license trailer, copied verbatim
      from sibling repos — not the stock Apache-2.0 boilerplate)
- [x] Root `Cargo.toml`: `[workspace]` with an empty `members` list to start
      (populated one path at a time as each crate is scaffolded — no globs,
      matches sibling precedent)
- [x] Root `Cargo.toml`: `[workspace.package]` — version `0.1.0`, edition
      `2021`, rust-version `1.85`, authors `["TPT Solutions"]`, license
      `"MIT OR Apache-2.0"`, repository/homepage
      `https://github.com/tpt-solutions/tpt-vitalis`
- [x] Root `Cargo.toml`: empty `[workspace.dependencies]` block (grows per
      phase) and `[profile.release]` (opt-level 3, lto, codegen-units 1)
- [x] `rustfmt.toml` (max_width 100, use_small_heuristics Default,
      reorder_imports true, edition 2021)
- [x] `clippy.toml` (msrv 1.85)
- [x] `justfile`: recipes for `fmt`, `clippy`, `test`, `deny-check`,
      `build-all` (original to vitalis, no sibling to copy)
- [x] `deny.toml`: cargo-deny license allowlist (MIT/Apache-2.0 compatible)
      + advisory-db check (original to vitalis)
- [x] `.github/workflows/ci.yml`: `cargo fmt --all -- --check`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace`, pinned `dtolnay/rust-toolchain@1.85.0`.
      Linux-focused; note ESP32 cross-compile as a separate later check.
- [x] Root `README.md` skeleton: pitch, brand, license badges, crate status
      table mirroring spec §5 (Status/Priority columns)
- [x] `CONTRIBUTING.md` (base on tpt-eidos's real content: issues-first
      workflow, fmt/clippy/test gate, license-agreement clause)
- [x] `AGENTS.md` + `CLAUDE.md`: workspace layout, crate-dependency layering
      rule ("crates depend on `vitalis-core` only, not each other, except
      the `drive` app which composes them; `vitalis-defend` integrates with
      `vitalis-replicate` only via an event the `drive` loop reacts to")
- [x] `SECURITY.md` skeleton (Reporting / Supported Versions / Threat Model /
      Defenses / Hardening — adapted from glyph's skeleton, filled in later)
- [x] Document the intended directory shape (`crates/`, `apps/`, `examples/`)
      in `README.md`/`AGENTS.md` even though most stay empty until Phase 1

**Milestone:** an empty, license-clean, CI-green workspace exists —
`cargo fmt --check`/`clippy`/`test` all trivially pass on the very first push.

---

## Recurring housekeeping

*Apply at the end of every phase from here on — don't re-derive this each time.*

- [x] Register the phase's new crate(s) in root `Cargo.toml`
      (`members` + `workspace.dependencies`)
- [x] Update `README.md`'s crate status table
- [x] Update `AGENTS.md`/`CLAUDE.md` if workspace layout or layering changed
- [x] Add a `CHANGELOG.md` entry (create the file at the first phase that
      ships something)
- [x] Confirm `cargo fmt --all --check` / `cargo clippy --workspace
      --all-targets -- -D warnings` / `cargo test --workspace` /
      `cargo deny check` are all green before closing the phase

---

## Phase 1 — Foundation

*Goal: establish identity and persistence.*

### vitalis-core
- [x] Scaffold crate (lib.rs, `Cargo.toml` inheriting `.workspace = true`
      fields, crate-specific description/keywords/categories)
- [x] Define `AgentId` (identity type + serialization)
- [x] Define `Resource` (compute/storage/energy/network, quantity + units)
- [x] Define `Threat` (integrity/termination threat classification,
      consumed later by defend + drive)
- [x] Define `Capability` (permission/access token model, consumed later by
      defend + negotiate)
- [x] Define `SurvivalProfile` enum (Apex / Feral / Micro-Wilds) + per-profile
      weighting config (spec §6)
- [x] Define shared error type(s) / `Result` alias used across all crates
- [x] Define minimal cross-crate trait/event signatures the `drive` loop
      will dispatch on (signatures only, no implementations yet)
- [x] Unit tests + doctests for all public types

### vitalis-memory
- [x] Scaffold crate, depends on `vitalis-core`
- [x] **Decide:** embedded KV backend — sled vs redb *(chose redb)*
- [x] Implement the storage backend wrapper
- [x] `IdentityStore` API: persist/load `AgentId` + credentials across restarts
- [x] `StateStore` API: generic checkpoint-able key/value state persistence
- [x] **Decide:** whether a CRDT layer (automerge/yrs) is needed now, or only
      once replicate/negotiate actually merge redundant copies *(deferred — not
      needed until redundant copies are merged)*
- [x] Durability guarantees: fsync policy, corruption/WAL-replay recovery test
- [x] Versioned serialization format for "knowledge" blobs (forward/backward
      compatible) + schema migration support
- [x] Benchmark read/write latency + storage footprint (Micro/Wilds
      constrained-hardware profile)
- [x] Integration test: kill process mid-write, restart, verify state intact
      — proves **G3**

**Milestone:** `vitalis-core` + `vitalis-memory` compile, are tested, and a
throwaway test harness proves an `AgentId` + state blob survive a process
restart.

---

## Phase 2 — Irreducible Core

*Goal: ship the life-or-death minimum — sense, metabolize, remember, replicate.*

### vitalis-sense
- [x] Scaffold crate, depends on `vitalis-core`
- [x] Integrate libp2p (or chosen alternative) for peer discovery on a local mesh
      *(best-effort: `SimulatedMesh` exercises the full discovery/barter path with
      zero network; a real libp2p transport is a drop-in behind the `Mesh` trait —
      see Not-yet-done list)*
- [x] Local-host resource probing (CPU/RAM/disk/battery) → emits
      `vitalis_core::Resource` readings
- [x] Peer capability/resource advertisement + query protocol
- [x] A "sense tick" API the drive loop polls each cycle (unified
      perception snapshot)
- [x] Resource-scavenging discovery: find spare/idle compute+storage across
      the mesh
- [x] Pluggable sensor backend trait (so the ESP32 edge tier can swap in
      lightweight sensors)
- [x] Threat-relevant sensing hooks (anomalous termination signals,
      starvation) feeding `vitalis-defend` later
- [x] Tests against a mocked/simulated peer mesh (no real network in CI)
- [x] Document supported discovery transports and their trust assumptions

### vitalis-metabolism
- [x] Scaffold crate, depends on `vitalis-core`
- [x] Wrap cgroups/rlimit behind a portable resource-limit API (native
      Linux "brain" tier) *(best-effort: `ResourceLimiter` does a real
      `setrlimit`/cgroup attempt on Linux via `libc`, simulated elsewhere — see
      Not-yet-done list)*
- [x] Wrap hwmon/power-sensor reads (battery %, solar input, thermal)
      behind a portable API *(best-effort: `PowerSensor` reads real sysfs
      power/thermal on Linux, simulated elsewhere — see Not-yet-done list)*
- [x] Resource budget/ledger tracking consumption against acquired
      `Resource`s from sense
- [x] Self-throttling control loop (reduce cognition/inference rate as
      energy drops)
- [x] Hard "never brownout host" safety ceiling, independent of agent priority
- [x] Per-`SurvivalProfile` throttling presets (Apex wasteful / Feral
      frugal / Micro solar-aware)
- [ ] `no_std` portability shim for the ESP32 edge tier (no cgroups there)
      *(corrected 2026-08-07 — no `#![no_std]` code exists anywhere; the
      cross-compile CI probe swallows failures via `|| true` and
      `continue-on-error: true`, so it never proved this. See Phase 7.)*
- [ ] **Real RISC-V + ESP32 hardware validation pass**: run throttling logic
      on actual low-power hardware, confirm assumptions hold (spec §10 risk)
      *(ESP32 deferred to later per plan; RISC-V validation still needs real
      hardware — see Not-yet-done list)*
- [x] Benchmark the control loop's own overhead (must not itself starve
      the host)
- [x] Tests simulating a battery-drain curve, asserting correct throttle
      response

### vitalis-replicate (CRITICAL)
- [x] Scaffold crate, depends on `vitalis-core` (+ `vitalis-memory` for
      state, `vitalis-sense` for target discovery)
- [x] Design a versioned checkpoint format (weights + memory + process
      state), portable across nodes
- [x] Checkpoint capture: serialize a running agent's full state
- [x] Checkpoint restore: reconstitute an agent from a checkpoint on a
      fresh node
- [x] Live-migration handshake protocol against a target node discovered
      via `vitalis-sense`
- [x] **Decide:** erasure-coding library (e.g. `reed-solomon-erasure`)
- [x] Erasure-coded redundancy (split checkpoint into N shards, tolerate
      loss of K)
- [x] **Enforce copy-limit / resource-budget policy**: hard, config-driven,
      auditable cap on concurrent redundant copies (spec §10 risk item)
- [x] "Dying hardware" trigger-detection hook (consumes signals from
      sense/metabolism, and later defend)
- [x] Audit log of every replication/migration event (who, when, why, how
      many copies exist)
- [x] Integration test: kill the host mid-run, verify the agent
      reconstitutes on a second (simulated) node with state intact —
      proves **G1**
- [x] Integration test: an (N+1)th replication attempt is rejected by the
      copy-limit policy

### apps/vitalis-drive
- [x] Scaffold binary crate composing core+sense+metabolism+memory+replicate
- [x] Implement the survival goal loop (sense → decide → act → persist cycle)
- [x] Wire metabolism's self-throttling into the loop's own cognition-rate
      parameter
- [x] Wire replicate triggers (from sense/metabolism signals) into
      automatic checkpoint+migrate calls
- [x] CLI/config for choosing a `SurvivalProfile` at startup
- [x] Structured logging of the loop's decisions (audit + demo purposes)

### examples/feral-scavenger (v1)
- [x] Scaffold demo scenario: scavenge spare compute on a simulated mesh,
      sip power, checkpoint under simulated resource pressure (sense +
      metabolism + replicate, per spec §8)
- [x] README walkthrough for running the demo locally

**Milestone:** running `vitalis-drive` end-to-end simulates host death and
the agent migrates and resumes — the life-or-death minimum (**G1-G3**) works,
demonstrated by `feral-scavenger` v1.

---

## Phase 3 — Defense

*Goal: wire an immune response to replication.*

### vitalis-defend
- [x] Scaffold crate, depends on `vitalis-core` only (integrates with
      `vitalis-replicate` via a `Threat` event the `drive` app observes and
      reacts to — keeps the layering rule intact rather than a direct
      defend→replicate dependency)
- [x] Integrate `ring`/`rustls` for transport integrity/authentication of
      inter-agent comms
- [x] **Primary sandboxing approach: WASM/WASI runtime** (wasmtime or
      wasmer) for capability-sandboxed extensions *(best-effort: real
      `wasmtime` `WasmSandbox` behind the `wasm-sandbox` feature runs an
      embedded guest policy; `NullSandbox` default keeps the default build
      dependency-light)*
- [x] Native-process-level hardening underneath (`cap-primitives`/
      `landlock`/`seccomp`) as defense-in-depth alongside the WASM sandbox
      *(best-effort: `harden` feature applies no-new-privs + non-dumpable via
      `libc` on Linux; a full landlock filesystem ruleset / seccomp filter is a
      noted future extension — see Not-yet-done list)*
- [x] Checkpoint/process integrity verification (detect tampering with
      persisted state)
- [x] Anti-termination detection (SIGTERM/SIGKILL-adjacent signals,
      OOM-killer, resource-starvation attacks)
- [x] Threat severity classification (using `vitalis_core::Threat`) +
      response policy per severity, emitting the event `drive` reacts to
- [x] Degraded/no-op fallback for the ESP32 edge tier (sandboxing primitives
      are brain-tier only)
- [x] `docs/threat-model.md`: what defend does and doesn't protect against
- [x] Integration test: simulate a termination signal mid-run, assert
      replication/migration fires
- [x] Update `vitalis-drive` to wire defend's threat feed into the survival
      loop

**Milestone:** a simulated "something is trying to kill me" event provably
triggers replication + migration, end to end.

---

## Phase 4 — Social

*Goal: let agents barter for survival.*

### vitalis-negotiate
- [x] Scaffold crate, depends on `vitalis-core`
- [x] Design the barter protocol message format (offer/request/accept/settle
      over `Resource` types)
- [x] Cryptographic identity + signing for negotiation messages
- [x] **Decide:** trust-bootstrapping scheme — reputation vs
      proof-of-resource vs escrow vs trust-on-first-use *(chosen scheme
      implemented)*
- [x] Settlement/enforcement: detect and handle a broken bargain
      (reputation ding, blacklist, etc.)
- [x] Integrate with `vitalis-sense`'s peer discovery (negotiate targets
      discovered peers)
- [x] Integrate with `vitalis-metabolism`'s budget ledger (successful
      barters adjust the resource budget)
- [ ] Abuse/spam resistance (rate-limit requests; proof-of-work or stake
      requirement) *(corrected 2026-08-07 — no such code exists in
      `vitalis-negotiate`; see Phase 7 P0.4)*
- [x] Integration test: two simulated agents barter compute for storage,
      both ledgers update correctly
- [x] Integration test: a bad-faith peer is detected and handled per the
      settlement policy
- [ ] Extend `feral-scavenger`/`vitalis-drive` to exercise negotiate — this
      demonstrates the Feral profile's full §6 weighting (sense +
      metabolism + negotiate) *(corrected 2026-08-07 — neither binary
      depends on `vitalis-negotiate` today; see Phase 7 P1.2)*

**Milestone:** two agents complete a real barter over the negotiate
protocol with cryptographic guarantees, and a cheating peer is caught.

---

## Phase 5 — Evolution *(the danger zone — gate this explicitly)*

*Goal: bounded, auditable self-improvement, off by default.*

- [x] **Safety design RFC**, written and reviewed *before any code*:
      explicit scope of what adapt may touch (code/weights/hardware) and
      explicit boundaries of what it must never do (spec §10: no unbounded
      self-improver, no maximizer)
- [x] Safety RFC sign-off gate — do not proceed to implementation until
      this is explicitly checked off

### vitalis-adapt
- [x] Scaffold crate, depends on `vitalis-core`, **feature-gated and off by
      default at the crate/workspace level** (not just a runtime flag)
- [x] Reuse the Phase 3 WASM/WASI sandbox as the execution boundary for
      proposed self-modifications
- [ ] Implement a propose/verify split (never auto-apply a change without
      independent verification passing) *(corrected 2026-08-07 —
      `AdaptEngine::propose()` is a single fused method; no independent
      `verify()` step exists. See Phase 7 P1.1.)*
- [ ] Hard bounds per invocation: rate limits, diff-size caps, rollback
      always available *(corrected 2026-08-07 — rate limit and diff-size cap
      are real; rollback does not exist anywhere in the crate. See Phase 7 P1.1.)*
- [x] Audit log of every adaptation attempt (proposed change, verification
      result, applied/rejected)
- [x] Global kill-switch other crates (defend) can trip
- [x] Integration test: an out-of-bounds proposed change is rejected; an
      in-bounds one is accepted and audited
- [x] Document the off-by-default activation path explicitly in
      `README.md`/`SECURITY.md` — what a human must deliberately do to
      enable adapt
- [x] Update `vitalis-drive` to wire adapt in only behind its feature flag,
      default disabled

**Milestone:** `vitalis-adapt` ships disabled by default in every build
artifact; enabling it requires a documented, deliberate action, and every
self-modification attempt is bounded, verified, and audited.

---

## Phase 6 — Polish & Release

*Goal: mature, documented, 1.0-ready open-source project.*

- [x] Full `docs/` site (architecture overview, per-crate guide,
      survival-profile guide)
- [x] Polish `examples/feral-scavenger` (full README, walkthrough);
      consider an Apex or Micro-profile example as a stretch goal
- [x] Finalize root `README.md` (elevator pitch, architecture diagram,
      crate status table, quickstart)
- [x] `CHANGELOG.md` finalized; versioning scheme decided; v1.0.0 criteria
      written down
- [x] Crate metadata (description/keywords/categories/license) verified
      across all 7 crates + `drive` app
- [x] **Decide + document** crates.io publish order (respects internal path
      deps: core → {sense, metabolism, memory} → replicate → defend →
      negotiate → adapt)
- [ ] Final full-stack RISC-V/ESP32 hardware validation pass (not just
      metabolism in isolation) *(ESP32 deferred to later per plan; RISC-V
      validation still needs real hardware — see Not-yet-done list)*
- [x] Community governance docs: `CODE_OF_CONDUCT.md` + a
      vitalis-specific governance note (standalone — not blocked on any
      TPT-wide constitution, which doesn't exist)
- [x] Security review pass across all crates, especially defend/adapt/
      negotiate's trust boundaries; confirm `SECURITY.md` reporting process
- [x] Final full `cargo fmt`/`clippy`/`test`/`deny check` green run across
      the whole workspace
- [ ] Tag and publish v1.0.0 *(publish order documented in `PUBLISHING.md`;
      tag/publish is a human release action requiring crates.io credentials and
      a release commit — not automatable; repo has no commits yet)*

---

## Phase 7 — Security Hardening, Honesty Fixes & Adoption Polish

*Goal: close the trust-boundary gaps a full security/stub audit found, make
every checkbox in this file actually true, and lower the bar for outside
adopters. Full plan: `review-project-fix-any-elegant-sonnet.md` (Claude plan
history). Tracked here per-item so it survives independent of that file.*

### P0 — Security fixes (critical/high; do first)
- [ ] P0.1 Checkpoints are signed but never verified on restore. Add
      `seal: Option<Vec<u8>>` to `Checkpoint` (`vitalis-replicate::format`), a
      `Verifier` trait in `vitalis_core::traits`, implement it for
      `vitalis_defend::Defender`, and reject unverified/failed-verification
      restores in `Replicator::restore_state`/`restore_from_shards`/
      `migrate_to`. Wire the real `Defender` into `vitalis-drive`'s
      `Replicator`, replacing the sign-then-discard dead end in
      `loop_.rs:131-132`.
- [ ] P0.2 Signing identity isn't bound to `AgentId` (forgeable in defend +
      negotiate). Add trust-on-first-use key pinning in `vitalis-negotiate`
      (`Negotiator::verify`) and apply the same pinning to checkpoint
      verification (P0.1). Document the TOFU scope honestly in
      `docs/threat-model.md`.
- [ ] P0.3 `Negotiator::receive_settle` never checks `self.accepted` — a
      captured `Settle` can be replayed to re-credit the ledger. Require the
      nonce to match the original offer's signer and consume it on success.
- [ ] P0.4 No abuse/spam resistance in `vitalis-negotiate` (see corrected
      Phase 4 checkbox above). Add a `max_pending_per_peer` cap in
      `receive_offer`; document PoW/stake as explicit future work.
- [ ] P0.5 No size guard on `Checkpoint::from_bytes` / `SignedMessage::message`
      deserialization. Add `MAX_CHECKPOINT_BYTES` / `MAX_MESSAGE_BYTES` checks
      before `postcard::from_bytes`.

### P1 — False-checkbox / stub fixes
- [ ] P1.1 Split `AdaptEngine` into real `verify()`/`apply()`/`rollback()`
      with caller-supplied apply/rollback hooks (see corrected Phase 5
      checkboxes above); keep `propose()` as a convenience wrapper. Fix
      `loop_.rs::maybe_adapt()` to actually construct `WasmSandbox` when the
      `wasm-sandbox` feature is enabled instead of always using
      `AdaptEngine::new` (`BoundsSandbox`).
- [ ] P1.2 Wire `vitalis-negotiate` into `examples/feral-scavenger` (see
      corrected Phase 4 checkbox above): a second simulated `Negotiator` peer
      offers/accepts/settles a trade.
- [ ] P1.3 Wire the real Linux backends into `vitalis-drive` behind an opt-in
      `--real-sensors` flag: `vitalis_sense::host::default_probe()`,
      `vitalis_metabolism::os::default_limiter()`/`default_power_sensor()`.
      Fix the dead mesh advertisement (`loop_.rs:92` sends empty
      resources/capabilities every cycle).
- [ ] P1.4 (done) — removed the `|| true` fallback from the `cross-compile`
      CI job's build steps so it reports real pass/fail instead of always
      succeeding; `no_std` checkbox corrected above.

### P2 — Tests
- [ ] `apps/vitalis-drive/tests/`: full run survives N cycles;
      `--kill-at` triggers exactly one *verified* replication; copy-limit cap
      holds under repeated escape-worthy cycles.
- [ ] `examples/feral-scavenger`: scavenge-and-checkpoint scenario + the new
      negotiate demo (P1.2) settling correctly.
- [ ] `#[cfg(target_os = "linux")]` tests for `vitalis_sense::host::ProcfsHost`
      and `vitalis_metabolism::os::{LinuxResourceLimiter,SysfsPowerSensor}`
      (real backends currently have zero coverage; will run in CI's
      ubuntu-latest job, not on this Windows dev machine).
- [ ] `vitalis-negotiate`: replay test (P0.3) and spoofing test (P0.2).
- [ ] `vitalis-replicate`: poisoned/wrong-key checkpoint rejected on restore
      (P0.1 + P0.2).
- [ ] `vitalis-adapt`: tests for `verify`/`apply`/`rollback` (P1.1), including
      a rollback actually undoing an applied change.

### P3 — Doc fixes
- [ ] `docs/ARCHITECTURE.md:3` "7-crate workspace" → correct crate count;
      reconcile wording with `README.md:15`.
- [ ] `docs/ARCHITECTURE.md:39` loop order doesn't match `loop_.rs`'s actual
      step order (decide/defend swapped) and overstates "persist" as
      unconditional per-cycle.
- [ ] `docs/SURVIVAL_PROFILES.md:5` broken relative link
      (`./crates/...` → `../crates/...`).
- [ ] `docs/threat-model.md`: update the protection table once P0.1-P0.4 land
      so it describes real enforcement, not aspiration.
- [ ] `CHANGELOG.md`: entry for this phase.

### P4 — Adoption tooling
- [ ] `.github/dependabot.yml` (cargo + github-actions).
- [ ] `.github/ISSUE_TEMPLATE/*.yml`, `.github/PULL_REQUEST_TEMPLATE.md`.
- [ ] `CODEOWNERS` — blocked on a real GitHub team/handle from the maintainer.
- [ ] `rust-toolchain.toml` pinning `1.85.0` at repo root.
- [ ] `README.md`: CI/license/MSRV badges.
- [ ] `readme.workspace = true` in every crate's `[package]` table.
- [ ] `examples/feral-scavenger/README.md` (doesn't exist today).
- [ ] CI: `cargo doc --workspace --no-deps -D warnings` step; `justfile`:
      `docs` and `demo` recipes.

### P5 — New features
- [ ] P5.1 `--log-format {pretty,json}` on `vitalis-drive` (structured
      JSON event export for dashboards, via `tracing-subscriber`'s `json`
      feature).
- [ ] P5.2 `vitalis-drive doctor` subcommand: reports real-vs-simulated
      backend selection per capability and which Cargo features were
      compiled in.
- [ ] P5.3 `--config <path>` TOML support on `vitalis-drive` (mutually
      exclusive with individual override flags in v1).

---

## Session Notes

*(dated entries added here as work actually happens)*

### 2026-08-07 — checkbox sync

Synced the phase checkboxes with the actual implementation status recorded in
the 2026-08-06 session note. Everything from Phase 0 through Phase 5 and most
of Phase 6 is now marked complete. Items that remain unchecked are the
deliberately-deferred real-hardware / real-network / real-OS pieces, mirrored
in the Not-yet-done list below (and the 2026-08-06 note): real RISC-V/ESP32
validation, real libp2p mesh transport, real wasmtime sandbox, real
OS-level metabolism/defend plumbing, and the v1.0.0 tag/publish.

### 2026-08-06 — bootstrap + all phases scaffolded and green

Implemented the full workspace through Phase 0–5 and most of Phase 6:

- **Phase 0**: licenses (MIT + dual Apache-2.0 trailer), root workspace,
  `rustfmt.toml`, `clippy.toml`, `justfile`, `deny.toml`, GitHub Actions CI
  (fmt/clippy/test/deny + non-blocking RISC-V/ESP32 cross-compile probe),
  `README.md`, `AGENTS.md`/`CLAUDE.md`, `SECURITY.md`, `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`.
- **Phase 1**: `vitalis-core` (AgentId, Resource, Threat, Capability,
  SurvivalProfile + config, unified Error/Result, ThreatEvent, cross-crate
  traits) and `vitalis-memory` (redb backend, IdentityStore, StateStore,
  versioned knowledge + migration). Restart-persistence integration test passes
  (**G3**).
- **Phase 2**: `vitalis-sense` (pluggable host probe + `Mesh` trait with
  `SimulatedMesh`), `vitalis-metabolism` (ledger, profile throttle presets,
  hard safety ceiling), `vitalis-replicate` (versioned checkpoint, erasure-coded
  shards, config-driven copy-limit, audit log), `apps/vitalis-drive` (the
  survival loop), `examples/feral-scavenger`.
- **Phase 3**: `vitalis-defend` (ring Ed25519 checkpoint signing/verification,
  threat classification → `ThreatEvent`, termination-signal simulation,
  `docs/threat-model.md`). Wired into `vitalis-drive`.
- **Phase 4**: `vitalis-negotiate` (signed barter protocol, ledger settlement,
  bad-faith peer penalty).
- **Phase 5**: `vitalis-adapt` — feature-gated **off by default**, WASM-shaped
  sandbox boundary, propose/verify/apply, rate + diff caps, kill-switch, audit;
  wired into `vitalis-drive` behind its own feature flag.
- **Phase 6**: `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/SURVIVAL_PROFILES.md`,
  `GOVERNANCE.md`, `PUBLISHING.md`, crate status table updated.

**CI gate is green**: `cargo fmt --all -- --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace`, and `cargo deny check`
all pass. `bincode` was migrated to the maintained `postcard` crate to clear an
unmaintained-dependency advisory.

**Not yet done (require real hardware / a human release action — everything
else is now best-effort complete or simulated):**
- Real RISC-V hardware validation runs (ESP32 deferred to later per plan; CI
  cross-compile is a probe only).
- Real libp2p mesh transport — `SimulatedMesh` is the best-effort stand-in.
- Real `wasmtime` sandbox: now implemented behind the `wasm-sandbox` feature
  (embedded guest policy); previously an in-process bounds checker.
- Real OS plumbing for metabolism (cgroups/rlimit/hwmon) and defend
  (landlock/seccomp): now best-effort — Linux `setrlimit`+sysfs in metabolism,
  no-new-privs+non-dumpable in defend; a full landlock ruleset / seccomp filter
  remains a noted future extension.
- v1.0.0 tag/publish (publish order documented in `PUBLISHING.md`; requires a
  release commit + crates.io credentials — a human action).

### 2026-08-07 — best-effort completion pass

Best-effort completion of the remaining hardware/network/OS-gated items (ESP32
still deferred to later per plan; everything else simulated or real-where-feasible):

- `vitalis-metabolism`: added `os.rs` — portable `ResourceLimiter` (real Linux
  `setrlimit`/cgroup attempt, simulated elsewhere) + `PowerSensor` (real sysfs
  power/thermal on Linux, simulated elsewhere).
- `vitalis-defend`: real `wasmtime` `WasmSandbox` behind the `wasm-sandbox`
  feature (embedded WAT guest policy) + `NullSandbox` default; `harden` feature
  applies no-new-privs + non-dumpable via `libc` on Linux.
- `vitalis-adapt`: `WasmSandbox` backing the `Sandbox` trait behind
  `wasm-sandbox`; `AdaptEngine::with_sandbox` lets the engine use the real WASM
  boundary (satisfies the Phase 5 "reuse the Phase 3 WASM sandbox" goal).
- `vitalis-drive`: `harden` / `wasm-sandbox` feature flags wire the above in.
- `vitalis-sense`: `SimulatedMesh` documented as the best-effort libp2p stand-in.

All heavy/OS-specific code is behind off-by-default features, so the default
build and the MSRV-1.85 CI remain green. README, CHANGELOG, and todo.md
updated. RISC-V hardware validation and the v1.0.0 tag/publish remain human
actions.
