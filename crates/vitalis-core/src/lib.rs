//! # vitalis-core
//!
//! Shared types and contracts for the entire TPT Vitalis survival stack.
//!
//! `vitalis-core` is the only crate every other survival crate depends on. It
//! contains no behavior that touches a network, the filesystem, or hardware —
//! only the shared vocabulary ([`AgentId`], [`Resource`], [`Threat`],
//! [`Capability`], [`SurvivalProfile`]), the shared [`Error`] type, the
//! cross-crate [`events`], and the generic [`traits`] that the `drive` loop
//! dispatches on.
//!
//! Layering rule: no survival crate depends on another survival crate except
//! via the shared types and event/trait signatures defined here. The
//! `vitalis-drive` app is the single place that composes the crates.

pub mod error;
pub mod events;
pub mod traits;
pub mod types;

pub use error::{Error, Result};
pub use events::ThreatEvent;
pub use types::{
    AgentId, Capability, CapabilityScope, Resource, ResourceKind, Severity, Snapshot,
    SurvivalProfile, SurvivalProfileConfig, Threat, ThreatClass, ThreatSignal,
};
