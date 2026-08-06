//! # vitalis-memory
//!
//! Persistent storage for Vitalis agents: identity, generic key/value state,
//! and versioned "knowledge" blobs. The backend is [redb], an embedded,
//! pure-Rust, transactional key/value store — no external process, safe to
//! embed in a constrained agent and durable across restarts and migrations
//! (proves goal **G3**).
//!
//! Layering: depends on `vitalis-core` only.

pub mod identity;
pub mod knowledge;
pub mod state;
pub mod store;

pub use identity::{Credential, IdentityRecord, IdentityStore};
pub use knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
pub use state::StateStore;
pub use store::{MemoryStore, MemoryStoreError};
