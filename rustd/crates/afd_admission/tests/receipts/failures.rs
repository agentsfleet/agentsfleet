//! Append failures, lost readiness marks and competing receipt writes.

use afd_core::error_code;

use super::{
    lane::Lane,
    queue::{admit_against, replay_against},
};
use crate::fake_redis::Reply;

/// The entry id the fake's `XADD` hands back.
const FAKE_RECEIPT: &str = "1790000000000-0";

/// An `XADD` that appends.
const APPENDS: Reply = Reply::Bulk(FAKE_RECEIPT);

/// The refusal a Dragonfly out of memory gives a write.
const OUT_OF_MEMORY: &str = "-OOM command not allowed when used memory > 'maxmemory'\r\n";

/// A hash write the index refuses.
const HSET_REFUSED: &str = "-ERR the index refused the write\r\n";

/// The receipt the stand-in sweeper records before the admission can.
const SWEPT_RECEIPT: &str = "1790000000000-9";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_full_queue_defers_the_admission_and_names_the_limit() {
    let lane = Lane::seed().await;

    let run = admit_against(&lane, &[("XADD", Reply::Raw(OUT_OF_MEMORY))]).await;

    let admitted = run
        .outcome
        .as_ref()
        .expect("a full queue defers; it does not refuse");
    assert!(!admitted.replayed, "the row this call inserted is fresh");
    assert_eq!(
        lane.receipt().await,
        None,
        "nothing was appended, so the sweeper still owes the entry"
    );
    let logged = run
        .event("admission_queue_full")
        .expect("a full queue is named as such, not as an outage");
    assert_eq!(
        logged.get("error_code").map(String::as_str),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert!(
        logged
            .get("reason")
            .is_some_and(|reason| !reason.contains("[UZ-")),
        "a deferred append's reason does not repeat its code: {logged:?}"
    );
    assert!(
        !run.seen.iter().any(|command| command.starts_with("HSET")),
        "a deferred admission marks nothing: {:?}",
        run.seen
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_ready_mark_still_admits_and_is_logged() {
    let lane = Lane::seed().await;

    let run = admit_against(
        &lane,
        &[("XADD", APPENDS), ("HSET", Reply::Raw(HSET_REFUSED))],
    )
    .await;

    let admitted = run
        .outcome
        .as_ref()
        .expect("the entry is durable, so a lost mark is not a failure");
    assert!(!admitted.replayed);
    assert_eq!(lane.receipt().await.as_deref(), Some(FAKE_RECEIPT));
    let logged = run
        .event("admission_ready_mark_failed")
        .expect("a mark that did not land is said out loud");
    assert_eq!(
        logged.get("error_code").map(String::as_str),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(
        logged.get("cause_error_code").map(String::as_str),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str()),
        "the refused write's own code rides `cause_error_code`: {logged:?}"
    );
    assert!(
        logged
            .get("reason")
            .is_some_and(|reason| reason.contains("HSET")),
        "the line names the write the index refused: {logged:?}"
    );
    assert!(
        logged
            .get("reason")
            .is_some_and(|reason| !reason.contains("[UZ-")),
        "a lost mark's reason does not repeat its code: {logged:?}"
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_receipt_the_sweeper_recorded_first_stands_and_is_logged() {
    let lane = Lane::seed().await;
    let sweeper = lane.sweep_first(SWEPT_RECEIPT).await;

    let run = admit_against(&lane, &[("XADD", APPENDS)]).await;
    drop(sweeper);

    let admitted = run
        .outcome
        .as_ref()
        .expect("a lost race for the receipt is not a failure");
    assert!(!admitted.replayed);
    assert_eq!(
        lane.receipt().await.as_deref(),
        Some(SWEPT_RECEIPT),
        "the receipt recorded first stands; this call's entry is the extra one"
    );
    let logged = run
        .event("admission_receipt_superseded")
        .expect("the extra entry is said out loud");
    assert_eq!(
        logged.get("receipt").map(String::as_str),
        Some(FAKE_RECEIPT),
        "the line names the entry that lost"
    );
    lane.cleanup().await;
}

/// A replay the queue will not take keeps the row owed, and its line says why
/// without repeating the code the line already carries.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_failed_replay_reason_carries_no_code_prefix() {
    let lane = Lane::seed().await;
    let deferred = admit_against(&lane, &[("XADD", Reply::Raw(OUT_OF_MEMORY))]).await;
    deferred
        .outcome
        .as_ref()
        .expect("a full queue defers; it does not refuse");

    let events = replay_against(&lane, &[("XADD", Reply::Raw(OUT_OF_MEMORY))]).await;

    let logged = events
        .iter()
        .find(|fields| {
            fields.get("event").map(String::as_str) == Some("admission_replay_queue_full")
        })
        .expect("a full queue is named as such on a replay too");
    assert_eq!(
        logged.get("error_code").map(String::as_str),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert!(
        logged
            .get("reason")
            .is_some_and(|reason| !reason.contains("[UZ-")),
        "a failed replay's reason does not repeat its code: {logged:?}"
    );
    assert_eq!(lane.receipt().await, None, "the row is still owed");
    lane.cleanup().await;
}
