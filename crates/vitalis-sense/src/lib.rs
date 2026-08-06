//! # vitalis-sense
//!
//! The agent's senses: discover peers, available compute, energy, storage,
//! and network across a mesh, and read the host's own resource state.
//!
//! The design is **transport-agnostic**. A production deployment would back
//! the [`Mesh`] trait with libp2p (or a similar mesh stack); this crate ships
//! a deterministic, network-free [`SimulatedMesh`] so the whole stack can be
//! tested in CI without real networking. Swapping in libp2p later is a
//! backend change only — the `drive` loop depends on the trait, not the
//! transport.
//!
//! Layering: depends on `vitalis-core` only.

pub mod host;
pub mod mesh;
pub mod sense;

#[cfg(target_os = "linux")]
pub use host::ProcfsHost;
pub use host::{HostProbe, SimulatedHost};
pub use mesh::{Mesh, PeerAdvertisement, SimulatedMesh};
pub use sense::{LocalSensor, SenseTick};
