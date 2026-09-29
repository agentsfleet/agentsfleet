//! A consumer-group create the server refuses for a reason other than the
//! group already existing.
//!
//! `BUSYGROUP` is the steady state a create treats as success. Every other
//! refusal is a real failure, and swallowing one as "already there" would
//! leave a fleet with no group and every later read answering `NOGROUP`. A
//! real Dragonfly refuses a malformed start id this way, so the fake stands in
//! for it and the suite needs no live service.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::{EventId, FleetStreams, GroupCursor};

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);

/// `TYPE` for a key that does not exist yet, so the create goes ahead.
const NO_KEY: &str = "+none\r\n";

/// The refusal a server gives a create whose start id will not parse.
const BAD_START: &str = "-ERR Invalid stream ID specified as stream command argument\r\n";

/// `BUSYGROUP`, the one refusal a create answers as success.
const ALREADY_THERE: &str = "-BUSYGROUP Consumer Group name already exists\r\n";

/// Restores a fleet's group against a fake whose `XGROUP` answers `reply`,
/// answering the outcome and every command the client sent.
async fn restore_against(reply: &'static str) -> (afd_dragonfly::error::Result<()>, Vec<String>) {
    install_subscriber();
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("TYPE", Reply::Raw(NO_KEY)),
        ("XGROUP", Reply::Raw(reply)),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2));
    let redis = tokio::time::timeout(BUDGET, Dragonfly::connect(&config))
        .await
        .expect("the fake answers PING, so connect must not hang")
        .expect("a fake that answers PONG must be accepted");
    let cursor = GroupCursor::After(EventId::of("not-an-id"));
    let outcome = tokio::time::timeout(
        BUDGET,
        FleetStreams::new(redis).restore_group("fleet-1", &cursor),
    )
    .await
    .expect("the fake answers, so the restore must not hang");
    (outcome, server.seen())
}

#[tokio::test]
async fn a_create_refused_for_any_reason_but_busygroup_is_reported() {
    let (outcome, seen) = restore_against(BAD_START).await;

    let refused = outcome.expect_err("a refused create must not read as a group in place");
    assert!(
        refused.is_command(),
        "the server refused the command: {refused}"
    );
    assert!(
        !refused.is_group_exists(),
        "only BUSYGROUP means already there"
    );
    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED,
        "{refused}"
    );
    assert!(
        seen.iter().any(|command| command.starts_with("XGROUP")),
        "the create must have reached the server: {seen:?}"
    );
}

#[tokio::test]
async fn a_create_answered_busygroup_is_success() {
    let (outcome, _seen) = restore_against(ALREADY_THERE).await;

    outcome.expect("BUSYGROUP means the group is already in place");
}
