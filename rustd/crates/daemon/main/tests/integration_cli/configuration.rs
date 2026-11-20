//! A spawned daemon applies its file before opening its HTTP listener.

use std::io::Write as _;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::{a_free_port, lane_knobs, spawn, stop_with, wait_until_ready};

#[test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
fn a_selected_file_drives_the_serving_daemon() {
    let port = a_free_port();
    let mut file = tempfile::NamedTempFile::new().expect("application file");
    writeln!(
        file,
        "[server]\nport = {port}\n[runner.run]\nttl_seconds = 40\n\
         [runner.recovery]\nresume_from_checkpoint = false\n\
         [runner.scheduling]\nmissed_wake_probe_seconds = 2\nmax_concurrent_recovery_assignments = 4"
    )
    .expect("deployment settings");
    let path = file.path().to_str().expect("UTF-8 fixture path");
    for through_environment in [false, true] {
        let mut knobs = lane_knobs();
        let args = if through_environment {
            knobs.push(("AGENTSFLEET_CONFIG_FILE", path.to_owned()));
            vec!["serve"]
        } else {
            vec!["--config", path, "serve"]
        };
        let mut child = spawn(&args, &knobs);
        wait_until_ready(port, &mut child);
        assert_eq!(stop_with("-TERM", child), 0);
    }
}

#[test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
fn an_explicit_port_overrides_the_file_before_typed_startup() {
    let port = a_free_port();
    let ignored = a_free_port();
    assert_ne!(port, ignored, "the two ports are distinct");
    let mut file = tempfile::NamedTempFile::new().expect("application file");
    writeln!(file, "[server]\nport = 'replaced-before-deserialization'")
        .expect("overridden file setting");
    let mut knobs = lane_knobs();
    knobs.push(("PORT", ignored.to_string()));
    let mut child = spawn(
        &[
            "--config",
            file.path().to_str().expect("UTF-8 fixture path"),
            "serve",
            "--port",
            &port.to_string(),
        ],
        &knobs,
    );
    wait_until_ready(port, &mut child);
    let overridden = SocketAddr::from((Ipv4Addr::LOCALHOST, ignored));
    TcpStream::connect_timeout(&overridden, Duration::from_millis(250))
        .expect_err("the environment port was overridden");
    assert_eq!(stop_with("-TERM", child), 0);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_selected_file_bounds_migration_lock_contention() {
    use afd_db::config::DbRole;
    use afd_db::migrate::lock::{MigrationLock, RetryPolicy};
    use afd_db::test_util::TestDatabase;
    use agentsfleetd::migrate::MigrateFailure;

    let lane = TestDatabase::create().await;
    let owner = lane.open(DbRole::Migrator, &[]).await;
    let held = MigrationLock::acquire(
        owner.acquire().await.expect("owner connection"),
        RetryPolicy::new(1, Duration::ZERO),
    )
    .await
    .expect("hold the actual migration lock");
    let mut file = tempfile::NamedTempFile::new().expect("application file");
    writeln!(
        file,
        "[database.migrator]\nmax_connections = 1\nmin_connections = 0\n\
         [database.migration]\nlock_max_attempts = 1\nlock_retry_interval_ms = 1"
    )
    .expect("short contention budget");
    let env = lane.env(&[(
        agentsfleetd::config::FILE_KNOB,
        file.path().to_str().expect("UTF-8 fixture path"),
    )]);
    let failure = tokio::time::timeout(Duration::from_secs(5), agentsfleetd::migrate(&env))
        .await
        .expect("the file replaces the default thirty-poll wait")
        .expect_err("the other migrator holds the lock");
    match failure {
        MigrateFailure::Run(error) => {
            assert!(error.is_migration_refused(), "{error}");
            assert_eq!(error.code(), af_core::error_code::STARTUP_MIGRATION_CHECK);
        }
        other => panic!("expected lock contention, got {other}"),
    }
    held.release().await;
    owner.close().await;
    lane.cleanup().await;
}
