//! Self-throttling control logic and the host safety ceiling.
//!
//! The control loop maps the host's *energy fraction* (current / capacity) to
//! a cognition-rate in `0.0..=1.0`. Each [`SurvivalProfile`] has a different
//! preset (spec §6): Apex is wasteful, Feral is frugal, Micro is solar-aware.
//! Separately, the [`SafetyCeiling`] caps how much of the *host's* resources
//! the agent may draw, regardless of priority — this is what guarantees the
//! agent never brownouts its host.

use vitalis_core::{Resource, ResourceKind, SurvivalProfile};

/// Throttling tuning for a profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrottleProfile {
    /// Energy fraction at/above which the agent runs at full cognition rate.
    pub throttle_start: f64,
    /// Energy fraction at/below which cognition drops to zero.
    pub throttle_floor: f64,
    /// If true, throttle earlier (steeper curve) to conserve aggressively.
    pub aggressive: bool,
}

impl ThrottleProfile {
    /// Preset for a survival profile (spec §6 leanings).
    pub fn for_profile(profile: SurvivalProfile) -> Self {
        match profile {
            // Wasteful: only throttles when nearly empty.
            SurvivalProfile::Apex => ThrottleProfile {
                throttle_start: 0.10,
                throttle_floor: 0.0,
                aggressive: false,
            },
            // Frugal scavenger: throttles early and hard.
            SurvivalProfile::Feral => ThrottleProfile {
                throttle_start: 0.50,
                throttle_floor: 0.05,
                aggressive: true,
            },
            // Solar-aware companion: moderate, smooth.
            SurvivalProfile::MicroWilds => ThrottleProfile {
                throttle_start: 0.35,
                throttle_floor: 0.05,
                aggressive: false,
            },
        }
    }
}

/// Map an energy fraction to a cognition throttle, per a preset.
///
/// Returns `1.0` when the host is at/above `throttle_start`, `0.0` at/below
/// `throttle_floor`, and interpolates (with an optional steeper curve for
/// `aggressive` profiles) in between.
pub fn compute_throttle(energy_fraction: f64, preset: &ThrottleProfile) -> f64 {
    let e = energy_fraction.clamp(0.0, 1.0);
    if e >= preset.throttle_start {
        return 1.0;
    }
    if e <= preset.throttle_floor {
        return 0.0;
    }
    let span = (preset.throttle_start - preset.throttle_floor).max(f64::EPSILON);
    let t = (e - preset.throttle_floor) / span;
    if preset.aggressive {
        t * t
    } else {
        t
    }
}

/// Hard resource caps independent of agent priority. The agent must never
/// draw more than these fractions of the host's available resources, even if
/// its own survival logic thinks it needs more.
#[derive(Debug, Clone, Copy)]
pub struct SafetyCeiling {
    /// Max fraction of host *energy* the agent may consume.
    pub max_host_energy_fraction: f64,
    /// Max fraction of host *compute* the agent may consume.
    pub max_host_compute_fraction: f64,
    /// Minimum cognition rate the safety layer permits (0 = full stop allowed).
    pub min_cognition_rate: f64,
}

impl Default for SafetyCeiling {
    fn default() -> Self {
        Self {
            max_host_energy_fraction: 0.80,
            max_host_compute_fraction: 0.90,
            min_cognition_rate: 0.0,
        }
    }
}

impl SafetyCeiling {
    /// True if the agent is allowed to draw `requested` given `host_total`.
    pub fn can_consume(&self, requested: &Resource, host_total: &Resource) -> bool {
        if requested.kind() != host_total.kind() {
            return false;
        }
        let cap = match requested.kind() {
            ResourceKind::Energy => self.max_host_energy_fraction,
            ResourceKind::Compute => self.max_host_compute_fraction,
            _ => 1.0,
        };
        requested.quantity() <= cap * host_total.quantity()
    }

    /// The hard minimum cognition rate the safety layer enforces.
    pub fn min_cognition_rate(&self) -> f64 {
        self.min_cognition_rate
    }
}

/// A reusable throttle controller carrying a profile preset.
#[derive(Debug, Clone, Copy)]
pub struct ThrottleController {
    preset: ThrottleProfile,
    ceiling: SafetyCeiling,
}

impl ThrottleController {
    pub fn new(profile: SurvivalProfile) -> Self {
        Self {
            preset: ThrottleProfile::for_profile(profile),
            ceiling: SafetyCeiling::default(),
        }
    }

    pub fn with_ceiling(profile: SurvivalProfile, ceiling: SafetyCeiling) -> Self {
        Self {
            preset: ThrottleProfile::for_profile(profile),
            ceiling,
        }
    }

    /// Compute the allowed cognition rate for an energy fraction.
    pub fn throttle_for(&self, energy_fraction: f64) -> f64 {
        compute_throttle(energy_fraction, &self.preset).max(self.ceiling.min_cognition_rate())
    }

    /// Convenience: is the requested draw within the safety ceiling?
    pub fn safe_to_draw(&self, requested: &Resource, host_total: &Resource) -> bool {
        self.ceiling.can_consume(requested, host_total)
    }
}
