//! # vitalis-defend
//!
//! The immune system. It provides:
//!
//! - **Integrity** — Ed25519 signing/verification of persisted checkpoints so
//!   tampering is detectable (`ring` backend).
//! - **Threat classification** — turns low-level [`ThreatSignal`]s into
//!   [`ThreatEvent`]s the `drive` loop reacts to (per the layering rule,
//!   `defend` reaches `replicate` *only* through these events — never a direct
//!   dependency).
//! - **Anti-termination** — detects termination-style signals (OOM, SIGKILL,
//!   SIGTERM) and classifies them as critical, which triggers replication +
//!   migration downstream.
//!
//! On the ESP32 edge tier, the native sandboxing primitives (landlock/seccomp)
//! are unavailable; the crate degrades to the signature + classification logic
//! only, which is still meaningful for checkpoint integrity.
//!
//! Layering: depends on `vitalis-core` only.

pub mod classify;
pub mod harden;
pub mod integrity;
pub mod signal;
pub mod wasm;

pub use classify::{Defender, ThreatClassifier};
pub use harden::{apply_hardening, HardeningMode, HardeningReport};
pub use integrity::{verify_checkpoint, CheckpointSeal, KeyPair};
pub use signal::{simulate_termination, TerminationSignal};
pub use wasm::{CapabilitySandbox, NullSandbox};

#[cfg(feature = "wasm-sandbox")]
pub use wasm::WasmSandbox;
