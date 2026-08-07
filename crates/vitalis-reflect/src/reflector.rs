//! The [`Reflector`] — bounded history + per-cycle prediction/evaluation.

use std::collections::VecDeque;

use crate::audit::{ReflectAudit, ReflectEntry};
use crate::evaluate::{evaluate_sample, EvaluationRecord};
use crate::predict::{predict_all, PredictedValue, PredictionRecord};
use crate::sample::ReflectSample;

/// Output of one [`Reflector::tick`]: the fresh prediction plus any predictions
/// that resolved against this cycle's sample.
#[derive(Debug, Clone)]
pub struct ReflectorOutput {
    /// The prediction made for this cycle (target = cycle + horizon).
    pub prediction: PredictionRecord,
    /// Evaluations of predictions whose target cycle was reached this cycle.
    pub evaluations: Vec<EvaluationRecord>,
}

/// Summary of prediction accuracy so far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    /// Mean absolute energy error (joules) over resolved energy predictions.
    pub mean_energy_abs_error: f64,
    /// Number of resolved energy predictions.
    pub energy_n: usize,
    /// Hit rate (0..=1) over resolved threat-likelihood predictions.
    pub threat_hit_rate: f64,
    /// Number of resolved threat-likelihood predictions.
    pub threat_n: usize,
    /// Hit rate (0..=1) over resolved peer-outcome predictions.
    pub peer_hit_rate: f64,
    /// Number of resolved peer-outcome predictions.
    pub peer_n: usize,
}

/// Observational self-introspection engine.
///
/// Holds a *bounded* history (`window + horizon + slack`) and a bounded set of
/// pending predictions, so a long-running loop never grows memory unboundedly.
/// It predicts and evaluates only; it never triggers behavior.
pub struct Reflector {
    window: usize,
    horizon: usize,
    history: VecDeque<ReflectSample>,
    pending: VecDeque<PredictionRecord>,
    audit: ReflectAudit,
}

impl Reflector {
    pub fn new(window: usize, horizon: usize) -> Self {
        let window = window.max(1);
        let horizon = horizon.max(1);
        let cap = window + horizon + 4;
        Self {
            window,
            horizon,
            history: VecDeque::with_capacity(cap),
            pending: VecDeque::with_capacity(cap),
            audit: ReflectAudit::new(),
        }
    }

    pub fn window(&self) -> usize {
        self.window
    }

    pub fn horizon(&self) -> usize {
        self.horizon
    }

    pub fn audit(&self) -> &ReflectAudit {
        &self.audit
    }

    /// Number of samples retained in the rolling history.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Number of not-yet-resolved predictions queued.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Feed one cycle's sample through the reflector: resolve any predictions
    /// whose target cycle has arrived, record the new sample, and emit a fresh
    /// prediction `horizon` cycles ahead.
    pub fn tick(&mut self, sample: ReflectSample) -> ReflectorOutput {
        let cap = self.window + self.horizon + 4;

        // 1. Resolve pending predictions whose target cycle has arrived.
        let mut evaluations = Vec::new();
        while let Some(front) = self.pending.front() {
            if front.target_cycle <= sample.cycle {
                let pred = self.pending.pop_front().expect("front exists");
                let evals = evaluate_sample(&pred, &sample);
                if !evals.is_empty() {
                    self.audit.record(ReflectEntry {
                        prediction: pred,
                        evaluation: Some(evals.clone()),
                    });
                    evaluations.extend(evals);
                }
            } else {
                break;
            }
        }

        // 2. Record the new sample, keeping the history bounded.
        self.history.push_back(sample.clone());
        while self.history.len() > cap {
            self.history.pop_front();
        }

        // 3. Emit a fresh prediction for this cycle.
        let prediction = predict_all(&self.history, &sample, self.window, self.horizon);
        self.pending.push_back(prediction.clone());
        while self.pending.len() > cap {
            self.pending.pop_front();
        }

        ReflectorOutput {
            prediction,
            evaluations,
        }
    }

    /// Calibration summary across all resolved predictions in the audit log.
    pub fn calibration(&self) -> Calibration {
        let mut e_err = 0.0;
        let mut e_n = 0usize;
        let mut t_hit = 0usize;
        let mut t_n = 0usize;
        let mut p_hit = 0usize;
        let mut p_n = 0usize;
        for e in self.audit.entries() {
            if let Some(evals) = &e.evaluation {
                for ev in evals {
                    match ev.kind {
                        PredictedValue::Energy => {
                            e_err += ev.error.abs();
                            e_n += 1;
                        }
                        PredictedValue::ThreatLikelihood => {
                            t_n += 1;
                            if ev.hit {
                                t_hit += 1;
                            }
                        }
                        PredictedValue::PeerOutcome => {
                            p_n += 1;
                            if ev.hit {
                                p_hit += 1;
                            }
                        }
                    }
                }
            }
        }
        Calibration {
            mean_energy_abs_error: if e_n > 0 { e_err / e_n as f64 } else { 0.0 },
            energy_n: e_n,
            threat_hit_rate: if t_n > 0 {
                t_hit as f64 / t_n as f64
            } else {
                0.0
            },
            threat_n: t_n,
            peer_hit_rate: if p_n > 0 {
                p_hit as f64 / p_n as f64
            } else {
                0.0
            },
            peer_n: p_n,
        }
    }
}
