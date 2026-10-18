//! The lease verbs a run calls mid-flight, against live datastores: its
//! standing, an interim message, its tool-call records and the write fence a
//! schedule write proves itself with. Each reads the lease through
//! `live_lease.sql` on the database's clock, so a lapsed or superseded holder
//! is refused the same way by all four.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_db::Precondition as _;
use afd_fleet::lease::write_fence::WriteFence;
use afd_wire::message_verb::{MESSAGES_PER_RUN_MAX, MessageRequest};
use afd_wire::tool_detail::ToolCallRecordsRequest;

use crate::integration_report_owes_destination::stage::stage;
use crate::report_seed::held;
use crate::rows::{LAPSE, Row};

/// The thread a message-capable event was asked from.
const THREAD: &str = "C0FIXTURE/1700000000.000100";

/// One finished call's record, as a runner posts it.
const RECORD: &str = r#"{"call_number":1,"arguments":{},"truncated_arguments":false,"output":"ok","output_line_count":1,"truncated":false}"#;

/// The runner's own live lease answers its standing; another token, another
/// runner and a lapsed lease are each refused.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_standing_answers_only_the_live_holder() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease = held.issued.lease_id.clone();
    let token = held.fence.as_u64();

    let standing = plane
        .standing(&held.runner, lease.clone(), token)
        .await
        .expect("the live holder stands");
    let stale = plane.standing(&held.runner, lease.clone(), token + 1).await;
    let foreign = plane.standing(&held.spare, lease.clone(), token).await;
    held.fixtures
        .age(Row::Lease(lease.as_str()), "lease_expires_at", LAPSE)
        .await;
    let lapsed = plane.standing(&held.runner, lease, token).await;

    assert_eq!(standing.fleet_id.as_str(), held.fleet);
    assert_eq!(standing.event_id, held.event_id);
    let code = |answer: afd_fleet::Result<_>| answer.err().map(|error| error.code());
    assert_eq!(code(stale), Some(afd_fleet::Error::stale_fence().code()));
    let not_found = Some(afd_fleet::Error::lease_not_found().code());
    assert_eq!(
        code(foreign),
        not_found,
        "another runner's lease reads as none"
    );
    assert_eq!(code(lapsed), not_found, "a lapsed lease reads as none");
    held.fixtures.cleanup().await;
}

/// A run asked from a thread may speak into it, and each message counts; a
/// run with nowhere to speak is refused before anything is counted.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_messages_count_against_the_live_holder() {
    let stage = stage().await;
    let reply = Reply::To {
        connector: Provider::Slack.id(),
        address: THREAD,
    };
    stage.admit(Producer::Webhook, "speaks", reply).await;
    let (lease_id, lease) = stage.lease_next(stage.fixtures.enrolled_at()).await;
    let lease_id = Uuid7::parse(&lease_id).expect("a lease id is a v7 spelling");
    let plane = stage.fixtures.plane();
    let say = |text: &'static str| MessageRequest {
        fencing_token: lease.fence.as_u64(),
        text: Cow::Borrowed(text),
    };

    let first = plane
        .message(&stage.runner, lease_id.clone(), &say("working on it"))
        .await
        .expect("the live holder speaks into its thread");
    let second = plane
        .message(&stage.runner, lease_id, &say("nearly there"))
        .await
        .expect("and again");

    assert_eq!(
        (first.part, second.part),
        (1, 2),
        "each message takes the next line"
    );
    assert_eq!(first.destination, THREAD);
    let silent = held().await;
    let refused = silent
        .fixtures
        .plane()
        .message(
            &silent.runner,
            silent.issued.lease_id.clone(),
            &MessageRequest {
                fencing_token: silent.fence.as_u64(),
                text: Cow::Borrowed("hello?"),
            },
        )
        .await
        .err()
        .map(|error| error.code());
    assert_eq!(refused, Some(afd_core::error_code::MESSAGE_NO_CHANNEL));
    silent.fixtures.cleanup().await;
    stage.fixtures.cleanup().await;
}

/// A run speaks at most [`MESSAGES_PER_RUN_MAX`] times; the next message is
/// refused as over the cap, not as a lost lease.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_run_is_refused_past_its_message_cap() {
    let stage = stage().await;
    let reply = Reply::To {
        connector: Provider::Slack.id(),
        address: THREAD,
    };
    stage.admit(Producer::Webhook, "chatty", reply).await;
    let (lease_id, lease) = stage.lease_next(stage.fixtures.enrolled_at()).await;
    let lease_id = Uuid7::parse(&lease_id).expect("a lease id is a v7 spelling");
    let plane = stage.fixtures.plane();
    let request = MessageRequest {
        fencing_token: lease.fence.as_u64(),
        text: Cow::Borrowed("still going"),
    };

    for part in 1..=MESSAGES_PER_RUN_MAX {
        let said = plane
            .message(&stage.runner, lease_id.clone(), &request)
            .await
            .expect("a message under the cap is said");
        assert_eq!(said.part, part);
    }
    let over = plane.message(&stage.runner, lease_id, &request).await;

    assert_eq!(
        over.err().map(|error| error.code()),
        Some(afd_core::error_code::MESSAGE_LIMIT_REACHED)
    );
    stage.fixtures.cleanup().await;
}

/// A record posted under the live fence is kept; the same post under a token
/// the fleet has moved past is refused.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_tool_call_records_keep_only_under_the_live_fence() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease = held.issued.lease_id.as_str();
    let post = |token: u64| format!(r#"{{"fencing_token":{token},"calls":[{RECORD}]}}"#);
    let (live, stale) = (post(held.fence.as_u64()), post(held.fence.as_u64() + 1));
    let live: ToolCallRecordsRequest<'_> = serde_json::from_str(&live).expect("a well-formed post");
    let stale: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&stale).expect("a well-formed post");

    let kept = plane
        .record_tool_calls(&held.runner, lease, &live, held.now)
        .await
        .expect("the live holder's record is kept");
    let refused = plane
        .record_tool_calls(&held.runner, lease, &stale, held.now)
        .await
        .err()
        .map(|error| error.code());

    assert_eq!(kept.stored_count, 1);
    assert_eq!(refused, Some(afd_fleet::Error::stale_fence().code()));
    held.fixtures.cleanup().await;
}

/// The write fence holds for the live holder's own token and for no other.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_write_fence_holds_only_for_the_live_token() {
    let held = held().await;
    let token = held.fence.as_u64();
    let fence =
        |presented| WriteFence::new(held.runner.clone(), held.issued.lease_id.clone(), presented);
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");

    let live = fence(token)
        .holds(&mut connection)
        .await
        .expect("the fence reads");
    let stale = fence(token + 1)
        .holds(&mut connection)
        .await
        .expect("the fence reads");
    drop(connection);

    assert!(live, "the live holder's token passes");
    assert!(!stale, "a token the fleet never issued does not");
    held.fixtures.cleanup().await;
}
