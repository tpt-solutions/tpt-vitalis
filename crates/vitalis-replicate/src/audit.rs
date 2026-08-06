//! Audit log of every replication / migration event.
//!
//! "Who, when, why, how many copies exist." The log is append-only and
//! in-memory (it is itself checkpointed via `vitalis-memory` by the `drive`
//! app), giving a tamper-evident-ish record of an agent's reproductive life.

use std::sync::Mutex;
use vitalis_core::AgentId;

/// Why a replication/migration happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationReason {
    /// Routine checkpoint for persistence.
    Checkpoint,
    /// Proactive redundancy (keeping N copies alive).
    Survival,
    /// A "dying hardware" trigger fired.
    DyingHardware,
    /// Live migration to another node.
    Migration,
    /// Manual / operator-requested.
    Manual,
}

impl ReplicationReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReplicationReason::Checkpoint => "checkpoint",
            ReplicationReason::Survival => "survival",
            ReplicationReason::DyingHardware => "dying-hardware",
            ReplicationReason::Migration => "migration",
            ReplicationReason::Manual => "manual",
        }
    }
}

/// One audit entry.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub agent_id: AgentId,
    pub reason: ReplicationReason,
    pub timestamp_secs: u64,
    pub live_copies_after: usize,
    pub detail: String,
}

/// Append-only audit log of replication events.
#[derive(Default)]
pub struct ReplicationAudit {
    entries: Mutex<Vec<AuditEntry>>,
}

impl ReplicationAudit {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry.
    pub fn record(&self, entry: AuditEntry) {
        if let Ok(mut g) = self.entries.lock() {
            g.push(entry);
        }
    }

    /// All entries so far (oldest first).
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Number of recorded events.
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
