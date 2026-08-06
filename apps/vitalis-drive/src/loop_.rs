//! The survival goal loop: sense → decide → act → persist, forever.

use serde::Serialize;
use vitalis_core::traits::Defend;
use vitalis_core::{AgentId, Result, SurvivalProfile};
use vitalis_defend::{Defender, TerminationSignal};
use vitalis_memory::{IdentityRecord, IdentityStore, MemoryStore, StateStore};
use vitalis_metabolism::Metabolism;
use vitalis_replicate::{ReplicationReason, Replicator};
use vitalis_sense::sense::{LocalSensor, SenseTick};

/// What profile to run as, and how much redundancy to keep.
#[derive(Debug, Clone)]
pub struct DriveConfig {
    pub profile: SurvivalProfile,
    pub max_copies: usize,
    /// Simulated battery capacity (joules) the agent draws against.
    pub energy_capacity: f64,
    /// Energy spent per cognition cycle (joules).
    pub cost_per_cycle: f64,
    /// Fraction of capacity at which the agent proactively replicates.
    pub replicate_below: f64,
    /// Optional cycle at which a simulated termination signal fires (Phase 3
    /// immune-response demo). When set, the agent classifies a SIGKILL and
    /// triggers replication + "migration".
    pub simulate_kill_at: Option<u64>,
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            profile: SurvivalProfile::Feral,
            max_copies: 3,
            energy_capacity: 50_000.0,
            cost_per_cycle: 4_000.0,
            replicate_below: 0.4,
            simulate_kill_at: None,
        }
    }
}

/// Serialized agent state carried in a checkpoint.
#[derive(Debug, Clone, Serialize)]
pub struct AgentState {
    pub agent_id: String,
    pub profile: SurvivalProfile,
    pub cycle: u64,
    pub energy_remaining: f64,
}

/// What happened over a drive run.
#[derive(Debug, Clone)]
pub struct DriveOutcome {
    pub cycles_run: u64,
    pub replications: u32,
    pub final_throttle: f64,
    pub survived: bool,
    pub final_energy: f64,
}

/// The reference survival agent.
pub struct Drive {
    agent_id: AgentId,
    config: DriveConfig,
    tick: SenseTick,
    metabolism: Metabolism,
    replicator: Replicator,
    defender: Defender,
    state_store: StateStore,
    energy_remaining: f64,
    replicated: bool,
}

impl Drive {
    pub fn new(config: DriveConfig) -> Result<Self> {
        let agent_id = AgentId::new();
        let sensor = LocalSensor::simulated(agent_id);
        let tick = SenseTick::new(sensor);
        let metabolism = Metabolism::new(config.profile);
        let replicator = Replicator::new(agent_id, config.max_copies, 4, 2);

        // The immune system: signs checkpoints + classifies threats.
        let defender = Defender::new()?;

        // Persist identity so the agent can re-establish who it is on restart.
        let memory: MemoryStore = MemoryStore::in_memory()?;
        IdentityStore::new(memory.clone()).save(&IdentityRecord::new(agent_id, vec![]))?;
        let state_store = StateStore::new(memory);

        // Advertise this agent's presence on the (simulated) mesh.
        tick.sensor()
            .advertise(&[], &[])
            .unwrap_or_else(|e| tracing::warn!("advertise failed: {e}"));

        let energy_remaining = config.energy_capacity;
        Ok(Self {
            agent_id,
            config,
            tick,
            metabolism,
            replicator,
            defender,
            state_store,
            energy_remaining,
            replicated: false,
        })
    }

    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Build the current state blob.
    fn state(&self, cycle: u64) -> Result<Vec<u8>> {
        let st = AgentState {
            agent_id: self.agent_id.to_string(),
            profile: self.config.profile,
            cycle,
            energy_remaining: self.energy_remaining,
        };
        serde_json::to_vec(&st).map_err(|e| vitalis_core::Error::Encode(e.to_string()))
    }

    /// Capture + (attempt to) authorize a redundant copy, persisting the
    /// checkpoint to memory.
    fn replicate(&mut self, reason: ReplicationReason) -> Result<()> {
        let state = self.state(self.tick.cycle())?;
        self.replicator.capture(&state)?;
        let bytes = self.replicator.checkpoint_bytes()?;
        // Sign the checkpoint for integrity (defend layer).
        let seal = self.defender.seal(&bytes);
        assert!(self.defender.verify(&seal, &bytes));
        // Persist checkpoint durably (proves G3 alongside memory).
        self.state_store.put("checkpoint", &bytes)?;
        // Erasure-code into shards to demonstrate redundancy.
        let shards = self.replicator.shards()?;
        self.state_store.put("shards", &shards.len())?;
        // Enforce the copy-limit; if at cap, log and continue (bounded!).
        match self.replicator.authorize_copy(reason) {
            Ok(()) => tracing::info!(
                cycle = self.tick.cycle(),
                copies = self.replicator.live_copy_count(),
                "replicated (redundant copy authorized)"
            ),
            Err(e) => tracing::warn!("replication capped: {e}"),
        }
        self.replicated = true;
        Ok(())
    }

    /// Optional bounded self-improvement step. Compiled in only when the
    /// `adapt` feature is explicitly enabled — never in a default build.
    #[cfg(feature = "adapt")]
    fn maybe_adapt(&self) {
        use vitalis_adapt::{AdaptEngine, ChangeKind, ProposedChange};
        let engine = AdaptEngine::new(4096, 5);
        let change = ProposedChange::new(
            self.tick.cycle(),
            ChangeKind::Code,
            vec![0u8; 16],
            "self-tuning cognition rate",
        );
        match engine.propose(&change) {
            Ok(_) => tracing::info!(cycle = self.tick.cycle(), "adapt: change applied (audited)"),
            Err(e) => tracing::warn!(cycle = self.tick.cycle(), "adapt: change refused: {e}"),
        }
    }

    /// Run the survival loop for up to `max_cycles`.
    pub fn run(&mut self, max_cycles: u64) -> Result<DriveOutcome> {
        let mut replications = 0u32;
        let mut cycles_run = 0u64;

        for cycle in 1..=max_cycles {
            cycles_run = cycle;
            // 1. SENSE
            let snap = self.tick.poll()?;
            // 2. METABOLIZE: update ledger with current host resources.
            for r in &snap.resources {
                let _ = self.metabolism.acquire(r);
            }
            // 3. DECIDE: energy fraction drives the throttle.
            let frac = (self.energy_remaining / self.config.energy_capacity).clamp(0.0, 1.0);
            let throttle = self.metabolism.tick_energy(frac)?;

            tracing::info!(
                cycle,
                energy = self.energy_remaining,
                throttle,
                peers = snap.peers.len(),
                "sense/decide"
            );

            // 4. DEFEND: classify environment signals into threats. We decide the
            // threat based on the energy we'll have *after* this cycle's spend,
            // so a cycle that would deplete us is treated as a critical
            // (escape-worthy) starvation threat.
            use vitalis_defend::signal::simulate_termination;
            let next_energy = (self.energy_remaining - self.config.cost_per_cycle).max(0.0);
            let signal = if self.config.simulate_kill_at == Some(cycle) {
                simulate_termination(TerminationSignal::SigKill)
            } else if next_energy <= 0.0 {
                simulate_termination(TerminationSignal::Starvation)
            } else if frac < self.config.replicate_below {
                // Low (but not yet depleted) battery: a warning-grade signal.
                vitalis_core::ThreatSignal {
                    source: "metabolism".into(),
                    code: "LOW_BATTERY".into(),
                    message: "energy below safe floor".into(),
                }
            } else {
                // Benign heartbeat — not a threat, but the cycle still runs.
                vitalis_core::ThreatSignal {
                    source: "drive".into(),
                    code: "HEARTBEAT".into(),
                    message: "nominal".into(),
                }
            };

            if let Some(ev) = self.defender.classify(&signal)? {
                tracing::warn!(
                    cycle,
                    class = ?ev.threat.class(),
                    severity = ?ev.severity(),
                    "threat classified"
                );
                // The immune response: a critical threat triggers replication +
                // (downstream) migration. defend never calls replicate directly;
                // the drive loop observes the event and acts.
                if ev.is_escape_worthy() && !self.replicated {
                    self.replicate(ReplicationReason::DyingHardware)?;
                    replications += 1;
                }
            }

            // 5. ACT: spend energy; an actual depletion ends the run.
            self.energy_remaining = (self.energy_remaining - self.config.cost_per_cycle).max(0.0);
            #[cfg(feature = "adapt")]
            self.maybe_adapt();
            if self.energy_remaining <= 0.0 {
                tracing::warn!(
                    cycle,
                    "energy exhausted — agent would migrate and halt here"
                );
                break;
            }
        }

        let outcome = DriveOutcome {
            cycles_run,
            replications,
            final_throttle: self.metabolism.throttle_level(),
            survived: self.energy_remaining > 0.0,
            final_energy: self.energy_remaining,
        };
        Ok(outcome)
    }
}
