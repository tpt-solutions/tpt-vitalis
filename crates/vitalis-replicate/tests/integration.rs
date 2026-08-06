use vitalis_core::traits::Replicate;
use vitalis_core::AgentId;
use vitalis_replicate::audit::ReplicationReason;
use vitalis_replicate::shards::{decode_shards, encode_shards};
use vitalis_replicate::Replicator;

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
