//! Compare a prediction against what actually happened.

use serde::{Deserialize, Serialize};
use vitalis_core::{AgentId, Severity};

use crate::predict::{PredictedValue, Prediction};
use crate::sample::ReflectSample;

/// The result of scoring one prediction against ground truth.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EvaluationRecord {
    /// What was predicted.
    pub kind: PredictedValue,
    /// The cycle the prediction was about.
    pub target_cycle: u64,
    /// The predicted value (joules or 0..=1 probability).
    pub predicted: f64,
    /// The actual value (joules, or 1.0/0.0 for categorical outcomes).
    pub actual: f64,
    /// Signed error `actual - predicted`.
    pub error: f64,
    /// 1.0 for a categorical hit (or a close energy call), 0.0 otherwise.
    pub score: f64,
    /// Whether this counts as a hit.
    pub hit: bool,
    /// The peer evaluated (only for `PeerOutcome`).
    pub peer: Option<AgentId>,
}

/// Evaluate a single prediction against a known actual scalar.
///
/// * `Energy`: `hit` iff `|error| <= uncertainty`; `score` decays linearly from
///   1.0 at zero error to 0.0 at/beyond the uncertainty band.
/// * `ThreatLikelihood` / `PeerOutcome`: categorical — `hit` iff the rounded
///   prediction matches the binary actual (0.0/1.0); `score` is 1.0 or 0.0.
pub fn evaluate_prediction(pred: &Prediction, actual: f64) -> EvaluationRecord {
    let error = actual - pred.value;
    let (score, hit) = match pred.kind {
        PredictedValue::Energy => {
            let band = pred.uncertainty.max(1e-9);
            let s = (1.0 - (error.abs() / band).clamp(0.0, 1.0)).clamp(0.0, 1.0);
            (s, error.abs() <= band)
        }
        PredictedValue::ThreatLikelihood | PredictedValue::PeerOutcome => {
            let predicted_binary = if pred.value >= 0.5 { 1.0 } else { 0.0 };
            let actual_binary = if actual >= 0.5 { 1.0 } else { 0.0 };
            let h = predicted_binary == actual_binary;
            (if h { 1.0 } else { 0.0 }, h)
        }
    };
    EvaluationRecord {
        kind: pred.kind,
        target_cycle: pred.target_cycle,
        predicted: pred.value,
        actual,
        error,
        score,
        hit,
        peer: pred.peer,
    }
}

/// Evaluate a pending [`PredictionRecord`](crate::predict::PredictionRecord)
/// against the sample that reached its target cycle. Peer-outcome predictions
/// are only evaluated when a matching ground-truth `peer_outcomes` entry is
/// present; otherwise they are skipped (observational — the main loop has no
/// ground truth).
pub fn evaluate_sample(
    preds: &crate::predict::PredictionRecord,
    sample: &ReflectSample,
) -> Vec<EvaluationRecord> {
    let mut out = Vec::new();
    for p in &preds.predictions {
        match p.kind {
            PredictedValue::Energy => {
                out.push(evaluate_prediction(p, sample.energy));
            }
            PredictedValue::ThreatLikelihood => {
                let occurred = sample
                    .threat
                    .as_ref()
                    .map(|t| matches!(t.severity(), Severity::Warning | Severity::Critical))
                    .unwrap_or(false);
                out.push(evaluate_prediction(p, if occurred { 1.0 } else { 0.0 }));
            }
            PredictedValue::PeerOutcome => {
                if let Some(peer) = p.peer {
                    if let Some(outcome) = sample.peer_outcomes.iter().find(|o| o.peer == peer) {
                        out.push(evaluate_prediction(
                            p,
                            if outcome.honored { 1.0 } else { 0.0 },
                        ));
                    }
                }
            }
        }
    }
    out
}
