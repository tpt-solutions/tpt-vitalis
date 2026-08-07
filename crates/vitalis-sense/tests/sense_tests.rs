use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use vitalis_core::traits::Sense;
use vitalis_core::{AgentId, Capability, CapabilityScope, Resource, ResourceKind};
use vitalis_sense::host::SimulatedHost;
#[cfg(target_os = "linux")]
use vitalis_sense::host::{HostProbe, ProcfsHost};
use vitalis_sense::mesh::{Mesh, SimulatedMesh};
use vitalis_sense::sense::LocalSensor;

fn sensor_on(mesh: SimulatedMesh, energy: f64) -> LocalSensor {
    let probe = Box::new(SimulatedHost::new(2.0, 1.0, energy, 1.0));
    LocalSensor::new(AgentId::new(), probe, Box::new(mesh))
}

#[test]
fn simulated_snapshot_has_four_resources() {
    let s = LocalSensor::simulated(AgentId::new());
    let snap = s.snapshot().unwrap();
    assert_eq!(snap.resources.len(), 4);
    assert!(snap.total(ResourceKind::Energy) > 0.0);
}

#[test]
fn peers_advertise_and_discover() {
    let shared = Arc::new(Mutex::new(HashMap::new()));
    let a = sensor_on(SimulatedMesh::shared(shared.clone()), 100.0);
    let b = sensor_on(SimulatedMesh::shared(shared.clone()), 100.0);

    a.advertise(
        &[Resource::new(ResourceKind::Energy, 80.0, "J")],
        &[Capability::new("replicate", CapabilityScope::Replicate)],
    )
    .unwrap();

    // Raw discovery sees the advertised peer (and self).
    let discovered = b.peers().unwrap();
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].resources[0].quantity(), 80.0);
    assert_eq!(a.peers().unwrap().len(), 1);

    // A unified snapshot excludes the agent's own id.
    let snap_b = b.snapshot().unwrap();
    assert_eq!(snap_b.peers.len(), 1);
    let snap_a = a.snapshot().unwrap();
    assert_eq!(snap_a.peers.len(), 0);
}

#[test]
fn scavenge_finds_resource_rich_peer() {
    let shared = Arc::new(Mutex::new(HashMap::new()));
    let rich = sensor_on(SimulatedMesh::shared(shared.clone()), 1_000.0);
    let poor = sensor_on(SimulatedMesh::shared(shared.clone()), 1.0);

    rich.advertise(&[Resource::new(ResourceKind::Energy, 900.0, "J")], &[])
        .unwrap();
    poor.advertise(&[Resource::new(ResourceKind::Energy, 1.0, "J")], &[])
        .unwrap();

    let need = Resource::new(ResourceKind::Energy, 500.0, "J");
    let found = poor.scavenge(&need).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resources[0].quantity(), 900.0);
}

#[test]
fn leave_removes_peer() {
    let mesh = SimulatedMesh::new();
    let id = AgentId::new();
    mesh.advertise(&id, &[Resource::new(ResourceKind::Energy, 10.0, "J")], &[])
        .unwrap();
    assert!(mesh.query_peer(&id).unwrap().is_some());
    mesh.leave(&id).unwrap();
    assert!(mesh.query_peer(&id).unwrap().is_none());
}

/// Real Linux host probe reads `/proc` and sysfs (runs only on Linux CI).
#[cfg(target_os = "linux")]
#[test]
fn procfs_host_reads_real_resources() {
    let probe = ProcfsHost::new();
    let resources = probe.read().unwrap();
    // Four canonical kinds, all finite and non-negative.
    assert_eq!(resources.len(), 4);
    for r in &resources {
        assert!(
            r.is_valid(),
            "resource {r:?} should be finite & non-negative"
        );
    }
    // Memory from /proc/meminfo should be a real, positive amount.
    let mem = resources
        .iter()
        .find(|r| r.kind() == ResourceKind::Storage)
        .unwrap();
    assert!(mem.quantity() > 0.0);
}
