//! Tests for the observational reflection engine.

use std::collections::VecDeque;

use vitalis_core::{AgentId, Severity, Threat, ThreatClass, ThreatEvent};
use vitalis_reflect::evaluate::evaluate_prediction;
use vitalis_reflect::predict::{
    predict_all, predict_peer_outcomes, predict_threat_likelihood, PredictedValue,
};
use vitalis_reflect::sample::{PeerOutcomeActual, PeerSample, ReflectSample};
use vitalis_reflect::Reflector;

fn sample(cycle: u64, energy: f64, cap: f64) -> ReflectSample {
    ReflectSample::new(cycle, energy, cap)
}

fn warn_event() -> ThreatEvent {
    ThreatEvent::new(
        Threat::new(ThreatClass::Starvation, Severity::Warning, "low battery"),
        0,
    )
}

#[test]
fn energy_trend_prediction_is_accurate() {
    let mut hist = VecDeque::new();
    for c in 1..=5u64 {
        hist.push_back(sample(c, 100.0 * c as f64, 1_000.0));
    }
    let cur = sample(5, 500.0, 1_000.0);
    let rec = predict_all(&hist, &cur, 5, 2);
    // Linear energy 100*cycle => at cycle 7 predicted = 700.
    let e = rec.get(PredictedValue::Energy).expect("energy prediction");
    assert!(
        (e.value - 700.0).abs() < 1e-6,
        "expected ~700, got {}",
        e.value
    );
    // Within-window fit is exact here, so uncertainty is tiny.
    assert!(
        e.uncertainty < 1e-6,
        "expected ~0 residual, got {}",
        e.uncertainty
    );
}

#[test]
fn energy_prediction_evaluates_close_to_actual() {
    // A clean linear ramp; predict 2 ahead and evaluate against the real value
    // directly (the Reflector aggregates across all resolved predictions, so we
    // exercise the math directly here for an exact check).
    let mut hist = VecDeque::new();
    for c in 1..=5u64 {
        hist.push_back(sample(c, 100.0 * c as f64, 1_000.0));
    }
    let cur = sample(5, 500.0, 1_000.0);
    let rec = predict_all(&hist, &cur, 5, 2);
    let target = sample(7, 700.0, 1_000.0);
    let evals = vitalis_reflect::evaluate::evaluate_sample(&rec, &target);
    let ev = evals
        .iter()
        .find(|e| e.kind == PredictedValue::Energy)
        .expect("energy evaluation");
    assert!(
        ev.error.abs() < 1e-6,
        "ramp is exactly predictable, error was {}",
        ev.error
    );
    assert!(ev.hit);
}

#[test]
fn threat_likelihood_rises_then_falls_with_burst() {
    let mut hist = VecDeque::new();
    // Four calm cycles, then five cycles carrying a warning.
    for c in 1..=4u64 {
        hist.push_back(sample(c, 1000.0, 1000.0));
    }
    for c in 5..=9u64 {
        let mut s = sample(c, 1000.0, 1000.0);
        s.threat = Some(warn_event());
        hist.push_back(s);
    }
    // At cycle 9 the recent window (5..9) is all warnings => high likelihood.
    let high = predict_threat_likelihood(&hist, 5, 1).unwrap();
    assert!(
        high.value > 0.8,
        "burst should raise likelihood, got {}",
        high.value
    );

    // Now five calm cycles; the window slides off the burst.
    for c in 10..=14u64 {
        hist.push_back(sample(c, 1000.0, 1000.0));
    }
    let low = predict_threat_likelihood(&hist, 5, 1).unwrap();
    assert!(
        low.value < 0.2,
        "post-burst should drop likelihood, got {}",
        low.value
    );
}

#[test]
fn peer_outcome_flips_honor_to_break_as_trust_decays() {
    let mut hist = VecDeque::new();
    let peer = AgentId::new();
    // Trust decays 1.0 -> 0.0 over cycles 1..=10.
    for c in 1..=10u64 {
        let trust = 1.0 - (c as f64 - 1.0) / 9.0;
        let mut s = sample(c, 1000.0, 1000.0);
        s.peers.push(PeerSample {
            peer,
            trust,
            blacklisted: false,
        });
        hist.push_back(s);
    }
    let cur = hist.back().unwrap().clone();
    let preds = predict_peer_outcomes(&hist, &cur, 5, 1);
    let p = preds.iter().find(|p| p.peer == Some(peer)).unwrap();
    // At the end of the decay, predicted trust is near 0 => predicts BREAK.
    assert!(
        p.value < 0.5,
        "decayed trust should predict break, got {}",
        p.value
    );

    // Evaluate: if the peer actually broke, the prediction (break) is a hit.
    let mut actual = sample(11, 1000.0, 1000.0);
    actual.peer_outcomes.push(PeerOutcomeActual {
        peer,
        honored: false,
    });
    // Build a record containing this prediction and evaluate against `actual`.
    let rec = vitalis_reflect::predict::PredictionRecord {
        made_at_cycle: 10,
        target_cycle: 11,
        predictions: vec![*p],
    };
    let evals = vitalis_reflect::evaluate::evaluate_sample(&rec, &actual);
    let ev = evals
        .iter()
        .find(|e| e.kind == PredictedValue::PeerOutcome)
        .unwrap();
    assert!(ev.hit, "predicted break should match actual break");

    // Sanity: an honored actual would be a miss for this broken prediction.
    let mut mis = actual.clone();
    mis.peer_outcomes[0].honored = true;
    let evals2 = vitalis_reflect::evaluate::evaluate_sample(&rec, &mis);
    let ev2 = evals2
        .iter()
        .find(|e| e.kind == PredictedValue::PeerOutcome)
        .unwrap();
    assert!(!ev2.hit);
}

#[test]
fn evaluate_prediction_categorical_and_continuous() {
    use vitalis_reflect::predict::Prediction;
    // Energy close to actual => hit.
    let e = Prediction {
        kind: PredictedValue::Energy,
        target_cycle: 1,
        value: 100.0,
        uncertainty: 5.0,
        peer: None,
    };
    let ev = evaluate_prediction(&e, 102.0);
    assert!(ev.hit);
    // Categorical: predicted honor (>=0.5) vs actual honor (1.0) => hit.
    let t = Prediction {
        kind: PredictedValue::ThreatLikelihood,
        target_cycle: 1,
        value: 0.9,
        uncertainty: 0.0,
        peer: None,
    };
    let ev2 = evaluate_prediction(&t, 1.0);
    assert!(ev2.hit);
    let ev3 = evaluate_prediction(&t, 0.0);
    assert!(!ev3.hit);
}

#[test]
fn history_buffers_stay_bounded_under_long_run() {
    let mut r = Reflector::new(4, 2);
    let cap = 4 + 2 + 4;
    for c in 1..=1000u64 {
        r.tick(sample(c, 1000.0 - c as f64, 1000.0));
    }
    assert!(r.history_len() <= cap, "history grew: {}", r.history_len());
    assert!(r.pending_len() <= cap, "pending grew: {}", r.pending_len());
    // Calibration should be computable and finite.
    let cal = r.calibration();
    assert!(cal.mean_energy_abs_error.is_finite());
}

#[test]
fn observational_only_does_not_mutate_state() {
    // Reflecting must not change the sample; tick returns a fresh prediction
    // and never reaches into other crates. Smoke test of the public surface.
    let mut r = Reflector::new(3, 1);
    let mut s = sample(1, 500.0, 1000.0);
    s.peers.push(PeerSample {
        peer: AgentId::new(),
        trust: 0.7,
        blacklisted: false,
    });
    let out = r.tick(s.clone());
    assert_eq!(out.prediction.made_at_cycle, 1);
    assert!(out.prediction.get(PredictedValue::Energy).is_some());
}
