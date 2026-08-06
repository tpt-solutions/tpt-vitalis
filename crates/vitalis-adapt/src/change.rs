//! A proposed self-modification.

use serde::{Deserialize, Serialize};

/// What aspect of the agent a change touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    /// Agent code / logic.
    Code,
    /// Model weights / parameters.
    Weights,
    /// Hardware configuration.
    Hardware,
}

/// A proposed change to the agent. The `diff` is opaque to `adapt` — the
/// sandbox/verifier is what decides whether it is safe to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedChange {
    pub id: u64,
    pub kind: ChangeKind,
    pub diff: Vec<u8>,
    pub description: String,
}

impl ProposedChange {
    pub fn new(id: u64, kind: ChangeKind, diff: Vec<u8>, description: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            diff,
            description: description.into(),
        }
    }

    /// The diff size in bytes — the primary bound checked by the sandbox.
    pub fn diff_size(&self) -> usize {
        self.diff.len()
    }
}
