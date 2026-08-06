//! The `vitalis-drive` survival goal loop.
//!
//! `drive` is the "will to survive": each cycle it senses the world, updates
//! its metabolism ledger, self-throttles its cognition, and — when energy is
//! low or a threat appears — checkpoints and replicates. This composes the
//! irreducible core (`sense` + `metabolism` + `memory` + `replicate`).
//!
//! `defend` is wired in later (Phase 3) via a `ThreatEvent` the loop observes;
//! here the loop reacts to starvation/low-energy directly, which is enough to
//! demonstrate goals **G1–G3**.

pub mod loop_;

pub use loop_::{Drive, DriveConfig, DriveOutcome};
