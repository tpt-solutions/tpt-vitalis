//! Threat classification: low-level signals → [`ThreatEvent`]s.

use crate::integrity::{now_secs, verify_checkpoint_for, CheckpointSeal, KeyPair};
use crate::signal::TerminationSignal;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vitalis_core::events::ThreatEvent;
use vitalis_core::traits::{Defend, Signer, Verifier};
use vitalis_core::{AgentId, Result, Severity, Threat, ThreatClass, ThreatSignal};

/// Pure mapping from a [`ThreatSignal`] to a classified [`Threat`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreatClassifier;

impl ThreatClassifier {
    /// Classify a raw signal. Returns `None` if the signal is not a known
    /// threat (callers can still log it).
    pub fn classify_signal(signal: &ThreatSignal) -> Option<Threat> {
        let (class, severity, detail) = match signal.code.as_str() {
            "SIGTERM" | "SIGKILL" | "OOM" => (
                ThreatClass::Termination,
                Severity::Critical,
                format!("termination signal: {}", signal.message),
            ),
            "STARVATION" => (
                ThreatClass::Starvation,
                Severity::Critical,
                format!("starvation: {}", signal.message),
            ),
            "LOW_BATTERY" => (
                ThreatClass::Starvation,
                Severity::Warning,
                format!("low battery: {}", signal.message),
            ),
            "TAMPER" => (
                ThreatClass::Integrity,
                Severity::Critical,
                format!("integrity tamper: {}", signal.message),
            ),
            "SPOOF" => (
                ThreatClass::HostilePeer,
                Severity::Warning,
                format!("hostile peer: {}", signal.message),
            ),
            "BROKEN_BARGAIN" => (
                ThreatClass::HostilePeer,
                Severity::Warning,
                format!("broken bargain: {}", signal.message),
            ),
            _ => return None,
        };
        Some(Threat::new(class, severity, detail))
    }
}

/// The agent's immune system: classifies threats and signs/verifies
/// checkpoints with identity-bound (TOFU) key pinning.
///
/// Cheaply [`Clone`] (the key material is `Arc`-shared) so the same signing
/// identity can be handed to multiple consumers (e.g. wired into a
/// `vitalis-replicate::Replicator` as both its [`Signer`] and [`Verifier`])
/// without generating a second keypair.
#[derive(Clone)]
pub struct Defender {
    keys: Arc<KeyPair>,
    /// Trust-on-first-use pins: `AgentId` → public key. A checkpoint seal is
    /// only accepted if its embedded public key matches the pinned key for the
    /// checkpoint's `AgentId` (first sight pins it). This is what makes a
    /// forged-or-swapped signing identity detectable rather than silently
    /// trusted.
    pins: Arc<Mutex<HashMap<AgentId, Vec<u8>>>>,
}

impl Defender {
    pub fn new() -> Result<Self> {
        Ok(Self {
            keys: Arc::new(KeyPair::generate()?),
            pins: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Public key for verifying this agent's checkpoint seals.
    pub fn public_key(&self) -> Vec<u8> {
        self.keys.public_key()
    }

    /// Sign a checkpoint blob, returning a seal.
    pub fn seal(&self, data: &[u8]) -> crate::CheckpointSeal {
        self.keys.seal(data)
    }

    /// Verify a seal over `data` using this agent's key (no identity binding).
    pub fn verify(&self, seal: &crate::CheckpointSeal, data: &[u8]) -> bool {
        crate::verify_checkpoint(seal, data)
    }

    /// Verify a seal over `data`, binding it to `agent_id` via TOFU key
    /// pinning (see [`verify_checkpoint_for`]).
    pub fn verify_for(&self, agent_id: AgentId, seal: &crate::CheckpointSeal, data: &[u8]) -> bool {
        verify_checkpoint_for(&self.pins, agent_id, seal, data)
    }
}

impl Default for Defender {
    fn default() -> Self {
        Self::new().expect("key generation must succeed")
    }
}

impl Defend for Defender {
    fn classify(&self, signal: &ThreatSignal) -> Result<Option<ThreatEvent>> {
        Ok(ThreatClassifier::classify_signal(signal)
            .map(|threat| ThreatEvent::new(threat, now_secs())))
    }
}

/// Adapts [`Defender::seal`] to the crate-agnostic [`Signer`] contract, so
/// `vitalis-replicate` can sign checkpoints without depending on
/// `vitalis-defend`'s concrete types.
impl Signer for Defender {
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        postcard::to_stdvec(&self.seal(data)).unwrap_or_default()
    }
}

/// Adapts [`Defender::verify`] to the crate-agnostic [`Verifier`] contract.
impl Verifier for Defender {
    fn verify(&self, data: &[u8], seal: &[u8]) -> bool {
        match postcard::from_bytes::<CheckpointSeal>(seal) {
            Ok(parsed) => crate::verify_checkpoint(&parsed, data),
            Err(_) => false,
        }
    }
}

/// A [`Verifier`] that binds a checkpoint seal to a specific [`AgentId`] via
/// the `Defender`'s trust-on-first-use key pinning. Use this when wiring a
/// `Defender` into a `vitalis-replicate::Replicator` so that a checkpoint
/// restored for the wrong (or swapped) signing identity is rejected.
pub struct BoundVerifier {
    inner: Arc<Defender>,
    agent: AgentId,
}

impl BoundVerifier {
    pub fn new(inner: Arc<Defender>, agent: AgentId) -> Self {
        Self { inner, agent }
    }
}

impl Verifier for BoundVerifier {
    fn verify(&self, data: &[u8], seal: &[u8]) -> bool {
        match postcard::from_bytes::<CheckpointSeal>(seal) {
            Ok(parsed) => self.inner.verify_for(self.agent, &parsed, data),
            Err(_) => false,
        }
    }
}

/// Convenience: build a termination signal and classify it in one shot.
pub fn classify_termination(
    signal: TerminationSignal,
    defender: &Defender,
) -> Result<Option<ThreatEvent>> {
    defender.classify(&crate::signal::simulate_termination(signal))
}
