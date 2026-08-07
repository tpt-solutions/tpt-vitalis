//! The negotiator: signs/verifies barter messages and settles trades.

use crate::crypto::{verify, Signer};
use crate::message::{BarterMessage, SignedMessage};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
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
    /// detection. The mapped [`AgentId`] is the *proposer* whose `Settle` we
    /// expect; a `Settle` for this nonce is only honoured if it is signed by
    /// that same proposer (replay / forgery protection, see [`Self::receive_settle`]).
    accepted: HashMap<u64, AgentId>,
    /// Nonces we have ourselves *proposed* (awaiting the accepter's
    /// settlement). Populated in [`Self::propose_offer`] and consumed when the
    /// matching `Settle` is received, so a captured settle cannot be replayed
    /// to re-credit the ledger (P0.3).
    /// Trust-on-first-use pins: `AgentId` → public key. A message is only
    /// trusted if its public key matches the pinned key for its `AgentId`.
    /// First sight pins the key; a later mismatch (a forged/swapped
    /// identity) is rejected. Pre-seed a peer with [`Negotiator::pin`] for
    /// out-of-band trust establishment.
    pins: Mutex<HashMap<AgentId, Vec<u8>>>,
    /// Maximum number of outstanding (accepted, not yet settled) offers we
    /// will hold per peer. Guards against a peer spamming offers to exhaust
    /// memory / stall settlement (abuse resistance; see P0.4).
    max_pending_per_peer: usize,
    /// Nonces this agent has proposed, awaiting the accepter's settlement.
    proposed: Mutex<HashSet<u64>>,
}

impl Negotiator {
    /// Construct with the default per-peer pending-offer cap (16).
    pub fn new(initial: &[Resource]) -> Result<Self> {
        Self::with_limits(initial, 16)
    }

    /// Construct with an explicit per-peer pending-offer cap.
    pub fn with_limits(initial: &[Resource], max_pending_per_peer: usize) -> Result<Self> {
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
            pins: Mutex::new(HashMap::new()),
            max_pending_per_peer,
            proposed: Mutex::new(HashSet::new()),
        })
    }

    /// Pre-establish trust in `agent`'s `public_key` (out-of-band / known-peer).
    /// Subsequent messages claiming that identity but signed by a different key
    /// are rejected.
    pub fn pin(&self, agent: AgentId, public_key: Vec<u8>) {
        if let Ok(mut pins) = self.pins.lock() {
            pins.insert(agent, public_key);
        }
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

    /// Verify a received message's signature and payload integrity, binding the
    /// sender's public key to their `AgentId` via trust-on-first-use pinning.
    pub fn verify(&self, sm: &SignedMessage) -> bool {
        if !verify(&sm.public_key, &sm.payload, &sm.signature) {
            return false;
        }
        if sm.message().is_err() {
            return false;
        }
        let mut pins = self.pins.lock().unwrap_or_else(|e| e.into_inner());
        match pins.get(&sm.signer) {
            Some(pinned) if pinned.as_slice() != sm.public_key.as_slice() => false,
            Some(_) => true,
            None => {
                pins.insert(sm.signer, sm.public_key.clone());
                true
            }
        }
    }

    /// Propose a trade: I give `gives`, I want `wants`.
    pub fn propose_offer(&self, gives: Resource, wants: Resource) -> SignedMessage {
        let nonce = rand::random::<u64>();
        // Track the nonce as one we are awaiting settlement for so a captured
        // `Settle` cannot be replayed to re-credit our ledger (P0.3).
        if let Ok(mut proposed) = self.proposed.lock() {
            proposed.insert(nonce);
        }
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
        // Abuse resistance (P0.4): bound the outstanding offers we accept from
        // any single peer so a flood of offers can't exhaust memory or stall
        // settlement.
        let pending_for_peer = self
            .accepted
            .values()
            .filter(|p| **p == offer.signer)
            .count();
        if pending_for_peer >= self.max_pending_per_peer {
            return Err(Error::Resource(format!(
                "peer {} has {} pending offers (cap {})",
                offer.signer, pending_for_peer, self.max_pending_per_peer
            )));
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
    ///
    /// NOTE: the nonce is intentionally *not* consumed here — it is consumed
    /// only when the counterparty's `Settle` is received (see
    /// [`Self::receive_settle`]), so that both sides settle exactly once.
    pub fn deliver(&mut self, gives: Resource, nonce: u64) -> Result<SignedMessage> {
        self.ledger.debit(&gives)?;
        Ok(self.sign(&BarterMessage::Settle {
            gives: gives.clone(),
            wants: Resource::new(gives.kind(), 0.0, gives.unit()),
            nonce,
        }))
    }

    /// Receive a peer's settlement: credit what they delivered to us.
    ///
    /// Replay / forgery protection (P0.3): the settle is only honoured if its
    /// nonce corresponds to a trade this agent is actually a party to, and the
    /// nonce is consumed on success so a captured `Settle` cannot be replayed
    /// to re-credit the ledger:
    ///
    /// * **Offeree side** — the nonce must be one we *accepted*, and the
    ///   `Settle` must be signed by the very proposer we accepted it from
    ///   (binds the nonce to the original offer's signer).
    /// * **Proposer side** — the nonce must be one we *proposed*, awaiting the
    ///   accepter's settlement.
    pub fn receive_settle(&mut self, settle: &SignedMessage) -> Result<()> {
        if !self.verify(settle) {
            return Err(Error::Integrity("settle signature invalid".into()));
        }
        let BarterMessage::Settle { gives, nonce, .. } = settle.message()? else {
            return Err(Error::Invalid("expected a Settle".into()));
        };
        // Offeree side: we accepted this nonce from a specific proposer; the
        // settle must come from that same proposer.
        if let Some(expected) = self.accepted.get(&nonce) {
            if *expected == settle.signer {
                self.accepted.remove(&nonce);
                self.ledger.credit(&gives);
                return Ok(());
            }
            return Err(Error::Integrity(format!(
                "settle nonce {nonce} signed by wrong peer"
            )));
        }
        // Proposer side: we proposed this nonce and await the accepter's
        // settlement. Consume it so it cannot be replayed.
        if self
            .proposed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&nonce)
        {
            self.ledger.credit(&gives);
            return Ok(());
        }
        Err(Error::Integrity(format!(
            "settle nonce {nonce} not recognized as an active trade"
        )))
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
