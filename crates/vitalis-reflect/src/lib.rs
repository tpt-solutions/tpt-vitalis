//! # vitalis-reflect
//!
//! Observational self-introspection for the Vitalis survival stack.
//!
//! `vitalis-reflect` lets an agent *predict* its own near-future trajectory —
//! energy, threat likelihood, and peer outcomes — and *evaluate* those
//! predictions against what actually happened. It is strictly observational:
//! it changes no behavior and takes no action. Wiring a predicted
//! energy-critical horizon into a proactive-replication trigger is an explicit,
//! deferred future step — see the crate design notes — mirroring
//! `vitalis-adapt`'s off-by-default gating philosophy rather than letting an
//! unproven predictor steer real replication/migration behavior.
//!
//! **Layering:** `vitalis-reflect` depends on `vitalis-core` **only**. The
//! `vitalis-drive` app feeds it plain scalars each cycle; it never reaches into
//! `metabolism` / `defend` / `negotiate` directly, per the workspace layering
//! rule in `AGENTS.md`.

pub mod audit;
pub mod evaluate;
pub mod predict;
pub mod reflector;
pub mod sample;

pub use audit::{ReflectAudit, ReflectEntry};
pub use evaluate::{evaluate_prediction, evaluate_sample, EvaluationRecord};
pub use predict::{predict_all, PredictedValue, Prediction, PredictionRecord};
pub use reflector::{Calibration, Reflector, ReflectorOutput};
pub use sample::{PeerOutcomeActual, PeerSample, ReflectSample};
