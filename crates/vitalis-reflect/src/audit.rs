//! Audit log of reflection activity: predictions made and how they scored.
//!
//! "Predicted what, when, how close." The log is append-only and in-memory
//! (itself checkpointed via `vitalis-memory` by the `drive` app), giving a
//! record of an agent's self-prediction accuracy over time. Mirrors
//! `vitalis-replicate`'s `ReplicationAudit` shape.

use std::sync::Mutex;

use crate::evaluate::EvaluationRecord;
use crate::predict::PredictionRecord;

/// One audit entry: a prediction and (once its target cycle arrives) how it
/// scored.
#[derive(Debug, Clone)]
pub struct ReflectEntry {
    /// The prediction that was made.
    pub prediction: PredictionRecord,
    /// How the prediction scored, once resolved. `None` while pending.
    pub evaluation: Option<Vec<EvaluationRecord>>,
}

/// Append-only audit log of reflection activity.
#[derive(Default)]
pub struct ReflectAudit {
    entries: Mutex<Vec<ReflectEntry>>,
}

impl ReflectAudit {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry (a prediction, optionally already evaluated).
    pub fn record(&self, entry: ReflectEntry) {
        if let Ok(mut g) = self.entries.lock() {
            g.push(entry);
        }
    }

    /// All entries so far (oldest first).
    pub fn entries(&self) -> Vec<ReflectEntry> {
        self.entries.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Number of recorded entries.
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
