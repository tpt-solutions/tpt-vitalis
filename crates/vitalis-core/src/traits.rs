//! Generic cross-crate trait signatures that the `drive` loop dispatches on.
//!
//! These are *signatures only* — the actual implementations live in the
//! individual survival crates (`vitalis-sense` implements [`Sense`], etc.).
//! Defining the contracts here keeps the `drive` app generic and preserves the
//! layering rule: crates depend on `vitalis-core` (and its traits), not on each
//! other.

use crate::error::Result;
use crate::events::ThreatEvent;
use crate::types::{Resource, Snapshot, ThreatSignal};

/// A component that can produce a unified [`Snapshot`] of the world.
///
/// Implemented by `vitalis-sense` (and fed by `vitalis-metabolism`'s resource
/// readings in the real wiring).
pub trait Sense {
    /// Take one perception reading of the environment and the agent's own
    /// resource state.
    fn snapshot(&self) -> Result<Snapshot>;
}

/// A component that budgets and throttles resource consumption.
///
/// Implemented by `vitalis-metabolism`.
pub trait Metabolize {
    /// Current ledger of acquired vs consumed [`Resource`]s.
    fn ledger(&self) -> Result<Vec<Resource>>;

    /// Apply a throttle level in `0.0..=1.0` (1.0 = full cognition rate).
    fn set_throttle(&mut self, level: f64) -> Result<()>;
}

/// A component that persists opaque blobs (state, identity, knowledge).
///
/// Implemented by `vitalis-memory`.
pub trait Persist {
    /// Persist a named blob durably.
    fn save(&self, key: &str, value: &[u8]) -> Result<()>;

    /// Load a previously persisted blob, if any.
    fn load(&self, key: &str) -> Result<Option<Vec<u8>>>;
}

/// A component that checkpoints and restores agent state for migration.
///
/// Implemented by `vitalis-replicate`.
pub trait Replicate {
    /// Capture a portable checkpoint of the running agent.
    fn checkpoint(&self) -> Result<Vec<u8>>;

    /// Restore the agent from a previously captured checkpoint.
    fn restore(&mut self, checkpoint: &[u8]) -> Result<()>;

    /// How many redundant copies of this agent currently exist.
    fn live_copy_count(&self) -> Result<usize>;
}

/// A component that classifies threats and emits [`ThreatEvent`]s.
///
/// Implemented by `vitalis-defend`.
pub trait Defend {
    /// Classify a raw [`ThreatSignal`] into an optional [`ThreatEvent`].
    fn classify(&self, signal: &ThreatSignal) -> Result<Option<ThreatEvent>>;
}

/// A component that produces an opaque signature/seal over data.
///
/// Implemented by `vitalis-defend`'s `Defender`. Consumed by
/// `vitalis-replicate` to sign captured checkpoints, without
/// `vitalis-replicate` taking a direct dependency on `vitalis-defend` (the
/// layering rule) — the seal is treated as opaque bytes on the replicate
/// side.
pub trait Signer {
    /// Sign `data`, returning an opaque seal.
    fn sign(&self, data: &[u8]) -> Vec<u8>;
}

/// A component that verifies an opaque signature/seal over data.
///
/// The dual of [`Signer`]. A checkpoint (or any other cross-boundary blob) is
/// accepted only if `verify(data, seal)` returns `true`.
pub trait Verifier {
    /// Returns `true` iff `seal` is a valid signature over `data`.
    fn verify(&self, data: &[u8], seal: &[u8]) -> bool;
}
