//! Rubric row R1 — the daemon boots on compose and answers `/readyz` with 200.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
//!
//! This is the row that cannot be proven by any of the other suites. They drive
//! `boot` and the supervisor as values; R1 is a claim about a PORT — that a
//! process reachable over TCP answers a readiness probe — and the only way to
//! check it is to bind one and connect to it.
//!
//! [`EPHEMERAL`] is bound rather than 3000, and the resolved address is read
//! back from the listener. A fixed port would make the suite fail when a
//! developer has the daemon running, which is exactly when they are most likely
//! to run it. `boot` takes the port as an argument, so no environment variable
//! is involved — the CLI rejects 0 as an operator's answer, and a harness that
//! wants the kernel to choose asks for it directly.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, and a missing lane knob is one"
)]

mod support;

use afd_core::env::MapEnv;
use agentsfleetd::serve::boot;
use agentsfleetd::supervisor::Supervisor;

use self::support::install_subscriber;

/// Where the lane publishes the Postgres it brought up.
const DATABASE_LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Where the lane publishes the TLS Redis it brought up.
const REDIS_LANE_KNOB: &str = "TEST_REDIS_URL";

/// Where the lane extracted the Redis certificate authority to.
const REDIS_CA_LANE_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The port that asks the kernel to choose one.
const EPHEMERAL: u16 = 0;

/// Sixty-four hex characters. Boot validates the key; nothing here decrypts.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Reads a lane knob, failing with the command that sets it.
fn lane(knob: &str) -> String {
    std::env::var(knob).unwrap_or_else(|_unset| {
        panic!("{knob} is unset — run these through `make test-integration-rustd`")
    })
}

/// An environment pointed at the lane's services, on an ephemeral port.
fn lane_environment() -> MapEnv {
    MapEnv::from_pairs(
        [
            ("DATABASE_URL_API", lane(DATABASE_LANE_KNOB).as_str()),
            ("REDIS_URL_API", lane(REDIS_LANE_KNOB).as_str()),
            ("REDIS_TLS_CA_CERT_FILE", lane(REDIS_CA_LANE_KNOB).as_str()),
            ("ENCRYPTION_MASTER_KEY", GOOD_KEK),
        ]
        .into_iter()
        // The provider and the login pepper are required at boot. This lane
        // boots for real, so it must supply both — each is resolved rather
        // than used, so well-formed is enough and nothing here reaches the
        // issuer or takes a digest.
        .chain(support::SESSION_PEPPER)
        .chain(support::IDENTITY),
    )
}

/// Reads one HTTP response's status line from a fresh connection.
async fn get_status(address: std::net::SocketAddr, path: &str) -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("the booted daemon accepts connections");
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("the request is written");

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("the daemon answers");
    let text = String::from_utf8_lossy(&response);
    let status = text
        .split_whitespace()
        .nth(1)
        .expect("an HTTP response has a status code");
    status.parse().expect("the status code is a number")
}

/// R1 — boot to ready on compose.
///
/// The rubric spells this `curl -fsS localhost:3000/readyz` after
/// `agentsfleetd serve`. Same claim, driven in-process so the port is
/// ephemeral and the teardown is asserted rather than left to a signal.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_boot_to_ready_on_compose() {
    install_subscriber();

    let mut supervisor = Supervisor::new();
    let booted = boot(&lane_environment(), EPHEMERAL, &mut supervisor)
        .await
        .expect("the lane's Postgres and Redis are up");

    // Every task the daemon spawns, in spawn order: the pub/sub pump the live
    // streams read through, the four sweepers §6 put under the supervisor, the
    // accept loop, and the analytics flush that drains queued product events
    // before the process exits. Asserted as the WHOLE inventory rather than as
    // a `contains`, because the claim this test makes is C2 — nothing runs
    // outside the supervisor — and a subset check would pass for a sweeper that
    // had quietly gone back to a bare `tokio::spawn` and so would never be
    // cancelled at shutdown.
    //
    // `inventory::OTLP_EXPORT` is in `BACKGROUND_TASKS` and deliberately not
    // here: the exporter's flush loop is spawned only where a span endpoint is
    // configured, and the lane configures none. Listing it would make this
    // assertion fail on the very environment it exists to describe.
    assert_eq!(
        supervisor.inventory(),
        vec![
            agentsfleetd::inventory::HUB_PUMP,
            agentsfleetd::sweepers::LIVENESS,
            agentsfleetd::sweepers::RECLAIM,
            agentsfleetd::sweepers::RETENTION,
            agentsfleetd::sweepers::REPAIR,
            agentsfleetd::inventory::OUTBOUND_WORKER,
            agentsfleetd::serve::ACCEPT_LOOP,
            agentsfleetd::inventory::ANALYTICS_FLUSH,
        ],
        "a booted daemon supervises its pump, its sweepers, its connector \
         answer worker, its accept loop and its analytics flush"
    );
    assert_ne!(
        booted.address.port(),
        0,
        "the bound port is resolved, not the request"
    );

    assert_eq!(
        get_status(booted.address, "/readyz").await,
        200,
        "R1: every dependency is up, so the instance takes traffic"
    );
    assert_eq!(
        get_status(booted.address, "/healthz").await,
        200,
        "liveness answers for the process and nothing else"
    );

    // Teardown, asserted rather than left to a drop: the accept loop is parked
    // in `accept()` and must be interrupted by the token, not waited out.
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        agentsfleetd::daemon::Daemon::new(supervisor)
            .run(std::future::pending(), std::future::ready(())),
    )
    .await
    .expect("cancellation interrupts a blocked accept, rather than waiting out JOIN_TIMEOUT");

    assert!(
        report.is_clean(),
        "the accept loop stops when asked: {report:?}"
    );
    drop(booted);
}

/// A second boot on the same services succeeds, so nothing is left behind.
///
/// Cheap, and it catches the failure that a single-boot test cannot see: a
/// connection, advisory lock or consumer group the first boot did not release.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_a_second_boot_finds_nothing_left_behind() {
    install_subscriber();

    for attempt in 0..2_u8 {
        let mut supervisor = Supervisor::new();
        let booted = boot(&lane_environment(), EPHEMERAL, &mut supervisor)
            .await
            .unwrap_or_else(|failure| panic!("boot {attempt} failed: {failure}"));

        assert_eq!(get_status(booted.address, "/readyz").await, 200);

        let report = agentsfleetd::daemon::Daemon::new(supervisor)
            .run(std::future::pending(), std::future::ready(()))
            .await;
        assert!(
            report.is_clean(),
            "boot {attempt} tore down clean: {report:?}"
        );
        drop(booted);
    }
}
