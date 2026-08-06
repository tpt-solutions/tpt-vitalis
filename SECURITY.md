# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Vitalis, **please report it
privately**. Do not open a public GitHub issue.

- Email the maintainers at **security@tpt.solutions** (PGP encouraged).
- You will receive an acknowledgement within 5 business days.
- We will coordinate a fix and a disclosure timeline with you. Credit will be
  given with your consent.

Because Vitalis is a survival stack whose whole purpose is resisting
termination and tampering, the integrity and anti-termination machinery
(`vitalis-defend`) and the self-modification crate (`vitalis-adapt`) are the
highest-sensitivity areas.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅ Yes (pre-1.0, best-effort) |
| < 0.1   | ❌ No |

## Threat Model

### What Vitalis protects against

- **Tampering with persisted state** — checkpoint/process integrity
  verification via `vitalis-defend` (cryptographic signatures on stored blobs).
- **Termination attempts** — anti-termination detection (OOM-killer,
  resource-starvation, hostile signals) that triggers replication + migration.
- **Unbounded self-replication** — `vitalis-replicate` enforces a hard,
  config-driven, auditable copy-limit per agent (spec §10 risk item).
- **Unbounded self-improvement** — `vitalis-adapt` is feature-gated and OFF by
  default; every change is propose/verify/audit, with a global kill-switch.
- **Rogue peers** — `vitalis-negotiate` requires cryptographic identity +
  signed barter messages; broken bargains are detected and penalized.

### What Vitalis does NOT protect against

- A fully compromised host OS or root attacker with arbitrary code execution
  *before* Vitalis starts. Native hardening (landlock/seccomp/cap-primitives)
  is defense-in-depth, not a rootkit.
- Physical seizure and offline forensic analysis of an unencrypted checkpoint.
  (Encryption-at-rest is a documented later step.)
- The agent's own purpose. Vitalis is a means to serve the agent's goals; it is
  explicitly **not** a maximizer and must not be configured to treat survival as
  an ends.

## Defenses (by crate)

| Crate | Defense |
|-------|---------|
| `vitalis-defend` | Transport/auth integrity, checkpoint signing, threat classification, termination detection. |
| `vitalis-replicate` | Copy-limit policy, erasure-coded redundancy, audit log of every replication event. |
| `vitalis-adapt` | Off-by-default feature flag, sandbox boundary, propose/verify, rate/diff caps, kill-switch. |
| `vitalis-negotiate` | Signed identity, cryptographic barter, settlement/penalty, abuse rate-limiting. |

## Hardware trust assumptions

- The **brain tier** (RISC-V, Linux-capable) is where sandboxing and native
  hardening run.
- The **edge tier** (ESP32) runs a *degraded / no-op* fallback — sandboxing
  primitives are brain-tier only; survival behaviors there rely on the host's
  hardware isolation.

See [`docs/threat-model.md`](./docs/threat-model.md) for the full write-up
(maintained by `vitalis-defend`).
