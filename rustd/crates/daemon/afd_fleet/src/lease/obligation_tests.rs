//! The outbound workers' wake, over a bus that refuses and one that delivers.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::time::Duration;

use af_core::error_code;
use af_core::test_util::trace::Capture;
use af_crypto::entropy::Entropy;
use afd_bus::{Received, Subject};

use super::EVENT_OUTBOUND_WAKE_LOST;
use crate::lease::store::Leases;
use crate::lease::test_dead;

/// How long a published wake is given to reach its reader.
const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// A refused wake is logged once, coded as the bus's failure, and the call
/// still returns: the committed row waits for the next claim pass.
#[tokio::test]
async fn a_refused_wake_is_logged_with_the_bus_code() {
    let log = Capture::install();
    test_dead::leases().wake_delivery(&test_dead::id(1)).await;

    let line = log.only(EVENT_OUTBOUND_WAKE_LOST);
    assert_eq!(
        line.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(line.field("reason"), Some("backend"));
}

/// A wake the bus takes names the owed row on the outbound subject, and
/// logs nothing.
#[tokio::test]
async fn a_wake_names_the_owed_row_on_the_outbound_subject() {
    let log = Capture::install();
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    let mut workers = bus
        .subscribe(&Subject::OutboundWake)
        .await
        .expect("the in-process bus subscribes");
    let owed = test_dead::id(2);

    Leases::new(test_dead::database(), bus, Entropy::new())
        .wake_delivery(&owed)
        .await;

    let received = tokio::time::timeout(DELIVERY_BUDGET, workers.recv())
        .await
        .expect("the wake arrives within budget")
        .expect("the subscription stays live");
    let message = match received {
        Received::Message(message) => Some(message),
        Received::Lagged(_) | Received::Gap => None,
    }
    .expect("the wake is a message, not a lag or gap notice");
    assert_eq!(message.payload.as_str(), owed.as_str());
    assert!(
        log.events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_OUTBOUND_WAKE_LOST)),
        "a wake the bus took is not logged as lost"
    );
}
