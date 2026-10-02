#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::report::FailureClass;

use super::{Delivery, ReportSpool};
use crate::client::Verb;
use crate::error;
use crate::report::{Ending, report};
use crate::storage_home::StorageHome;
use crate::test_support::{Answer, FLEET_ID, LEASE_ID, drain, json, lease, plane};

fn spooled_report(home: &StorageHome) -> ReportSpool {
    let spool = ReportSpool::new(home);
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Failed {
        class: FailureClass::RunnerCrash,
        detail: "killed",
    };
    spool
        .hold(
            &Uuid7::parse(LEASE_ID).unwrap(),
            &report(&lease, &ending, Duration::ZERO),
        )
        .unwrap();
    spool
}

#[tokio::test]
async fn test_spooled_report_replays_once() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    // The process that wrote this is gone before it posted anything.
    drop(spooled_report(&home));
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"ok": true})));

    let after_restart = ReportSpool::new(&home);
    for spooled in after_restart.pending().unwrap() {
        assert_eq!(spooled.deliver(&plane).await.unwrap(), Delivery::Accepted);
    }
    for spooled in after_restart.pending().unwrap() {
        spooled.deliver(&plane).await.unwrap();
    }

    let posted = drain(&mut calls);
    assert_eq!(posted.len(), 1, "posted once, and only once");
    assert_eq!(posted[0].verb, Verb::Report);
    let body: serde_json::Value = serde_json::from_slice(posted[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["lease_id"], LEASE_ID);
    assert_eq!(body["failure_reason"], "runner_crash");
}

#[tokio::test]
async fn a_report_the_daemon_refuses_for_good_is_not_kept() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = spooled_report(&home);
    let (plane, _calls) = plane(|_call| Answer::Fail(error::refused(Verb::Report, 409, None)));

    let delivered = spool
        .pending()
        .unwrap()
        .remove(0)
        .deliver(&plane)
        .await
        .unwrap();

    assert_eq!(delivered, Delivery::Refused);
    assert!(spool.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_report_the_daemon_cannot_take_yet_stays_spooled() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = spooled_report(&home);
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let (plane, _calls) = plane(move |_call| {
        counted.fetch_add(1, Ordering::SeqCst);
        Answer::Fail(error::unavailable(Verb::Report, 503))
    });

    let kept = spool
        .pending()
        .unwrap()
        .remove(0)
        .deliver(&plane)
        .await
        .unwrap_err();

    assert!(kept.is_retryable());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(spool.pending().unwrap().len(), 1);
}

#[test]
fn only_reports_are_pending() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    fs::write(home.spool().join("notes.txt"), b"operator").unwrap();

    assert!(ReportSpool::new(&home).pending().unwrap().is_empty());
}

#[test]
fn a_spool_that_is_gone_refuses_to_hold() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = ReportSpool::new(&home);
    fs::remove_dir(home.spool()).unwrap();
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Failed {
        class: FailureClass::RunnerCrash,
        detail: "x",
    };

    let held = spool.hold(
        &Uuid7::parse(LEASE_ID).unwrap(),
        &report(&lease, &ending, Duration::ZERO),
    );

    assert!(held.is_err());
    assert!(spool.pending().is_err());
}
