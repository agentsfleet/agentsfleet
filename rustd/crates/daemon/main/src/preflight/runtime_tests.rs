//! Clock boundaries and recovery budgets resolve through the production startup reader.

use af_core::env::MapEnv;
use std::time::Duration;

use super::runtime::read;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn deployment_defaults_keep_the_current_recovery_budget() -> TestResult {
    let settings = read(&MapEnv::default())?;
    assert_eq!(settings.clock.ttl(), Duration::from_secs(35));
    assert_eq!(settings.clock.offline_after(), Duration::from_secs(105));
    assert_eq!(settings.fallback, Duration::from_secs(5));
    assert_eq!(settings.claims.get(), 8);
    assert_eq!(settings.requests.get(), 256);
    assert!(!settings.resume_from_checkpoint);
    Ok(())
}

#[test]
fn validated_clock_edges_accept_a_strictly_shorter_fallback() -> TestResult {
    for (ttl, fallback) in [("33", "32"), ("43199", "43198")] {
        let settings = read(&MapEnv::from_pairs([
            ("AGENTSFLEET_RUN_TTL_SECONDS", ttl),
            ("AGENTSFLEET_ASSIGN_FALLBACK_SECONDS", fallback),
        ]))?;
        assert!(settings.fallback < settings.clock.ttl());
        assert_eq!(settings.clock.offline_after(), settings.clock.ttl() * 3);
    }
    Ok(())
}

#[test]
fn an_expired_work_interval_must_be_shorter_than_one_run_lifetime() -> TestResult {
    use crate::config::maintenance::{ExpiredWork, Maintenance};
    use af_core::timing::RunSettings;
    let with_interval = |interval_seconds| {
        super::runtime::operations(Maintenance {
            expired_work: ExpiredWork { interval_seconds },
            ..Maintenance::default()
        })
    };
    let run_settings = RunSettings::default;
    let accepted =
        super::runtime::configured(&MapEnv::default(), with_interval(34)?, run_settings())?;
    assert_eq!(accepted.clock.ttl(), Duration::from_secs(35));
    for interval in [35, 3600] {
        let refusal = super::runtime::configured(
            &MapEnv::default(),
            with_interval(interval)?,
            run_settings(),
        )
        .err()
        .ok_or("an expired-work interval as long as a run lifetime was accepted")?;
        assert!(
            format!("{refusal:?}").contains("expired_work_interval_seconds"),
            "the refusal names the interval: {refusal:?}"
        );
    }
    Ok(())
}

#[test]
fn configured_clock_and_budget_reach_the_typed_settings() -> TestResult {
    let settings = read(&MapEnv::from_pairs([
        ("AGENTSFLEET_RUN_TTL_SECONDS", "40"),
        ("AGENTSFLEET_ASSIGN_FALLBACK_SECONDS", "2"),
        ("AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS", "4"),
        ("AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS", "128"),
        ("AGENTSFLEET_RESUME_FROM_CHECKPOINT", "true"),
    ]))?;
    assert_eq!(settings.clock.ttl(), Duration::from_secs(40));
    assert_eq!(settings.clock.offline_after(), Duration::from_secs(120));
    assert_eq!(settings.fallback, Duration::from_secs(2));
    assert_eq!(settings.claims.get(), 4);
    assert_eq!(settings.requests.get(), 128);
    assert!(settings.resume_from_checkpoint);
    Ok(())
}

#[test]
fn unsafe_clock_and_zero_or_malformed_budgets_refuse_startup() {
    for pair in [
        ("AGENTSFLEET_RUN_TTL_SECONDS", "32"),
        ("AGENTSFLEET_RUN_TTL_SECONDS", "43200"),
        ("AGENTSFLEET_ASSIGN_FALLBACK_SECONDS", "35"),
        ("AGENTSFLEET_ASSIGN_FALLBACK_SECONDS", "0"),
        ("AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS", "0"),
        ("AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS", "65"),
        ("AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS", "0"),
        ("AGENTSFLEET_RESUME_FROM_CHECKPOINT", "yes"),
        ("AGENTSFLEET_RUN_TTL_SECONDS", "thirty-five"),
    ] {
        assert!(read(&MapEnv::from_pairs([pair])).is_err(), "{pair:?}");
    }
}
