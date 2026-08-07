//! Ed25519 checkpoint integrity.
//!
//! A checkpoint blob is signed with an agent-held Ed25519 key. On load, the
//! signature is verified; a mismatch means the persisted state was tampered
//! with (or corrupted) and must not be trusted.

use ring::signature::{self, Ed25519KeyPair, KeyPair as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use vitalis_core::{AgentId, Error, Result};

/// A signing/verification key pair wrapper.
pub struct KeyPair {
    inner: Ed25519KeyPair,
}

impl KeyPair {
    /// Generate a fresh Ed25519 key pair.
    pub fn generate() -> Result<Self> {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|e| Error::Integrity(format!("keygen failed: {e}")))?;
        let inner = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .map_err(|e| Error::Integrity(format!("key parse failed: {e}")))?;
        Ok(Self { inner })
    }

    /// The public key bytes (for verification by peers / on restore).
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.public_key().as_ref().to_vec()
    }

    /// Sign `data`, returning a [`CheckpointSeal`].
    pub fn seal(&self, data: &[u8]) -> CheckpointSeal {
        let sig = self.inner.sign(data);
        CheckpointSeal {
            signature: sig.as_ref().to_vec(),
            public_key: self.public_key(),
        }
    }
}

/// A signature + the public key it was made with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointSeal {
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}

/// Verify a seal over `data`, binding it to `agent_id` for trust-on-first-use
/// (TOFU) key pinning. The first time a given `agent_id` is seen, its public
/// key is pinned; thereafter only that key is accepted for that identity. A
/// seal signed by a different key for the same `agent_id` is rejected, which is
/// what makes identity forgery detectable (see `docs/threat-model.md`).
///
/// Returns `false` if the seal's public key is pinned to a *different* key than
/// the one that signed it, or if the signature itself fails to verify.
pub fn verify_checkpoint_for(
    pin_store: &Mutex<HashMap<AgentId, Vec<u8>>>,
    agent_id: AgentId,
    seal: &CheckpointSeal,
    data: &[u8],
) -> bool {
    {
        let mut pins = pin_store.lock().unwrap_or_else(|e| e.into_inner());
        match pins.get(&agent_id) {
            Some(pinned) if pinned.as_slice() != seal.public_key.as_slice() => return false,
            Some(_) => {}
            None => {
                pins.insert(agent_id, seal.public_key.clone());
            }
        }
    }
    verify_checkpoint(seal, data)
}

/// Verify a seal over `data` (no identity binding).
pub fn verify_checkpoint(seal: &CheckpointSeal, data: &[u8]) -> bool {
    let pk = signature::UnparsedPublicKey::new(&signature::ED25519, &seal.public_key);
    pk.verify(data, &seal.signature).is_ok()
}

/// Current unix timestamp in seconds (used to stamp seals/events).
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
