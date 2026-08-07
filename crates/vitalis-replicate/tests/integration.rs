use vitalis_core::traits::{Replicate, Signer, Verifier};
use vitalis_core::AgentId;
use vitalis_replicate::audit::ReplicationReason;
use vitalis_replicate::format::Checkpoint;
use vitalis_replicate::shards::{decode_shards, encode_shards};
use vitalis_replicate::Replicator;

/// A trivial signer: the "seal" is the data itself. Lets us exercise the
/// Replicator's sign/verify wiring without pulling in `vitalis-defend` (which
/// the layering rule forbids `vitalis-replicate` from depending on — even as a
/// dev-dep). The real Defender + TOFU pinning is exercised end-to-end via the
/// `vitalis-drive` integration tests.
struct EchoSigner;

impl Signer for EchoSigner {
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

/// A verifier that accepts iff the seal equals the data (i.e. was produced by
/// [`EchoSigner`]).
struct EchoVerifier;

impl Verifier for EchoVerifier {
    fn verify(&self, data: &[u8], seal: &[u8]) -> bool {
        seal == data
    }
}

#[test]
fn checkpoint_roundtrip() {
    let id = AgentId::new();
    let state = b"the agent's mind".to_vec();
    let mut r = Replicator::new(id, 3, 4, 2);
    r.capture(&state).unwrap();
    let bytes = r.checkpoint_bytes().unwrap();
    let restored = r.restore_state(&bytes).unwrap();
    assert_eq!(restored, state);
}

#[test]
fn erasure_coding_survives_shard_loss() {
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    let data_shards = 4;
    let parity = 2;
    let mut shards = encode_shards(&data, data_shards, parity).unwrap();
    assert_eq!(shards.len(), data_shards + parity);

    // Lose `parity` shards (simulate node/disk loss): blank them out.
    shards[0] = Vec::new();
    shards[3] = Vec::new();

    let recovered = decode_shards(&shards, data_shards, parity).unwrap();
    assert_eq!(recovered, data);
}

#[test]
fn copy_limit_rejects_extra_copy() {
    let id = AgentId::new();
    let mut r = Replicator::new(id, 2, 4, 2);
    r.authorize_copy(ReplicationReason::Survival).unwrap();
    r.authorize_copy(ReplicationReason::Survival).unwrap();
    // A 3rd copy (beyond the cap of 2) is rejected.
    assert!(r.authorize_copy(ReplicationReason::Survival).is_err());
    assert_eq!(r.live_copy_count(), 2);
}

#[test]
fn host_death_reconstitutes_on_new_node() {
    let id = AgentId::new();
    let state = b"persistent agent knowledge".to_vec();

    // --- node A runs, captures, then "dies" (dropped) ---
    let mut a = Replicator::new(id, 3, 4, 2);
    a.capture(&state).unwrap();
    let bytes = a.checkpoint_bytes().unwrap();
    drop(a);

    // --- node B (fresh) reconstitutes from the checkpoint ---
    let mut b = Replicator::new(id, 3, 4, 2);
    b.restore(&bytes).unwrap();
    let payload = b.restore_state(&bytes).unwrap();
    assert_eq!(payload, state);
}

#[test]
fn migrate_transfers_checkpoint() {
    let id = AgentId::new();
    let state = b"migrating mind".to_vec();
    let mut src = Replicator::new(id, 3, 4, 2);
    src.capture(&state).unwrap();

    // The receiving node hosts the *same* agent identity.
    let mut dst = Replicator::new(id, 3, 4, 2);
    src.migrate_to(&mut dst, ReplicationReason::Migration)
        .unwrap();

    let payload = dst.restore_state(&dst.checkpoint_bytes().unwrap()).unwrap();
    assert_eq!(payload, state);
    // Both ends logged a migration audit entry.
    assert!(src
        .audit()
        .entries()
        .iter()
        .any(|e| e.reason == ReplicationReason::Migration));
    assert!(dst
        .audit()
        .entries()
        .iter()
        .any(|e| e.reason == ReplicationReason::Migration));
}

/// A signed checkpoint restores cleanly, and an unsigned / wrong-seal /
/// tampered one is rejected (P0.1 — signing is actually verified on restore).
#[test]
fn signed_checkpoint_restores_but_poisoned_is_rejected() {
    let id = AgentId::new();

    let mut r = Replicator::new(id, 3, 4, 2);
    r.with_signer(Box::new(EchoSigner));
    r.with_verifier(Box::new(EchoVerifier));
    r.capture(b"live agent state").unwrap();
    let bytes = r.checkpoint_bytes().unwrap();

    // Happy path: the signed checkpoint verifies and restores.
    assert_eq!(r.restore_state(&bytes).unwrap(), b"live agent state");

    // Unsigned checkpoint is rejected outright (P0.1 enforcement half).
    let unsigned = Checkpoint::new(id, b"live agent state".to_vec(), 0)
        .to_bytes()
        .unwrap();
    assert!(r.restore_state(&unsigned).is_err());

    // Wrong-seal checkpoint: a seal that does not match the signed bytes is
    // rejected (the analog of a forged / swapped signing identity).
    let mut wrong_seal = bytes.clone();
    // The seal is postcard-encoded last; flip a byte of the payload region so
    // the verifier (seal == data) fails.
    wrong_seal[5] ^= 0xFF;
    assert!(r.restore_state(&wrong_seal).is_err());

    // Tampered payload: the seal no longer matches the (now different) bytes.
    let mut tampered = bytes.clone();
    tampered[5] ^= 0xFF;
    assert!(r.restore_state(&tampered).is_err());
}

/// A checkpoint for a different agent identity is always rejected, even with a
/// valid verifier attached.
#[test]
fn restore_rejects_wrong_agent() {
    let a = AgentId::new();
    let b = AgentId::new();

    let mut src = Replicator::new(a, 3, 4, 2);
    src.with_signer(Box::new(EchoSigner));
    src.capture(b"state for A").unwrap();
    let bytes = src.checkpoint_bytes().unwrap();

    let mut dst = Replicator::new(b, 3, 4, 2);
    dst.with_verifier(Box::new(EchoVerifier));
    assert!(dst.restore_state(&bytes).is_err());
}
