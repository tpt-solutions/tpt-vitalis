//! # vitalis-metabolism
//!
//! The agent's resource budget and self-throttling control loop. It tracks
//! acquired vs consumed [`Resource`]s (the ledger), converts the host's energy
//! state into a cognition-rate throttle (per [`SurvivalProfile`]), and enforces
//! a hard "never brownout the host" safety ceiling that holds regardless of
//! agent priority.
//!
//! Hardware specifics (cgroups, rlimit, hwmon) are intentionally behind the
//! `Ledger` / `ThrottleController` APIs so the same logic runs on the
//! brain tier and on a constrained ESP32 edge tier (where the limits are
//! simulated). The *decision* logic is what we test; the OS plumbing is a
//! later native binding.
//!
//! Layering: depends on `vitalis-core` only.

pub mod ledger;
pub mod os;
pub mod throttle;

pub use ledger::ResourceLedger;
pub use os::{
    default_limiter, default_power_sensor, LimiterMode, LimiterOutcome, PowerReading, PowerSensor,
    ResourceLimiter, SimulatedLimiter, SimulatedPower,
};
pub use throttle::{SafetyCeiling, ThrottleController, ThrottleProfile};

use vitalis_core::traits::Metabolize;
use vitalis_core::{Error, Resource, Result, SurvivalProfile};

/// The running metabolism: a ledger plus the current cognition throttle.
#[derive(Debug, Clone)]
pub struct Metabolism {
    ledger: ResourceLedger,
    throttle_level: f64,
    profile: SurvivalProfile,
}

impl Metabolism {
    /// Construct a metabolism for a survival profile.
    pub fn new(profile: SurvivalProfile) -> Self {
        Self {
            ledger: ResourceLedger::new(),
            throttle_level: 1.0,
            profile,
        }
    }

    /// Record acquired resources (sense/metabolism intake).
    pub fn acquire(&mut self, r: &Resource) -> Result<()> {
        self.ledger.acquire(r)
    }

    /// Record consumed resources; fails if the kind would go negative.
    pub fn consume(&mut self, r: &Resource) -> Result<()> {
        self.ledger.consume(r)
    }

    /// Current cognition-rate throttle in `0.0..=1.0`.
    pub fn throttle_level(&self) -> f64 {
        self.throttle_level
    }

    /// Recompute the throttle from the current energy fraction using this
    /// profile's preset, then clamp to the safety ceiling's hard minimum.
    pub fn tick_energy(&mut self, energy_fraction: f64) -> Result<f64> {
        let preset = ThrottleProfile::for_profile(self.profile);
        let computed = throttle::compute_throttle(energy_fraction, &preset);
        let floor = SafetyCeiling::default().min_cognition_rate();
        self.throttle_level = computed.max(floor);
        Ok(self.throttle_level)
    }

    pub fn profile(&self) -> SurvivalProfile {
        self.profile
    }
}

impl Metabolize for Metabolism {
    fn ledger(&self) -> Result<Vec<Resource>> {
        Ok(self.ledger.balances())
    }

    fn set_throttle(&mut self, level: f64) -> Result<()> {
        if !level.is_finite() || !(0.0..=1.0).contains(&level) {
            return Err(Error::Invalid(format!(
                "throttle must be 0..=1, got {level}"
            )));
        }
        self.throttle_level = level;
        Ok(())
    }
}
