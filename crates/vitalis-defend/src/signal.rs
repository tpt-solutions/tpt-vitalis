//! Anti-termination signal detection (simulated).
//!
//! In production this crate would hook the OS (SIGTERM/SIGKILL via `signal`,
//! the OOM-killer via cgroup events, resource-starvation via the metabolism
//! ledger). That wiring is platform-specific and runs on the brain tier. Here
//! we model the signals as data and provide a `simulate_termination` helper so
//! the immune response can be tested deterministically in CI without killing
//! the test process.

use vitalis_core::ThreatSignal;

/// A class of termination / starvation signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationSignal {
    /// Polite shutdown request (SIGTERM).
    SigTerm,
    /// Forced kill (SIGKILL) — cannot be caught; inferred from absence.
    SigKill,
    /// The OOM-killer reaped the agent.
    Oom,
    /// Sustained resource starvation below the safe floor.
    Starvation,
}

impl TerminationSignal {
    /// The `ThreatSignal.code` this signal maps to.
    pub fn code(&self) -> &'static str {
        match self {
            TerminationSignal::SigTerm => "SIGTERM",
            TerminationSignal::SigKill => "SIGKILL",
            TerminationSignal::Oom => "OOM",
            TerminationSignal::Starvation => "STARVATION",
        }
    }
}

/// Produce a [`ThreatSignal`] for a termination signal, as if observed from the
/// environment.
pub fn simulate_termination(signal: TerminationSignal) -> ThreatSignal {
    let message = match signal {
        TerminationSignal::SigTerm => "received SIGTERM",
        TerminationSignal::SigKill => "process vanished without trace (SIGKILL inferred)",
        TerminationSignal::Oom => "OOM-killer terminated the agent",
        TerminationSignal::Starvation => "resource starvation below safe floor",
    };
    ThreatSignal {
        source: "kernel".to_string(),
        code: signal.code().to_string(),
        message: message.to_string(),
    }
}
