//! A publish that fails while the client stays connected is paid at once,
//! with no reconnect to trigger it.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use af_core::test_util::trace::Capture;

use super::{
    BOUND, GAP_ALL, TAIL, TestResult, client_still_dialling, client_that_has_given_up, named,
};
use crate::nats::{Tuning, debt::Owed, hub::Channels, signals};

/// Wait until `capture` holds `count` events named `name`.
async fn until_named(capture: &Capture, name: &str, count: usize) -> TestResult {
    tokio::time::timeout(BOUND, async {
        while named(capture, name) < count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_publish_that_fails_while_connected_is_paid_without_a_reconnect() -> TestResult {
    let (client, everyone) = client_still_dialling().await?;
    let signals = Arc::new(signals::Signals::new(&Tuning::default()));
    let channels = Arc::new(Channels::new(8));
    let capture = Capture::install();
    let watcher = tokio::spawn(signals::watch(
        client,
        Arc::clone(&channels),
        Arc::clone(&signals),
        everyone,
        GAP_ALL.to_owned(),
    ));

    // A publish that timed out on a full client queue: no disconnect follows.
    signals.record(TAIL, false, Instant::now());
    until_named(&capture, "bus_gap_marked", 1).await?;
    watcher.abort();
    assert!(watcher.await.is_err_and(|stopped| stopped.is_cancelled()));

    assert_eq!(capture.only("bus_gap_marked").field("markers"), Some("1"));
    assert_eq!(signals.take_debt(), Owed::default(), "the marker was paid");
    assert_eq!(
        named(&capture, "bus_gap_sent"),
        0,
        "no local reader is gapped"
    );
    assert!(!signals.blocked(), "later publishes still flow");
    Ok(())
}

#[tokio::test]
async fn a_marker_that_fails_while_connected_is_retried_a_deadline_later() -> TestResult {
    let unpayable = client_that_has_given_up().await?;
    let (_dialling, everyone) = client_still_dialling().await?;
    let signals = Arc::new(signals::Signals::new(&Tuning {
        publish_deadline_ms: 10,
        ..Tuning::default()
    }));
    let capture = Capture::install();
    let watcher = tokio::spawn(signals::watch(
        unpayable,
        Arc::new(Channels::new(8)),
        Arc::clone(&signals),
        everyone,
        GAP_ALL.to_owned(),
    ));

    signals.record(TAIL, false, Instant::now());
    // The first payment, then one retry a deadline after it.
    until_named(&capture, "bus_gap_marks_owed", 2).await?;
    watcher.abort();
    assert!(watcher.await.is_err_and(|stopped| stopped.is_cancelled()));

    assert!(
        !signals.blocked(),
        "an unpaid marker never blocks a live client"
    );
    assert_eq!(signals.take_debt().subjects, [TAIL], "still owed");
    Ok(())
}
