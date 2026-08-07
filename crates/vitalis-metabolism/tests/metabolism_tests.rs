use vitalis_core::traits::Metabolize;
use vitalis_core::{Resource, ResourceKind, SurvivalProfile};
use vitalis_metabolism::ledger::ResourceLedger;
#[cfg(target_os = "linux")]
use vitalis_metabolism::os::{LimiterMode, LinuxResourceLimiter, SysfsPowerSensor};
use vitalis_metabolism::throttle::{
    compute_throttle, SafetyCeiling, ThrottleController, ThrottleProfile,
};
use vitalis_metabolism::Metabolism;

#[test]
fn ledger_tracks_balance() {
    let mut l = ResourceLedger::new();
    l.acquire(&Resource::new(ResourceKind::Energy, 100.0, "J"))
        .unwrap();
    l.consume(&Resource::new(ResourceKind::Energy, 30.0, "J"))
        .unwrap();
    assert_eq!(l.balance(ResourceKind::Energy), 70.0);
    // over-consuming is rejected
    assert!(l
        .consume(&Resource::new(ResourceKind::Energy, 999.0, "J"))
        .is_err());
}

#[test]
fn throttle_full_when_energy_high() {
    let p = ThrottleProfile::for_profile(SurvivalProfile::Feral);
    assert_eq!(compute_throttle(1.0, &p), 1.0);
    assert_eq!(compute_throttle(0.5, &p), 1.0); // at throttle_start
}

#[test]
fn feral_throttles_earlier_than_apex() {
    let apex = ThrottleProfile::for_profile(SurvivalProfile::Apex);
    let feral = ThrottleProfile::for_profile(SurvivalProfile::Feral);
    // At 30% energy, Apex is still full (wasteful); Feral is throttled.
    assert_eq!(compute_throttle(0.30, &apex), 1.0);
    assert!(compute_throttle(0.30, &feral) < 1.0);
}

#[test]
fn battery_drain_curve_drops_throttle() {
    let mut m = Metabolism::new(SurvivalProfile::Feral);
    let mut last = 1.0;
    for step in (0..=10).rev() {
        let frac = step as f64 / 10.0;
        let t = m.tick_energy(frac).unwrap();
        // throttle is monotonically non-increasing as energy drains
        assert!(
            t <= last + 1e-9,
            "throttle rose while draining: {t} > {last}"
        );
        last = t;
    }
    // at empty, throttle hits the safety floor
    assert_eq!(
        m.throttle_level(),
        SafetyCeiling::default().min_cognition_rate()
    );
}

#[test]
fn safety_ceiling_blocks_excessive_draw() {
    let c = SafetyCeiling::default();
    let host = Resource::new(ResourceKind::Energy, 100.0, "J");
    // 81% would exceed the 80% ceiling
    assert!(!c.can_consume(&Resource::new(ResourceKind::Energy, 81.0, "J"), &host));
    assert!(c.can_consume(&Resource::new(ResourceKind::Energy, 80.0, "J"), &host));

    let ctrl = ThrottleController::new(SurvivalProfile::Apex);
    assert!(!ctrl.safe_to_draw(&Resource::new(ResourceKind::Energy, 81.0, "J"), &host));
}

#[test]
fn set_throttle_rejects_out_of_range() {
    let mut m = Metabolism::new(SurvivalProfile::MicroWilds);
    assert!(m.set_throttle(1.5).is_err());
    assert!(m.set_throttle(-0.1).is_err());
    assert!(m.set_throttle(0.4).is_ok());
    assert_eq!(m.throttle_level(), 0.4);
}

/// Real Linux resource limiter applies a real OS primitive (runs only on Linux).
#[cfg(target_os = "linux")]
#[test]
fn linux_limiter_applies_real_os_primitive() {
    let lim = LinuxResourceLimiter;
    assert_eq!(lim.mode(), LimiterMode::Os);
    // Applying a memory cap attempts setrlimit/RLIMIT_AS; it must report a
    // definitive outcome without panicking.
    let out = lim.apply(0.5, 0.5);
    assert!(out.enforced || out.mode == LimiterMode::Simulated);
}

/// Real Linux power sensor reads sysfs and reports a real reading (runs only on
/// Linux).
#[cfg(target_os = "linux")]
#[test]
fn linux_power_sensor_reads_real_hw() {
    let sensor = SysfsPowerSensor;
    let r = sensor.read().unwrap();
    assert!(r.real);
    assert!(r.energy_joules > 0.0);
}
