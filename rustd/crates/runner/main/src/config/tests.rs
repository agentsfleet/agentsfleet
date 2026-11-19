use super::*;
use af_core::env::MapEnv;
use std::io::Write as _;

type Checked = Result<(), Box<dyn std::error::Error>>;

#[test]
fn installed_runner_profile_preserves_defaults() -> Checked {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../deploy/baremetal/agentsfleet-runner.toml"
    );
    let selected = read(&MapEnv::from_pairs([(ENV_CONFIG_FILE, path)]))
        .map_err(|status| format!("installed profile refused: {status:?}"))?;
    assert_eq!(selected.inference, af_inference::Timeouts::DEFAULT);
    assert_eq!(
        selected.provider_retries,
        af_providers::RetryPolicy::DEFAULT
    );
    assert_eq!(
        selected.supervisor,
        afr_supervisor::config::RuntimePolicy::DEFAULT
    );
    let defaults =
        read(&MapEnv::default()).map_err(|status| format!("defaults refused: {status:?}"))?;
    assert_eq!(defaults.supervisor, selected.supervisor);
    Ok(())
}

#[test]
fn custom_profile_reaches_component_policies() -> Checked {
    let mut file = tempfile::NamedTempFile::new()?;
    write!(
        file,
        "[inference]\nconnect_timeout_seconds=2\nstream_idle_timeout_seconds=7\n\
        [provider_retries]\nmax_attempts=1\nmax_stream_open_attempts=1\n\
        [supervisor]\ndaemon_max_attempts=2\nrenewal_interval_ms=1000\nmax_concurrent_file_operations=1"
    )?;
    let env = MapEnv::from_pairs([(ENV_CONFIG_FILE, file.path().to_str().ok_or("UTF-8 path")?)]);
    let selected = read(&env).map_err(|status| format!("custom profile refused: {status:?}"))?;
    assert_eq!(selected.inference.connect().as_secs(), 2);
    assert_eq!(selected.inference.stream_idle().as_secs(), 7);
    assert_ne!(
        selected.provider_retries,
        af_providers::RetryPolicy::DEFAULT
    );
    assert_eq!(selected.supervisor.settings().daemon_max_attempts, 2);
    assert_eq!(selected.supervisor.settings().renewal_interval_ms, 1000);
    assert_eq!(
        selected
            .supervisor
            .settings()
            .max_concurrent_file_operations,
        1
    );
    Ok(())
}

#[test]
fn invalid_or_unknown_settings_refuse_before_connecting() -> Checked {
    for invalid in [
        "[inference]\nconnect_timeout_seconds=0",
        "[inference]\nstream_idle_timeout_seconds=3601",
        "[provider_retries]\nmax_attempts=0",
        "[provider_retries]\nmax_stream_open_attempts=11",
        "[supervisor]\ndaemon_initial_backoff_ms=100\ndaemon_max_backoff_ms=99",
        "[supervisor]\nrenewal_interval_ms=5001",
        "[supervisor]\nmax_concurrent_file_operations=0",
        "[supervisor]\nmax_workers=16",
        "[secrets]\ntoken=1",
        "[broken",
    ] {
        let mut file = tempfile::NamedTempFile::new()?;
        write!(file, "{invalid}")?;
        let env =
            MapEnv::from_pairs([(ENV_CONFIG_FILE, file.path().to_str().ok_or("UTF-8 path")?)]);
        assert!(read(&env).is_err(), "{invalid}");
    }
    assert!(
        read(&MapEnv::from_pairs([(
            ENV_CONFIG_FILE,
            "/definitely-missing-runner-profile"
        )]))
        .is_err()
    );
    Ok(())
}
