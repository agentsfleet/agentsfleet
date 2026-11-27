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

// `[checkpoint] cipher` reaches the plane as the cipher it names: encrypting
// when unset, plain when asked, and refused in any other spelling.
#[test]
fn the_checkpoint_cipher_defaults_to_encryption_and_refuses_an_unknown_spelling() -> TestResult {
    use af_wire::durable_task::CheckpointCipher;
    let unset = read(&MapEnv::default())?;
    assert_eq!(unset.checkpoint_cipher, CheckpointCipher::Aes256Gcm);
    for (spelling, cipher) in [
        ("aes_256_gcm", CheckpointCipher::Aes256Gcm),
        ("plain", CheckpointCipher::Plain),
    ] {
        let named = read(&MapEnv::from_pairs([(
            "AGENTSFLEET_CHECKPOINT_CIPHER",
            spelling,
        )]))?;
        assert_eq!(named.checkpoint_cipher, cipher, "{spelling}");
    }
    for unknown in ["none", "AES_256_GCM", "aes256_gcm", ""] {
        let refused = read(&MapEnv::from_pairs([(
            "AGENTSFLEET_CHECKPOINT_CIPHER",
            unknown,
        )]));
        assert_eq!(
            refused.map(|settings| settings.checkpoint_cipher).ok(),
            None,
            "{unknown}"
        );
    }
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
