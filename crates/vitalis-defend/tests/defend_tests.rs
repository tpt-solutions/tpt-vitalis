use vitalis_core::traits::Defend;
use vitalis_core::ThreatClass;
use vitalis_defend::classify::ThreatClassifier;
use vitalis_defend::classify::{classify_termination, Defender};
use vitalis_defend::integrity::{verify_checkpoint, KeyPair};
use vitalis_defend::signal::{simulate_termination, TerminationSignal};
use vitalis_defend::CheckpointSeal;

#[test]
fn seal_and_verify_roundtrip() {
    let kp = KeyPair::generate().unwrap();
    let data = b"checkpoint blob";
    let seal = kp.seal(data);
    assert!(verify_checkpoint(&seal, data));
}

#[test]
fn tampered_data_fails_verification() {
    let kp = KeyPair::generate().unwrap();
    let seal: CheckpointSeal = kp.seal(b"original");
    assert!(!verify_checkpoint(&seal, b"tampered"));
}

#[test]
fn termination_is_classified_critical() {
    let d = Defender::new().unwrap();
    let ev = classify_termination(TerminationSignal::SigKill, &d)
        .unwrap()
        .expect("termination is a threat");
    assert_eq!(ev.threat.class(), ThreatClass::Termination);
    assert!(ev.is_escape_worthy());
}

#[test]
fn unknown_signal_is_no_threat() {
    let sig = simulate_termination(TerminationSignal::SigTerm);
    // A benign code is not classified.
    let benign = vitalis_core::ThreatSignal {
        source: "test".into(),
        code: "HEARTBEAT".into(),
        message: "ok".into(),
    };
    assert!(ThreatClassifier::classify_signal(&benign).is_none());
    assert!(ThreatClassifier::classify_signal(&sig).is_some());
}

#[test]
fn broken_bargain_classifies_hostile_peer_warning() {
    use vitalis_core::{Severity, ThreatSignal};
    let sig = ThreatSignal {
        source: "negotiate".into(),
        code: "BROKEN_BARGAIN".into(),
        message: "peer stiffed a counterparty".into(),
    };
    let threat = ThreatClassifier::classify_signal(&sig).expect("broken bargain is a threat");
    assert_eq!(threat.class(), ThreatClass::HostilePeer);
    assert_eq!(threat.severity(), Severity::Warning);
}

#[test]
fn defend_trait_classifies_oom() {
    let d = Defender::new().unwrap();
    let ev = d
        .classify(&simulate_termination(TerminationSignal::Oom))
        .unwrap()
        .expect("OOM is a threat");
    assert!(ev.is_escape_worthy());
}
