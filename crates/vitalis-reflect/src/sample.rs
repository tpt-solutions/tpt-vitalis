//! What `vitalis-drive` hands to the [`Reflector`](crate::Reflector) each cycle.

use serde::{Deserialize, Serialize};
use vitalis_core::AgentId;

/// A single peer's reputation snapshot, sampled for prediction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PeerSample {
    /// The peer's identity.
    pub peer: AgentId,
    /// The peer's current trust score (0..=1).
    pub trust: f64,
    /// Whether the peer is currently blacklisted.
    pub blacklisted: bool,
}

/// Ground-truth of how a peer actually behaved, supplied where known so a
/// peer-outcome prediction can be evaluated. Empty in the main loop
/// (observational — the drive never knows a peer's true intent), and filled in
/// by the `feral-scavenger` demo where outcomes are scripted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PeerOutcomeActual {
    /// The peer this outcome is about.
    pub peer: AgentId,
    /// Did the peer honor the bargain? (`true` = honored, `false` = broke).
    pub honored: bool,
}

/// One reflection cycle's input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectSample {
    /// Monotonic cycle index (supplied by the drive loop).
    pub cycle: u64,
    /// Current energy (joules) — the quantity `predict` extrapolates forward.
    pub energy: f64,
    /// Energy capacity (joules), kept for normalization / sanity.
    pub energy_capacity: f64,
    /// The threat event observed this cycle, if any.
    pub threat: Option<vitalis_core::ThreatEvent>,
    /// Peer reputation samples for peer-outcome prediction.
    pub peers: Vec<PeerSample>,
    /// Ground-truth peer outcomes for evaluating peer-outcome predictions.
    pub peer_outcomes: Vec<PeerOutcomeActual>,
}

impl ReflectSample {
    /// Build a minimal sample with no peers / threats (the common drive case).
    pub fn new(cycle: u64, energy: f64, energy_capacity: f64) -> Self {
        Self {
            cycle,
            energy,
            energy_capacity,
            threat: None,
            peers: Vec::new(),
            peer_outcomes: Vec::new(),
        }
    }
}
