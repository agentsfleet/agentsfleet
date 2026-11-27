//! Checked timing retains safety relationships when deployment defaults change.

use std::time::Duration;

use af_core::timing::{DAY, MAX_RUN_SECONDS};

use super::{
    HEARTBEAT_INTERVAL, MIN_RUN_SECONDS, RUN_TTL, RUNNER_OFFLINE_AFTER, RunClock, RunSettings,
    SANDBOX_PARK_IDLE, UNCORDON_AFTER,
};

type Checked = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_and_validated_clocks_share_all_default_durations() -> Checked {
    let settings = garde::Unvalidated::new(RunSettings::default()).validate()?;
    for clock in [
        RunClock::DEFAULT,
        RunClock::default(),
        RunClock::new(&settings),
    ] {
        assert_eq!(clock.ttl(), RUN_TTL);
        assert_eq!(clock.heartbeat_interval(), HEARTBEAT_INTERVAL);
        assert_eq!(clock.offline_after(), RUNNER_OFFLINE_AFTER);
        assert_eq!(clock.uncordon_after(), UNCORDON_AFTER);
        assert_eq!(clock.sandbox_idle_park(), SANDBOX_PARK_IDLE);
    }
    Ok(())
}

#[test]
fn omitted_offline_threshold_follows_ttl_and_an_explicit_threshold_wins() -> Checked {
    let mut settings = RunSettings {
        ttl_seconds: 40,
        ..RunSettings::default()
    };
    let derived = garde::Unvalidated::new(settings).validate()?;
    assert_eq!(
        RunClock::new(&derived).offline_after(),
        Duration::from_secs(120)
    );
    settings.offline_after_seconds = Some(90);
    let explicit = garde::Unvalidated::new(settings).validate()?;
    assert_eq!(
        RunClock::new(&explicit).offline_after(),
        Duration::from_secs(90)
    );
    Ok(())
}

#[test]
fn each_invalid_safety_relationship_is_rejected() -> Checked {
    let changes: [fn(&mut RunSettings); 10] = [
        |s| s.ttl_seconds = MIN_RUN_SECONDS - 1,
        |s| s.ttl_seconds = MAX_RUN_SECONDS + 1,
        |s| s.heartbeat_interval_seconds = 0,
        |s| s.heartbeat_interval_seconds = 53,
        |s| s.offline_after_seconds = Some(s.ttl_seconds - 1),
        |s| s.offline_after_seconds = Some(MAX_RUN_SECONDS * 3 + 1),
        |s| s.healthy_before_uncordon_seconds = s.heartbeat_interval_seconds,
        |s| s.healthy_before_uncordon_seconds = RUNNER_OFFLINE_AFTER.as_secs(),
        |s| s.sandbox_idle_park_seconds = 0,
        |s| s.sandbox_idle_park_seconds = DAY.as_secs() + 1,
    ];
    for change in changes {
        let mut settings = RunSettings::default();
        change(&mut settings);
        let failure = garde::Unvalidated::new(settings)
            .validate()
            .err()
            .ok_or("unsafe timing must refuse validation")?;
        assert_ne!(failure.to_string(), "");
    }
    Ok(())
}
