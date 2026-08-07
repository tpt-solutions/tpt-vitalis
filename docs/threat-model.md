# Vitalis Threat Model

> Maintained by `vitalis-defend`. This is the authoritative description of what
> the immune system does and does **not** protect against.

## Scope

`vitalis-defend` is the agent's immune response. It operates at two layers:

1. **Integrity** — every persisted checkpoint/blob is signed with an Ed25519
   key (`ring` backend). On load, the signature is verified. A mismatch means
   the persisted state was tampered with or corrupted and must not be trusted.
2. **Threat classification** — low-level `ThreatSignal`s (termination, OOM,
   starvation, tamper, hostile peer) are classified into `ThreatEvent`s that
   the `drive` loop observes and reacts to.

Crucially, `defend` never depends directly on `replicate`. The immune response
fires by **emitting an event**; the `drive` app decides to replicate/migrate.
This keeps the immune system decoupled from reproduction.

## What it protects against

| Threat | Detection | Response |
|--------|-----------|----------|
| Checkpoint tampering | Signature verification on load; the `Replicator` rejects any checkpoint whose seal is missing or fails verification, and a `BoundVerifier` binds the seal to the expected `AgentId` via trust-on-first-use (TOFU) key pinning (P0.1 + P0.2) | Reject blob; force re-checkpoint from a known-good copy |
| SIGTERM / SIGKILL | Signal simulation / OS hook (brain tier) | Critical `ThreatEvent` → replicate + migrate |
| OOM-killer | cgroup event / signal | Critical `ThreatEvent` → replicate + migrate |
| Resource starvation | Metabolism ledger below safe floor | Warning → Critical escalation → replicate |
| Hostile peer (spoof/broken bargain) | Negotiation-layer signal; forged identities are rejected once a peer's key is pinned (TOFU, P0.2), captured `Settle`s cannot be replayed (P0.3), and a per-peer pending-offer cap bounds abuse (P0.4) | Warning; penalize peer (see `vitalis-negotiate`) |

## What it does NOT protect against

- **A fully compromised host / root attacker before Vitalis starts.** Native
  hardening (landlock/seccomp/cap-primitives) is defense-in-depth on the brain
  tier but cannot defend against a rootkit with arbitrary code execution
  beforehand.
- **Physical seizure + offline forensic analysis** of an *unencrypted*
  checkpoint. At-rest encryption is a documented later addition.
- **The agent's own purpose.** Vitalis serves the agent's goals; it is
  explicitly **not** a maximizer (spec §10). `vitalis-adapt` is the only
  self-modifying crate and is feature-gated off by default.

## ESP32 edge tier

The ESP32 edge tier runs a **degraded / no-op** fallback: the Ed25519 signing
and threat-classification logic still run (they are pure compute), but the
native sandboxing primitives are brain-tier only. Survival there relies on the
host MCU's hardware isolation.

## Auditing

Replication events are logged by `vitalis-replicate`'s audit log; integrity
failures should be surfaced to operators via the structured logging the `drive`
loop emits. Neither log should be the *sole* source of truth — checkpoint
signatures are the cryptographic guarantee.
