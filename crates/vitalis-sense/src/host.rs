//! Host resource probing.
//!
//! A [`HostProbe`] reads the local machine's compute/memory/storage/energy
//! state and reports it as Vitalis [`Resource`]s. On Linux we read `/proc`
//! directly (no external dependency); everywhere else we fall back to a
//! [`SimulatedHost`] so the stack still runs (e.g. on the ESP32 edge tier or
//! in CI on a non-Linux host).

use vitalis_core::{Resource, ResourceKind, Result};

/// Reads the host's resource state.
pub trait HostProbe {
    /// Current resource readings for this host.
    fn read(&self) -> Result<Vec<Resource>>;
}

/// A deterministic, configurable fake host. Useful for tests, simulation, and
/// constrained/embedded targets that have no OS to query.
#[derive(Debug, Clone)]
pub struct SimulatedHost {
    compute: f64,
    storage: f64,
    energy: f64,
    network: f64,
}

impl SimulatedHost {
    /// Construct a simulated host with the given raw readings (in canonical
    /// units: compute units, bytes, joules, bytes/sec).
    pub fn new(compute: f64, storage: f64, energy: f64, network: f64) -> Self {
        Self {
            compute,
            storage,
            energy,
            network,
        }
    }

    /// A reasonable "healthy laptop" default for demos.
    pub fn healthy() -> Self {
        Self::new(
            4.0,
            256.0 * 1024.0 * 1024.0 * 1024.0,
            50_000.0,
            100.0 * 1024.0 * 1024.0,
        )
    }
}

impl HostProbe for SimulatedHost {
    fn read(&self) -> Result<Vec<Resource>> {
        Ok(vec![
            Resource::new(
                ResourceKind::Compute,
                self.compute,
                ResourceKind::Compute.unit(),
            ),
            Resource::new(
                ResourceKind::Storage,
                self.storage,
                ResourceKind::Storage.unit(),
            ),
            Resource::new(
                ResourceKind::Energy,
                self.energy,
                ResourceKind::Energy.unit(),
            ),
            Resource::new(
                ResourceKind::Network,
                self.network,
                ResourceKind::Network.unit(),
            ),
        ])
    }
}

/// Reads real Linux host stats from `/proc` and sysfs.
///
/// This is the brain-tier backend. On the ESP32 edge tier you would use a
/// [`SimulatedHost`] or a hardware-specific probe instead.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct ProcfsHost {
    start: std::time::Instant,
}

#[cfg(target_os = "linux")]
impl ProcfsHost {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    fn read_file(path: &str) -> Result<String> {
        std::fs::read_to_string(path).map_err(vitalis_core::Error::from)
    }
}

#[cfg(target_os = "linux")]
impl HostProbe for ProcfsHost {
    fn read(&self) -> Result<Vec<Resource>> {
        // Memory (MemTotal from /proc/meminfo, in kB -> bytes).
        let meminfo = Self::read_file("/proc/meminfo")?;
        let mem_total_kb = meminfo
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
            .unwrap_or(0.0)
            * 1024.0;

        // Load average (1-min) as a rough compute-pressure proxy.
        let loadavg = Self::read_file("/proc/loadavg")?;
        let load1 = loadavg
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<f64>().ok())
            .unwrap_or(0.0);
        let compute = (load1.max(0.0) + 1.0).clamp(0.0, f64::MAX);

        // Energy: try the power supply sysfs; fall back to a fixed supply.
        let energy = read_battery_joules().unwrap_or(50_000.0);

        // Disk: free space of the current directory.
        let storage = fs_free_bytes(".").unwrap_or(0.0);

        Ok(vec![
            Resource::new(ResourceKind::Compute, compute, ResourceKind::Compute.unit()),
            Resource::new(ResourceKind::Storage, storage, ResourceKind::Storage.unit()),
            Resource::new(ResourceKind::Energy, energy, ResourceKind::Energy.unit()),
            Resource::new(
                ResourceKind::Network,
                100.0 * 1024.0 * 1024.0,
                ResourceKind::Network.unit(),
            ),
        ])
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
            // microamp-hours * microvolts -> joules (approx, / 3.6e9)
            return Some((c_uh * v_uv) / 3.6e9);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn fs_free_bytes(path: &str) -> Option<f64> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    // Best-effort: treat available blocks * block size as "free". Not portable
    // to all filesystems but fine for a probe.
    Some((meta.blocks() as f64) * (meta.blksize() as f64))
}

/// Convenience constructor that picks the best available probe for the host.
pub fn default_probe() -> Box<dyn HostProbe> {
    #[cfg(target_os = "linux")]
    {
        Box::new(ProcfsHost::new())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::new(SimulatedHost::healthy())
    }
}
