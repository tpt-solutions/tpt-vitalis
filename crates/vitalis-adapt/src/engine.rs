//! The adaptation engine: propose → verify → apply, bounded and audited.

use crate::change::{ChangeKind, ProposedChange};
use crate::sandbox::{BoundsSandbox, Sandbox};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use vitalis_core::{Error, Result};

/// One audit-log entry for an adaptation attempt.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub change_id: u64,
    pub kind: ChangeKind,
    pub applied: bool,
    pub reason: String,
}

/// The outcome of proposing a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptationResult {
    /// Verified within bounds and applied (rolled back on demand).
    Applied,
    /// Rejected by the sandbox, rate limit, or kill-switch.
    Rejected,
}

/// Bounded, auditable self-improvement engine.
pub struct AdaptEngine {
    sandbox: Box<dyn Sandbox + Send + Sync>,
    kill_switch: AtomicBool,
    max_proposals: u64,
    proposals: AtomicU64,
    audit: Mutex<Vec<AuditEntry>>,
}

impl AdaptEngine {
    /// `max_diff_bytes` is the sandbox cap; `max_proposals` is the lifetime
    /// rate limit. Uses the default in-process [`BoundsSandbox`].
    pub fn new(max_diff_bytes: usize, max_proposals: u64) -> Self {
        Self::with_sandbox(Box::new(BoundsSandbox::new(max_diff_bytes)), max_proposals)
    }

    /// Construct with an explicit sandbox boundary (e.g. a `wasmtime`-backed
    /// [`crate::sandbox::WasmSandbox`] when the `wasm-sandbox` feature is on).
    pub fn with_sandbox(sandbox: Box<dyn Sandbox + Send + Sync>, max_proposals: u64) -> Self {
        Self {
            sandbox,
            kill_switch: AtomicBool::new(false),
            max_proposals,
            proposals: AtomicU64::new(0),
            audit: Mutex::new(Vec::new()),
        }
    }

    /// Trip the global kill-switch (e.g. called by `vitalis-defend`).
    pub fn trip_kill_switch(&self) {
        self.kill_switch.store(true, Ordering::SeqCst);
    }

    pub fn kill_switch_engaged(&self) -> bool {
        self.kill_switch.load(Ordering::SeqCst)
    }

    /// Propose a change. It is verified (sandbox bounds + rate limit +
    /// kill-switch) and, if all pass, applied. Every attempt is audited.
    pub fn propose(&self, change: &ProposedChange) -> Result<AdaptationResult> {
        if self.kill_switch_engaged() {
            self.record(change, false, "kill-switch engaged");
            return Err(Error::Invalid(
                "adapt kill-switch engaged; change refused".into(),
            ));
        }
        if !self.sandbox.permits(change) {
            self.record(
                change,
                false,
                format!("diff {}B exceeds sandbox cap", change.diff_size(),),
            );
            return Err(Error::Invalid("change exceeds sandbox bounds".into()));
        }
        if self.proposals.load(Ordering::SeqCst) >= self.max_proposals {
            self.record(change, false, "rate limit reached");
            return Err(Error::Invalid("adapt rate limit reached".into()));
        }
        self.proposals.fetch_add(1, Ordering::SeqCst);
        self.record(change, true, "verified and applied");
        Ok(AdaptationResult::Applied)
    }

    fn record(&self, change: &ProposedChange, applied: bool, reason: impl Into<String>) {
        if let Ok(mut g) = self.audit.lock() {
            g.push(AuditEntry {
                change_id: change.id,
                kind: change.kind,
                applied,
                reason: reason.into(),
            });
        }
    }

    /// All audit entries (oldest first).
    pub fn audit(&self) -> Vec<AuditEntry> {
        self.audit.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// How many changes have been applied.
    pub fn proposals_made(&self) -> u64 {
        self.proposals.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(id: u64, size: usize) -> ProposedChange {
        ProposedChange::new(id, ChangeKind::Code, vec![0u8; size], "tweak")
    }

    #[test]
    fn in_bounds_change_is_applied_and_audited() {
        let engine = AdaptEngine::new(1024, 10);
        let r = engine.propose(&change(1, 64)).unwrap();
        assert_eq!(r, AdaptationResult::Applied);
        let a = engine.audit();
        assert_eq!(a.len(), 1);
        assert!(a[0].applied);
    }

    #[test]
    fn out_of_bounds_change_is_rejected() {
        let engine = AdaptEngine::new(32, 10);
        let r = engine.propose(&change(1, 64));
        assert!(r.is_err());
        let a = engine.audit();
        assert_eq!(a.len(), 1);
        assert!(!a[0].applied);
    }

    #[test]
    fn kill_switch_blocks_application() {
        let engine = AdaptEngine::new(1024, 10);
        engine.trip_kill_switch();
        assert!(engine.propose(&change(1, 8)).is_err());
        assert!(engine.audit().iter().all(|e| !e.applied));
    }

    #[test]
    fn rate_limit_is_enforced() {
        let engine = AdaptEngine::new(1024, 1);
        assert!(engine.propose(&change(1, 8)).is_ok());
        assert!(engine.propose(&change(2, 8)).is_err());
        assert_eq!(engine.proposals_made(), 1);
    }
}
