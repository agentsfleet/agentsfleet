//! Checked timing retains safety relationships when deployment defaults change.

use af_core::timing::{self, LeaseClock, LeaseSettings};
use std::time::Duration;

type Checked = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_and_validated_clocks_share_all_default_durations() -> Checked {
    let settings = garde::Unvalidated::new(LeaseSettings::default()).validate()?;
    for clock in [
        LeaseClock::DEFAULT,
        LeaseClock::default(),
        LeaseClock::new(&settings),
    ] {
        assert_eq!(clock.ttl(), timing::LEASE_TTL);
        assert_eq!(clock.max_runtime(), timing::MAX_RUNTIME);
        assert_eq!(clock.heartbeat_interval(), timing::HEARTBEAT_INTERVAL);
        assert_eq!(clock.offline_after(), timing::RUNNER_OFFLINE_AFTER);
        assert_eq!(clock.uncordon_after(), timing::UNCORDON_AFTER);
        assert_eq!(clock.sandbox_idle_hold(), timing::SANDBOX_HOLD_IDLE);
    }
    Ok(())
}

#[test]
fn omitted_offline_threshold_follows_ttl_and_an_explicit_threshold_wins() -> Checked {
    let mut settings = LeaseSettings {
        ttl_seconds: 40,
        ..LeaseSettings::default()
    };
    let derived = garde::Unvalidated::new(settings).validate()?;
    assert_eq!(
        LeaseClock::new(&derived).offline_after(),
        Duration::from_secs(120)
    );
    settings.offline_after_seconds = Some(90);
    let explicit = garde::Unvalidated::new(settings).validate()?;
    assert_eq!(
        LeaseClock::new(&explicit).offline_after(),
        Duration::from_secs(90)
    );
    Ok(())
}

#[test]
fn each_invalid_safety_relationship_is_rejected() -> Checked {
    let changes: [fn(&mut LeaseSettings); 11] = [
        |s| s.ttl_seconds = timing::MIN_LEASE_SECONDS - 1,
        |s| s.max_runtime_seconds = s.ttl_seconds,
        |s| s.max_runtime_seconds = timing::MAX_RUNTIME.as_secs() + 1,
        |s| s.heartbeat_interval_seconds = 0,
        |s| s.heartbeat_interval_seconds = 53,
        |s| s.offline_after_seconds = Some(s.ttl_seconds - 1),
        |s| s.offline_after_seconds = Some(timing::MAX_RUNTIME.as_secs() * 3 + 1),
        |s| s.healthy_before_uncordon_seconds = s.heartbeat_interval_seconds,
        |s| s.healthy_before_uncordon_seconds = timing::RUNNER_OFFLINE_AFTER.as_secs(),
        |s| s.sandbox_idle_hold_seconds = 0,
        |s| s.sandbox_idle_hold_seconds = timing::MAX_RUNTIME.as_secs() + 1,
    ];
    for change in changes {
        let mut settings = LeaseSettings::default();
        change(&mut settings);
        let failure = garde::Unvalidated::new(settings)
            .validate()
            .err()
            .ok_or("unsafe timing must refuse validation")?;
        assert_ne!(failure.to_string(), "");
    }
    Ok(())
}
