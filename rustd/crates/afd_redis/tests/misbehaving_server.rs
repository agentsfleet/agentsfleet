//! What this client does when the server answers, but answers wrongly.
//!
//! Every other target here points at a Redis that works, so none of them can
//! reach the branches written for one that does not. The distinction these hold
//! is the one an operator is paged by: a reply this client did not expect is
//! Redis being *wrong*, and a socket that dies mid-command is Redis being
//! *gone*. Collapsing them sends someone to restart a server that is running
//! fine, or to debug a query against a server that is not there at all.
//!
//! These need no live service — the fake is the service — so they are not
//! `#[ignore]`d and run in the fast lane with everything else.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;
use std::time::Duration;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_redis::streams::FleetStreams;

#[path = "support/fake_redis.rs"]
mod fake_redis;
#[path = "support/recorder.rs"]
mod recorder;

use self::fake_redis::{FakeRedis, Reply, install_subscriber};
use self::recorder::Recorder;

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);

/// Points a configuration at the fake, with a deadline a test can wait out.
fn config_for(server: &FakeRedis) -> RedisConfig {
    RedisConfig::from_url(RedisRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2))
}

/// Checks one API-role connection emitted one correlated start/failure pair.
fn assert_connection_pair(recorder: &Recorder) {
    let events: Vec<_> = recorder
        .events()
        .into_iter()
        .filter(|record| record.fields.get("role").is_some_and(|role| role == "api"))
        .collect();
    assert_eq!(
        events.len(),
        2,
        "one connection must emit one pair: {events:?}"
    );
    let started = events.first().expect("the pair has a started event");
    let failed = events.last().expect("the pair has a failed event");
    assert_eq!(started.level, tracing::Level::INFO);
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(
        started.fields.get("event").map(String::as_str),
        Some("redis_connect_started")
    );
    assert_eq!(
        failed.fields.get("event").map(String::as_str),
        Some("redis_connect_failed")
    );
    assert_eq!(
        started.fields.get("attempt_id"),
        failed.fields.get("attempt_id")
    );
    assert!(failed.fields.contains_key("error_code"));
}

/// A socket that accepts the client and never answers the liveness probe is
/// bounded by the connection budget itself. The elapsed deadline is this
/// crate's fact, so it has no invented driver source.
#[tokio::test]
async fn test_redis_connect_honours_its_deadline() {
    let recorder = Recorder::install();
    let server = FakeRedis::spawn(&[("PING", Reply::Silent)]).await;
    let budget = Duration::from_millis(100);
    let config = RedisConfig::from_url(RedisRole::Api, server.url())
        .with_request_timeout(Duration::from_secs(2))
        .with_connect_timeout(budget);

    let started = std::time::Instant::now();
    let error = Redis::connect(&config)
        .await
        .expect_err("a silent liveness probe must time out");

    assert!(
        error.is_unavailable(),
        "a connect timeout is an outage: {error}"
    );
    assert!(
        error.to_string().contains("100ms"),
        "the failure must carry its configured budget: {error}"
    );
    assert!(
        error.source().is_none(),
        "an elapsed deadline has no lower-level cause"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the configured deadline must bound the whole connection"
    );

    assert_connection_pair(&recorder);
}

/// A `PING` answered with anything but `PONG` is an unexpected reply, not a
/// successful connection.
///
/// `ConnectionManager::new` returning is not proof that Redis serves — a TCP
/// handshake with a process that is listening proves only that something
/// accepted the socket. `connect` pings for exactly this reason, and this is
/// the case where the ping comes back and is still wrong: boot must refuse,
/// rather than hand out a connection whose first real command is the one that
/// discovers the problem.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_ping_that_is_not_pong_refuses_the_connection() {
    install_subscriber();
    let server = FakeRedis::spawn(&[("PING", Reply::Raw("+WRONG\r\n"))]).await;

    let error = tokio::time::timeout(BUDGET, Redis::connect(&config_for(&server)))
        .await
        .expect("a server that answers must not hang the connect")
        .expect_err("a reply that is not PONG must refuse the connection");

    assert!(
        error.is_command(),
        "a wrong answer is the server being wrong, not unreachable: {error}"
    );
    assert!(
        error.to_string().contains("PING"),
        "the failure must name the command that produced it: {error}"
    );
}

/// An `XADD` that answers with an empty id is an unexpected reply.
///
/// The id is the caller's handle to the event it just wrote. An empty one
/// parses as a perfectly good `String`, so nothing upstream would reject it —
/// it would travel as an [`afd_redis::streams::EventId`] and fail later, at an
/// `XACK` that cannot say what it is acknowledging. Refusing it here is what
/// keeps that from becoming a debugging session.
#[tokio::test(flavor = "multi_thread")]
async fn test_an_empty_xadd_id_is_refused_rather_than_handed_out() {
    install_subscriber();
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        // An empty bulk string: well-formed RESP carrying an id that is not one.
        ("XADD", Reply::Raw("$0\r\n\r\n")),
    ])
    .await;

    let redis = tokio::time::timeout(BUDGET, Redis::connect(&config_for(&server)))
        .await
        .expect("the fake answers PING, so connect must not hang")
        .expect("a fake that answers PONG must be accepted");

    let error = tokio::time::timeout(
        BUDGET,
        FleetStreams::new(redis).append("fleet-1", &[("kind", "created")]),
    )
    .await
    .expect("the fake answers, so the append must not hang")
    .expect_err("an empty id must be refused");

    assert!(
        error.is_command(),
        "an empty id is a reply shape, not an outage: {error}"
    );
    assert!(
        error.to_string().contains("XADD"),
        "the failure must name the command that produced it: {error}"
    );
}

/// A socket that dies mid-command is Redis being unreachable, not a command
/// that failed.
///
/// This is the classification the two halves of `error::classify` exist to
/// separate, and the one a live server cannot produce on demand. An operator
/// reading "unavailable" goes and looks at Redis; one reading a command error
/// goes and looks at the query, which here would be the wrong place entirely.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_socket_that_dies_mid_command_reports_redis_unreachable() {
    install_subscriber();
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        // Accepted, never answered, socket closed: the shape of a server that
        // restarts with a command already in flight.
        ("XADD", Reply::Hangup),
    ])
    .await;

    let redis = tokio::time::timeout(BUDGET, Redis::connect(&config_for(&server)))
        .await
        .expect("the fake answers PING, so connect must not hang")
        .expect("a fake that answers PONG must be accepted");

    let error = tokio::time::timeout(
        BUDGET,
        FleetStreams::new(redis).append("fleet-1", &[("kind", "created")]),
    )
    .await
    .expect("a dropped socket must fail the command, not hang it")
    .expect_err("a dropped socket must fail the command");

    assert!(
        error.is_unavailable(),
        "a dead socket is an outage, not a bad command: {error}"
    );
}
