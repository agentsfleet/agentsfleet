//! Clock boundaries and recovery budgets resolve through the production startup reader.

use af_core::env::MapEnv;
use std::time::Duration;

use super::runtime::read;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn deployment_defaults_keep_the_current_recovery_budget() -> TestResult {
    let settings = read(&MapEnv::default())?;
    assert_eq!(settings.clock.ttl(), Duration::from_secs(38));
    assert_eq!(settings.clock.offline_after(), Duration::from_secs(114));
    assert_eq!(settings.assignments.get(), 8);
    assert_eq!(settings.requests.get(), 256);
    assert!(!settings.resume_from_checkpoint);
    Ok(())
}

#[test]
fn configured_clock_and_budget_reach_the_typed_settings() -> TestResult {
    let settings = read(&MapEnv::from_pairs([
        ("AGENTSFLEET_RUN_TTL_SECONDS", "40"),
        ("AGENTSFLEET_RECOVERY_CONCURRENT_ASSIGNMENTS", "4"),
        ("AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS", "128"),
        ("AGENTSFLEET_RESUME_FROM_CHECKPOINT", "true"),
    ]))?;
    assert_eq!(settings.clock.ttl(), Duration::from_secs(40));
    assert_eq!(settings.clock.offline_after(), Duration::from_secs(120));
    assert_eq!(settings.assignments.get(), 4);
    assert_eq!(settings.requests.get(), 128);
    assert!(settings.resume_from_checkpoint);
    Ok(())
}

#[test]
fn unsafe_clock_and_zero_or_malformed_budgets_refuse_startup() {
    for pair in [
        ("AGENTSFLEET_RUN_TTL_SECONDS", "32"),
        ("AGENTSFLEET_RUN_TTL_SECONDS", "43200"),
        ("AGENTSFLEET_RECOVERY_CONCURRENT_ASSIGNMENTS", "0"),
        ("AGENTSFLEET_RECOVERY_CONCURRENT_ASSIGNMENTS", "65"),
        ("AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS", "0"),
        ("AGENTSFLEET_RESUME_FROM_CHECKPOINT", "yes"),
        ("AGENTSFLEET_RUN_TTL_SECONDS", "thirty-five"),
    ] {
        assert!(read(&MapEnv::from_pairs([pair])).is_err(), "{pair:?}");
    }
}
