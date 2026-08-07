//! The negotiator: signs/verifies barter messages, settles trades, and
//! builds up peer reputation from direct trade outcomes and gossip.
//!
//! Reputation is a bounded, decayed Beta-reputation model (Jøsang) split into
//! two pools per peer:
//!
//! * **Direct** — this agent's own trade outcomes with that peer. This is
//!   the *only* pool that can ever set [`Reputation::blacklisted`].
//! * **Hearsay** — gossiped [`BarterMessage::ReputationReport`] (direct
//!   witness) and [`BarterMessage::RelayedReputationReport`] (forwarded,
//!   hop-discounted) claims from other peers, keyed by the *original*
//!   witness so the same claim relayed via multiple paths doesn't inflate
//!   its own corroboration. Hearsay moves the continuous trust score but can
//!   never blacklist a peer by itself — that guarantee is structural (the
//!   two pools are different fields, written by different code paths), not
//!   a threshold that has to be tuned correctly.
//!
//! Both direct and hearsay evidence decay toward the neutral prior over
//! cycles (a configurable half-life), so old incidents stop mattering if a
//! peer has since behaved — forgiveness, not permanent excommunication.

use crate::crypto::{verify, Signer};
use crate::message::{BarterMessage, SignedMessage};
use std::collections::HashMap;
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

/// Reputation snapshot for a peer: starts trustworthy; drops on bad faith;
/// blacklisted peers are never bartered with. Derived from a
/// `ReputationRecord` as of a given cycle — see [`Negotiator::reputation`].
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

/// Optimistic Beta-reputation prior: an unseen peer starts fully trusted
/// (mirrors the previous flat `trust: 1.0` default).
const PRIOR_SUCCESS: f64 = 1.0;
const PRIOR_FAILURE: f64 = 0.0;
/// A direct witness's report's maximum contribution to a subject's trust
/// score. Always strictly less than a direct trade outcome's implicit
/// weight of 1.0, so hearsay can move the number but never dominates it.
const HEARSAY_WEIGHT: f64 = 0.35;
/// Multiplicative discount applied to [`HEARSAY_WEIGHT`] per hop beyond the
/// first, so a claim's influence shrinks geometrically with distance from
/// its original witness.
const PER_HOP_DECAY: f64 = 0.6;
/// The furthest a relayed claim is trusted to have traveled from its
/// original witness. Bounds how far (and how much) a false claim can
/// propagate through the mesh.
const MAX_HOPS: u32 = 3;
/// Decayed direct-failure weight at or above which a peer is blacklisted.
/// Two undecayed full failures (2 x 1.0) cross this; one does not —
/// preserves the "two strikes" feel of the previous flat model.
const DIRECT_BLACKLIST_THRESHOLD: f64 = 1.5;
/// A gossiped report older than this many cycles (relative to `now`) is
/// treated as stale and ignored.
const MAX_REPORT_AGE_CYCLES: u64 = 100;
/// Default half-life (in cycles) over which reputation evidence decays
/// toward the neutral prior — the forgiveness knob.
const DEFAULT_DECAY_HALF_LIFE_CYCLES: u64 = 50;

/// A single piece of hearsay: someone's claimed trust in a subject, and how
/// far it has traveled from its original witness.
#[derive(Debug, Clone, Copy)]
struct Hearsay {
    trust: f64,
    as_of_cycle: u64,
    hops: u32,
}

/// Per-peer reputation bookkeeping backing the public [`Reputation`]
/// snapshot. See the module docs for the direct/hearsay split.
#[derive(Debug, Clone, Default)]
struct ReputationRecord {
    direct_successes: f64,
    direct_failures: f64,
    /// Monotonic, never decays: lifetime count of broken bargains with this
    /// peer, for observability only.
    dings: u32,
    /// Decayed weight of direct failures. This — and only this — gates
    /// `blacklisted`.
    direct_failure_weight: f64,
    /// Original-witness `AgentId` -> their claim. Keyed by the *origin*, not
    /// the immediate reporter/relayer, so the same underlying claim arriving
    /// via multiple relay paths is deduplicated rather than double-counted.
    hearsay: HashMap<AgentId, Hearsay>,
    /// The cycle at which this record was last updated by a direct outcome
    /// (decay is computed from here at read time).
    last_event_cycle: u64,
}

impl ReputationRecord {
    fn decay_factor(elapsed: u64, half_life: u64) -> f64 {
        if half_life == 0 {
            return 1.0;
        }
        0.5_f64.powf(elapsed as f64 / half_life as f64)
    }

    fn decayed_direct(&self, now: u64, half_life: u64) -> (f64, f64) {
        let factor = Self::decay_factor(now.saturating_sub(self.last_event_cycle), half_life);
        (
            self.direct_successes * factor,
            self.direct_failures * factor,
        )
    }

    fn decayed_failure_weight(&self, now: u64, half_life: u64) -> f64 {
        let factor = Self::decay_factor(now.saturating_sub(self.last_event_cycle), half_life);
        self.direct_failure_weight * factor
    }

    /// Fold non-stale hearsay into extra (successes, failures), each entry
    /// capped at `HEARSAY_WEIGHT * PER_HOP_DECAY^(hops-1)`.
    fn hearsay_contribution(&self, now: u64) -> (f64, f64) {
        let mut successes = 0.0;
        let mut failures = 0.0;
        for h in self.hearsay.values() {
            if now.saturating_sub(h.as_of_cycle) > MAX_REPORT_AGE_CYCLES {
                continue;
            }
            let weight = HEARSAY_WEIGHT * PER_HOP_DECAY.powi(h.hops.saturating_sub(1) as i32);
            let trust = h.trust.clamp(0.0, 1.0);
            successes += trust * weight;
            failures += (1.0 - trust) * weight;
        }
        (successes, failures)
    }

    /// Distinct, non-stale original witnesses currently backing this
    /// record's hearsay-derived trust.
    fn hearsay_origins(&self, now: u64) -> Vec<AgentId> {
        self.hearsay
            .iter()
            .filter(|(_, h)| now.saturating_sub(h.as_of_cycle) <= MAX_REPORT_AGE_CYCLES)
            .map(|(origin, _)| *origin)
            .collect()
    }

    fn snapshot(&self, now: u64, half_life: u64) -> Reputation {
        let (direct_s, direct_f) = self.decayed_direct(now, half_life);
        let (hearsay_s, hearsay_f) = self.hearsay_contribution(now);
        let successes = direct_s + hearsay_s;
        let failures = direct_f + hearsay_f;
        let trust =
            (successes + PRIOR_SUCCESS) / (successes + failures + PRIOR_SUCCESS + PRIOR_FAILURE);
        let blacklisted = self.decayed_failure_weight(now, half_life) >= DIRECT_BLACKLIST_THRESHOLD;
        Reputation {
            trust,
            blacklisted,
            dings: self.dings,
        }
    }

    /// Record a direct trade outcome (`ratio` in `[0,1]`; `1.0` = fully
    /// honored, `0.0` = fully broken), decaying prior evidence to `now`
    /// first.
    fn record_direct_outcome(&mut self, ratio: f64, now: u64, half_life: u64) {
        let (decayed_s, decayed_f) = self.decayed_direct(now, half_life);
        let decayed_fw = self.decayed_failure_weight(now, half_life);
        let ratio = ratio.clamp(0.0, 1.0);
        self.direct_successes = decayed_s + ratio;
        self.direct_failures = decayed_f + (1.0 - ratio);
        self.direct_failure_weight = decayed_fw + (1.0 - ratio);
        if ratio < 1.0 {
            self.dings += 1;
        }
        self.last_event_cycle = now;
    }
}

/// The fraction of `expected` that `delivered` actually satisfies, clamped
/// to `[0, 1]`. A [`ResourceKind`] mismatch is a complete failure (`0.0`) —
/// there is no meaningful partial credit across different kinds.
fn fulfillment_ratio(expected: &Resource, delivered: &Resource) -> f64 {
    if expected.kind() != delivered.kind() {
        return 0.0;
    }
    if expected.quantity() <= 0.0 {
        return 1.0;
    }
    (delivered.quantity() / expected.quantity()).clamp(0.0, 1.0)
}

/// A trade this agent is a party to, awaiting completion.
#[derive(Debug, Clone)]
struct PendingTrade {
    /// The counterparty's identity. Always `Some` immediately on the offeree
    /// side (known from the `Offer`'s signer); starts `None` on the proposer
    /// side until bound by [`Negotiator::receive_accept`].
    counterparty: Option<AgentId>,
    /// What we expect the counterparty to deliver (kind + quantity),
    /// captured at trade-open time so a `Settle` can be checked against the
    /// actual promise instead of trusted blindly.
    expect: Resource,
    /// The cycle this trade opened, used by [`Negotiator::sweep_timeouts`]
    /// to detect a broken bargain automatically.
    opened_at: u64,
}

/// A broken bargain automatically detected by [`Negotiator::sweep_timeouts`].
#[derive(Debug, Clone)]
pub struct BrokenBargain {
    pub peer: AgentId,
    pub trust_after: f64,
    pub dings_after: u32,
}

/// An agent that barters resources over signed messages.
pub struct Negotiator {
    id: AgentId,
    signer: Signer,
    ledger: Ledger,
    reputation: HashMap<AgentId, ReputationRecord>,
    /// Nonces we have accepted (offeree side): the trade we owe delivery-
    /// awareness for, keyed by the proposer we expect a `Settle` from.
    accepted: HashMap<u64, PendingTrade>,
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
    /// Nonces we have ourselves *proposed* (proposer side): the trade we
    /// opened, awaiting the accepter's identity (bound by
    /// [`Negotiator::receive_accept`]) and settlement.
    proposed: Mutex<HashMap<u64, PendingTrade>>,
    /// Half-life (cycles) over which reputation evidence decays toward the
    /// neutral prior.
    decay_half_life_cycles: u64,
}

impl Negotiator {
    /// Construct with the default per-peer pending-offer cap (16) and
    /// default reputation decay half-life.
    pub fn new(initial: &[Resource]) -> Result<Self> {
        Self::with_limits(initial, 16)
    }

    /// Construct with an explicit per-peer pending-offer cap.
    pub fn with_limits(initial: &[Resource], max_pending_per_peer: usize) -> Result<Self> {
        Self::with_limits_and_decay(
            initial,
            max_pending_per_peer,
            DEFAULT_DECAY_HALF_LIFE_CYCLES,
        )
    }

    /// Construct with an explicit per-peer pending-offer cap and reputation
    /// decay half-life (cycles). The shorter half-life is mainly useful for
    /// tests that want to exercise forgiveness without simulating hundreds
    /// of cycles.
    pub fn with_limits_and_decay(
        initial: &[Resource],
        max_pending_per_peer: usize,
        decay_half_life_cycles: u64,
    ) -> Result<Self> {
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
            proposed: Mutex::new(HashMap::new()),
            decay_half_life_cycles,
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
        if sm.message().is_err() {
            return false;
        }
        self.verify_binding(sm.signer, &sm.public_key, &sm.payload, &sm.signature)
    }

    /// Verify a raw signed payload's signature and TOFU-bind `signer`'s
    /// public key. Used both by [`Self::verify`] and to independently verify
    /// a relayed report's embedded provenance.
    fn verify_binding(
        &self,
        signer: AgentId,
        public_key: &[u8],
        payload: &[u8],
        signature: &[u8],
    ) -> bool {
        if !verify(public_key, payload, signature) {
            return false;
        }
        let mut pins = self.pins.lock().unwrap_or_else(|e| e.into_inner());
        match pins.get(&signer) {
            Some(pinned) if pinned.as_slice() != public_key => false,
            Some(_) => true,
            None => {
                pins.insert(signer, public_key.to_vec());
                true
            }
        }
    }

    /// Propose a trade: I give `gives`, I want `wants`.
    pub fn propose_offer(&self, gives: Resource, wants: Resource, now: u64) -> SignedMessage {
        let nonce = rand::random::<u64>();
        // Track the nonce as one we are awaiting settlement for so a captured
        // `Settle` cannot be replayed to re-credit our ledger (P0.3), and so
        // we can later detect a broken bargain (once the accepter's identity
        // is bound via `receive_accept`) via `sweep_timeouts`.
        if let Ok(mut proposed) = self.proposed.lock() {
            proposed.insert(
                nonce,
                PendingTrade {
                    counterparty: None,
                    expect: wants.clone(),
                    opened_at: now,
                },
            );
        }
        self.sign(&BarterMessage::Offer {
            gives,
            wants,
            nonce,
        })
    }

    /// Bind the identity of whoever accepted one of our offers. This closes
    /// a forged-settlement gap: without it, any signer could claim credit
    /// for a nonce it merely observed on the wire (nonces travel in the
    /// plaintext `Offer`), since a bare nonce alone proves nothing about who
    /// is entitled to settle it.
    pub fn receive_accept(&self, accept: &SignedMessage) -> Result<()> {
        if !self.verify(accept) {
            return Err(Error::Integrity("accept signature invalid".into()));
        }
        let BarterMessage::Accept { nonce, .. } = accept.message()? else {
            return Err(Error::Invalid("expected an Accept".into()));
        };
        let mut proposed = self.proposed.lock().unwrap_or_else(|e| e.into_inner());
        let trade = proposed
            .get_mut(&nonce)
            .ok_or_else(|| Error::Invalid(format!("no proposed trade for nonce {nonce}")))?;
        match trade.counterparty {
            None => {
                trade.counterparty = Some(accept.signer);
                Ok(())
            }
            Some(existing) if existing == accept.signer => Ok(()),
            Some(_) => Err(Error::Integrity(format!(
                "nonce {nonce} already accepted by a different peer"
            ))),
        }
    }

    /// React to a peer's offer. Returns an `Accept` message if we can and will
    /// fulfil it (recording the nonce so we can later detect a broken bargain),
    /// else `None`.
    pub fn receive_offer(
        &mut self,
        offer: &SignedMessage,
        now: u64,
    ) -> Result<Option<SignedMessage>> {
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
        if self.reputation(offer.signer, now).blacklisted {
            return Ok(None);
        }
        // Abuse resistance (P0.4): bound the outstanding offers we accept from
        // any single peer so a flood of offers can't exhaust memory or stall
        // settlement.
        let pending_for_peer = self
            .accepted
            .values()
            .filter(|p| p.counterparty == Some(offer.signer))
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
        self.accepted.insert(
            nonce,
            PendingTrade {
                counterparty: Some(offer.signer),
                expect: gives.clone(),
                opened_at: now,
            },
        );
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

    /// Receive a peer's settlement: credit what they delivered to us and
    /// score the outcome against what was actually promised.
    ///
    /// Replay / forgery protection (P0.3 + the proposer-side identity gap):
    /// the settle is only honoured if its nonce corresponds to a trade this
    /// agent is actually a party to, *and* the settle's signer matches the
    /// counterparty this agent recorded for that trade — on the proposer
    /// side that means a `Settle` is rejected unless [`Self::receive_accept`]
    /// already bound the accepter's identity, closing the gap where a third
    /// party could otherwise claim credit for an unrelated nonce.
    ///
    /// Scoring: the delivered resource is checked against the trade's
    /// recorded promise via `fulfillment_ratio` (0.0 on a `ResourceKind`
    /// mismatch), so an under-delivery is graduated bad faith, not either a
    /// silently-accepted full success or an all-or-nothing failure.
    pub fn receive_settle(&mut self, settle: &SignedMessage, now: u64) -> Result<()> {
        if !self.verify(settle) {
            return Err(Error::Integrity("settle signature invalid".into()));
        }
        let BarterMessage::Settle { gives, nonce, .. } = settle.message()? else {
            return Err(Error::Invalid("expected a Settle".into()));
        };

        // Offeree side: we accepted this nonce from a specific proposer; the
        // settle must come from that same proposer.
        if let Some(trade) = self.accepted.get(&nonce) {
            let expected = trade
                .counterparty
                .expect("offeree-side trades always know their proposer");
            if expected != settle.signer {
                return Err(Error::Integrity(format!(
                    "settle nonce {nonce} signed by wrong peer"
                )));
            }
            let ratio = fulfillment_ratio(&trade.expect, &gives);
            self.accepted.remove(&nonce);
            self.ledger.credit(&gives);
            self.record_direct_outcome(settle.signer, ratio, now);
            return Ok(());
        }

        // Proposer side: we proposed this nonce and are awaiting the
        // accepter's settlement. Only honour it from the peer we actually
        // bound via `receive_accept` — a nonce alone is not enough (this is
        // the fix for the forged-credit gap: previously *any* signer could
        // settle any nonce this agent had proposed).
        let proposer_trade = {
            let mut proposed = self.proposed.lock().unwrap_or_else(|e| e.into_inner());
            match proposed.get(&nonce).map(|t| t.counterparty) {
                Some(Some(expected)) if expected == settle.signer => proposed.remove(&nonce),
                Some(Some(_)) => {
                    return Err(Error::Integrity(format!(
                        "settle nonce {nonce} signed by wrong peer"
                    )));
                }
                Some(None) => {
                    return Err(Error::Integrity(format!(
                        "settle nonce {nonce} has no bound accepter (call receive_accept first)"
                    )));
                }
                None => None,
            }
        };
        if let Some(trade) = proposer_trade {
            let ratio = fulfillment_ratio(&trade.expect, &gives);
            self.ledger.credit(&gives);
            self.record_direct_outcome(settle.signer, ratio, now);
            return Ok(());
        }

        Err(Error::Integrity(format!(
            "settle nonce {nonce} not recognized as an active trade"
        )))
    }

    /// Drain any pending trade (either side) older than `timeout_cycles`
    /// (relative to `now`) that never settled, and penalize its counterparty
    /// as a fully broken bargain. This is what makes broken-bargain
    /// detection automatic rather than something only an external caller
    /// can trigger via [`Self::penalize`]. A proposed trade nobody ever
    /// accepted (no bound counterparty) just expires quietly — there is no
    /// one to blame.
    pub fn sweep_timeouts(&mut self, now: u64, timeout_cycles: u64) -> Vec<BrokenBargain> {
        let mut broken = Vec::new();

        let expired: Vec<u64> = self
            .accepted
            .iter()
            .filter(|(_, t)| now.saturating_sub(t.opened_at) >= timeout_cycles)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in expired {
            if let Some(trade) = self.accepted.remove(&nonce) {
                if let Some(peer) = trade.counterparty {
                    broken.push(self.penalize_and_snapshot(peer, now));
                }
            }
        }

        let expired: Vec<u64> = {
            let proposed = self.proposed.lock().unwrap_or_else(|e| e.into_inner());
            proposed
                .iter()
                .filter(|(_, t)| now.saturating_sub(t.opened_at) >= timeout_cycles)
                .map(|(nonce, _)| *nonce)
                .collect()
        };
        for nonce in expired {
            let trade = {
                let mut proposed = self.proposed.lock().unwrap_or_else(|e| e.into_inner());
                proposed.remove(&nonce)
            };
            if let Some(peer) = trade.and_then(|t| t.counterparty) {
                broken.push(self.penalize_and_snapshot(peer, now));
            }
        }

        broken
    }

    fn penalize_and_snapshot(&mut self, peer: AgentId, now: u64) -> BrokenBargain {
        self.record_direct_outcome(peer, 0.0, now);
        let rep = self.reputation(peer, now);
        BrokenBargain {
            peer,
            trust_after: rep.trust,
            dings_after: rep.dings,
        }
    }

    /// Manually penalize a peer for bad faith detected out-of-band.
    /// Equivalent to a fully broken bargain (ratio `0.0`).
    pub fn penalize(&mut self, peer: AgentId, now: u64) {
        self.record_direct_outcome(peer, 0.0, now);
    }

    fn record_direct_outcome(&mut self, peer: AgentId, ratio: f64, now: u64) {
        let half_life = self.decay_half_life_cycles;
        self.reputation
            .entry(peer)
            .or_default()
            .record_direct_outcome(ratio, now, half_life);
    }

    /// Reputation snapshot for a peer as of `now` (default trustworthy if
    /// unseen). Decay is computed purely at read time; nothing is mutated.
    pub fn reputation(&self, peer: AgentId, now: u64) -> Reputation {
        match self.reputation.get(&peer) {
            Some(record) => record.snapshot(now, self.decay_half_life_cycles),
            None => Reputation::default(),
        }
    }

    /// Sign a report of this agent's own current (already-decayed) direct
    /// view of `subject`, for gossiping to a peer ("direct gossip" — this
    /// agent is the original witness).
    pub fn report_reputation(&self, subject: AgentId, now: u64) -> SignedMessage {
        let rep = self.reputation(subject, now);
        self.sign(&BarterMessage::ReputationReport {
            subject,
            trust: rep.trust,
            dings: rep.dings,
            as_of_cycle: now,
        })
    }

    /// Forward a reputation claim this agent received (direct or already
    /// relayed) to another peer ("indirect gossip"), incrementing the hop
    /// count. Returns `None` if relaying would exceed `MAX_HOPS` or `msg`
    /// is not a reputation message — bounding how far (and how much) a claim
    /// can propagate through the mesh. Relaying a claim does not require
    /// this agent to believe it; that mirrors how gossip actually spreads.
    pub fn relay_reputation_report(&self, msg: &SignedMessage) -> Option<SignedMessage> {
        let inner = msg.message().ok()?;
        let (subject, origin, hops, provenance) = match inner {
            BarterMessage::ReputationReport { subject, .. } => {
                // The message being relayed *is* the hop-1 original — its
                // own bytes become the provenance every future hop carries.
                let provenance = postcard::to_stdvec(msg).ok()?;
                (subject, msg.signer, 1, provenance)
            }
            BarterMessage::RelayedReputationReport {
                subject,
                origin,
                hops,
                provenance,
            } => (subject, origin, hops, provenance),
            _ => return None,
        };
        if hops >= MAX_HOPS {
            return None;
        }
        Some(self.sign(&BarterMessage::RelayedReputationReport {
            subject,
            origin,
            hops: hops + 1,
            provenance,
        }))
    }

    /// Receive a gossiped [`BarterMessage::ReputationReport`] or
    /// [`BarterMessage::RelayedReputationReport`] about a third party and
    /// fold it into that party's hearsay pool, keyed by the *original*
    /// witness (see module docs). Ignored (not an error — this is normal
    /// operation, not a protocol violation) when: the claim is self-vouching
    /// or self-accusing, the immediate sender is already blacklisted in this
    /// agent's own direct view (garbage-in defense), the claim has traveled
    /// too far (`hops > MAX_HOPS`), or the claim is stale/future-dated.
    ///
    /// Hearsay never touches `blacklisted` — see the module docs. It can
    /// only move the continuous trust score.
    pub fn receive_reputation_report(&mut self, report: &SignedMessage, now: u64) -> Result<()> {
        if !self.verify(report) {
            return Err(Error::Integrity(
                "reputation report signature invalid".into(),
            ));
        }
        if self.reputation(report.signer, now).blacklisted {
            return Ok(());
        }
        match report.message()? {
            BarterMessage::ReputationReport {
                subject,
                trust,
                as_of_cycle,
                ..
            } => self.fold_hearsay(subject, report.signer, trust, as_of_cycle, 1, now),
            BarterMessage::RelayedReputationReport {
                subject,
                origin,
                hops,
                provenance,
            } => {
                if hops == 0 || hops > MAX_HOPS {
                    return Ok(());
                }
                let inner: SignedMessage = match postcard::from_bytes(&provenance) {
                    Ok(inner) => inner,
                    Err(_) => return Ok(()),
                };
                if inner.signer != origin {
                    return Ok(());
                }
                if !self.verify_binding(
                    inner.signer,
                    &inner.public_key,
                    &inner.payload,
                    &inner.signature,
                ) {
                    return Ok(());
                }
                let BarterMessage::ReputationReport {
                    subject: inner_subject,
                    trust,
                    as_of_cycle,
                    ..
                } = inner.message()?
                else {
                    return Ok(());
                };
                if inner_subject != subject {
                    // Outer claim and embedded provenance disagree about who
                    // this is even about — tampered or malformed, reject.
                    return Ok(());
                }
                self.fold_hearsay(subject, origin, trust, as_of_cycle, hops, now)
            }
            _ => Err(Error::Invalid("expected a reputation report".into())),
        }
    }

    fn fold_hearsay(
        &mut self,
        subject: AgentId,
        origin: AgentId,
        trust: f64,
        as_of_cycle: u64,
        hops: u32,
        now: u64,
    ) -> Result<()> {
        if subject == self.id || subject == origin {
            return Ok(());
        }
        if as_of_cycle > now || now.saturating_sub(as_of_cycle) > MAX_REPORT_AGE_CYCLES {
            return Ok(());
        }
        let entry = self.reputation.entry(subject).or_default();
        let is_fresher = match entry.hearsay.get(&origin) {
            Some(existing) => as_of_cycle >= existing.as_of_cycle,
            None => true,
        };
        if is_fresher {
            entry.hearsay.insert(
                origin,
                Hearsay {
                    trust: trust.clamp(0.0, 1.0),
                    as_of_cycle,
                    hops,
                },
            );
        }
        Ok(())
    }

    /// Distinct, non-stale original witnesses currently backing `subject`'s
    /// hearsay-derived trust — the auditability answer to "is this broad
    /// consensus or one claim relayed repeatedly?" There is no sound way to
    /// algorithmically detect a well-reputed peer choosing to lie, so this
    /// makes the *breadth* of evidence inspectable instead.
    pub fn hearsay_origins(&self, subject: AgentId, now: u64) -> Vec<AgentId> {
        self.reputation
            .get(&subject)
            .map(|r| r.hearsay_origins(now))
            .unwrap_or_default()
    }

    /// Nonces we have accepted but not yet seen delivered (offeree side).
    pub fn pending_count(&self) -> usize {
        self.accepted.len()
    }

    /// Nonces we have proposed but not yet seen settled (proposer side).
    pub fn proposed_pending_count(&self) -> usize {
        self.proposed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}
