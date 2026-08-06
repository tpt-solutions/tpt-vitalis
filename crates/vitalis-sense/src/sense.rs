//! The sense tick: combine host probing + mesh discovery into a unified
//! [`Snapshot`](vitalis_core::Snapshot) the `drive` loop polls each cycle.

use crate::host::HostProbe;
use crate::mesh::{scavenge, Mesh, PeerAdvertisement, SimulatedMesh};
use vitalis_core::traits::Sense;
use vitalis_core::{AgentId, Capability, Resource, Result, Snapshot};

/// A sensor that reads the local host and the surrounding mesh.
pub struct LocalSensor {
    self_id: AgentId,
    probe: Box<dyn HostProbe>,
    mesh: Box<dyn Mesh>,
}

impl LocalSensor {
    pub fn new(self_id: AgentId, probe: Box<dyn HostProbe>, mesh: Box<dyn Mesh>) -> Self {
        Self {
            self_id,
            probe,
            mesh,
        }
    }

    /// A simulated sensor: fake host + isolated in-memory mesh.
    pub fn simulated(self_id: AgentId) -> Self {
        Self::new(
            self_id,
            Box::new(crate::host::SimulatedHost::healthy()),
            Box::new(SimulatedMesh::new()),
        )
    }

    /// Publish this agent's current offering to the mesh.
    pub fn advertise(&self, resources: &[Resource], capabilities: &[Capability]) -> Result<()> {
        self.mesh.advertise(&self.self_id, resources, capabilities)
    }

    /// Withdraw from the mesh.
    pub fn leave(&self) -> Result<()> {
        self.mesh.leave(&self.self_id)
    }

    /// Currently-known peers.
    pub fn peers(&self) -> Result<Vec<PeerAdvertisement>> {
        self.mesh.discover()
    }

    /// Find peers that can spare at least `needed` of a resource.
    pub fn scavenge(&self, needed: &Resource) -> Result<Vec<PeerAdvertisement>> {
        Ok(scavenge(&self.peers()?, needed))
    }
}

impl Sense for LocalSensor {
    fn snapshot(&self) -> Result<Snapshot> {
        let resources = self.probe.read()?;
        let peers = self
            .mesh
            .discover()?
            .into_iter()
            .filter(|p| p.id != self.self_id)
            .map(|p| p.id)
            .collect();
        Ok(Snapshot {
            resources,
            peers,
            threat_signals: Vec::new(),
            cycle: 0,
        })
    }
}

/// A drive-loop-facing sense tick. Owns the sensor and a monotonically
/// increasing cycle counter, producing a fresh [`Snapshot`] each poll.
pub struct SenseTick {
    sensor: LocalSensor,
    cycle: u64,
}

impl SenseTick {
    pub fn new(sensor: LocalSensor) -> Self {
        Self { sensor, cycle: 0 }
    }

    /// Poll the environment and advance the cycle counter.
    pub fn poll(&mut self) -> Result<Snapshot> {
        let mut snap = self.sensor.snapshot()?;
        self.cycle += 1;
        snap.cycle = self.cycle;
        Ok(snap)
    }

    /// Access the underlying sensor (e.g. to advertise or scavenge).
    pub fn sensor(&self) -> &LocalSensor {
        &self.sensor
    }

    pub fn cycle(&self) -> u64 {
        self.cycle
    }
}
