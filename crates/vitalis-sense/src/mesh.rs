//! Mesh discovery and peer advertisement.
//!
//! A [`Mesh`] lets an agent advertise what it has (resources + capabilities)
//! and discover what peers have. The shipped backend is a deterministic
//! in-memory [`SimulatedMesh`] — a *simulated* local mesh that exercises the
//! full discovery + barter path with zero network (best-effort stand-in for a
//! real libp2p gossip/discovery transport). Because the `drive` loop depends
//! only on this trait, the transport is swappable without touching the rest
//! of the stack: a production [`Mesh`] backed by libp2p (or another mesh
//! transport) can be dropped in behind the same trait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vitalis_core::{AgentId, Capability, Error, Resource, Result};

/// What a peer is offering on the mesh.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PeerAdvertisement {
    pub id: AgentId,
    pub resources: Vec<Resource>,
    pub capabilities: Vec<Capability>,
}

/// A discovery + advertisement medium.
pub trait Mesh {
    /// Publish this agent's current offering to the mesh.
    fn advertise(
        &self,
        self_id: &AgentId,
        resources: &[Resource],
        capabilities: &[Capability],
    ) -> Result<()>;

    /// Enumerate all currently-known peers (including stale ones; the caller
    /// decides relevance).
    fn discover(&self) -> Result<Vec<PeerAdvertisement>>;

    /// Look up a specific peer's advertisement.
    fn query_peer(&self, id: &AgentId) -> Result<Option<PeerAdvertisement>>;

    /// Withdraw this agent from the mesh.
    fn leave(&self, self_id: &AgentId) -> Result<()>;
}

/// A network-free, in-process mesh. Multiple agents in the same process can
/// join the same `SimulatedMesh` (shared via [`SimulatedMesh::shared`]) to
/// exercise discovery and barter in tests.
#[derive(Clone)]
pub struct SimulatedMesh {
    peers: Arc<Mutex<HashMap<AgentId, PeerAdvertisement>>>,
}

impl SimulatedMesh {
    /// An isolated mesh (no other participants).
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// A mesh shared with other handles created from the same `Arc`.
    pub fn shared(peers: Arc<Mutex<HashMap<AgentId, PeerAdvertisement>>>) -> Self {
        Self { peers }
    }

    /// Access the underlying shared peer map (for tests / inspection).
    pub fn peer_map(&self) -> &Arc<Mutex<HashMap<AgentId, PeerAdvertisement>>> {
        &self.peers
    }
}

impl Default for SimulatedMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl Mesh for SimulatedMesh {
    fn advertise(
        &self,
        self_id: &AgentId,
        resources: &[Resource],
        capabilities: &[Capability],
    ) -> Result<()> {
        let mut g = self
            .peers
            .lock()
            .map_err(|_| Error::Transport("mesh lock poisoned".into()))?;
        g.insert(
            *self_id,
            PeerAdvertisement {
                id: *self_id,
                resources: resources.to_vec(),
                capabilities: capabilities.to_vec(),
            },
        );
        Ok(())
    }

    fn discover(&self) -> Result<Vec<PeerAdvertisement>> {
        let g = self
            .peers
            .lock()
            .map_err(|_| Error::Transport("mesh lock poisoned".into()))?;
        Ok(g.values().cloned().collect())
    }

    fn query_peer(&self, id: &AgentId) -> Result<Option<PeerAdvertisement>> {
        let g = self
            .peers
            .lock()
            .map_err(|_| Error::Transport("mesh lock poisoned".into()))?;
        Ok(g.get(id).cloned())
    }

    fn leave(&self, self_id: &AgentId) -> Result<()> {
        let mut g = self
            .peers
            .lock()
            .map_err(|_| Error::Transport("mesh lock poisoned".into()))?;
        g.remove(self_id);
        Ok(())
    }
}

/// Find peers on the mesh that can spare at least `needed` of a resource kind.
pub fn scavenge(peers: &[PeerAdvertisement], needed: &Resource) -> Vec<PeerAdvertisement> {
    peers
        .iter()
        .filter(|p| {
            p.resources
                .iter()
                .any(|r| r.kind() == needed.kind() && r.quantity() >= needed.quantity())
        })
        .cloned()
        .collect()
}
