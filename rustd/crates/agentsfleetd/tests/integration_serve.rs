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

use afd_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use agentsfleetd::serve::boot;
use agentsfleetd::supervisor::Supervisor;

use crate::support::install_subscriber;

/// Where the lane publishes the Postgres it brought up.
const DATABASE_LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Where the lane publishes the TLS Dragonfly it brought up.
const DRAGONFLY_LANE_KNOB: &str = "TEST_DRAGONFLY_URL";

/// Where the lane extracted the Dragonfly certificate authority to.
const DRAGONFLY_CA_LANE_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// The port that asks the kernel to choose one.
const EPHEMERAL: u16 = 0;

/// Sixty-four hex characters. Boot validates the key; nothing here decrypts.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Reads a lane knob, failing with the command that sets it.
fn lane(knob: &str) -> String {
    ProcessEnv.get(knob).unwrap_or_else(|| {
        panic!("{knob} is unset — run these through `make test-integration-rustd`")
    })
}

/// An environment pointed at the lane's services, on an ephemeral port.
fn lane_environment() -> MapEnv {
    MapEnv::from_pairs(
        [
            ("DATABASE_URL_API", lane(DATABASE_LANE_KNOB).as_str()),
            ("DRAGONFLY_URL", lane(DRAGONFLY_LANE_KNOB).as_str()),
            (
                "DRAGONFLY_TLS_CA_CERT_FILE",
                lane(DRAGONFLY_CA_LANE_KNOB).as_str(),
            ),
            ("ENCRYPTION_MASTER_KEY", GOOD_KEK),
        ]
        .into_iter()
        // The provider and the login pepper are required at boot. This lane
        // boots for real, so it must supply both — each is resolved rather
        // than used, so well-formed is enough and nothing here reaches the
        // issuer or takes a digest.
        .chain(crate::support::SESSION_PEPPER)
        .chain(crate::support::IDENTITY),
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
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_boot_to_ready_on_compose() {
    install_subscriber();

    let mut supervisor = Supervisor::new();
    let booted = boot(&lane_environment(), EPHEMERAL, &mut supervisor)
        .await
        .expect("the lane's Postgres and Dragonfly are up");

    assert_supervises_every_task(&supervisor);
    assert_ne!(
        booted.address.port(),
        0,
        "the bound port is resolved, not the request"
    );

    // Asserted BEFORE `/readyz`, deliberately. The readiness probe acquires a
    // connection, and on a cold lazy pool that acquire opens one — so checking
    // after it would pass on a daemon that never warmed at all. Boot is the
    // only thing that can have filled the pool at this point.
    assert!(
        booted.database.size() >= booted.database.min_connections(),
        "boot warms the pool to its floor: {} open against a floor of {}",
        booted.database.size(),
        booted.database.min_connections()
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

    assert_tears_down_promptly(supervisor, &booted).await;
    drop(booted);
}

/// The complete task inventory proves all long-lived workers are supervised.
/// The telemetry exporter is absent because this fixture has no span endpoint.
fn assert_supervises_every_task(supervisor: &Supervisor) {
    assert_eq!(
        supervisor.inventory(),
        vec![
            agentsfleetd::sweepers::LIVENESS,
            agentsfleetd::sweepers::RETENTION,
            agentsfleetd::sweepers::FLEET_CENSUS,
            agentsfleetd::sweepers::AUTHORING,
            agentsfleetd::sweepers::BILLING,
            agentsfleetd::inventory::HUB_PUMP,
            agentsfleetd::inventory::OUTBOUND_WORKER,
            agentsfleetd::outbound::OUTBOUND_PRODUCER,
            agentsfleetd::serve::ACCEPT_LOOP,
            agentsfleetd::inventory::ANALYTICS_FLUSH,
        ],
        "a booted daemon supervises its pump, its sweepers, both halves of \
         outbound delivery, its accept loop and its analytics flush"
    );
}

/// Teardown, asserted rather than left to a drop: the accept loop is parked
/// in `accept()` and must be interrupted, not waited out.
///
/// `draining` is what interrupts it, and leaving it off is not a detail this
/// test can skip — it is the difference between a clean stop and a ten
/// second hang. The loop selects over the DRAIN's token rather than the
/// supervisor's, because stopping the accept and cutting the
/// connections already accepted are two different instants and the bound
/// between them is the drain's whole purpose. A `Daemon` built without the
/// drain therefore cancels every other task and leaves this one parked in
/// `accept()` until `JOIN_TIMEOUT` abandons it. `serve::run` hands it over
/// for exactly this reason; a teardown that did not would be asserting on a
/// shutdown sequence production does not run.
async fn assert_tears_down_promptly(supervisor: Supervisor, booted: &agentsfleetd::serve::Booted) {
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        agentsfleetd::daemon::Daemon::new(supervisor)
            .draining(booted.drain.clone())
            .run(std::future::pending(), std::future::ready(())),
    )
    .await
    .expect("cancellation interrupts a blocked accept, rather than waiting out JOIN_TIMEOUT");

    assert!(
        report.is_clean(),
        "the accept loop stops when asked: {report:?}"
    );
}

/// A second boot on the same services succeeds, so nothing is left behind.
///
/// Cheap, and it catches the failure that a single-boot test cannot see: a
/// connection, advisory lock or consumer group the first boot did not release.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_a_second_boot_finds_nothing_left_behind() {
    install_subscriber();

    for attempt in 0..2_u8 {
        let mut supervisor = Supervisor::new();
        let booted = boot(&lane_environment(), EPHEMERAL, &mut supervisor)
            .await
            .unwrap_or_else(|failure| panic!("boot {attempt} failed: {failure}"));

        assert_eq!(get_status(booted.address, "/readyz").await, 200);

        // `draining` for the reason the teardown above spells out: without it
        // the accept loop is never told to stop and the report comes back
        // carrying it as abandoned, ten seconds later.
        let report = agentsfleetd::daemon::Daemon::new(supervisor)
            .draining(booted.drain.clone())
            .run(std::future::pending(), std::future::ready(()))
            .await;
        assert!(
            report.is_clean(),
            "boot {attempt} tore down clean: {report:?}"
        );
        drop(booted);
    }
}
