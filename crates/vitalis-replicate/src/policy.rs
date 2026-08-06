//! Config-driven, auditable copy-limit policy (spec §10).
//!
//! Unbounded replication is a plague. This policy enforces a hard cap on the
//! number of *concurrent redundant copies* of an agent and rejects any extra
//! replication attempt — the decision is pure and deterministic so it can be
//! tested and audited.

use std::sync::atomic::{AtomicUsize, Ordering};
use vitalis_core::{Error, Result};

/// A hard cap on concurrent redundant copies of one agent.
#[derive(Debug)]
pub struct CopyLimitPolicy {
    max_copies: usize,
    live: AtomicUsize,
}

impl CopyLimitPolicy {
    pub fn new(max_copies: usize) -> Self {
        Self {
            max_copies,
            live: AtomicUsize::new(0),
        }
    }

    /// The configured maximum number of concurrent copies.
    pub fn max_copies(&self) -> usize {
        self.max_copies
    }

    /// Current number of live copies.
    pub fn live_copies(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }

    /// Attempt to authorize one more copy. Errors if it would exceed the cap.
    pub fn authorize_copy(&self) -> Result<()> {
        let cur = self.live.load(Ordering::SeqCst);
        if cur >= self.max_copies {
            return Err(Error::Replication(format!(
                "copy-limit reached: {cur}/{max} concurrent copies",
                max = self.max_copies
            )));
        }
        self.live.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Record that a copy has been retired (e.g. merged or killed).
    pub fn retire_copy(&self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}
