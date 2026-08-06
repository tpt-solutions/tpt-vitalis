//! Versioned checkpoint envelope.
//!
//! The checkpoint carries the agent's serialized state plus identity and
//! metadata so it can be reconstituted on a fresh node. The format is
//! versioned and magic-prefixed so we can evolve it forward without ambiguity.

use serde::{Deserialize, Serialize};
use vitalis_core::{AgentId, Error, Result};

/// Magic prefix identifying a Vitalis checkpoint blob.
pub const CHECKPOINT_MAGIC: &[u8; 4] = b"VCP1";

/// Current on-disk checkpoint schema version.
pub const CHECKPOINT_VERSION: u32 = 1;

/// Hard ceiling on a single checkpoint blob. Rejected before deserializing,
/// so a hostile or corrupt peer can't force an unbounded allocation just by
/// claiming a huge (or crafted) payload.
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;

/// A portable agent checkpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub version: u32,
    pub agent_id: AgentId,
    pub payload: Vec<u8>,
    pub created_secs: u64,
    /// Opaque signature blob over [`Checkpoint::signable_bytes`], produced by
    /// whatever integrity backend the caller wires in (e.g. `vitalis-defend`'s
    /// `CheckpointSeal`, postcard-encoded). `None` means unsigned.
    pub seal: Option<Vec<u8>>,
}

impl Checkpoint {
    pub fn new(agent_id: AgentId, payload: Vec<u8>, created_secs: u64) -> Self {
        Self {
            version: CHECKPOINT_VERSION,
            agent_id,
            payload,
            created_secs,
            seal: None,
        }
    }

    /// The canonical bytes a [`vitalis_core::traits::Signer`]/
    /// [`vitalis_core::traits::Verifier`] signs/checks: every field except
    /// the seal itself (signing the seal would be circular).
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let unsealed = Checkpoint {
            seal: None,
            ..self.clone()
        };
        postcard::to_stdvec(&unsealed).map_err(|e| Error::Encode(e.to_string()))
    }

    /// Serialize to a magic-prefixed byte blob.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let body = postcard::to_stdvec(self).map_err(|e| Error::Encode(e.to_string()))?;
        let mut out = CHECKPOINT_MAGIC.to_vec();
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Parse a magic-prefixed checkpoint blob, validating size, magic, and
    /// version. Does **not** verify the seal — callers that need integrity
    /// guarantees go through `Replicator::restore_state`, which checks the
    /// seal when a `Verifier` is configured.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(Error::Invalid(format!(
                "checkpoint of {} bytes exceeds the {MAX_CHECKPOINT_BYTES}-byte limit",
                bytes.len()
            )));
        }
        if bytes.len() < CHECKPOINT_MAGIC.len() {
            return Err(Error::Invalid("checkpoint too short".into()));
        }
        if &bytes[..CHECKPOINT_MAGIC.len()] != CHECKPOINT_MAGIC {
            return Err(Error::Invalid("bad checkpoint magic".into()));
        }
        let body = &bytes[CHECKPOINT_MAGIC.len()..];
        let ckpt: Checkpoint =
            postcard::from_bytes(body).map_err(|e| Error::Encode(e.to_string()))?;
        if ckpt.version > CHECKPOINT_VERSION {
            return Err(Error::Invalid(format!(
                "unsupported checkpoint version {}",
                ckpt.version
            )));
        }
        Ok(ckpt)
    }
}
