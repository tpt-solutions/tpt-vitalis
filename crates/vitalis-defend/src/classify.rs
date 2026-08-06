//! Threat classification: low-level signals → [`ThreatEvent`]s.

use crate::integrity::{now_secs, CheckpointSeal, KeyPair};
use crate::signal::TerminationSignal;
use std::sync::Arc;
use vitalis_core::events::ThreatEvent;
use vitalis_core::traits::{Defend, Signer, Verifier};
use vitalis_core::{Result, Severity, Threat, ThreatClass, ThreatSignal};

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
            _ => return None,
        };
        Some(Threat::new(class, severity, detail))
    }
}

/// The agent's immune system: classifies threats and signs checkpoints.
///
/// Cheaply [`Clone`] (the key material is `Arc`-shared) so the same signing
/// identity can be handed to multiple consumers (e.g. wired into a
/// `vitalis-replicate::Replicator` as both its [`Signer`] and [`Verifier`])
/// without generating a second keypair.
#[derive(Clone)]
pub struct Defender {
    keys: Arc<KeyPair>,
}

impl Defender {
    pub fn new() -> Result<Self> {
        Ok(Self {
            keys: Arc::new(KeyPair::generate()?),
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

    /// Verify a seal over `data` using this agent's key.
    pub fn verify(&self, seal: &crate::CheckpointSeal, data: &[u8]) -> bool {
        crate::verify_checkpoint(seal, data)
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

/// Convenience: build a termination signal and classify it in one shot.
pub fn classify_termination(
    signal: TerminationSignal,
    defender: &Defender,
) -> Result<Option<ThreatEvent>> {
    defender.classify(&crate::signal::simulate_termination(signal))
}
