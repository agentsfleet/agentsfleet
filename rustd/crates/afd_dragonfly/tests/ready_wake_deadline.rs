//! The doorbell's two commands against a node that never answers.
//!
//! The ring and the rung peek exist to make a poll faster, so each gives up at
//! `READY_WAKE_DEADLINE` rather than the datastore's request timeout. A
//! refused port fails at once and proves nothing about the deadline; a node
//! that takes the command and says nothing is the case it exists for, and only
//! the fake can hold one still.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_core::test_util::trace::Capture;
use afd_core::timing::READY_WAKE_DEADLINE;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::ready::{Partition, ReadyIndex};
use afd_observability::metrics::label::fleet::ReadyWake;
use afd_observability::test_util::Capture as Metrics;

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);
/// The datastore's own wait, far past the doorbell's: a command that took
/// this long was cut off by the request timeout, not the deadline.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
/// Well over the deadline, well under the request timeout.
const GAVE_UP_WITHIN: Duration = Duration::from_secs(1);
const WAKE_FAILURES: &str = "agentsfleet_fleet_ready_wake_failures_total";
const KIND: &str = "kind";
const FLEET: &str = "0192f6a0-0000-7000-8000-000000000001";

async fn index_over(server: &FakeRedis) -> ReadyIndex {
    install_subscriber();
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(REQUEST_TIMEOUT);
    let redis = tokio::time::timeout(BUDGET, Dragonfly::connect(&config))
        .await
        .expect("the fake answers PING, so connect must not hang")
        .expect("a fake that answers PONG must be accepted");
    ReadyIndex::new(redis)
}

/// A mark whose ring is never answered still returns, inside the deadline's
/// reach rather than the request timeout's, with the ring reported once.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_ring_nobody_answers_gives_up_at_the_deadline() {
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("HSET", Reply::Raw(":1\r\n")),
        ("SPUBLISH", Reply::Silent),
    ])
    .await;
    let index = index_over(&server).await;
    let capture = Capture::install();
    let metrics = Metrics::install();
    let rings = metrics.sum(WAKE_FAILURES, &[(KIND, ReadyWake::Ring.as_str())]);

    let started = Instant::now();
    index
        .mark(FLEET)
        .await
        .expect("the mark itself was written");

    assert!(
        started.elapsed() < GAVE_UP_WITHIN,
        "the ring waited {:?}, past the {READY_WAKE_DEADLINE:?} deadline",
        started.elapsed()
    );
    let line = capture.only("ready_wake_ring_failed");
    let partition = Partition::of(FLEET).index().to_string();
    assert_eq!(line.field("partition"), Some(partition.as_str()));
    assert!(
        metrics.sum(WAKE_FAILURES, &[(KIND, ReadyWake::Ring.as_str())]) > rings,
        "the lost ring is counted under kind=ring"
    );
}

/// A rung peek the node never answers fails at the deadline, named, so a slow
/// node costs a poll its rung peek and not a request timeout.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_rung_peek_nobody_answers_gives_up_at_the_deadline() {
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("HRANDFIELD", Reply::Silent),
    ])
    .await;
    let index = index_over(&server).await;

    let started = Instant::now();
    let error = index
        .peek_rung(Partition::of(FLEET), 1)
        .await
        .expect_err("a node that never answers fails the peek");

    assert!(
        started.elapsed() < GAVE_UP_WITHIN,
        "the deadline cut it off"
    );
    let said = error.to_string();
    assert!(
        said.contains("HRANDFIELD") && said.contains(&format!("{READY_WAKE_DEADLINE:?}")),
        "the timeout names its command and the deadline: {said}"
    );
}
