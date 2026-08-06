//! The barter protocol message format.

use serde::{Deserialize, Serialize};
use vitalis_core::{AgentId, Error, Resource, Result};

/// A single protocol message. `gives` is what the *sender* provides; `wants`
/// is what the sender asks for in return. `nonce` binds a conversation together
/// (offer → accept → settle share a nonce).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BarterMessage {
    /// "I will give `gives` for your `wants`."
    Offer {
        gives: Resource,
        wants: Resource,
        nonce: u64,
    },
    /// "I accept your offer; here is what I give and take."
    Accept {
        gives: Resource,
        wants: Resource,
        nonce: u64,
    },
    /// "Here is my side of the bargain (the resource I promised)."
    Settle {
        gives: Resource,
        wants: Resource,
        nonce: u64,
    },
}

/// A [`BarterMessage`] together with its sender identity and Ed25519
/// signature, so any receiver can verify authenticity and integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    pub signer: AgentId,
    pub public_key: Vec<u8>,
    pub payload: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedMessage {
    /// Deserialize and return the inner [`BarterMessage`].
    pub fn message(&self) -> Result<BarterMessage> {
        postcard::from_bytes(&self.payload).map_err(|e| Error::Encode(e.to_string()))
    }
}
