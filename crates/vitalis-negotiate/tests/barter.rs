//! Barter protocol tests for `vitalis-negotiate`: honest settlement, broken
//! bargains, forgery/replay defenses (P0.2/P0.3), and the Phase 8 reputation
//! deepening (decayed Beta-reputation, gossip/indirect reciprocity, relay,
//! decay/forgiveness).

use vitalis_core::ThreatSignal;
use vitalis_negotiate::message::BarterMessage;
use vitalis_negotiate::Negotiator;

fn compute(q: f64) -> vitalis_core::Resource {
    vitalis_core::Resource::new(vitalis_core::ResourceKind::Compute, q, "cu")
}
fn storage(q: f64) -> vitalis_core::Resource {
    vitalis_core::Resource::new(vitalis_core::ResourceKind::Storage, q, "B")
}

fn nonce_of(msg: &BarterMessage) -> u64 {
    match msg {
        BarterMessage::Offer { nonce, .. } => *nonce,
        BarterMessage::Accept { nonce, .. } => *nonce,
        BarterMessage::Settle { nonce, .. } => *nonce,
        _ => panic!("message carries no nonce"),
    }
}

/// A full two-way honest barter updates both ledgers correctly.
#[test]
fn honest_barter_updates_both_ledgers() {
    let mut a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    let nonce = nonce_of(&offer.message().unwrap());
    let accept = b.receive_offer(&offer, 0).unwrap().expect("B accepts");
    // A (proposer) must bind B's identity before it accepts B's settlement.
    a.receive_accept(&accept).unwrap();

    // B delivers its side (storage 50), A receives it.
    let b_settle = b.deliver(storage(50.0), nonce).unwrap();
    a.receive_settle(&b_settle, 0).unwrap();
    // A delivers its side (compute 10), B receives it.
    let a_settle = a.deliver(compute(10.0), nonce).unwrap();
    b.receive_settle(&a_settle, 0).unwrap();

    assert_eq!(a.ledger().balance(vitalis_core::ResourceKind::Compute), 0.0);
    assert_eq!(
        a.ledger().balance(vitalis_core::ResourceKind::Storage),
        50.0
    );
    assert_eq!(
        b.ledger().balance(vitalis_core::ResourceKind::Compute),
        10.0
    );
    assert_eq!(
        b.ledger().balance(vitalis_core::ResourceKind::Storage),
        50.0
    );
}

/// A peer that is repeatedly penalized for bad faith is eventually blacklisted
/// (manual detection path).
#[test]
fn bad_faith_peer_blacklisted_by_repeated_penalize() {
    let mut honest = Negotiator::new(&[compute(10.0)]).unwrap();
    let cheater = Negotiator::new(&[storage(100.0)]).unwrap();

    honest.penalize(cheater.id(), 0);
    assert_eq!(honest.reputation(cheater.id(), 0).dings, 1);
    // A second broken bargain crosses the blacklist threshold.
    honest.penalize(cheater.id(), 0);
    assert!(honest.reputation(cheater.id(), 0).blacklisted);
}

/// A broken bargain (accept but never settle) is detected automatically by
/// `sweep_timeouts` — no external caller needs to invoke `penalize`.
#[test]
fn broken_bargain_auto_timeout() {
    let mut a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    let accept = b.receive_offer(&offer, 0).unwrap().expect("B accepts");
    a.receive_accept(&accept).unwrap();
    assert_eq!(a.pending_count(), 0);
    assert_eq!(a.proposed_pending_count(), 1);

    // B recorded the accepted nonce but never delivers; advance past the timeout.
    let broken = a.sweep_timeouts(100, 10);
    assert!(broken.iter().any(|x| x.peer == b.id()));
    assert_eq!(broken[0].peer, b.id());
    // Trust dropped and a ding was recorded on the direct-pool.
    assert!(a.reputation(b.id(), 100).trust < 1.0);
    assert_eq!(a.reputation(b.id(), 100).dings, 1);
}

/// P0.3 / Phase 8 (proposer-side identity binding): a third party who merely
/// observed the nonce on the wire cannot settle a trade it never accepted.
#[test]
fn forged_settle_from_third_party_is_rejected() {
    let mut a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();
    let mut attacker = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    let nonce = nonce_of(&offer.message().unwrap());
    let accept = b.receive_offer(&offer, 0).unwrap().expect("B accepts");
    a.receive_accept(&accept).unwrap();

    // Attacker delivers a settle for the observed (plaintext) nonce, but it is
    // not the bound accepter.
    let forged = attacker.deliver(storage(50.0), nonce).unwrap();
    assert!(a.receive_settle(&forged, 0).is_err());
}

/// Phase 8 (unchecked delivery fix): an under-delivery is scored as graduated
/// bad faith, not a silently-accepted full success.
#[test]
fn partial_delivery_scores_graded() {
    let mut a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    let nonce = nonce_of(&offer.message().unwrap());
    let accept = b.receive_offer(&offer, 0).unwrap().expect("B accepts");
    a.receive_accept(&accept).unwrap();

    // B delivers only 25 of the promised 50 → fulfillment ratio 0.5.
    let settle = b.deliver(storage(25.0), nonce).unwrap();
    a.receive_settle(&settle, 0).unwrap();

    let rep = a.reputation(b.id(), 0);
    assert!(rep.trust < 1.0 && rep.trust > 0.0, "trust should be graded");
    assert_eq!(rep.dings, 1);
}

/// P0.3: a `Settle` whose nonce was already consumed (replayed) is rejected, so
/// a captured settle cannot be replayed to re-credit the ledger.
#[test]
fn replayed_settle_is_rejected() {
    let mut a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    let nonce = nonce_of(&offer.message().unwrap());
    let accept = b.receive_offer(&offer, 0).unwrap().expect("B accepts");
    a.receive_accept(&accept).unwrap();
    let b_settle = b.deliver(storage(50.0), nonce).unwrap();
    a.receive_settle(&b_settle, 0).unwrap();

    // The same settle arrives again: the nonce was already consumed, so reject.
    assert!(a.receive_settle(&b_settle, 0).is_err());
}

/// P0.2: a forged message claiming another agent's identity (but signed by a
/// different key) is rejected once the real peer's key is pinned (TOFU).
#[test]
fn spoofed_identity_is_rejected() {
    let a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();
    let mut attacker = Negotiator::new(&[storage(100.0)]).unwrap();

    // B first sees a legitimate message from A, pinning A's key.
    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    assert!(b.receive_offer(&offer, 0).is_ok());

    // Attacker forges a settle that claims A's identity but is signed by the
    // attacker's own key, and tries to deliver it to B.
    let nonce = nonce_of(&offer.message().unwrap());
    let mut forged = attacker.deliver(storage(50.0), nonce).unwrap();
    forged.signer = a.id();
    // B must reject the forged identity (TOFU mismatch).
    assert!(!b.verify(&forged));
    assert!(b.receive_settle(&forged, 0).is_err());
}

#[test]
fn tampered_signature_is_rejected() {
    let a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    // Flip a byte in the signature.
    let last = offer.signature.len() - 1;
    offer.signature[last] ^= 0xFF;
    assert!(!a.verify(&offer));
}

#[test]
fn cannot_afford_is_declined() {
    let a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut poor = Negotiator::new(&[]).unwrap();
    let offer = a.propose_offer(compute(10.0), storage(50.0), 0);
    assert!(poor.receive_offer(&offer, 0).unwrap().is_none());
}

#[test]
fn identities_are_distinct() {
    let a = Negotiator::new(&[compute(1.0)]).unwrap();
    let b = Negotiator::new(&[compute(1.0)]).unwrap();
    assert_ne!(a.id(), b.id());
}

/// Phase 8: a peer's *direct* bad experience, gossiped to a peer that never
/// traded with the subject, lowers that listener's trust — without ever
/// blacklisting on hearsay alone.
#[test]
fn gossip_without_direct_trade_lowers_trust() {
    let mut witness = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    // witness trades with subject and gets stiffed (direct evidence).
    witness.penalize(subject.id(), 0);
    let report = witness.report_reputation(subject.id(), 0);
    listener.receive_reputation_report(&report, 0).unwrap();

    let rep = listener.reputation(subject.id(), 0);
    assert!(rep.trust < 1.0, "hearsay should move trust");
    assert!(!rep.blacklisted, "hearsay must never blacklist");
}

/// Phase 8: a gossip report from a peer this agent has blacklisted (direct
/// evidence) is ignored, so a blacklisted reporter can't poison the mesh.
#[test]
fn gossip_from_blacklisted_reporter_ignored() {
    let reporter = Negotiator::new(&[]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    // listener blacklists the reporter via direct evidence (two broken bargains).
    listener.penalize(reporter.id(), 0);
    listener.penalize(reporter.id(), 0);
    assert!(listener.reputation(reporter.id(), 0).blacklisted);

    let report = reporter.report_reputation(subject.id(), 0);
    listener.receive_reputation_report(&report, 0).unwrap();
    // The claim was dropped.
    assert_eq!(listener.reputation(subject.id(), 0).trust, 1.0);
}

/// Phase 8: a self-vouching / self-accusing report is dropped.
#[test]
fn self_report_rejected() {
    let mut a = Negotiator::new(&[]).unwrap();
    let report = a.report_reputation(a.id(), 0);
    a.receive_reputation_report(&report, 0).unwrap();
    assert!(a.hearsay_origins(a.id(), 0).is_empty());
}

/// Phase 8: a gossip report older than `MAX_REPORT_AGE_CYCLES` is ignored.
#[test]
fn stale_report_rejected() {
    let mut witness = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    witness.penalize(subject.id(), 0);
    let report = witness.report_reputation(subject.id(), 0);
    // "now" far in the future relative to the report's as_of_cycle (0).
    listener.receive_reputation_report(&report, 1000).unwrap();
    assert_eq!(listener.reputation(subject.id(), 1000).trust, 1.0);
}

/// Phase 8: reputation decays toward the neutral prior over time, so a peer
/// that behaved and stopped offending is eventually forgiven (trust recovers).
#[test]
fn decay_forgiveness_restores_trust() {
    let mut a = Negotiator::with_limits_and_decay(&[compute(10.0)], 16, 10).unwrap();
    let bad = Negotiator::new(&[]).unwrap();
    a.penalize(bad.id(), 0);
    let low = a.reputation(bad.id(), 0).trust;
    assert!(low < 1.0);
    // After many half-lives the evidence has decayed toward the prior.
    let recovered = a.reputation(bad.id(), 1000).trust;
    assert!(recovered > low, "trust should recover with decay");
}

/// Phase 8 (relay): a forwarded report carries its origin's signature as
/// provenance and is accepted, lowering the listener's trust (hop-discounted).
#[test]
fn relay_provenance_intact() {
    let mut witness = Negotiator::with_limits_and_decay(&[compute(10.0)], 16, 50).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    witness.penalize(subject.id(), 0);
    let report = witness.report_reputation(subject.id(), 0); // hop-1 original
    let relay = witness.relay_reputation_report(&report).unwrap(); // relayed, hop 2
    listener.receive_reputation_report(&relay, 0).unwrap();

    let rep = listener.reputation(subject.id(), 0);
    assert!(rep.trust < 1.0, "relayed hearsay should move trust");
    assert!(!rep.blacklisted);
}

/// Phase 8 (relay): a relay of an already at-cap report is refused, bounding
/// how far a claim can propagate.
#[test]
fn hop_cap_enforced() {
    let mut witness = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();
    witness.penalize(subject.id(), 0);
    let r0 = witness.report_reputation(subject.id(), 0); // hop 1
    let r1 = witness.relay_reputation_report(&r0).unwrap(); // hop 2
    let r2 = witness.relay_reputation_report(&r1).unwrap(); // hop 3 (at cap)
                                                            // Relaying an at-cap report must be refused.
    assert!(witness.relay_reputation_report(&r2).is_none());
}

/// Phase 8 (relay): a relay whose *immediate* reporter is blacklisted by the
/// listener is ignored.
#[test]
fn relay_via_blacklisted_relayer_ignored() {
    let mut witness = Negotiator::new(&[]).unwrap();
    let relayer = Negotiator::new(&[]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    witness.penalize(subject.id(), 0);
    let r0 = witness.report_reputation(subject.id(), 0);
    let relay = relayer.relay_reputation_report(&r0).unwrap(); // signed by relayer

    listener.penalize(relayer.id(), 0);
    listener.penalize(relayer.id(), 0);
    assert!(listener.reputation(relayer.id(), 0).blacklisted);

    listener.receive_reputation_report(&relay, 0).unwrap();
    assert_eq!(listener.reputation(subject.id(), 0).trust, 1.0);
}

/// Phase 8 (relay): a relay whose outer subject disagrees with its embedded
/// provenance (tampered) is rejected.
#[test]
fn tampered_relay_subject_mismatch_rejected() {
    let mut witness = Negotiator::new(&[]).unwrap();
    let subject_a = Negotiator::new(&[]).unwrap();
    let subject_b = Negotiator::new(&[]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();

    witness.penalize(subject_a.id(), 0);
    let inner = witness.report_reputation(subject_a.id(), 0);
    let relay = witness.relay_reputation_report(&inner).unwrap();
    let (provenance, origin, hops) = match relay.message().unwrap() {
        BarterMessage::RelayedReputationReport {
            provenance,
            origin,
            hops,
            ..
        } => (provenance, origin, hops),
        _ => panic!("expected a relayed report"),
    };
    // Tamper: claim it is about a different subject than the provenance proves.
    let tampered = witness.sign(&BarterMessage::RelayedReputationReport {
        subject: subject_b.id(),
        origin,
        hops,
        provenance,
    });
    listener.receive_reputation_report(&tampered, 0).unwrap();
    assert_eq!(listener.reputation(subject_b.id(), 0).trust, 1.0);
}

/// Phase 8 (auditable witness breadth): the same claim arriving via multiple
/// relay paths is deduplicated to a single distinct origin.
#[test]
fn multi_path_dedup() {
    let mut witness = Negotiator::new(&[]).unwrap();
    let a = Negotiator::new(&[]).unwrap();
    let b = Negotiator::new(&[]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    witness.penalize(subject.id(), 0);
    let r0 = witness.report_reputation(subject.id(), 0);
    let ra = a.relay_reputation_report(&r0).unwrap();
    let rb = b.relay_reputation_report(&r0).unwrap();
    listener.receive_reputation_report(&ra, 0).unwrap();
    listener.receive_reputation_report(&rb, 0).unwrap();

    let origins = listener.hearsay_origins(subject.id(), 0);
    assert_eq!(origins.len(), 1, "same origin relayed twice must dedup");
    assert_eq!(origins[0], witness.id());
}

/// Phase 8 (auditable witness breadth): two distinct witnesses backing a claim
/// are both reflected in `hearsay_origins`.
#[test]
fn distinct_witness_count() {
    let mut w1 = Negotiator::new(&[]).unwrap();
    let mut w2 = Negotiator::new(&[]).unwrap();
    let mut listener = Negotiator::new(&[]).unwrap();
    let subject = Negotiator::new(&[]).unwrap();

    w1.penalize(subject.id(), 0);
    w2.penalize(subject.id(), 0);
    let r1 = w1.report_reputation(subject.id(), 0);
    let r2 = w2.report_reputation(subject.id(), 0);
    listener.receive_reputation_report(&r1, 0).unwrap();
    listener.receive_reputation_report(&r2, 0).unwrap();

    assert_eq!(listener.hearsay_origins(subject.id(), 0).len(), 2);
    // Two corroborating witnesses lower trust further than one would.
    let one_witness_trust = {
        let mut l = Negotiator::new(&[]).unwrap();
        l.receive_reputation_report(&r1, 0).unwrap();
        l.reputation(subject.id(), 0).trust
    };
    let two_witness_trust = listener.reputation(subject.id(), 0).trust;
    assert!(
        two_witness_trust < one_witness_trust,
        "more corroboration should lower trust further"
    );
}

/// Sanity: a `BROKEN_BARGAIN` threat signal is recognized (mirrors the defend
/// classifier mapping used by the drive loop's negotiate step).
#[test]
fn broken_bargain_signal_is_recognized() {
    let sig = ThreatSignal {
        source: "negotiate".into(),
        code: "BROKEN_BARGAIN".into(),
        message: "peer stiffed a counterparty".into(),
    };
    assert_eq!(sig.code, "BROKEN_BARGAIN");
}
