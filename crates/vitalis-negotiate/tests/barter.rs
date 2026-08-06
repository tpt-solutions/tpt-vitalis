use vitalis_core::{Resource, ResourceKind};
use vitalis_negotiate::message::BarterMessage;
use vitalis_negotiate::Negotiator;

fn compute(q: f64) -> Resource {
    Resource::new(ResourceKind::Compute, q, "cu")
}
fn storage(q: f64) -> Resource {
    Resource::new(ResourceKind::Storage, q, "B")
}

#[test]
fn honest_barter_updates_both_ledgers() {
    let mut a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut b = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = a.propose_offer(compute(10.0), storage(50.0));
    let nonce = match offer.message().unwrap() {
        BarterMessage::Offer { nonce, .. } => nonce,
        _ => panic!("offer"),
    };

    let _accept = b.receive_offer(&offer).unwrap().expect("B accepts");
    // B delivers its side (storage 50), A receives it.
    let b_settle = b.deliver(storage(50.0), nonce).unwrap();
    a.receive_settle(&b_settle).unwrap();
    // A delivers its side (compute 10), B receives it.
    let a_settle = a.deliver(compute(10.0), nonce).unwrap();
    b.receive_settle(&a_settle).unwrap();

    assert_eq!(a.ledger().balance(ResourceKind::Compute), 0.0);
    assert_eq!(a.ledger().balance(ResourceKind::Storage), 50.0);
    assert_eq!(b.ledger().balance(ResourceKind::Compute), 10.0);
    assert_eq!(b.ledger().balance(ResourceKind::Storage), 50.0);
}

#[test]
fn bad_faith_peer_is_penalized() {
    let mut honest = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut cheater = Negotiator::new(&[storage(100.0)]).unwrap();

    let offer = honest.propose_offer(compute(10.0), storage(50.0));
    let accept = cheater
        .receive_offer(&offer)
        .unwrap()
        .expect("cheater accepts");
    let _ = accept;
    // Cheater recorded the accepted nonce but never delivers.
    assert_eq!(cheater.pending_count(), 1);

    // Honest agent, detecting no settlement, penalizes the cheater.
    honest.penalize(cheater.id());
    assert_eq!(honest.reputation(cheater.id()).dings, 1);
    // A second broken bargain blacklists the peer.
    honest.penalize(cheater.id());
    assert!(honest.reputation(cheater.id()).blacklisted);
}

#[test]
fn cannot_afford_is_declined() {
    let a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut poor = Negotiator::new(&[]).unwrap();
    let offer = a.propose_offer(compute(10.0), storage(50.0));
    assert!(poor.receive_offer(&offer).unwrap().is_none());
}

#[test]
fn tampered_signature_is_rejected() {
    let a = Negotiator::new(&[compute(10.0)]).unwrap();
    let mut offer = a.propose_offer(compute(10.0), storage(50.0));
    // Flip a byte in the signature.
    let last = offer.signature.len() - 1;
    offer.signature[last] ^= 0xFF;
    assert!(!a.verify(&offer));
}

#[test]
fn identities_are_distinct() {
    let a = Negotiator::new(&[compute(1.0)]).unwrap();
    let b = Negotiator::new(&[compute(1.0)]).unwrap();
    assert_ne!(a.id(), b.id());
}
