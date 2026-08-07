//! The replicator: checkpoint capture/restore, erasure-coded sharding,
//! migration, copy-limit enforcement, and audit.

use crate::audit::{AuditEntry, ReplicationAudit, ReplicationReason};
use crate::format::Checkpoint;
use crate::policy::CopyLimitPolicy;
use crate::shards::{decode_shards, encode_shards};
use std::time::{SystemTime, UNIX_EPOCH};
use vitalis_core::traits::{Replicate, Signer, Verifier};
use vitalis_core::{AgentId, Error, Result};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Orchestrates an agent's reproductive lifecycle.
pub struct Replicator {
    agent_id: AgentId,
    current: Option<Checkpoint>,
    policy: CopyLimitPolicy,
    audit: ReplicationAudit,
    data_shards: usize,
    parity_shards: usize,
    /// Optional integrity backend: signs captured checkpoints. `None` means
    /// checkpoints are left unsigned (only safe for tests / trusted stores).
    signer: Option<Box<dyn Signer + Send + Sync>>,
    /// Optional integrity backend: verifies checkpoint seals on restore. When
    /// set, an unsigned or failed-verification checkpoint is *rejected* — this
    /// is the enforcement half of the signing contract (see P0.1).
    verifier: Option<Box<dyn Verifier + Send + Sync>>,
}

impl Replicator {
    /// `max_copies` is the hard cap on concurrent redundant copies; `data_shards`
    /// / `parity_shards` configure erasure coding (you may lose `parity_shards`).
    pub fn new(
        agent_id: AgentId,
        max_copies: usize,
        data_shards: usize,
        parity_shards: usize,
    ) -> Self {
        Self {
            agent_id,
            current: None,
            policy: CopyLimitPolicy::new(max_copies),
            audit: ReplicationAudit::new(),
            data_shards,
            parity_shards,
            signer: None,
            verifier: None,
        }
    }

    /// Attach a signing backend. Captured checkpoints will carry a seal over
    /// their signable bytes.
    pub fn with_signer(&mut self, signer: Box<dyn Signer + Send + Sync>) -> &mut Self {
        self.signer = Some(signer);
        self
    }

    /// Attach a verification backend. Restores will reject unsigned or
    /// failed-verification checkpoints.
    pub fn with_verifier(&mut self, verifier: Box<dyn Verifier + Send + Sync>) -> &mut Self {
        self.verifier = Some(verifier);
        self
    }

    /// The configured copy limit.
    pub fn policy(&self) -> &CopyLimitPolicy {
        &self.policy
    }

    pub fn audit(&self) -> &ReplicationAudit {
        &self.audit
    }

    /// Capture a checkpoint of the running agent's state blob, signing it if a
    /// [`Signer`] is configured.
    pub fn capture(&mut self, state: &[u8]) -> Result<()> {
        let mut ckpt = Checkpoint::new(self.agent_id, state.to_vec(), now_secs());
        if let Some(signer) = &self.signer {
            let signable = ckpt.signable_bytes()?;
            ckpt.seal = Some(signer.sign(&signable));
        }
        self.current = Some(ckpt);
        self.audit.record(AuditEntry {
            agent_id: self.agent_id,
            reason: ReplicationReason::Checkpoint,
            timestamp_secs: now_secs(),
            live_copies_after: self.policy.live_copies(),
            detail: format!("state {} bytes", state.len()),
        });
        Ok(())
    }

    /// The current checkpoint as a magic-prefixed blob, or an error if none
    /// has been captured yet.
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        match &self.current {
            Some(c) => c.to_bytes(),
            None => Err(Error::Replication("no checkpoint captured".into())),
        }
    }

    /// Restore state from a checkpoint blob, returning the agent state payload.
    ///
    /// When a [`Verifier`] is configured, the checkpoint's seal must be present
    /// and verify successfully; an unsigned or failed-verification checkpoint is
    /// rejected outright (this is the enforcement half of the signing contract,
    /// see P0.1). A checkpoint for a different `agent_id` is always rejected.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let ckpt = Checkpoint::from_bytes(bytes)?;
        if ckpt.agent_id != self.agent_id {
            return Err(Error::Replication(format!(
                "checkpoint is for {} but this agent is {}",
                ckpt.agent_id, self.agent_id
            )));
        }
        if let Some(verifier) = &self.verifier {
            let seal = ckpt
                .seal
                .as_ref()
                .ok_or_else(|| Error::Integrity("checkpoint is unsigned".into()))?;
            let signable = ckpt.signable_bytes()?;
            if !verifier.verify(&signable, seal) {
                return Err(Error::Integrity(
                    "checkpoint seal verification failed".into(),
                ));
            }
        }
        self.current = Some(ckpt.clone());
        Ok(ckpt.payload)
    }

    /// Erasure-code the current checkpoint into shards.
    pub fn shards(&self) -> Result<Vec<Vec<u8>>> {
        let bytes = self.checkpoint_bytes()?;
        encode_shards(&bytes, self.data_shards, self.parity_shards)
    }

    /// Recover state from (possibly partial) shards.
    pub fn restore_from_shards(&mut self, shards: &[Vec<u8>]) -> Result<Vec<u8>> {
        let bytes = decode_shards(shards, self.data_shards, self.parity_shards)?;
        self.restore_state(&bytes)
    }

    /// Number of live concurrent copies (from the copy-limit policy).
    pub fn live_copy_count(&self) -> usize {
        self.policy.live_copies()
    }

    /// Authorize one more redundant copy, enforcing the hard cap.
    pub fn authorize_copy(&mut self, reason: ReplicationReason) -> Result<()> {
        self.policy.authorize_copy()?;
        self.audit.record(AuditEntry {
            agent_id: self.agent_id,
            reason,
            timestamp_secs: now_secs(),
            live_copies_after: self.policy.live_copies(),
            detail: String::new(),
        });
        Ok(())
    }

    /// Live-migrate this agent's checkpoint into `target` (simulated target
    /// node). Records a migration audit entry on both ends.
    pub fn migrate_to(&self, target: &mut Replicator, reason: ReplicationReason) -> Result<()> {
        let bytes = self.checkpoint_bytes()?;
        let payload = target.restore_state(&bytes)?;
        let _ = payload;
        let ts = now_secs();
        self.audit.record(AuditEntry {
            agent_id: self.agent_id,
            reason,
            timestamp_secs: ts,
            live_copies_after: self.policy.live_copies(),
            detail: format!("migrated to {}", target.agent_id),
        });
        target.audit.record(AuditEntry {
            agent_id: target.agent_id,
            reason,
            timestamp_secs: ts,
            live_copies_after: target.policy.live_copies(),
            detail: format!("migrated from {}", self.agent_id),
        });
        Ok(())
    }
}

impl Replicate for Replicator {
    fn checkpoint(&self) -> Result<Vec<u8>> {
        self.checkpoint_bytes()
    }

    fn restore(&mut self, checkpoint: &[u8]) -> Result<()> {
        self.restore_state(checkpoint).map(|_| ())
    }

    fn live_copy_count(&self) -> Result<usize> {
        Ok(self.live_copy_count())
    }
}
