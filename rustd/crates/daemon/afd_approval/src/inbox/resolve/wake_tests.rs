//! A lost notification is logged after the durable gate answer.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_core::error_code;
use af_core::test_util::trace::Capture;
use af_crypto::entropy::Entropy;
use afd_admission::Admissions;

use crate::inbox::Inbox;

/// The fleet the wake names; nothing answers, so any valid identifier will do.
const FLEET: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeee4";

/// The decision is already durable, so the lost notification is a warning whose
/// `reason` is the failure's sentence and causes, never its code again.
#[tokio::test]
async fn a_failed_wake_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let database = afd_db::test_util::unreachable_db();
    let bus = afd_bus::test_util::unreachable();
    let inbox = Inbox::new(
        database.clone(),
        bus.clone(),
        Admissions::new(database, bus, Entropy::new()),
    );

    inbox
        .wake_parked_delivery(&af_core::id::Uuid7::parse(FLEET).expect("canonical fleet"))
        .await;

    let logged = capture.only("admission_wake_ring_failed");
    assert_eq!(
        logged.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(logged.field("fleet_id"), Some(FLEET));
    logged.assert_uncoded_reason();
}
