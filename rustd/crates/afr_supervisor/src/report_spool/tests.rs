#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::time::Duration;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::report::FailureClass;
use bytes::Bytes;

use super::{Delivery, ReportSpool};
use crate::client::Verb;
use crate::error;
use crate::report::{Ending, report};
use crate::storage_home::StorageHome;
use crate::test_support::{Answer, FLEET_ID, LEASE_ID, drain, json, lease, plane};

/// A crashed run's report, encoded as the lease loop encodes it.
fn report_bytes() -> Bytes {
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Failed {
        class: FailureClass::RunnerCrash,
        detail: "killed",
    };
    Bytes::from(serde_json::to_vec(&report(&lease, &ending, Duration::ZERO)).unwrap())
}

fn spooled(home: &StorageHome) -> ReportSpool {
    let spool = ReportSpool::new(home);
    spool
        .hold(&Uuid7::parse(LEASE_ID).unwrap(), report_bytes())
        .unwrap();
    spool
}

fn home() -> (tempfile::TempDir, StorageHome) {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    (root, home)
}

/// What one delivery against a daemon answering `answer` leaves in the spool.
async fn deliver_once(answer: fn() -> Answer) -> (Delivery, usize, StorageHome, tempfile::TempDir) {
    let (root, home) = home();
    let spool = spooled(&home);
    let (plane, _calls) = plane(move |_call| answer());
    let delivery = spool.pending().unwrap()[0].deliver(&plane).await.unwrap();
    let left = spool.pending().unwrap().len();
    (delivery, left, home, root)
}

#[tokio::test]
async fn test_spooled_report_replays_once() {
    let (_root, home) = home();
    // The process that wrote this is gone before it posted anything.
    drop(spooled(&home));
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"ok": true})));

    let after_restart = ReportSpool::new(&home);
    for spooled in after_restart.pending().unwrap() {
        assert!(matches!(
            spooled.deliver(&plane).await.unwrap(),
            Delivery::Settled
        ));
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
async fn an_answer_that_settles_the_lease_removes_the_entry() {
    let settling: [fn() -> Answer; 3] = [
        || {
            Answer::Fail(error::refused(
                Verb::Report,
                409,
                Some(error_code::RUN_STALE_FENCING_TOKEN),
            ))
        },
        || {
            Answer::Fail(error::refused(
                Verb::Report,
                404,
                Some(error_code::RUN_LEASE_NOT_FOUND),
            ))
        },
        || {
            Answer::Fail(error::refused(
                Verb::Report,
                409,
                Some(error_code::RUN_LEASE_LOST),
            ))
        },
    ];
    for answer in settling {
        let (delivery, left, ..) = deliver_once(answer).await;

        assert!(matches!(delivery, Delivery::Settled));
        assert_eq!(left, 0);
    }
}

#[tokio::test]
async fn an_answer_a_later_attempt_could_change_keeps_the_entry() {
    let keeping: [fn() -> Answer; 7] = [
        || Answer::Fail(error::unavailable(Verb::Report, 503)),
        || Answer::Fail(error::unavailable(Verb::Report, 429)),
        || Answer::Fail(error::refused(Verb::Report, 401, None)),
        || Answer::Fail(error::refused(Verb::Report, 403, None)),
        || Answer::Fail(error::refused(Verb::Report, 408, None)),
        || Answer::Fail(error::refused(Verb::Report, 413, None)),
        || Answer::Fail(error::refused(Verb::Report, 429, None)),
    ];
    for answer in keeping {
        let (delivery, left, ..) = deliver_once(answer).await;

        assert!(matches!(delivery, Delivery::Kept(_)), "{delivery:?}");
        assert_eq!(left, 1);
    }
}

#[tokio::test]
async fn a_report_the_daemon_cannot_read_is_set_aside_not_retried() {
    let (delivery, left, home, _root) = deliver_once(|| {
        Answer::Fail(error::refused(
            Verb::Report,
            400,
            Some(error_code::INVALID_REQUEST),
        ))
    })
    .await;

    assert!(matches!(delivery, Delivery::Rejected));
    assert_eq!(left, 0, "no longer pending");
    let set_aside = home.spool().join(LEASE_ID).with_extension("rejected");
    assert!(set_aside.exists(), "kept for an operator");
}

#[tokio::test]
async fn an_entry_another_delivery_already_removed_still_settles() {
    let (_root, home) = home();
    let spool = spooled(&home);
    let (plane, _calls) = plane(|_call| json(&serde_json::json!({"ok": true})));
    let entry = spool.pending().unwrap().remove(0);
    let twin = entry.clone();

    assert!(matches!(
        twin.deliver(&plane).await.unwrap(),
        Delivery::Settled
    ));
    assert!(matches!(
        entry.deliver(&plane).await.unwrap(),
        Delivery::Settled
    ));
}

#[tokio::test]
async fn an_entry_that_cannot_be_removed_is_an_error() {
    let (_root, home) = home();
    let spool = spooled(&home);
    let entry = spool.pending().unwrap().remove(0);
    fs::remove_file(home.spool().join(LEASE_ID).with_extension("json")).unwrap();
    fs::create_dir(home.spool().join(LEASE_ID).with_extension("json")).unwrap();
    let (plane, _calls) = plane(|_call| json(&serde_json::json!({"ok": true})));

    assert!(entry.deliver(&plane).await.is_err());
}

#[test]
fn only_reports_are_pending() {
    let (_root, home) = home();
    fs::write(home.spool().join("notes.txt"), b"operator").unwrap();

    assert!(ReportSpool::new(&home).pending().unwrap().is_empty());
}

#[test]
fn a_spool_that_is_gone_refuses_to_hold() {
    let (_root, home) = home();
    let spool = ReportSpool::new(&home);
    fs::remove_dir(home.spool()).unwrap();

    assert!(
        spool
            .hold(&Uuid7::parse(LEASE_ID).unwrap(), report_bytes())
            .is_err()
    );
    assert!(spool.pending().is_err());
}
