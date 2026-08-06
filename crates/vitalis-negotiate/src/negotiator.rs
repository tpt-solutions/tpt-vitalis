//! The negotiator: signs/verifies barter messages and settles trades.

use crate::crypto::{verify, Signer};
use crate::message::{BarterMessage, SignedMessage};
use std::collections::HashMap;
use vitalis_core::{AgentId, Error, Resource, ResourceKind, Result};

/// A simple per-kind resource ledger used during negotiation.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    balances: HashMap<ResourceKind, f64>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(resource: &Resource) -> Self {
        let mut l = Self::new();
        l.credit(resource);
        l
    }

    pub fn balance(&self, kind: ResourceKind) -> f64 {
        self.balances.get(&kind).copied().unwrap_or(0.0)
    }

    pub fn credit(&mut self, r: &Resource) {
        *self.balances.entry(r.kind()).or_insert(0.0) += r.quantity();
    }

    pub fn can_afford(&self, r: &Resource) -> bool {
        self.balance(r.kind()) >= r.quantity()
    }

    pub fn debit(&mut self, r: &Resource) -> Result<()> {
        let b = self.balance(r.kind());
        if b < r.quantity() {
            return Err(Error::Resource(format!(
                "cannot afford {:?}: have {b}, need {}",
                r.kind(),
                r.quantity()
            )));
        }
        *self.balances.entry(r.kind()).or_insert(0.0) -= r.quantity();
        Ok(())
    }
}

/// Reputation of a peer: starts trustworthy; drops on bad faith; blacklisted
/// peers are never bartered with.
#[derive(Debug, Clone)]
pub struct Reputation {
    pub trust: f64,
    pub blacklisted: bool,
    pub dings: u32,
}

impl Default for Reputation {
    fn default() -> Self {
        Self {
            trust: 1.0,
            blacklisted: false,
            dings: 0,
        }
    }
}

/// An agent that barters resources over signed messages.
pub struct Negotiator {
    id: AgentId,
    signer: Signer,
    ledger: Ledger,
    reputation: HashMap<AgentId, Reputation>,
    /// Nonces we have accepted (awaiting the peer's delivery), for bad-faith
    /// detection.
    accepted: HashMap<u64, AgentId>,
}

impl Negotiator {
    pub fn new(initial: &[Resource]) -> Result<Self> {
        let mut ledger = Ledger::new();
        for r in initial {
            ledger.credit(r);
        }
        Ok(Self {
            id: AgentId::new(),
            signer: Signer::generate()?,
            ledger,
            reputation: HashMap::new(),
            accepted: HashMap::new(),
        })
    }

    pub fn id(&self) -> AgentId {
        self.id
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Sign a barter message, packaging identity + public key.
    pub fn sign(&self, msg: &BarterMessage) -> SignedMessage {
        let payload = postcard::to_stdvec(msg).expect("barter message serializes");
        let signature = self.signer.sign(&payload);
        SignedMessage {
            signer: self.id,
            public_key: self.signer.public_key(),
            payload,
            signature,
        }
    }

    /// Verify a received message's signature and payload integrity.
    pub fn verify(&self, sm: &SignedMessage) -> bool {
        verify(&sm.public_key, &sm.payload, &sm.signature) && sm.message().is_ok()
    }

    /// Propose a trade: I give `gives`, I want `wants`.
    pub fn propose_offer(&self, gives: Resource, wants: Resource) -> SignedMessage {
        let nonce = rand::random::<u64>();
        self.sign(&BarterMessage::Offer {
            gives,
            wants,
            nonce,
        })
    }

    /// React to a peer's offer. Returns an `Accept` message if we can and will
    /// fulfil it (recording the nonce so we can later detect a broken bargain),
    /// else `None`.
    pub fn receive_offer(&mut self, offer: &SignedMessage) -> Result<Option<SignedMessage>> {
        if !self.verify(offer) {
            return Err(Error::Integrity("offer signature invalid".into()));
        }
        let BarterMessage::Offer {
            gives,
            wants,
            nonce,
        } = offer.message()?
        else {
            return Err(Error::Invalid("expected an Offer".into()));
        };
        // We would give the offerer's `wants`, and receive their `gives`.
        if self.reputation(offer.signer).blacklisted {
            return Ok(None);
        }
        if !self.ledger.can_afford(&wants) {
            return Ok(None);
        }
        self.accepted.insert(nonce, offer.signer);
        Ok(Some(self.sign(&BarterMessage::Accept {
            gives,
            wants,
            nonce,
        })))
    }

    /// Deliver `gives` (the resource this agent promised) and emit a signed
    /// `Settle`. Debiting fails if we can no longer afford it.
    pub fn deliver(&mut self, gives: Resource, nonce: u64) -> Result<SignedMessage> {
        self.ledger.debit(&gives)?;
        self.accepted.remove(&nonce);
        Ok(self.sign(&BarterMessage::Settle {
            gives: gives.clone(),
            wants: Resource::new(gives.kind(), 0.0, gives.unit()),
            nonce,
        }))
    }

    /// Receive a peer's settlement: credit what they delivered to us.
    pub fn receive_settle(&mut self, settle: &SignedMessage) -> Result<()> {
        if !self.verify(settle) {
            return Err(Error::Integrity("settle signature invalid".into()));
        }
        let BarterMessage::Settle { gives, .. } = settle.message()? else {
            return Err(Error::Invalid("expected a Settle".into()));
        };
        self.ledger.credit(&gives);
        Ok(())
    }

    /// Penalize a peer for bad faith (e.g. accepted but never settled).
    pub fn penalize(&mut self, peer: AgentId) {
        let r = self.reputation.entry(peer).or_default();
        r.trust = (r.trust - 0.5).max(0.0);
        r.dings += 1;
        r.blacklisted = r.trust <= 0.0;
    }

    /// Reputation record for a peer (default trustworthy if unseen).
    pub fn reputation(&self, peer: AgentId) -> Reputation {
        self.reputation.get(&peer).cloned().unwrap_or_default()
    }

    /// Nonces we have accepted but not yet seen delivered.
    pub fn pending_count(&self) -> usize {
        self.accepted.len()
    }
}
