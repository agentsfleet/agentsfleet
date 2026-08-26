//! The subcommands against live datastores, in-process and at the boundary.
//!
//! Two halves, because they prove different things:
//!
//! 1. **In-process** — [`agentsfleetd::cli::run`] driven with a signal that has
//!    already resolved, so `serve` and `migrate` reach their SUCCESS arms
//!    without a process to kill.
//! 2. **Spawned** — the binary, with a real signal sent to it. This is the only
//!    way to observe an exit CODE, and the only way to observe that `--port`
//!    reaches the listener: the port is what a caller has to connect to, and
//!    the Zig daemon's `--port` did not survive the port to Rust at all. Every
//!    one of these fails against a daemon that reads only `PORT`.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, and a missing lane knob is one"
)]

mod support;

use std::io::Read as _;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use afd_core::env::MapEnv;
use agentsfleetd::cli::{Cli, FAILURE, SUCCESS, run};
use clap::Parser as _;

use self::support::install_subscriber;

/// The binary under test, built by Cargo for this suite.
const DAEMON: &str = env!("CARGO_BIN_EXE_agentsfleetd");

/// Where the lane publishes the Postgres it brought up.
const DATABASE_LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Where the lane publishes the TLS Redis it brought up.
const REDIS_LANE_KNOB: &str = "TEST_REDIS_URL";

/// Where the lane extracted the Redis certificate authority to.
const REDIS_CA_LANE_KNOB: &str = "TEST_REDIS_CA_CERT";

/// Sixty-four hex characters. Boot validates the key; nothing here decrypts.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// How long a spawned daemon is given to bind, and to stop once signalled.
const PATIENCE: Duration = Duration::from_secs(20);

/// Reads a lane knob, failing with the command that sets it.
fn lane(knob: &str) -> String {
    std::env::var(knob).unwrap_or_else(|_unset| {
        panic!("{knob} is unset — run these through `make test-integration-rustd`")
    })
}

/// The Postgres pool size a spawned daemon is held to.
///
/// Two, not the production default. Each of these tests boots a REAL daemon,
/// several run at once, and the whole Rust workspace's live-service suites
/// share one compose Postgres with `max_connections = 100`. At the production
/// pool size a handful of daemons plus the rest of the suite exhausts the
/// server, and what that looks like is not "too many connections" — it is a
/// daemon that never finishes booting and a test that times out waiting for
/// `/readyz`, which reads like a bug in boot.
///
/// Two is enough for the probe to answer, which is all these tests ask of it.
const LANE_POOL_SIZE: &str = "2";

/// Every knob a booting daemon needs, pointed at the lane's services.
///
/// `PORT` is NOT among them. Each test states the port it means, so what is
/// being asserted is visible in the test rather than inherited from a fixture.
fn lane_knobs() -> Vec<(&'static str, String)> {
    vec![
        ("DATABASE_URL_API", lane(DATABASE_LANE_KNOB)),
        ("DATABASE_URL_MIGRATOR", lane(DATABASE_LANE_KNOB)),
        ("REDIS_URL_API", lane(REDIS_LANE_KNOB)),
        ("REDIS_TLS_CA_CERT_FILE", lane(REDIS_CA_LANE_KNOB)),
        ("ENCRYPTION_MASTER_KEY", GOOD_KEK.to_owned()),
        ("DATABASE_POOL_SIZE", LANE_POOL_SIZE.to_owned()),
    ]
    .into_iter()
    .chain(support::SESSION_PEPPER.map(|(knob, value)| (knob, value.to_owned())))
    .chain(support::IDENTITY.map(|(knob, value)| (knob, value.to_owned())))
    .collect()
}

/// The lane's knobs as an in-process environment.
fn lane_environment() -> MapEnv {
    MapEnv::from_pairs(
        lane_knobs()
            .iter()
            .map(|(knob, value)| (*knob, value.as_str())),
    )
}

/// A port nothing is listening on, by binding one and letting it go.
///
/// Racy in principle and not in practice: the kernel does not hand the same
/// ephemeral port out twice in the microseconds between this and the daemon's
/// own bind. The alternative — a fixed port — fails whenever a developer has
/// the daemon running, which is exactly when they run this.
fn a_free_port() -> u16 {
    let probe = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port can be bound");
    probe
        .local_addr()
        .expect("a bound listener reports its address")
        .port()
}

/// Spawns the daemon with exactly `knobs`, and nothing inherited.
///
/// The three knobs are stripped for the same reason `tests/binary.rs` strips
/// them, plus `PORT`: a developer with `PORT` exported would otherwise make the
/// `--port` precedence test pass for the wrong reason.
fn spawn(args: &[&str], knobs: &[(&str, String)]) -> Child {
    let mut command = Command::new(DAEMON);
    for knob in [
        "DATABASE_URL_API",
        "DATABASE_URL_MIGRATOR",
        "REDIS_URL_API",
        "REDIS_TLS_CA_CERT_FILE",
        "ENCRYPTION_MASTER_KEY",
        "PORT",
        "DATABASE_POOL_SIZE",
    ]
    .into_iter()
    .chain(support::SESSION_PEPPER.map(|(knob, _value)| knob))
    .chain(support::IDENTITY_KNOBS)
    {
        command.env_remove(knob);
    }
    for (knob, value) in knobs {
        command.env(knob, value);
    }
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the daemon binary runs")
}

/// Waits until `port` answers `/readyz` with 200, or gives up.
fn wait_until_ready(port: u16, child: &mut Child) {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let deadline = Instant::now() + PATIENCE;

    while Instant::now() < deadline {
        if let Ok(status) = child.try_wait()
            && let Some(status) = status
        {
            panic!("the daemon exited before it listened on {port}: {status}");
        }
        if let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(250)) {
            use std::io::Write as _;
            let request = "GET /readyz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            if stream.write_all(request.as_bytes()).is_ok() {
                let mut response = String::new();
                if stream.read_to_string(&mut response).is_ok() && response.contains(" 200 ") {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ignored = child.kill();
    panic!("the daemon never answered /readyz on {port} within {PATIENCE:?}");
}

/// Sends `signal` to `child` and reports the status it exited with.
///
/// `kill(1)` rather than `Child::kill`, which sends SIGKILL — the one signal
/// this daemon cannot catch, and therefore the one that proves nothing about
/// its shutdown.
fn stop_with(signal: &str, mut child: Child) -> i32 {
    let killed = Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()
        .expect("kill(1) runs");
    assert!(killed.success(), "kill {signal} was delivered");

    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("the child's status is readable") {
            return status.code().unwrap_or_else(|| {
                panic!("the daemon was killed by a signal rather than exiting on {signal}")
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ignored = child.kill();
    panic!("the daemon did not exit within {PATIENCE:?} of {signal}");
}

/// `serve` reaches a clean stop and reports success.
///
/// The in-process half: the signal has already resolved, so this drives
/// `cli::run`'s `Serve` arm from boot through `Daemon::run` to the status,
/// which no spawned process can show as a value.
#[test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
fn test_serve_stops_clean_and_reports_success() {
    install_subscriber();

    let status = run(
        &Cli::try_parse_from([
            "agentsfleetd",
            "serve",
            "--port",
            &a_free_port().to_string(),
        ])
        .expect("the port is valid"),
        &lane_environment(),
        tokio::runtime::Runtime::new,
        std::future::ready(()),
    );

    assert_eq!(
        status, SUCCESS,
        "a daemon that booted and stopped when asked exits 0"
    );
}

/// `migrate` applies what is missing and reports success.
#[test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
fn test_migrate_applies_and_reports_success() {
    install_subscriber();

    let status = run(
        &Cli::try_parse_from(["agentsfleetd", "migrate"]).expect("migrate takes no arguments"),
        &lane_environment(),
        tokio::runtime::Runtime::new,
        std::future::ready(()),
    );

    assert_eq!(status, SUCCESS, "the lane's Postgres accepts the schema");

    let again = run(
        &Cli::try_parse_from(["agentsfleetd", "migrate"]).expect("migrate takes no arguments"),
        &lane_environment(),
        tokio::runtime::Runtime::new,
        std::future::ready(()),
    );
    assert_eq!(
        again, SUCCESS,
        "a second run is a no-op, not a failure — which is what makes this safe in an init container"
    );
}

/// `migrate` against a database that is not there is a refusal, not a hang.
#[test]
fn test_migrate_refuses_a_database_that_will_not_answer() {
    let status = run(
        &Cli::try_parse_from(["agentsfleetd", "migrate"]).expect("migrate takes no arguments"),
        &MapEnv::from_pairs([(
            "DATABASE_URL_MIGRATOR",
            "postgres://afd:afd@127.0.0.1:1/afd?sslmode=disable",
        )]),
        tokio::runtime::Runtime::new,
        std::future::ready(()),
    );

    assert_eq!(status, FAILURE, "a migration that could not run exits 1");
}

/// SIGTERM stops a serving daemon, and it exits 0.
///
/// The orchestrator's signal. `--port` is given explicitly, so this also pins
/// that the flag reaches the listener — the whole point of retiring the
/// hand-rolled parser that dropped it.
#[test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
fn test_sigterm_stops_a_serving_daemon() {
    let port = a_free_port();
    let mut child = spawn(&["serve", "--port", &port.to_string()], &lane_knobs());
    wait_until_ready(port, &mut child);

    assert_eq!(
        stop_with("-TERM", child),
        0,
        "a daemon asked to stop by an orchestrator stopped clean"
    );
}

/// SIGINT stops a serving daemon too, and it exits 0.
///
/// The terminal's signal. Watched separately from SIGTERM because they arrive
/// from different places, and a daemon that honours one and not the other
/// hangs for whichever half of its operators uses the other.
#[test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
fn test_sigint_stops_a_serving_daemon() {
    let port = a_free_port();
    let mut child = spawn(&["serve", "--port", &port.to_string()], &lane_knobs());
    wait_until_ready(port, &mut child);

    assert_eq!(stop_with("-INT", child), 0, "Ctrl-C stops it just as well");
}

/// With no `--port`, the daemon binds `PORT`.
///
/// The fallback `clap` documents in `--help`, asserted rather than assumed.
#[test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
fn test_the_port_environment_variable_is_the_fallback() {
    let port = a_free_port();
    let mut knobs = lane_knobs();
    knobs.push(("PORT", port.to_string()));

    let mut child = spawn(&["serve"], &knobs);
    wait_until_ready(port, &mut child);

    assert_eq!(stop_with("-TERM", child), 0);
}

/// `--port` beats `PORT`, and the daemon is not reachable on the one it lost.
///
/// The precedence a `clap` `env` fallback promises. Asserting only that the
/// flag's port answers would pass against a daemon that bound BOTH, so the
/// environment's port is checked to be dead.
#[test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
fn test_the_port_flag_beats_the_environment() {
    let flagged = a_free_port();
    let ignored = a_free_port();
    assert_ne!(flagged, ignored, "the two ports are distinct");

    let mut knobs = lane_knobs();
    knobs.push(("PORT", ignored.to_string()));

    let mut child = spawn(&["serve", "--port", &flagged.to_string()], &knobs);
    wait_until_ready(flagged, &mut child);

    let overridden = SocketAddr::from((Ipv4Addr::LOCALHOST, ignored));
    assert!(
        TcpStream::connect_timeout(&overridden, Duration::from_millis(250)).is_err(),
        "the environment's port {ignored} lost to --port {flagged}, so nothing listens there"
    );

    assert_eq!(stop_with("-TERM", child), 0);
}
