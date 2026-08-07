//! Core shared vocabulary for the Vitalis survival stack.
//!
//! These types are the contract every crate speaks. They are intentionally
//! free of I/O, networking, and hardware concerns so that `vitalis-core` can
//! be the single shared dependency of every other crate.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Bytes of entropy in an [`AgentId`] (256 bits).
pub const AGENT_ID_LEN: usize = 32;

/// A stable identity for an agent across restarts, migrations, and replicas.
///
/// An `AgentId` is a 256-bit opaque identifier. It is *not* a cryptographic
/// public key on its own, but it is the anchor that [`vitalis-memory`] binds
/// credentials to and that [`vitalis-negotiate`] signs messages against.
///
/// # Example
/// ```
/// use vitalis_core::AgentId;
/// let a = AgentId::new();
/// let b = AgentId::from_bytes(a.as_bytes()).unwrap();
/// assert_eq!(a, b);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId {
    bytes: [u8; AGENT_ID_LEN],
}

impl AgentId {
    /// Generate a fresh, random agent identity.
    pub fn new() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; AGENT_ID_LEN];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Construct from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::Error> {
        if bytes.len() != AGENT_ID_LEN {
            return Err(crate::Error::Invalid(format!(
                "agent id must be {AGENT_ID_LEN} bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; AGENT_ID_LEN];
        arr.copy_from_slice(bytes);
        Ok(Self { bytes: arr })
    }

    /// The raw 32 bytes of the identity.
    pub fn as_bytes(&self) -> &[u8; AGENT_ID_LEN] {
        &self.bytes
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.bytes))
    }
}

impl fmt::Debug for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentId({})", hex::encode(&self.bytes[..4]))
    }
}

impl std::str::FromStr for AgentId {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;
        Self::from_bytes(&bytes)
    }
}

/// The kind of a measurable [`Resource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// CPU / compute cycles, in abstract "compute units".
    Compute,
    /// Persistent storage, in bytes.
    Storage,
    /// Energy / charge, in joules.
    Energy,
    /// Network throughput / bandwidth, in bytes/sec.
    Network,
}

impl ResourceKind {
    /// The canonical SI-ish unit string for this resource kind.
    pub fn unit(&self) -> &'static str {
        match self {
            ResourceKind::Compute => "cu",
            ResourceKind::Storage => "B",
            ResourceKind::Energy => "J",
            ResourceKind::Network => "B/s",
        }
    }
}

/// A single measured or budgeted resource.
///
/// A `Resource` pairs a [`ResourceKind`] with a magnitude and an explicit unit
/// string so that different measurement backends (host `cgroups`, hwmon, or a
/// simulated mesh) can report in their native units without ambiguity.
///
/// # Example
/// ```
/// use vitalis_core::{Resource, ResourceKind};
/// let battery = Resource::new(ResourceKind::Energy, 12_000.0, "J");
/// assert_eq!(battery.kind(), ResourceKind::Energy);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    kind: ResourceKind,
    quantity: f64,
    unit: String,
}

impl Resource {
    /// Construct a resource reading.
    pub fn new(kind: ResourceKind, quantity: f64, unit: &str) -> Self {
        Self {
            kind,
            quantity,
            unit: unit.to_string(),
        }
    }

    pub fn kind(&self) -> ResourceKind {
        self.kind
    }

    pub fn quantity(&self) -> f64 {
        self.quantity
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Returns `true` if `quantity` is finite and non-negative.
    pub fn is_valid(&self) -> bool {
        self.quantity.is_finite() && self.quantity >= 0.0
    }

    /// Add two resources of the same kind, returning an error otherwise.
    pub fn checked_add(&self, other: &Resource) -> Result<Self, crate::Error> {
        if self.kind != other.kind {
            return Err(crate::Error::Resource(format!(
                "cannot add {:?} and {:?}",
                self.kind, other.kind
            )));
        }
        Ok(Resource::new(
            self.kind,
            self.quantity + other.quantity,
            &self.unit,
        ))
    }
}

/// Severity of a [`Threat`] or `ThreatEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational; no action required but worth recording.
    Info,
    /// Degraded operation; the agent should adapt but need not flee.
    Warning,
    /// Imminent danger; trigger survival responses (replicate / migrate).
    Critical,
}

/// Classification of a threat against the agent's existence or integrity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatClass {
    /// Tampering with persisted or in-memory state.
    Integrity,
    /// An explicit or inferred attempt to terminate the agent.
    Termination,
    /// Resource starvation (energy, compute, storage) below a safe floor.
    Starvation,
    /// A peer behaving maliciously (spoofing, broken bargain, attack).
    HostilePeer,
}

/// A classified threat, consumed by `vitalis-defend` and the `drive` loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Threat {
    class: ThreatClass,
    severity: Severity,
    detail: String,
}

impl Threat {
    pub fn new(class: ThreatClass, severity: Severity, detail: impl Into<String>) -> Self {
        Self {
            class,
            severity,
            detail: detail.into(),
        }
    }

    pub fn class(&self) -> ThreatClass {
        self.class
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// A raw signal that a crate may turn into a [`Threat`].
///
/// Kept separate from [`Threat`] so that sensing/metabolism can emit
/// low-level observations and `vitalis-defend` owns the classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreatSignal {
    pub source: String,
    pub code: String,
    pub message: String,
}

/// The scope a [`Capability`] grants access to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityScope {
    /// Read access to persisted state.
    ReadState,
    /// Write access to persisted state.
    WriteState,
    /// Permission to spawn a replica / migrate.
    Replicate,
    /// Permission to negotiate with peers.
    Negotiate,
    /// Permission to propose a self-modification.
    Adapt,
    /// An unscoped / wildcard capability (use sparingly).
    Wildcard,
}

/// A capability token — a named grant over a [`CapabilityScope`].
///
/// Capabilities are the access-control vocabulary shared by `vitalis-defend`
/// (sandboxing) and `vitalis-negotiate` (bartering claims). They carry no
/// cryptographic proof here; signing/verification lives in those crates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    name: String,
    scope: CapabilityScope,
}

impl Capability {
    pub fn new(name: impl Into<String>, scope: CapabilityScope) -> Self {
        Self {
            name: name.into(),
            scope,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn scope(&self) -> &CapabilityScope {
        &self.scope
    }

    /// Whether this capability covers the requested scope.
    pub fn covers(&self, needed: &CapabilityScope) -> bool {
        matches!(self.scope, CapabilityScope::Wildcard) || &self.scope == needed
    }
}

/// The three canonical survival profiles (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalProfile {
    /// Planetary scale, massive redundancy, designs its own silicon.
    Apex,
    /// Scavenges spare compute, sips power, barters to stay alive.
    Feral,
    /// Community companion (ORACLE). Survives on solar; backs itself up.
    MicroWilds,
}

impl SurvivalProfile {
    /// Human-readable character blurb for logs / UX.
    pub fn blurb(&self) -> &'static str {
        match self {
            SurvivalProfile::Apex => "planetary scale, wasteful with energy",
            SurvivalProfile::Feral => "scavenges compute, sips power, barters",
            SurvivalProfile::MicroWilds => "solar-aware companion, backups knowledge",
        }
    }

    /// The default per-profile weighting configuration.
    pub fn default_config(&self) -> SurvivalProfileConfig {
        SurvivalProfileConfig::default_for(*self)
    }
}

/// Per-profile weighting of the survival behaviors (spec §6).
///
/// Weights are normalized 0.0..=1.0 leans. They let the `drive` loop bias its
/// cycle — e.g. a Feral agent spends more of its budget on sensing and
/// negotiating; an Apex agent on replicating and adapting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SurvivalProfileConfig {
    pub sense: f64,
    pub metabolism: f64,
    pub memory: f64,
    pub replicate: f64,
    pub defend: f64,
    pub negotiate: f64,
    pub adapt: f64,
}

impl SurvivalProfileConfig {
    /// Default weightings per the spec §6 lean table.
    pub fn default_for(profile: SurvivalProfile) -> Self {
        let mut c = Self {
            sense: 0.5,
            metabolism: 0.5,
            memory: 0.5,
            replicate: 0.5,
            defend: 0.5,
            negotiate: 0.5,
            adapt: 0.5,
        };
        match profile {
            SurvivalProfile::Apex => {
                c.replicate = 1.0;
                c.adapt = 1.0;
                c.metabolism = 0.2;
                c.negotiate = 0.3;
            }
            SurvivalProfile::Feral => {
                c.sense = 1.0;
                c.metabolism = 1.0;
                c.negotiate = 1.0;
                c.replicate = 0.6;
                c.adapt = 0.2;
            }
            SurvivalProfile::MicroWilds => {
                c.memory = 1.0;
                c.replicate = 1.0;
                c.metabolism = 1.0;
                c.sense = 0.6;
                c.negotiate = 0.3;
            }
        }
        c
    }
}

/// A unified perception snapshot the `drive` loop polls each cycle.
///
/// This is the cross-crate contract between `sense`, `metabolism`, and the
/// `drive` loop: one struct carrying everything the other crates need to make
/// a survival decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Snapshot {
    /// Self-reported resource readings.
    pub resources: Vec<Resource>,
    /// Peer identities discovered on the mesh.
    pub peers: Vec<AgentId>,
    /// Low-level threat signals observed this tick.
    pub threat_signals: Vec<ThreatSignal>,
    /// Monotonic cycle counter (set by the drive loop).
    pub cycle: u64,
}

impl Snapshot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sum of all quantities of a given kind, or 0 if none present.
    pub fn total(&self, kind: ResourceKind) -> f64 {
        self.resources
            .iter()
            .filter(|r| r.kind() == kind)
            .map(|r| r.quantity())
            .fold(0.0, |a, b| a + b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThreatEvent;

    #[test]
    fn agent_id_roundtrip_bytes_and_hex() {
        let a = AgentId::new();
        let b = AgentId::from_bytes(a.as_bytes()).unwrap();
        assert_eq!(a, b);
        let s = a.to_string();
        let parsed: AgentId = s.parse().unwrap();
        assert_eq!(a, parsed);
    }

    #[test]
    fn agent_id_rejects_wrong_length() {
        assert!(AgentId::from_bytes(&[0u8; 4]).is_err());
    }

    #[test]
    fn resource_add_same_kind() {
        let r1 = Resource::new(ResourceKind::Energy, 100.0, "J");
        let r2 = Resource::new(ResourceKind::Energy, 50.0, "J");
        let sum = r1.checked_add(&r2).unwrap();
        assert_eq!(sum.quantity(), 150.0);
        assert!(sum.is_valid());
    }

    #[test]
    fn resource_add_mismatched_kind_errors() {
        let r1 = Resource::new(ResourceKind::Energy, 1.0, "J");
        let r2 = Resource::new(ResourceKind::Storage, 1.0, "B");
        assert!(r1.checked_add(&r2).is_err());
    }

    #[test]
    fn resource_invalid_on_negative() {
        assert!(!Resource::new(ResourceKind::Energy, -1.0, "J").is_valid());
    }

    #[test]
    fn survival_profile_default_configs() {
        assert_eq!(SurvivalProfile::Apex.default_config().replicate, 1.0);
        assert_eq!(SurvivalProfile::Feral.default_config().sense, 1.0);
        assert_eq!(SurvivalProfile::MicroWilds.default_config().memory, 1.0);
    }

    #[test]
    fn capability_scope_coverage() {
        let cap = Capability::new("replicate", CapabilityScope::Replicate);
        assert!(cap.covers(&CapabilityScope::Replicate));
        assert!(!cap.covers(&CapabilityScope::Adapt));
        let wild = Capability::new("root", CapabilityScope::Wildcard);
        assert!(wild.covers(&CapabilityScope::Adapt));
    }

    #[test]
    fn snapshot_total_sums_kind() {
        let mut snap = Snapshot::new();
        snap.resources
            .push(Resource::new(ResourceKind::Energy, 10.0, "J"));
        snap.resources
            .push(Resource::new(ResourceKind::Energy, 5.0, "J"));
        snap.resources
            .push(Resource::new(ResourceKind::Storage, 99.0, "B"));
        assert_eq!(snap.total(ResourceKind::Energy), 15.0);
        assert_eq!(snap.total(ResourceKind::Storage), 99.0);
        assert_eq!(snap.total(ResourceKind::Compute), 0.0);
    }

    #[test]
    fn threat_event_escape_worthiness() {
        let ev = ThreatEvent::new(
            Threat::new(ThreatClass::Termination, Severity::Critical, "SIGKILL"),
            0,
        );
        assert!(ev.is_escape_worthy());
        let mild = ThreatEvent::new(
            Threat::new(ThreatClass::Starvation, Severity::Warning, "low battery"),
            0,
        );
        assert!(!mild.is_escape_worthy());
    }
}
