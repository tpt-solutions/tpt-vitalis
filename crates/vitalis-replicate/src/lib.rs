//! # vitalis-replicate
//!
//! The reproductive system of the agent: checkpoint a running agent's state,
//! live-migrate it off dying hardware, and keep erasure-coded redundant copies
//! — with a hard, auditable copy-limit so replication is survival, never a
//! plague (spec §10).
//!
//! Layering: depends on `vitalis-core` (shared types/events), `vitalis-memory`
//! (durable state), and `vitalis-sense` (discovering migration targets). It is
//! the one crate allowed those extra internal deps; everything else composes
//! it through the `drive` app.

pub mod audit;
pub mod format;
pub mod policy;
pub mod replicator;
pub mod shards;

pub use audit::{AuditEntry, ReplicationAudit, ReplicationReason};
pub use format::{Checkpoint, CHECKPOINT_MAGIC};
pub use policy::CopyLimitPolicy;
pub use replicator::Replicator;
pub use shards::{decode_shards, encode_shards};
