//! What an admission says when neither datastore answers.
//!
//! No datastore, deliberately: the pool points at a reserved port and the
//! queue at a loopback port nobody listens on. One admission walks three
//! failure arms in order — the fleet backlog the queue would not report, the
//! deployment ceiling the pool would not count, and the row the pool would not
//! commit — and each says why in a `reason` free of the registry code, which
//! the refusal line carries in its own field, and of a captured backtrace.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_core::test_util::trace::Capture;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_wire::event::EventType;

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The fleet and workspace the admission names; nothing answers, so any will do.
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
async fn a_refused_admission_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    let admissions = Admissions::for_tests(afd_db::test_util::unreachable_db(), queue);

    let refused = admissions
        .admit(Admission {
            producer: Producer::RepairVerification,
            key: Key::Repeated("log-reasons"),
            fleet: FLEET,
            workspace: FLEET,
            actor: "fixture:log-reasons",
            event_type: EventType::Chat,
            request_json: "{}",
            reply: Reply::None,
        })
        .await
        .expect_err("a pool that will not answer commits nothing");

    told_reason(&capture, "admission_backlog_unread");
    told_reason(&capture, "admission_ceiling_unread");
    told_reason(&capture, "admission_failed");
    assert_eq!(
        capture.only("admission_failed").field("error_code"),
        Some(refused.code().as_str())
    );
}
