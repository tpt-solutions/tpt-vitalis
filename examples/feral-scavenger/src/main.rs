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

    // --- Reputation deepening (Phase 8): gossip + multi-hop relay ---
    run_gossip()?;

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

    // Scavenger offers 2 cu in exchange for 500 J (the `now` cycle is threaded
    // through every step for reputation decay/timeout bookkeeping).
    let offer = scavenger.propose_offer(
        Resource::new(ResourceKind::Compute, 2.0, "cu"),
        Resource::new(ResourceKind::Energy, 500.0, "J"),
        0,
    );
    let accept = rich
        .receive_offer(&offer, 0)?
        .ok_or_else(|| vitalis_core::Error::Invalid("rich peer refused offer".into()))?;

    // The proposer (scavenger) binds the accepter's identity to the nonce, so a
    // third party that merely observed it on the wire cannot settle it.
    scavenger.receive_accept(&accept)?;

    // Rich peer delivers its promised 500 J and emits a Settle.
    let BarterMessage::Accept { nonce, .. } = accept.message()? else {
        return Err(vitalis_core::Error::Invalid("expected an Accept".into()));
    };
    let settle = rich.deliver(Resource::new(ResourceKind::Energy, 500.0, "J"), nonce)?;

    // Scavenger receives the settlement and credits the delivered energy.
    scavenger.receive_settle(&settle, 0)?;

    // Rich peer receives the scavenger's compute in return.
    let scav_settle = scavenger.deliver(Resource::new(ResourceKind::Compute, 2.0, "cu"), nonce)?;
    rich.receive_settle(&scav_settle, 0)?;

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

/// Phase 8 reputation deepening demo: a dishonest peer (the "liar") stiffs two
/// honest peers; each reports it, and a third honest peer that never traded
/// with the liar directly learns to distrust it from gossip alone — dropping
/// its trust further with corroboration, but never blacklisting on hearsay.
///
/// We also run the liar's detected broken bargain through
/// `vitalis_defend::ThreatClassifier` to show the same signal the drive loop
/// classifies each cycle.
fn run_gossip() -> vitalis_core::Result<()> {
    use vitalis_core::ThreatClass;
    use vitalis_defend::classify::ThreatClassifier;

    println!("\n--- reputation deepening demo (Phase 8 gossip + relay) ---");
    let mut honest1 = Negotiator::new(&[Resource::new(ResourceKind::Compute, 10.0, "cu")])?;
    let mut honest2 = Negotiator::new(&[Resource::new(ResourceKind::Compute, 10.0, "cu")])?;
    let mut honest3 = Negotiator::new(&[Resource::new(ResourceKind::Compute, 10.0, "cu")])?;
    let mut liar = Negotiator::new(&[Resource::new(ResourceKind::Energy, 1_000.0, "J")])?;

    // honest1 trades with the liar; the liar accepts then never delivers.
    let o1 = honest1.propose_offer(
        Resource::new(ResourceKind::Compute, 1.0, "cu"),
        Resource::new(ResourceKind::Energy, 100.0, "J"),
        1,
    );
    let a1 = liar.receive_offer(&o1, 1)?.expect("liar accepts");
    honest1.receive_accept(&a1)?;
    // No delivery: honest1 sweeps the timeout and records a broken bargain.
    let broken = honest1.sweep_timeouts(100, 10);
    assert_eq!(broken.len(), 1);
    assert!(honest1.reputation(liar.id(), 100).blacklisted);

    // honest3 likewise gets stiffed (its own broken bargain → direct evidence).
    let o3 = honest3.propose_offer(
        Resource::new(ResourceKind::Compute, 1.0, "cu"),
        Resource::new(ResourceKind::Energy, 100.0, "J"),
        1,
    );
    let a3 = liar.receive_offer(&o3, 1)?.expect("liar accepts");
    honest3.receive_accept(&a3)?;
    assert_eq!(honest3.sweep_timeouts(100, 10).len(), 1);

    // Both honest peers report the liar. honest2 never traded with the liar.
    let r1 = honest1.report_reputation(liar.id(), 100);
    let r3 = honest3.report_reputation(liar.id(), 100);
    honest2.receive_reputation_report(&r1, 100)?;
    let trust_after_one = honest2.reputation(liar.id(), 100).trust;
    honest2.receive_reputation_report(&r3, 100)?;
    let trust_after_two = honest2.reputation(liar.id(), 100).trust;

    println!(
        "  honest2 trust in liar: {:.3} (1 report) -> {:.3} (2 reports)",
        trust_after_one, trust_after_two
    );
    assert!(trust_after_one < 1.0, "hearsay should lower trust");
    assert!(
        trust_after_two < trust_after_one,
        "corroboration should lower trust further"
    );
    assert!(
        !honest2.reputation(liar.id(), 100).blacklisted,
        "hearsay must never blacklist"
    );
    assert_eq!(honest2.hearsay_origins(liar.id(), 100).len(), 2);

    // Feed the broken-bargain signal through the defend classifier, as the
    // drive loop does each cycle.
    let sig = vitalis_core::ThreatSignal {
        source: "negotiate".into(),
        code: "BROKEN_BARGAIN".into(),
        message: format!("peer {} broke a bargain", liar.id()),
    };
    let threat = ThreatClassifier::classify_signal(&sig).expect("broken bargain is a threat");
    assert_eq!(threat.class(), ThreatClass::HostilePeer);
    println!(
        "  defend classifies BROKEN_BARGAIN as {:?} ({:?})",
        threat.class(),
        threat.severity()
    );

    // Multi-hop relay: honest1's report is relayed to honest2 via a second hop,
    // carrying honest1's original signature as provenance.
    let relay = honest1
        .relay_reputation_report(&r1)
        .expect("relay within hop cap");
    // A fresh listener that only ever sees the relayed (indirect) report.
    let mut indirect = Negotiator::new(&[])?;
    indirect.receive_reputation_report(&relay, 100)?;
    assert!(indirect.reputation(liar.id(), 100).trust < 1.0);
    println!("  relayed (hop-2) report lowered an indirect listener's trust");
    Ok(())
}
