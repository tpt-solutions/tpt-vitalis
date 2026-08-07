//! End-to-end tests for the `vitalis-drive` survival loop.

use vitalis_core::SurvivalProfile;
use vitalis_drive::{Drive, DriveConfig};

#[test]
fn full_run_survives_n_cycles() {
    let cfg = DriveConfig {
        profile: SurvivalProfile::Feral,
        energy_capacity: 1_000_000.0,
        cost_per_cycle: 1_000.0,
        ..Default::default()
    };
    let mut d = Drive::new(cfg).unwrap();
    let out = d.run(20).unwrap();
    assert_eq!(out.cycles_run, 20);
    assert!(out.survived);
    assert!(out.final_energy > 0.0);
}

#[test]
fn kill_at_triggers_exactly_one_verified_replication() {
    let cfg = DriveConfig {
        profile: SurvivalProfile::Feral,
        energy_capacity: 1_000_000.0,
        cost_per_cycle: 1_000.0,
        simulate_kill_at: Some(2),
        ..Default::default()
    };
    let mut d = Drive::new(cfg).unwrap();
    let out = d.run(10).unwrap();
    // A single escape-worthy event fires exactly one replication (the drive
    // caps itself at one redundant copy per run).
    assert_eq!(out.replications, 1);
    // The persisted checkpoint carries a valid seal and verifies under the
    // wired `BoundVerifier` (P0.1).
    assert!(d.verify_stored_checkpoint().is_ok());
}

#[test]
fn replication_never_exceeds_copy_limit() {
    // Independent check that the replicator's copy-limit holds even when the
    // loop keeps trying to escape (mirrors the in-loop `authorize_copy` cap).
    use vitalis_core::AgentId;
    use vitalis_replicate::audit::ReplicationReason;
    use vitalis_replicate::Replicator;

    let id = AgentId::new();
    let max = 3;
    let mut r = Replicator::new(id, max, 4, 2);
    r.capture(b"state").unwrap();
    let mut authorized = 0;
    for _ in 0..(max * 3) {
        if r.authorize_copy(ReplicationReason::DyingHardware).is_ok() {
            authorized += 1;
        }
    }
    assert_eq!(authorized, max);
    assert_eq!(r.live_copy_count(), max);
}
