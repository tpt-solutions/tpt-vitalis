//! The barter protocol message format.

use serde::{Deserialize, Serialize};
use vitalis_core::{AgentId, Error, Resource, Result};

/// Hard ceiling on a signed barter message's inner payload. Checked before
/// deserializing so a hostile or corrupt peer can't force an unbounded
/// allocation just by claiming a huge (or crafted) payload.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

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
    /// A signed, gossiped opinion about a third party (`subject`), built only
    /// from the sender's own *direct* trade history with them. This is
    /// "direct gossip" — the sender is the original witness.
    ReputationReport {
        subject: AgentId,
        trust: f64,
        dings: u32,
        as_of_cycle: u64,
    },
    /// "Indirect gossip": forwarding a [`BarterMessage::ReputationReport`]
    /// this agent did not itself witness. `provenance` is the raw bytes of
    /// the *original* hop-1 [`SignedMessage`] (signed by `origin`), carried
    /// unchanged through every relay — trust in the claim is anchored to
    /// `origin`'s own signature, not to whatever an intermediate relayer
    /// claims. `hops` counts the distance from `origin` to whoever signed
    /// this outer message; a receiver bounds how far it will trust a claim
    /// that has traveled by capping `hops`.
    RelayedReputationReport {
        subject: AgentId,
        origin: AgentId,
        hops: u32,
        provenance: Vec<u8>,
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
        if self.payload.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Invalid(format!(
                "message payload of {} bytes exceeds the {MAX_MESSAGE_BYTES}-byte limit",
                self.payload.len()
            )));
        }
        postcard::from_bytes(&self.payload).map_err(|e| Error::Encode(e.to_string()))
    }
}
