//! Public pool settings keep file precedence and reject configured mistakes.

use std::{error::Error as _, time::Duration};

use af_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig, PoolOverrides, PoolSettings};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn role_settings_keep_the_existing_defaults_and_exact_units() -> TestResult {
    let env = MapEnv::from_pairs([
        ("DATABASE_POOL_SIZE", "12"),
        ("DATABASE_POOL_SIZE_API", "7"),
    ]);
    let settings = PoolSettings::resolve(&env, DbRole::Api, PoolOverrides::default())?;
    assert_eq!(settings.max_connections, 7);
    assert_eq!(settings.min_connections, 1);
    assert_eq!(settings.acquire_timeout_ms, 5_000);
    assert_eq!(settings.connect_timeout_ms, 10_000);
    assert_eq!(settings.idle_timeout_seconds, 600);
    assert_eq!(settings.max_lifetime_seconds, 1_800);
    assert_eq!(settings.ping_after_idle_ms, 1_000);
    Ok(())
}

#[test]
fn file_values_override_invalid_environment_values_without_reparsing_them() -> TestResult {
    let env = MapEnv::from_pairs([
        ("DATABASE_URL_API", "postgres://127.0.0.1/settings"),
        ("DATABASE_POOL_SIZE_API", "private-invalid-value"),
        ("DATABASE_MIN_POOL_SIZE_API", "private-invalid-value"),
        ("DATABASE_ACQUIRE_TIMEOUT_MS_API", "private-invalid-value"),
    ]);
    let settings = PoolSettings::resolve(
        &env,
        DbRole::Api,
        PoolOverrides {
            max_connections: Some(3),
            min_connections: Some(0),
            acquire_timeout_ms: Some(750),
            connect_timeout_ms: Some(600),
            idle_timeout_seconds: Some(60),
            max_lifetime_seconds: Some(120),
            ping_after_idle_ms: Some(0),
        },
    )?;
    let config = PoolConfig::configured(&env, DbRole::Api, &settings)?;
    assert_eq!(config.max_connections(), 3);
    assert_eq!(config.min_connections(), 0);
    assert_eq!(config.acquire_timeout(), Duration::from_millis(750));
    assert_eq!(config.connect_timeout(), Duration::from_millis(600));
    assert_eq!(config.idle_timeout(), Duration::from_secs(60));
    assert_eq!(config.max_lifetime(), Duration::from_secs(120));
    assert_eq!(config.ping_after_idle(), Duration::ZERO);
    Ok(())
}

#[test]
fn a_bad_scoped_number_cannot_silently_use_the_shared_setting() -> TestResult {
    let env = MapEnv::from_pairs([
        ("DATABASE_POOL_SIZE", "9"),
        ("DATABASE_POOL_SIZE_API", "private-invalid-value"),
    ]);
    let failure = PoolSettings::resolve(&env, DbRole::Api, PoolOverrides::default())
        .err()
        .ok_or("an explicitly invalid override must refuse")?;
    assert!(failure.is_config());
    assert_eq!(failure.code().as_str(), "UZ-INTERNAL-001");
    assert!(failure.to_string().contains("DATABASE_POOL_SIZE_API"));
    assert!(!failure.to_string().contains("private-invalid-value"));
    assert!(failure.source().is_some());
    Ok(())
}

#[test]
fn unsafe_floors_timeouts_and_lifetimes_refuse_with_their_field() -> TestResult {
    for (file, field) in INVALID_BUDGETS {
        let failure = PoolSettings::resolve(&MapEnv::default(), DbRole::Api, file)
            .err()
            .ok_or("the configured budget must refuse")?;
        assert!(failure.is_config());
        assert_eq!(failure.code().as_str(), "UZ-INTERNAL-001");
        let source = failure
            .source()
            .ok_or("the typed validation report is retained")?;
        assert!(source.to_string().contains(field), "{field}: {source}");
        assert_ne!(source.to_string(), failure.to_string());
    }
    Ok(())
}

const OMITTED: PoolOverrides = PoolOverrides {
    max_connections: None,
    min_connections: None,
    acquire_timeout_ms: None,
    connect_timeout_ms: None,
    idle_timeout_seconds: None,
    max_lifetime_seconds: None,
    ping_after_idle_ms: None,
};

const INVALID_BUDGETS: [(PoolOverrides, &str); 8] = [
    (
        PoolOverrides {
            max_connections: Some(0),
            ..OMITTED
        },
        "max_connections",
    ),
    (
        PoolOverrides {
            max_connections: Some(65_536),
            ..OMITTED
        },
        "max_connections",
    ),
    (
        PoolOverrides {
            max_connections: Some(3),
            min_connections: Some(4),
            ..OMITTED
        },
        "min_connections",
    ),
    (
        PoolOverrides {
            acquire_timeout_ms: Some(0),
            ..OMITTED
        },
        "acquire_timeout_ms",
    ),
    (
        PoolOverrides {
            connect_timeout_ms: Some(0),
            ..OMITTED
        },
        "connect_timeout_ms",
    ),
    (
        PoolOverrides {
            idle_timeout_seconds: Some(0),
            ..OMITTED
        },
        "idle_timeout_seconds",
    ),
    (
        PoolOverrides {
            max_lifetime_seconds: Some(0),
            ..OMITTED
        },
        "max_lifetime_seconds",
    ),
    (
        PoolOverrides {
            ping_after_idle_ms: Some(600_000),
            ..OMITTED
        },
        "ping_after_idle_ms",
    ),
];
