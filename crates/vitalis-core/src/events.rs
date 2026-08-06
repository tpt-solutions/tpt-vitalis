//! Cross-crate events. These are the decoupling mechanism that keeps the
//! survival crates independent of one another (see the layering rule in
//! `AGENTS.md`).

use crate::types::{Severity, Threat};
use serde::{Deserialize, Serialize};

/// An event emitted when a threat is classified and the `drive` loop must
/// react.
///
/// `vitalis-defend` is the canonical producer of `ThreatEvent`s. `vitalis-replicate`
/// is *never* a direct dependency of `defend`; instead the `drive` app observes
/// `ThreatEvent`s (and signals from sense/metabolism) and decides to call into
/// `replicate`. This is the immune-response wiring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreatEvent {
    /// The classified threat.
    pub threat: Threat,
    /// Wall-clock-ish timestamp (seconds since epoch, supplied by producer).
    pub timestamp_secs: u64,
}

impl ThreatEvent {
    pub fn new(threat: Threat, timestamp_secs: u64) -> Self {
        Self {
            threat,
            timestamp_secs,
        }
    }

    /// Convenience: the severity of the embedded threat.
    pub fn severity(&self) -> Severity {
        self.threat.severity()
    }

    /// Whether this event is serious enough to trigger replication + migration.
    pub fn is_escape_worthy(&self) -> bool {
        matches!(self.severity(), Severity::Critical)
    }
}
