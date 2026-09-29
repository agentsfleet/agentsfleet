//! What an admission does once its row is committed and the queue, the ledger
//! or the readiness index then answers wrongly.
//!
//! The row is the record: every case below still answers the producer with its
//! fresh event, and the difference is what the ledger and the log say
//! afterwards. The queue is a fake — a real Dragonfly never runs out of memory
//! or refuses a hash write on demand — and Postgres is the lane's.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "../../afd_dragonfly/tests/support/subscriber.rs"]
mod subscriber;

#[path = "../../afd_dragonfly/tests/support/fake_redis.rs"]
#[allow(
    unused_imports,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::format_push_string,
    reason = "test support shared with `afd_dragonfly`, which uses the half this suite does not"
)]
mod fake_redis;

#[path = "receipt_faults/lane.rs"]
mod lane;

use afd_core::error_code;

use self::fake_redis::Reply;
use self::lane::{Lane, admit_against};

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
    assert!(
        logged
            .get("reason")
            .is_some_and(|reason| reason.contains("HSET")),
        "the line names the write the index refused: {logged:?}"
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
