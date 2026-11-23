//! What a turn says when neither Postgres nor the bus answers.
//!
//! No datastore, deliberately: the pool points at a reserved port and the
//! bus is one no broker answers for. One turn walks two
//! failure arms in order — the deployment ceiling the pool would not count,
//! and the row the pool would not commit — and each says why in a `reason` free of the registry code, which
//! the refusal line carries in its own field, and of a captured backtrace.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_wire::turn::TurnKind;
use afd_turns::{IncomingTurn, Key, Producer, Reply, Turns};

/// The fleet and workspace the turn names; nothing answers, so any will do.
const FLEET: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeee2";

/// The `reason` an event carried, proven free of a code prefix and of a
/// backtrace, which begins on a line of its own.
fn told_reason(capture: &Capture, event: &str) -> String {
    let logged = capture.only(event);
    assert!(
        logged
            .field("error_code")
            .is_some_and(|code| !code.is_empty()),
        "{event} names no error_code"
    );
    logged.assert_uncoded_reason();
    logged.field("reason").unwrap_or_default().to_owned()
}

#[tokio::test]
async fn a_refused_turn_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let turns = Turns::for_tests(
        afd_db::test_util::unreachable_db(),
        afd_bus::test_util::unreachable(),
    );
    let fleet = Uuid7::parse(FLEET).expect("a canonical fleet identifier");

    let refused = turns
        .admit(IncomingTurn {
            producer: Producer::ScheduleFire,
            key: Key::Repeated("log-reasons"),
            fleet: &fleet,
            workspace: &fleet,
            actor: "fixture:log-reasons",
            event_type: TurnKind::Chat,
            request_json: "{}",
            reply: Reply::None,
        })
        .await
        .expect_err("a pool that will not answer commits nothing");

    told_reason(&capture, "turn_ceiling_unread");
    told_reason(&capture, "turn_refused");
    assert_eq!(
        capture.only("turn_refused").field("error_code"),
        Some(refused.code().as_str())
    );
}

/// A frame for a fleet named by no canonical identifier is dropped before
/// the bus, and the drop says why under the internal code.
#[tokio::test]
async fn a_frame_for_an_unparsed_fleet_is_dropped_and_logged() {
    use std::borrow::Cow;

    use afd_api_wire::tail::TailFrame;

    let capture = Capture::install();
    let steer = afd_turns::view::Steer::new(Turns::for_tests(
        afd_db::test_util::unreachable_db(),
        afd_bus::test_util::unreachable(),
    ));
    let frame = TailFrame::TurnAdmitted {
        turn_id: Cow::Borrowed(FLEET),
        actor: Cow::Borrowed("steer:fixture"),
        event_type: Cow::Borrowed(TurnKind::Chat.as_str()),
        message: Cow::Borrowed("check the deploy"),
        message_trimmed: false,
        created_at: jiff::Timestamp::UNIX_EPOCH,
    };

    steer.announce("not-a-fleet-id", &frame).await;

    let dropped = capture.only("tail_frame_dropped");
    assert_eq!(dropped.field("reason"), Some("invalid_fleet_identifier"));
    assert_eq!(
        dropped.field("error_code"),
        Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
}
