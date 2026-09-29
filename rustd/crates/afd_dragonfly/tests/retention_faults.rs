//! What the trim does when `XPENDING` answers, but answers wrongly.
//!
//! The floor is the least of what the group has delivered and what it still
//! holds, so the pending summary is the one reply the trim cannot guess at: a
//! summary misread as "nothing pending" moves the floor past entries a
//! consumer still owes, and `XTRIM MINID` deletes them. A real Dragonfly never
//! sends either reply below, so the fake is the only way in. Each must be
//! refused before the trim reads its window or cuts anything.
//!
//! No live service — the fake is the service — so these run in the fast lane.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::FleetStreams;

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);

/// A stream length past the history bound plus its slack, so the trim goes
/// on to ask what the group owes instead of returning early.
const PAST_THE_SLACK: &str = ":1200\r\n";

/// The fleet group, three entries pending, delivered up to `1000-0`.
const ONE_GROUP: &str = "*1\r\n%4\r\n\
    $4\r\nname\r\n$11\r\nfleet_lease\r\n\
    $7\r\npending\r\n:3\r\n\
    $9\r\nconsumers\r\n:1\r\n\
    $17\r\nlast-delivered-id\r\n$6\r\n1000-0\r\n";

/// A summary that counts three pending entries and names none of them.
const COUNT_WITHOUT_OLDEST: &str = "*4\r\n:3\r\n$-1\r\n$-1\r\n$-1\r\n";

/// A summary of two fields where the command answers four.
const SHORT_SUMMARY: &str = "*2\r\n:3\r\n$6\r\n1000-0\r\n";

/// The commands that read the window and cut the stream, which a refused
/// summary must never reach.
const AFTER_THE_SUMMARY: [&str; 2] = ["XRANGE", "XTRIM"];

/// Trims one fleet against a fake whose `XPENDING` answers `summary`, and
/// answers the refusal plus every command the client sent.
async fn trim_against(summary: &'static str) -> (afd_dragonfly::Error, Vec<String>) {
    install_subscriber();
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("XLEN", Reply::Raw(PAST_THE_SLACK)),
        ("XINFO", Reply::Raw(ONE_GROUP)),
        ("XPENDING", Reply::Raw(summary)),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2));
    let redis = tokio::time::timeout(BUDGET, Dragonfly::connect(&config))
        .await
        .expect("the fake answers PING, so connect must not hang")
        .expect("a fake that answers PONG must be accepted");

    let refused = tokio::time::timeout(BUDGET, FleetStreams::new(redis).trim("fleet-1"))
        .await
        .expect("the fake answers, so the trim must not hang")
        .expect_err("a pending summary the trim cannot read must be refused");
    (refused, server.seen())
}

/// Asserts the trim stopped at the summary: nothing read past it, nothing cut.
fn assert_stopped_at_the_summary(seen: &[String]) {
    assert!(
        seen.iter().any(|command| command.starts_with("XPENDING")),
        "the trim must have asked for the summary: {seen:?}"
    );
    for after in AFTER_THE_SUMMARY {
        assert!(
            !seen.iter().any(|command| command.starts_with(after)),
            "{after} ran after a refused summary: {seen:?}"
        );
    }
}

/// A count with no oldest id is refused as a reply this client does not
/// understand, the way the driver's own decoder refuses it. Read as "nothing
/// pending", it would put the floor at the last delivered id, above the three
/// entries the summary says are still held.
#[tokio::test(flavor = "multi_thread")]
async fn a_pending_count_with_no_oldest_id_is_refused_and_nothing_is_trimmed() {
    let (refused, seen) = trim_against(COUNT_WITHOUT_OLDEST).await;
    assert!(
        refused.is_command(),
        "a summary with no oldest id is a reply shape, not an outage: {refused}"
    );
    assert!(
        refused.to_string().contains("XPENDING"),
        "the refusal must name the command that answered it: {refused}"
    );
    assert_stopped_at_the_summary(&seen);
}

/// A summary of any other shape fails the decode and is refused the same way.
#[tokio::test(flavor = "multi_thread")]
async fn a_pending_summary_of_another_shape_is_refused_and_nothing_is_trimmed() {
    let (refused, seen) = trim_against(SHORT_SUMMARY).await;
    assert!(
        refused.is_command(),
        "a summary of the wrong shape is a reply shape, not an outage: {refused}"
    );
    assert_stopped_at_the_summary(&seen);
}
