# Survival Profiles

Vitalis is the *same* stack, weighted differently per agent class (spec §6).
Each profile biases the `drive` loop's cycle via
[`SurvivalProfileConfig`](./crates/vitalis-core/src/types.rs) and selects
sensible defaults in the capability crates.

## Apex

- **Character:** planetary scale, massive redundancy, designs its own silicon.
  Wasteful with energy.
- **Leans on:** `adapt` (1.0), `replicate` (1.0); `metabolism` low (0.2),
  `negotiate` low (0.3).
- **Tuning:** `ThrottleProfile` is wasteful (only throttles when nearly empty);
  replication copy-limit is set high to keep many redundant copies.

## Feral

- **Character:** scavenges spare compute, sips power, barters to stay alive.
- **Leans on:** `sense` (1.0), `metabolism` (1.0), `negotiate` (1.0);
  `replicate` (0.6), `adapt` low (0.2).
- **Tuning:** `ThrottleProfile` is frugal (throttles early and hard); the
  `feral-scavenger` example demonstrates scavenging a resource-rich peer on the
  simulated mesh.

## Micro / Wilds

- **Character:** community companion (ORACLE). Survives on solar; backs itself up
  so the node's knowledge persists.
- **Leans on:** `memory` (1.0), `replicate` (1.0), `metabolism` (1.0, solar-
  aware); `sense` (0.6), `negotiate` (0.3).
- **Tuning:** `ThrottleProfile` is solar-aware/moderate; the energy fraction
  drives a smooth cognition curve rather than a hard cutoff.

## Selecting a profile

```sh
cargo run -p vitalis-drive -- --profile feral
cargo run -p vitalis-drive -- --profile apex
cargo run -p vitalis-drive -- --profile micro-wilds
```

The `drive` app reads the profile at startup and feeds it into the metabolism
throttle presets and (when the `adapt` feature is enabled) the adaptation
engine.
