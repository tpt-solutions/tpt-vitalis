//! The resource ledger: acquired vs consumed per [`ResourceKind`].

use std::collections::HashMap;
use vitalis_core::{Error, Resource, ResourceKind, Result};

/// Tracks how much of each resource kind has been acquired and consumed.
#[derive(Debug, Clone, Default)]
pub struct ResourceLedger {
    acquired: HashMap<ResourceKind, f64>,
    consumed: HashMap<ResourceKind, f64>,
}

impl ResourceLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an acquisition (intake) of `r`.
    pub fn acquire(&mut self, r: &Resource) -> Result<()> {
        if !r.is_valid() {
            return Err(Error::Resource(format!(
                "refusing to acquire invalid resource: {r:?}"
            )));
        }
        *self.acquired.entry(r.kind()).or_insert(0.0) += r.quantity();
        Ok(())
    }

    /// Record a consumption of `r`. Errors if it would drive the balance
    /// negative (we never silently go into debt).
    pub fn consume(&mut self, r: &Resource) -> Result<()> {
        if !r.is_valid() {
            return Err(Error::Resource("cannot consume invalid resource".into()));
        }
        let balance = self.balance(r.kind());
        let next = balance - r.quantity();
        if next < 0.0 {
            return Err(Error::Resource(format!(
                "insufficient {:?}: have {balance}, need {}",
                r.kind(),
                r.quantity()
            )));
        }
        *self.consumed.entry(r.kind()).or_insert(0.0) += r.quantity();
        Ok(())
    }

    /// Net balance (acquired - consumed) of a kind.
    pub fn balance(&self, kind: ResourceKind) -> f64 {
        self.acquired.get(&kind).copied().unwrap_or(0.0)
            - self.consumed.get(&kind).copied().unwrap_or(0.0)
    }

    /// Net balance of every kind that has ever been touched, as [`Resource`]s
    /// in their canonical units.
    pub fn balances(&self) -> Vec<Resource> {
        let mut kinds: Vec<_> = self.acquired.keys().copied().collect();
        kinds.extend(self.consumed.keys().copied());
        kinds.sort();
        kinds.dedup();
        kinds
            .into_iter()
            .map(|k| Resource::new(k, self.balance(k), k.unit()))
            .collect()
    }
}
