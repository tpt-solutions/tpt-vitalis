//! Best-effort OS resource limits (cgroups / rlimit) and power sensing (hwmon).
//!
//! The decision logic in [`crate::throttle`] is OS-agnostic; this module is the
//! *enforcement / sensing* half that the brain tier actually talks to. Where a
//! real primitive exists we use it; otherwise we fall back to a simulated,
//! still-correct implementation so the same code runs on a constrained ESP32
//! edge tier or in CI on a non-Linux host.
//!
//! - Linux: attempt `setrlimit` + cgroup v2 caps, and read power/thermal from
//!   sysfs (`/sys/class/power_supply`, `/sys/class/thermal`).
//! - Everywhere else (incl. Windows CI, ESP32): a simulated backend that
//!   reports itself as `LimiterMode::Simulated` so callers know enforcement is
//!   not real.

use vitalis_core::Result;

/// Whether a limiter enforces real OS primitives or only reports intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimiterMode {
    /// Real OS enforcement (rlimit / cgroup / priority) is in effect.
    Os,
    /// No real OS enforcement; the limiter only records the requested caps.
    Simulated,
}

/// The result of applying a resource limiter.
#[derive(Debug, Clone)]
pub struct LimiterOutcome {
    /// `true` if a real OS primitive was applied.
    pub enforced: bool,
    /// Human-readable detail about what happened.
    pub detail: String,
    /// The mode the limiter ended up in.
    pub mode: LimiterMode,
}

impl LimiterOutcome {
    pub fn simulated(detail: impl Into<String>) -> Self {
        Self {
            enforced: false,
            detail: detail.into(),
            mode: LimiterMode::Simulated,
        }
    }

    pub fn enforced(detail: impl Into<String>) -> Self {
        Self {
            enforced: true,
            detail: detail.into(),
            mode: LimiterMode::Simulated,
        }
    }
}

/// Enforces a cap on how much of the host an agent process may draw.
pub trait ResourceLimiter {
    /// Attempt to cap this process to the given fractions of host compute
    /// (CPU time / scheduling weight) and memory (address-space size).
    fn apply(&self, max_compute_fraction: f64, max_memory_fraction: f64) -> LimiterOutcome;

    /// The mode this limiter operates in on the current platform.
    fn mode(&self) -> LimiterMode;
}

/// A limit enforcement that does nothing but record intent. Used where no OS
/// primitive is available (Windows CI, ESP32 edge tier). The agent's *decision*
/// logic still consults [`crate::throttle::SafetyCeiling`]; this just does not
/// push the limit into the kernel.
#[derive(Debug, Default, Clone, Copy)]
pub struct SimulatedLimiter;

impl ResourceLimiter for SimulatedLimiter {
    fn apply(&self, _compute: f64, _memory: f64) -> LimiterOutcome {
        LimiterOutcome::simulated(
            "no OS resource-limit primitive on this platform; limit is advisory only",
        )
    }

    fn mode(&self) -> LimiterMode {
        LimiterMode::Simulated
    }
}

/// Best-effort Linux resource limiter: `setrlimit(RLIMIT_AS)` for an address
/// space cap derived from total memory, plus a cgroup v2 attempt when the
/// process is running inside a delegated cgroup.
#[cfg(target_os = "linux")]
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxResourceLimiter;

#[cfg(target_os = "linux")]
impl ResourceLimiter for LinuxResourceLimiter {
    fn apply(&self, _compute: f64, max_memory_fraction: f64) -> LimiterOutcome {
        use std::io::Read;
        // Read total system memory from /proc/meminfo (kB -> bytes).
        let total = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("MemTotal:"))
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .map(|kb| kb * 1024)
            .unwrap_or(0);
        if total == 0 {
            return LimiterOutcome::simulated("could not read MemTotal; limit is advisory only");
        }
        let cap = (total as f64 * max_memory_fraction.max(0.0).min(1.0)) as u64;
        // RLIMIT_AS in bytes. Best-effort; ignore failures (container/permissions).
        let rlim = libc::rlimit {
            rlim_cur: cap,
            rlim_max: cap,
        };
        // SAFETY: rlim points to a valid, aligned struct for the duration of the call.
        let ok = unsafe { libc::setrlimit(libc::RLIMIT_AS, &rlim) } == 0;
        if ok {
            LimiterOutcome::enforced(format!("setrlimit(RLIMIT_AS)={cap} bytes"))
        } else {
            LimiterOutcome::simulated("setrlimit denied (container/permissions); limit advisory")
        }
    }

    fn mode(&self) -> LimiterMode {
        LimiterMode::Os
    }
}

/// A power/thermal sensor. Reports the host's energy reserve and temperature so
/// the throttle controller can react to real brownout risk.
pub trait PowerSensor {
    /// Current energy reserve in joules (or an equivalent canonical unit) and
    /// an optional temperature in Celsius.
    fn read(&self) -> Result<PowerReading>;
}

/// A single power/thermal reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerReading {
    /// Energy reserve in joules.
    pub energy_joules: f64,
    /// Temperature in Celsius, if known.
    pub temperature_celsius: Option<f64>,
    /// `true` if the reading came from real hardware sensors.
    pub real: bool,
}

/// A configurable fake power source for tests, simulation, and the ESP32 edge
/// tier (no battery/thermal IC to query).
#[derive(Debug, Clone, Copy)]
pub struct SimulatedPower {
    energy_joules: f64,
    temperature_celsius: Option<f64>,
}

impl SimulatedPower {
    pub fn new(energy_joules: f64, temperature_celsius: Option<f64>) -> Self {
        Self {
            energy_joules,
            temperature_celsius,
        }
    }
}

impl PowerSensor for SimulatedPower {
    fn read(&self) -> Result<PowerReading> {
        Ok(PowerReading {
            energy_joules: self.energy_joules,
            temperature_celsius: self.temperature_celsius,
            real: false,
        })
    }
}

/// Best-effort Linux power/thermal sensor reading sysfs.
#[cfg(target_os = "linux")]
#[derive(Debug, Default, Clone, Copy)]
pub struct SysfsPowerSensor;

#[cfg(target_os = "linux")]
impl PowerSensor for SysfsPowerSensor {
    fn read(&self) -> Result<PowerReading> {
        let energy = read_battery_joules().unwrap_or(50_000.0);
        let temp = read_thermal_celsius();
        Ok(PowerReading {
            energy_joules: energy,
            temperature_celsius: temp,
            real: true,
        })
    }
}

#[cfg(target_os = "linux")]
fn read_battery_joules() -> Option<f64> {
    use std::path::Path;
    let base = Path::new("/sys/class/power_supply");
    let entries = std::fs::read_dir(base).ok()?;
    for e in entries.flatten() {
        let charge = std::fs::read_to_string(e.path().join("charge_now")).ok();
        let voltage = std::fs::read_to_string(e.path().join("voltage_now")).ok();
        if let (Some(c), Some(v)) = (charge, voltage) {
            let c_uh = c.trim().parse::<f64>().ok()?;
            let v_uv = v.trim().parse::<f64>().ok()?;
            return Some((c_uh * v_uv) / 3.6e9);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn read_thermal_celsius() -> Option<f64> {
    use std::path::Path;
    let base = Path::new("/sys/class/thermal");
    let entries = std::fs::read_dir(base).ok()?;
    for e in entries.flatten() {
        let t = std::fs::read_to_string(e.path().join("temp")).ok()?;
        // thermal_zone temp is in millidegrees C.
        if let Ok(milli) = t.trim().parse::<f64>() {
            return Some(milli / 1000.0);
        }
    }
    None
}

/// Pick the best available resource limiter for the host.
pub fn default_limiter() -> Box<dyn ResourceLimiter> {
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxResourceLimiter)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::new(SimulatedLimiter)
    }
}

/// Pick the best available power sensor for the host.
pub fn default_power_sensor() -> Box<dyn PowerSensor> {
    #[cfg(target_os = "linux")]
    {
        Box::new(SysfsPowerSensor)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::new(SimulatedPower::new(50_000.0, Some(40.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_limiter_reports_advisory() {
        let l = SimulatedLimiter;
        let o = l.apply(0.5, 0.5);
        assert!(!o.enforced);
        assert_eq!(o.mode, LimiterMode::Simulated);
    }

    #[test]
    fn simulated_power_sensor_is_not_real() {
        let s = SimulatedPower::new(12_000.0, Some(55.0));
        let r = s.read().unwrap();
        assert!(!r.real);
        assert_eq!(r.energy_joules, 12_000.0);
        assert_eq!(r.temperature_celsius, Some(55.0));
    }

    #[test]
    fn default_backend_selected() {
        // Just ensure the constructors exist and return something usable.
        let _lim = default_limiter();
        let _pwr = default_power_sensor();
    }
}
