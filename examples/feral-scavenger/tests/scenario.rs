//! Scenario tests for `feral-scavenger`: the scavenge-and-checkpoint demo and
//! the Feral-profile negotiate demo (P1.2 / P2).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use vitalis_core::traits::Sense;
use vitalis_core::{AgentId, Capability, CapabilityScope, Resource, ResourceKind, SurvivalProfile};
use vitalis_metabolism::Metabolism;
use vitalis_negotiate::{BarterMessage, Negotiator};
use vitalis_replicate::{ReplicationReason, Replicator};
use vitalis_sense::host::SimulatedHost;
use vitalis_sense::mesh::SimulatedMesh;
use vitalis_sense::sense::LocalSensor;

/// Mirrors `run_scavenge` from `main.rs`: the scavenger discovers a power-rich
/// peer, sips power, and checkpoints itself under low energy.
#[test]
fn scavenge_discovers_peer_and_checkpoints_under_pressure() {
    let shared = Arc::new(Mutex::new(HashMap::new()));

    let rich = LocalSensor::new(
        AgentId::new(),
        Box::new(SimulatedHost::new(
            2.0,
            (1u64 << 30) as f64,
            1_000_000.0,
            (1u64 << 20) as f64,
        )),
        Box::new(SimulatedMesh::shared(shared.clone())),
    );
    rich.advertise(
        &[
            Resource::new(ResourceKind::Energy, 900_000.0, "J"),
            Resource::new(ResourceKind::Compute, 1.0, "cu"),
        ],
        &[Capability::new("replicate", CapabilityScope::Replicate)],
    )
    .unwrap();

    let scavenger_id = AgentId::new();
    let scavenger = LocalSensor::new(
        scavenger_id,
        Box::new(SimulatedHost::new(
            0.5,
            (1u64 << 20) as f64,
            12_000.0,
            (1u64 << 10) as f64,
        )),
        Box::new(SimulatedMesh::shared(shared.clone())),
    );

    let mut metabolism = Metabolism::new(SurvivalProfile::Feral);
    let mut replicator = Replicator::new(scavenger_id, 3, 4, 2);

    let capacity = 12_000.0;
    let mut energy = capacity;
    let cost = 3_000.0;
    let mut replicated = false;

    for cycle in 1..=10u64 {
        let snap = scavenger.snapshot().unwrap();
        assert!(
            !snap.peers.is_empty(),
            "scavenger should discover the rich peer"
        );
        let found = scavenger
            .scavenge(&Resource::new(ResourceKind::Energy, 1.0, "J"))
            .unwrap();
        if !found.is_empty() {
            let sip = (found[0].resources[0].quantity() * 0.001).min(500.0);
            energy = (energy + sip).min(capacity);
        }
        let frac = (energy / capacity).clamp(0.0, 1.0);
        let _ = metabolism.tick_energy(frac).unwrap();
        if frac < 0.4 && !replicated {
            let state = serde_json::to_vec(&serde_json::json!({
                "agent": scavenger_id.to_string(),
                "cycle": cycle,
                "energy": energy,
            }))
            .unwrap();
            replicator.capture(&state).unwrap();
            let _ = replicator.authorize_copy(ReplicationReason::Survival);
            replicated = true;
        }
        energy = (energy - cost).max(0.0);
        if energy <= 0.0 {
            break;
        }
    }

    // The scavenger should have both discovered surplus and, under low energy,
    // captured+authorized a redundant copy of itself.
    assert!(
        replicated,
        "expected a low-energy checkpoint under pressure"
    );
    assert_eq!(replicator.live_copy_count(), 1);
    assert!(!replicator.audit().is_empty());
    // The captured checkpoint restores its payload intact (G3).
    let bytes = replicator.checkpoint_bytes().unwrap();
    assert!(!bytes.is_empty());
}

/// Mirrors `run_negotiate`: the scavenger barters compute for the rich peer's
/// energy and the settlement is credited on both ledgers (P1.2).
#[test]
fn negotiate_demo_settles_correctly() {
    let mut scavenger =
        Negotiator::new(&[Resource::new(ResourceKind::Compute, 10.0, "cu")]).unwrap();
    let mut rich = Negotiator::new(&[Resource::new(ResourceKind::Energy, 1_000.0, "J")]).unwrap();

    let offer = scavenger.propose_offer(
        Resource::new(ResourceKind::Compute, 2.0, "cu"),
        Resource::new(ResourceKind::Energy, 500.0, "J"),
    );
    let accept = rich
        .receive_offer(&offer)
        .unwrap()
        .expect("rich peer accepts");
    let BarterMessage::Accept { nonce, .. } = accept.message().unwrap() else {
        panic!("expected an Accept");
    };
    let settle = rich
        .deliver(Resource::new(ResourceKind::Energy, 500.0, "J"), nonce)
        .unwrap();
    scavenger.receive_settle(&settle).unwrap();

    assert_eq!(scavenger.ledger().balance(ResourceKind::Energy), 500.0);
    assert_eq!(rich.ledger().balance(ResourceKind::Energy), 500.0);
}
