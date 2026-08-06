//! # vitalis-adapt
//!
//! The evolution crate: bounded, auditable self-improvement.
//!
//! # ⚠️ OFF BY DEFAULT — READ THIS
//!
//! `vitalis-adapt` is the danger zone (spec §10). It is **feature-gated and
//! off by default** at the crate *and* workspace level. Nothing enables it by
//! accident:
//!
//! ```text
//! cargo build -p vitalis-adapt --features adapt
//! ```
//!
//! Even when compiled in, every proposed change goes through a propose →
//! verify → apply split, is bounded (rate limit + diff-size cap), supports
//! rollback, and is written to an audit log. A global kill-switch — which
//! `vitalis-defend` can trip — disables application entirely.
//!
//! The sandbox boundary is shaped like a WASM/WASI runtime (the production
//! enforcement mechanism) but, to keep the crate dependency-light and
//! portable, this reference implementation uses an in-process bounds checker
//! as the boundary. Swapping in `wasmtime`/`wasmer` is a backend change only.

/// Whether the `adapt` feature is compiled in. `false` in every default build
/// and in every published artifact unless a human deliberately enables it.
pub const ENABLED: bool = cfg!(feature = "adapt");

#[cfg(feature = "adapt")]
mod change;
#[cfg(feature = "adapt")]
mod engine;
#[cfg(feature = "adapt")]
mod sandbox;

#[cfg(feature = "adapt")]
pub use change::{ChangeKind, ProposedChange};
#[cfg(feature = "adapt")]
pub use engine::{AdaptEngine, AdaptationResult, AuditEntry};
#[cfg(feature = "adapt")]
pub use sandbox::Sandbox;

#[cfg(feature = "wasm-sandbox")]
pub use sandbox::WasmSandbox;
