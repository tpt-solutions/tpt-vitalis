//! Deterministic trend-extrapolation prediction (linear slope over a bounded
//! window — no ML dependency).

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use vitalis_core::{AgentId, Severity};

use crate::sample::ReflectSample;

/// The quantity being predicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PredictedValue {
    /// Energy (joules) at a future cycle.
    Energy,
    /// Probability (0..=1) that a warning-or-worse threat occurs next cycle.
    ThreatLikelihood,
    /// Probability (0..=1) that a peer will honor its next bargain.
    PeerOutcome,
}

/// A single prediction of one [`PredictedValue`] at a future cycle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Prediction {
    /// What is being predicted.
    pub kind: PredictedValue,
    /// The cycle this prediction is about.
    pub target_cycle: u64,
    /// Predicted magnitude: joules for `Energy`, a 0..=1 probability otherwise.
    pub value: f64,
    /// Half-width of a simple uncertainty band (deterministic heuristic).
    pub uncertainty: f64,
    /// The peer this prediction is about (only for `PeerOutcome`).
    pub peer: Option<AgentId>,
}

/// All predictions made at one cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionRecord {
    /// The cycle these predictions were made on.
    pub made_at_cycle: u64,
    /// The cycle the predictions target (`made_at_cycle + horizon`).
    pub target_cycle: u64,
    /// The individual predictions.
    pub predictions: Vec<Prediction>,
}

impl PredictionRecord {
    /// Find a prediction of a given kind (the first, if several).
    pub fn get(&self, kind: PredictedValue) -> Option<&Prediction> {
        self.predictions.iter().find(|p| p.kind == kind)
    }
}

/// Least-squares slope + intercept over (x, y) pairs. With fewer than two
/// points the slope is 0 and the intercept is the mean (or only) y.
fn linreg(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    if n == 1 {
        return (0.0, points[0].1);
    }
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n as f64;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n as f64;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (x, y) in points {
        sxx += (x - mean_x) * (x - mean_x);
        sxy += (x - mean_x) * (y - mean_y);
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    let intercept = mean_y - slope * mean_x;
    (slope, intercept)
}

/// Predict energy `horizon` cycles ahead using the last `window` energy
/// samples. Returns `None` if there is no history to extrapolate from.
pub fn predict_energy(
    history: &VecDeque<ReflectSample>,
    window: usize,
    horizon: usize,
) -> Option<Prediction> {
    let made_at = history.back().map(|s| s.cycle)?;
    let slice: Vec<(f64, f64)> = history
        .iter()
        .rev()
        .take(window)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|s| (s.cycle as f64, s.energy))
        .collect();
    if slice.is_empty() {
        return None;
    }
    let (slope, intercept) = linreg(&slice);
    let target = made_at + horizon as u64;
    let mut pred = intercept + slope * target as f64;
    if pred < 0.0 {
        pred = 0.0;
    }
    // Uncertainty = largest in-window residual (deterministic, simple, and
    // guaranteed non-negative).
    let mut unc = 0.0;
    for (x, y) in &slice {
        let resid = (y - (intercept + slope * *x)).abs();
        if resid > unc {
            unc = resid;
        }
    }
    Some(Prediction {
        kind: PredictedValue::Energy,
        target_cycle: target,
        value: pred,
        uncertainty: unc,
        peer: None,
    })
}

/// Predict the probability (0..=1) of a warning-or-worse threat at the target
/// cycle from the fraction of recent cycles that carried such a threat.
pub fn predict_threat_likelihood(
    history: &VecDeque<ReflectSample>,
    window: usize,
    horizon: usize,
) -> Option<Prediction> {
    let made_at = history.back().map(|s| s.cycle)?;
    let recent: Vec<&ReflectSample> = history.iter().rev().take(window).collect();
    if recent.is_empty() {
        return None;
    }
    let n = recent.len();
    let warnings = recent
        .iter()
        .filter(|s| {
            s.threat
                .as_ref()
                .map(|t| matches!(t.severity(), Severity::Warning | Severity::Critical))
                .unwrap_or(false)
        })
        .count();
    let p = warnings as f64 / n as f64;
    // Uncertainty shrinks with sample count.
    let unc = 0.5 / (n as f64).sqrt();
    Some(Prediction {
        kind: PredictedValue::ThreatLikelihood,
        target_cycle: made_at + horizon as u64,
        value: p,
        uncertainty: unc,
        peer: None,
    })
}

/// Predict, for each peer in `sample.peers`, the probability it will honor its
/// next bargain. Uses a linear trend on that peer's trust history, falling back
/// to the current sample trust (clamped to 0..=1) when no history exists.
pub fn predict_peer_outcomes(
    history: &VecDeque<ReflectSample>,
    sample: &ReflectSample,
    window: usize,
    horizon: usize,
) -> Vec<Prediction> {
    let made_at = sample.cycle;
    let target = made_at + horizon as u64;
    let mut out = Vec::new();
    for ps in &sample.peers {
        let series: Vec<(f64, f64)> = history
            .iter()
            .rev()
            .take(window)
            .filter_map(|s| {
                s.peers
                    .iter()
                    .find(|p| p.peer == ps.peer)
                    .map(|p| (s.cycle as f64, p.trust))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let predicted_trust = if series.is_empty() {
            ps.trust.clamp(0.0, 1.0)
        } else {
            let (slope, intercept) = linreg(&series);
            let v = intercept + slope * target as f64;
            v.clamp(0.0, 1.0)
        };
        out.push(Prediction {
            kind: PredictedValue::PeerOutcome,
            target_cycle: target,
            value: predicted_trust,
            uncertainty: 0.5,
            peer: Some(ps.peer),
        });
    }
    out
}

/// Build the full [`PredictionRecord`] for `sample` from the recent `history`.
pub fn predict_all(
    history: &VecDeque<ReflectSample>,
    sample: &ReflectSample,
    window: usize,
    horizon: usize,
) -> PredictionRecord {
    let mut predictions = Vec::new();
    if let Some(e) = predict_energy(history, window, horizon) {
        predictions.push(e);
    }
    if let Some(t) = predict_threat_likelihood(history, window, horizon) {
        predictions.push(t);
    }
    predictions.extend(predict_peer_outcomes(history, sample, window, horizon));
    PredictionRecord {
        made_at_cycle: sample.cycle,
        target_cycle: sample.cycle + horizon as u64,
        predictions,
    }
}
