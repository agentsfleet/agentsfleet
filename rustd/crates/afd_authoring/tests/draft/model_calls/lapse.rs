//! Dimension 4.8: a locking validity check reads the clock once its lock is
//! held, so a row that lapses while the check waits on the lock is refused.
//!
//! Each case lets the row lapse while another transaction only holds its lock
//! and writes nothing. A holder that wrote the row would make Postgres re-check
//! the predicate after the wait and hide the defect this guards against: a
//! deadline compared with a clock read before the wait.

use super::super::{attempt::Attempt, fixture::Fixture, fixture::now, mint};
use super::fixture::{self, Provider};
use afd_authoring::types::Source;
use afd_core::{error_code, id::Uuid7};
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Renew, Report};
use sqlx::{Connection as _, Postgres, Transaction};
use std::{sync::Arc, time::Duration};

/// How long the row has left when its lock is taken: long enough for the
/// check to be queued behind the lock before the row lapses.
const LAPSES_IN: Duration = Duration::from_millis(500);

/// Moves the attempt's lease deadline to `$2` past database time.
const LEASE_LAPSES: &str = "UPDATE authoring.attempts \
SET lease_expires_at = date_trunc('milliseconds', clock_timestamp()) + $2::interval \
WHERE id = $1::uuid";

/// Closes the draft with its retention deadline `$2` past database time.
const DRAFT_LAPSES: &str = "UPDATE authoring.drafts \
SET closed_at = now(), expires_at = date_trunc('milliseconds', clock_timestamp()) + $2::interval \
WHERE id = $1::uuid";

/// Takes the attempt row's lock and writes nothing.
const LOCK_ATTEMPT: &str =
    "SELECT pg_backend_pid() FROM authoring.attempts WHERE id = $1::uuid FOR UPDATE";

/// Takes the draft row's lock and writes nothing.
const LOCK_DRAFT: &str =
    "SELECT pg_backend_pid() FROM authoring.drafts WHERE id = $1::uuid FOR UPDATE";

/// Whether the attempt's lease is still live on database time.
const LEASE_LIVE: &str =
    "SELECT lease_expires_at > clock_timestamp() FROM authoring.attempts WHERE id = $1::uuid";

/// Whether the draft's retention deadline is still ahead on database time.
const DRAFT_LIVE: &str =
    "SELECT expires_at > clock_timestamp() FROM authoring.drafts WHERE id = $1::uuid";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// The answer receipts a report left for the attempt.
const ANSWERS: &str = "SELECT count(*) FROM authoring.answer_receipts WHERE attempt_id = $1::uuid";

/// One row whose deadline the case moves, and the statements that judge it.
struct Row<'a> {
    id: &'a Uuid7,
    lock: &'static str,
    live: &'static str,
}

/// Holds `row`'s lock while the check queues behind it, past the row's
/// deadline, then lets go without writing.
///
/// Asserts the check was queued while the row was still live, so a pass means
/// the row lapsed during the wait and not before the check began.
async fn hold_past_deadline(f: &Fixture, held: Transaction<'_, Postgres>, pid: i32, row: &Row<'_>) {
    let mut probe = f.connection().await;
    loop {
        let waiting: bool = sqlx::query_scalar(BLOCKING)
            .bind(pid)
            .fetch_one(&mut *probe)
            .await
            .expect("inspect the lock queue");
        if waiting {
            break;
        }
        tokio::task::yield_now().await;
    }
    let live: bool = sqlx::query_scalar(row.live)
        .bind(row.id.as_str())
        .fetch_one(&mut *probe)
        .await
        .expect("read the deadline");
    assert!(live, "the check must queue before the row lapses");
    tokio::time::sleep(LAPSES_IN).await;
    let live: bool = sqlx::query_scalar(row.live)
        .bind(row.id.as_str())
        .fetch_one(&mut *probe)
        .await
        .expect("read the deadline");
    assert!(!live, "the row lapses while its lock is held");
    held.rollback()
        .await
        .expect("the holder lets go without writing");
}

/// Commits `row`'s deadline [`LAPSES_IN`] ahead with `deadline`, then takes
/// its lock on `held`, writing nothing there; answers the holder's backend.
async fn lapse_and_lock(
    f: &Fixture,
    held: &mut Transaction<'_, Postgres>,
    deadline: &'static str,
    row: &Row<'_>,
) -> i32 {
    sqlx::query(deadline)
        .bind(row.id.as_str())
        .bind(LAPSES_IN)
        .execute(&mut *f.connection().await)
        .await
        .expect("move the deadline");
    sqlx::query_scalar(row.lock)
        .bind(row.id.as_str())
        .fetch_one(&mut **held)
        .await
        .expect("take the row lock")
}

fn renewal() -> Renew {
    Renew {
        fence: "1".into(),
        usage: meter(0),
    }
}

fn meter(runtime_ms: u64) -> Meter {
    Meter {
        knowledge: Knowledge::Known,
        calls: 0,
        input_tokens: Some(0),
        output_tokens: Some(0),
        runtime_ms,
        provider_cost_nanos: None,
    }
}

fn terminal() -> Report {
    Report::Terminal {
        accounting: Accounting {
            fence: "1".into(),
            report_id: mint().to_string(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: meter(1),
        },
        answer: Some("An answer the lapsed lease may not publish.\n".into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres"]
async fn test_expiry_checks_read_clock_after_lock() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect("known physical receipt");
    let lease = Row {
        id: &a.id,
        lock: LOCK_ATTEMPT,
        live: LEASE_LIVE,
    };
    let mut held = f.connection().await;

    // Lease renew.
    let mut tx = held.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&f, &mut tx, LEASE_LAPSES, &lease).await;
    let (renewed, ()) = tokio::join!(
        f.drafts.renew(&f.runner, &a.lease, renewal()),
        hold_past_deadline(&f, tx, pid, &lease)
    );
    assert_eq!(
        renewed
            .expect_err("a lease that lapsed in the wait is not renewed")
            .code(),
        error_code::AUTHORING_FENCE
    );

    // Report fence: the accounting settles, the answer is not published.
    let mut tx = held.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&f, &mut tx, LEASE_LAPSES, &lease).await;
    let (reported, ()) = tokio::join!(
        f.drafts.report(&f.runner, &a.lease, terminal()),
        hold_past_deadline(&f, tx, pid, &lease)
    );
    assert!(reported.expect("the accounting settles").settled);
    let answers: i64 = sqlx::query_scalar(ANSWERS)
        .bind(a.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("count answer receipts");
    assert_eq!(
        answers, 0,
        "a lease that lapsed in the wait publishes no answer"
    );

    // Draft read under lock.
    let draft = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("idle draft")
        .draft;
    let retained = Row {
        id: &draft.id,
        lock: LOCK_DRAFT,
        live: DRAFT_LIVE,
    };
    let mut tx = held.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&f, &mut tx, DRAFT_LAPSES, &retained).await;
    let (deleted, ()) = tokio::join!(
        f.drafts.delete(f.access(), &draft.id),
        hold_past_deadline(&f, tx, pid, &retained)
    );
    assert_eq!(
        deleted
            .expect_err("a draft past retention reads as gone")
            .code(),
        error_code::AUTHORING_NOT_FOUND
    );
    drop(held);
    f.cleanup().await;
}
