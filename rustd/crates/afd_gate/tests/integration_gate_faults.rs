//! The gate failures that need one datastore answering and the other gone.
//!
//! A recorded gate whose decision cannot be read needs a live queue to hold
//! the reference and a dead pool behind the mirror; a paused fleet whose
//! readiness mark cannot be cleared needs a live pool to take the pause and a
//! dead queue to refuse the clear. Each line says why in a `reason` free of
//! the registry code and of a captured backtrace.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/gate_fixture.rs"]
mod fixture;

use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_gate::gate::{Check, GateRef, Gates, Trigger, Verdict, Waiting};

use self::fixture::{Fixture, NOW, config, config_gates, connect_redis};

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The timeout the hand-recorded reference is kept alive for.
const REFERENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_hours(1);

/// A rule that stops the fleet on every chat.
const KILL_EVERY_CHAT: &str =
    r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}]}"#;

/// A recorded gate's decision that could not be read.
const EVENT_DECISION_UNREADABLE: &str = "gate_decision_read_failed";

/// A paused fleet that kept its readiness mark.
const EVENT_READY_STALE: &str = "gate_ready_clear_failed";

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
    let reason = logged
        .field("reason")
        .expect("a failure line says why")
        .to_owned();
    assert!(!reason.is_empty(), "{event} says nothing");
    assert!(
        !reason.contains("[UZ-"),
        "{event} repeats its code: {reason}"
    );
    assert!(
        !reason.contains('\n'),
        "{event} carries a backtrace: {reason}"
    );
    reason
}

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn an_unreadable_gate_decision_reason_carries_no_code_prefix() {
    let gates = Gates::new(
        afd_db::test_util::unreachable_db(),
        connect_redis().await,
        Entropy::new(),
    );
    let fleet = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let action = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let event_id = "event-decision-unreadable";
    gates
        .record(&fleet, event_id, &GateRef::new(action), REFERENCE_TIMEOUT)
        .await
        .expect("the live queue holds the reference");
    let config = config(false);

    let capture = Capture::install();
    let verdict = gates
        .check(
            Check {
                fleet_id: &fleet,
                workspace_id: &fleet,
                event_id,
                event_type: "chat",
                actor: "user:fixture",
                request_json: "{}",
                config: &config,
            },
            NOW,
        )
        .await;

    assert_eq!(
        verdict,
        Verdict::Await(Waiting::Unreadable),
        "a decision nobody could read waits rather than denying an approval"
    );
    told_reason(&capture, EVENT_DECISION_UNREADABLE);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stale_ready_mark_reason_carries_no_code_prefix() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let dead_queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    let gates = Gates::new(fixture.database.clone(), dead_queue, Entropy::new());
    let policy_kill = config_gates(KILL_EVERY_CHAT);

    let capture = Capture::install();
    let verdict = gates
        .check(fixture.check("event-ready-stale", &policy_kill), NOW)
        .await;

    assert_eq!(verdict, Verdict::Killed(Trigger::Policy));
    told_reason(&capture, EVENT_READY_STALE);
    drop(capture);
    fixture.cleanup().await;
}
