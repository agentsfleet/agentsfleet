//! What a wake says when the readiness index will not take the mark.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_admission::Admissions;
use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

use super::EVENT_WAKE_READY_MARK_FAILED;
use crate::inbox::Inbox;

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The fleet and gate the wake names; nothing answers, so any will do.
const FLEET: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeee4";

/// A queue that refuses every command.
fn dead_queue() -> Dragonfly {
    Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail")
}

/// The decision is already durable, so the lost mark is a warning whose
/// `reason` is the failure's sentence and causes, never its code again.
#[tokio::test]
async fn a_failed_wake_mark_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let database = afd_db::test_util::unreachable_db();
    let inbox = Inbox::new(
        database.clone(),
        dead_queue(),
        Admissions::new(database, dead_queue(), Entropy::new()),
    );

    inbox.wake_parked_delivery(FLEET, FLEET).await;

    let logged = capture.only(EVENT_WAKE_READY_MARK_FAILED);
    assert_eq!(
        logged.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(logged.field("fleet_id"), Some(FLEET));
    let reason = logged.field("reason").expect("a lost mark says why");
    assert!(!reason.is_empty(), "the failure line says nothing");
    assert!(!reason.contains("[UZ-"), "the code is repeated: {reason}");
    assert!(!reason.contains('\n'), "a backtrace rode along: {reason}");
}
