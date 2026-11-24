//! For the run verbs: a run that lapses while a verb waits on
//! its row lock no longer holds once the verb has the lock.
//!
//! Two verbs read `thread.runs` under a lock: the write fence a schedule
//! write proves itself with (`FOR SHARE`) and the tool-call records
//! (`FOR NO KEY UPDATE`). The interim message reads its run unlocked and
//! writes nothing to it, so it has no wait to lapse in. The
//! holder here only locks the row and writes nothing. A holder that wrote it
//! would make Postgres re-check the predicate after the wait and hide the
//! defect this guards against: a deadline compared with a clock read before
//! the wait.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_core::id::Uuid7;
use af_wire::tool_detail::ToolCallRecordsRequest;
use afd_db::Precondition as _;
use afd_fleet::run::write_fence::WriteFence;
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};

use crate::report_seed::held;
use crate::support::Fixtures;

/// How long the run has left when the row lock is taken: long enough for
/// the verb to queue behind the lock before the run lapses.
const LAPSES_IN: Duration = Duration::from_millis(500);

/// Moves the run's deadline to `$2` past database time.
const RUN_LAPSES: &str = "UPDATE thread.runs \
SET run_expires_at = date_trunc('milliseconds', clock_timestamp()) + $2::interval \
WHERE id = $1";

/// Takes the run row's lock and writes nothing.
const LOCK_RUN: &str = "SELECT pg_backend_pid() FROM thread.runs WHERE id = $1 FOR UPDATE";

/// Whether the run is still live on database time.
const RUN_LIVE: &str = "SELECT run_expires_at > clock_timestamp() FROM thread.runs WHERE id = $1";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// One finished call's record of `turn`, as a runner posts it.
fn record(turn: &str) -> String {
    format!(
        r#"{{"call_number":1,"arguments":{{}},"truncated_arguments":false,"output":"ok","output_line_count":1,"truncated":false,"turn_id":"{turn}","kind":"tool_call","tool":"shell","status":"succeeded"}}"#
    )
}

/// Commits a deadline on `run` lapsing in [`LAPSES_IN`], then takes the
/// row's lock on `held`; answers the holder's backend.
async fn lapse_and_lock(
    fixtures: &Fixtures,
    held: &mut Transaction<'_, Postgres>,
    run: &Uuid7,
) -> i32 {
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query(RUN_LAPSES)
        .bind(run)
        .bind(LAPSES_IN)
        .execute(&mut *connection)
        .await
        .expect("the run's deadline moves");
    sqlx::query_scalar(LOCK_RUN)
        .bind(run)
        .fetch_one(&mut **held)
        .await
        .expect("take the run's row lock")
}

/// Whether `run` is live on database time.
async fn live(probe: &mut PgConnection, run: &Uuid7) -> bool {
    sqlx::query_scalar(RUN_LIVE)
        .bind(run)
        .fetch_one(probe)
        .await
        .expect("read the run")
}

/// Holds the row lock while the verb queues behind it, past the run's
/// deadline, then lets go without writing.
///
/// Asserts the verb queued while the run was still live, so a pass means it
/// lapsed during the wait and not before the verb began.
async fn hold_past_deadline(
    fixtures: &Fixtures,
    held: Transaction<'_, Postgres>,
    pid: i32,
    run: &Uuid7,
) {
    let mut probe = fixtures.database.acquire().await.expect("a connection");
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
    assert!(
        live(&mut probe, run).await,
        "the verb must queue before the run lapses"
    );
    tokio::time::sleep(LAPSES_IN).await;
    assert!(
        !live(&mut probe, run).await,
        "the run lapses while the row lock is held"
    );
    held.rollback()
        .await
        .expect("the holder lets go without writing");
}

/// The write fence, proved on its own transaction behind the held lock.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_write_fence_refuses_a_run_lapsed_in_the_wait() {
    let held = held().await;
    let run = &held.issued.run_id;
    let fence = WriteFence::new(
        held.runner.clone(),
        held.issued.run_id.clone(),
        held.fence.as_u64(),
    );
    let mut holder = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut locked = holder.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&held.fixtures, &mut locked, run).await;
    let mut writer = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut write = writer.begin().await.expect("write transaction");

    let (holds, ()) = tokio::join!(
        fence.holds(&mut write),
        hold_past_deadline(&held.fixtures, locked, pid, run)
    );
    write.rollback().await.expect("the write lets go");

    assert!(
        !holds.expect("the fence reads"),
        "a run that lapsed in the wait no longer fences a write"
    );
    held.fixtures.cleanup().await;
}

/// The tool-call records, posted behind the held lock.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_tool_call_records_refuse_a_run_lapsed_in_the_wait() {
    let held = held().await;
    let run = &held.issued.run_id;
    let post = format!(
        r#"{{"fencing_token":{},"calls":[{}]}}"#,
        held.fence.as_u64(),
        record(held.turn_id.as_str())
    );
    let post: ToolCallRecordsRequest<'_> = serde_json::from_str(&post).expect("a well-formed post");
    let plane = held.fixtures.plane();
    let mut holder = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut locked = holder.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&held.fixtures, &mut locked, run).await;

    let (kept, ()) = tokio::join!(
        plane.record_tool_calls(&held.runner, run.as_str(), &post, held.now),
        hold_past_deadline(&held.fixtures, locked, pid, run)
    );

    assert_eq!(
        kept.err().map(|error| error.code()),
        Some(afd_fleet::Error::run_not_found().code()),
        "a run that lapsed in the wait keeps no record"
    );
    held.fixtures.cleanup().await;
}

/// Two writers of one run still take turns: a tool-call post, which locks the run
/// itself, waits for a renewal holding it and gives up at the login's lock bound.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_tool_record_post_still_waits_on_a_renewal() {
    let held = held().await;
    let run = &held.issued.run_id;
    let post = format!(
        r#"{{"fencing_token":{},"calls":[{}]}}"#,
        held.fence.as_u64(),
        record(held.turn_id.as_str())
    );
    let post: ToolCallRecordsRequest<'_> = serde_json::from_str(&post).expect("a well-formed post");
    let mut renewer = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut renewing = renewer.begin().await.expect("the renewal's transaction");
    let cancelled: Option<bool> = sqlx::query_scalar(afd_fleet::run::sql::shared::LOCK_CANCELLED)
        .bind(run)
        .bind(&held.runner)
        .fetch_optional(&mut *renewing)
        .await
        .expect("the renewal locks its run");
    assert_eq!(cancelled, Some(false), "the renewal holds a live run");

    let kept = held
        .fixtures
        .plane()
        .record_tool_calls(&held.runner, run.as_str(), &post, held.now)
        .await;

    assert_eq!(
        kept.err().map(|error| error.code()),
        Some(af_core::error_code::INTERNAL_DB_UNAVAILABLE),
        "the post waited for the renewal until the lock bound"
    );
    renewing.rollback().await.expect("the renewal lets go");
    held.fixtures.cleanup().await;
}
