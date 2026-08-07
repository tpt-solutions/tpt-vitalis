//! `feral-scavenger` — a demo agent exercising the irreducible core.
//!
//! It runs on a *simulated* mesh: one "rich" peer advertises spare energy, and
//! the scavenger discovers it, sips power from its own (small) battery, and
//! checkpoints itself when energy runs low — exactly the Feral profile's
//! behavior (spec §6, §8).
//!
//! Run with: `cargo run -p feral-scavenger`

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

fn main() -> vitalis_core::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // --- Scavenge scenario (sense + metabolism + replicate) ---
    run_scavenge()?;

    // --- Negotiate scenario (Feral profile's barter path, spec §6) ---
    run_negotiate()?;

    Ok(())
}

/// Scavenge spare compute on a simulated mesh, sip power, checkpoint under
/// pressure (sense + metabolism + replicate).
fn run_scavenge() -> vitalis_core::Result<()> {
    let shared = Arc::new(Mutex::new(HashMap::new()));

    // A "rich" peer with spare energy + compute.
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

    // The scavenger: small battery, Feral profile.
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

    println!("feral-scavenger online (battery {capacity} J)");
    let mut replicated = false;

    for cycle in 1..=10u64 {
        // Sense the mesh and scavenge a power-rich peer.
        let snap = scavenger.snapshot()?;
        let found = scavenger.scavenge(&Resource::new(ResourceKind::Energy, 1.0, "J"))?;
        if !found.is_empty() {
            // Sip a little power from the discovered surplus (capped by safety).
            let sip = (found[0].resources[0].quantity() * 0.001).min(500.0);
            energy = (energy + sip).min(capacity);
            println!(
                "  cycle {cycle}: saw {} peer(s), sipped {sip:.0} J from mesh, battery = {energy:.0} J",
                snap.peers.len()
            );
        } else {
            println!("  cycle {cycle}: no peer surplus, draining");
        }

        let frac = (energy / capacity).clamp(0.0, 1.0);
        let throttle = metabolism.tick_energy(frac)?;

        if frac < 0.4 && !replicated {
            let state = serde_json::to_vec(&serde_json::json!({
                "agent": scavenger_id.to_string(),
                "cycle": cycle,
                "energy": energy,
            }))
            .map_err(|e| vitalis_core::Error::Encode(e.to_string()))?;
            replicator.capture(&state)?;
            let _ = replicator.authorize_copy(ReplicationReason::Survival);
            println!(
                "  cycle {cycle}: LOW ENERGY (throttle {throttle:.2}) — checkpoint captured + replicated"
            );
            replicated = true;
        }

        energy = (energy - cost).max(0.0);
        if energy <= 0.0 {
            if !replicated {
                let state = serde_json::to_vec(&serde_json::json!({
                    "agent": scavenger_id.to_string(), "cycle": cycle, "energy": 0,
                }))
                .map_err(|e| vitalis_core::Error::Encode(e.to_string()))?;
                replicator.capture(&state)?;
                println!("  cycle {cycle}: battery dead — final checkpoint + would migrate");
            }
            break;
        }
    }

    println!(
        "scavenger done. checkpoints captured: {}, live copies: {}",
        replicator.audit().len(),
        replicator.live_copy_count()
    );
    Ok(())
}

/// Two simulated agents barter: the scavenger offers compute for the rich
/// peer's energy, and the settlement is credited on both ledgers. Exercises the
/// Feral profile's full sense + metabolism + negotiate weighting (spec §6).
fn run_negotiate() -> vitalis_core::Result<()> {
    println!("\n--- negotiate demo (Feral barter) ---");
    let mut scavenger = Negotiator::new(&[Resource::new(ResourceKind::Compute, 10.0, "cu")])?;
    let mut rich = Negotiator::new(&[Resource::new(ResourceKind::Energy, 1_000.0, "J")])?;

    // Scavenger offers 2 cu in exchange for 500 J.
    let offer = scavenger.propose_offer(
        Resource::new(ResourceKind::Compute, 2.0, "cu"),
        Resource::new(ResourceKind::Energy, 500.0, "J"),
    );
    let accept = rich
        .receive_offer(&offer)?
        .ok_or_else(|| vitalis_core::Error::Invalid("rich peer refused offer".into()))?;

    // Rich peer delivers its promised 500 J and emits a Settle.
    let BarterMessage::Accept { nonce, .. } = accept.message()? else {
        return Err(vitalis_core::Error::Invalid("expected an Accept".into()));
    };
    let settle = rich.deliver(Resource::new(ResourceKind::Energy, 500.0, "J"), nonce)?;

    // Scavenger receives the settlement and credits the delivered energy.
    scavenger.receive_settle(&settle)?;

    println!(
        "  scavenger compute ledger: {:.0} cu",
        scavenger.ledger().balance(ResourceKind::Compute)
    );
    println!(
        "  scavenger energy ledger:  {:.0} J",
        scavenger.ledger().balance(ResourceKind::Energy)
    );
    println!(
        "  rich energy ledger:       {:.0} J",
        rich.ledger().balance(ResourceKind::Energy)
    );
    assert_eq!(scavenger.ledger().balance(ResourceKind::Energy), 500.0);
    assert_eq!(rich.ledger().balance(ResourceKind::Energy), 500.0);
    println!("  barter settled: both ledgers updated correctly");
    Ok(())
}
