//! Native-process hardening — defense-in-depth alongside the WASM sandbox.
//!
//! On Linux with the `harden` feature, [`apply_hardening`] drops the process's
//! privilege-escalation capability (`PR_SET_NO_NEW_PRIVS`) and marks it
//! non-dumpable (`PR_SET_DUMPABLE`), so an attacker who compromises the agent
//! cannot `ptrace` it or read its memory via a core dump. On other platforms,
//! or without the feature, it is a no-op reporting [`HardeningMode::Simulated`].
//!
//! This is the reference hardening, not a complete sandbox. A full landlock
//! filesystem ruleset and/or a seccomp syscall filter are natural future
//! extensions but are intentionally left out of this best-effort pass; the
//! contract for running untrusted extensions remains the WASM runtime.

/// Whether hardening is real OS enforcement or only reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardeningMode {
    /// Real OS primitives applied.
    Os,
    /// No real enforcement; reported for visibility only.
    Simulated,
}

/// The outcome of a hardening attempt.
#[derive(Debug, Clone)]
pub struct HardeningReport {
    pub applied: bool,
    pub mode: HardeningMode,
    pub detail: String,
}

impl HardeningReport {
    fn simulated(detail: impl Into<String>) -> Self {
        Self {
            applied: false,
            mode: HardeningMode::Simulated,
            detail: detail.into(),
        }
    }
}

/// Apply best-effort native hardening. `allowed_paths` is accepted for API
/// symmetry with a future landlock-backed implementation and is currently
/// unused (no-new-privs + non-dumpable do not take a path list).
///
/// Safe to call on any platform: it degrades to a simulated no-op where the
/// primitives are unavailable.
pub fn apply_hardening(allowed_paths: &[&std::path::Path]) -> HardeningReport {
    apply_hardening_inner(allowed_paths)
}

#[cfg(all(feature = "harden", target_os = "linux"))]
fn apply_hardening_inner(_allowed_paths: &[&std::path::Path]) -> HardeningReport {
    // SAFETY: prctl with a valid option and integer args; no pointers are
    // touched. Both calls are best-effort: failure is reported, not fatal.
    let no_new_privs = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } == 0;
    let non_dumpable = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } == 0;
    let applied = no_new_privs || non_dumpable;
    HardeningReport {
        applied,
        mode: if applied {
            HardeningMode::Os
        } else {
            HardeningMode::Simulated
        },
        detail: format!("no-new-privs={no_new_privs}, non-dumpable={non_dumpable}"),
    }
}

#[cfg(not(all(feature = "harden", target_os = "linux")))]
fn apply_hardening_inner(_allowed_paths: &[&std::path::Path]) -> HardeningReport {
    HardeningReport::simulated(
        "hardening feature disabled or unsupported on this platform; no OS enforcement",
    )
}
