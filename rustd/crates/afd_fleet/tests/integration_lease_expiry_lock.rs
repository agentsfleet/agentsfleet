//! Dimension 4.8 for the lease verbs: a lease that lapses while a verb waits on
//! its row lock no longer holds once the verb has the lock.
//!
//! Three verbs read `runner.leases` under a lock: the write fence a schedule
//! write proves itself with (`FOR SHARE`), the tool-call records
//! (`FOR NO KEY UPDATE`), and the interim message count (an `UPDATE`). The
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

use std::borrow::Cow;
use std::time::Duration;

use afd_admission::{Producer, Reply};
use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_db::Precondition as _;
use afd_fleet::lease::write_fence::WriteFence;
use afd_wire::message_verb::MessageRequest;
use afd_wire::tool_detail::ToolCallRecordsRequest;
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};

use crate::integration_report_owes_destination::stage::stage;
use crate::report_seed::held;
use crate::support::Fixtures;

/// How long the lease has left when the row lock is taken: long enough for
/// the verb to queue behind the lock before the lease lapses.
const LAPSES_IN: Duration = Duration::from_millis(500);

/// Moves the lease's deadline to `$2` past database time.
const LEASE_LAPSES: &str = "UPDATE runner.leases \
SET lease_expires_at = date_trunc('milliseconds', clock_timestamp()) + $2::interval \
WHERE id = $1::uuid";

/// Takes the lease row's lock and writes nothing.
const LOCK_LEASE: &str =
    "SELECT pg_backend_pid() FROM runner.leases WHERE id = $1::uuid FOR UPDATE";

/// Whether the lease is still live on database time.
const LEASE_LIVE: &str =
    "SELECT lease_expires_at > clock_timestamp() FROM runner.leases WHERE id = $1::uuid";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// One finished call's record, as a runner posts it.
const RECORD: &str = r#"{"call_number":1,"arguments":{},"truncated_arguments":false,"output":"ok","output_line_count":1,"truncated":false}"#;

/// The thread a message-capable event was asked from.
const THREAD: &str = "C0FIXTURE/1700000000.000200";

/// Commits a deadline on `lease` lapsing in [`LAPSES_IN`], then takes the
/// row's lock on `held`; answers the holder's backend.
async fn lapse_and_lock(
    fixtures: &Fixtures,
    held: &mut Transaction<'_, Postgres>,
    lease: &str,
) -> i32 {
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query(LEASE_LAPSES)
        .bind(lease)
        .bind(LAPSES_IN)
        .execute(&mut *connection)
        .await
        .expect("the lease's deadline moves");
    sqlx::query_scalar(LOCK_LEASE)
        .bind(lease)
        .fetch_one(&mut **held)
        .await
        .expect("take the lease's row lock")
}

/// Whether `lease` is live on database time.
async fn live(probe: &mut PgConnection, lease: &str) -> bool {
    sqlx::query_scalar(LEASE_LIVE)
        .bind(lease)
        .fetch_one(probe)
        .await
        .expect("read the lease")
}

/// Holds the row lock while the verb queues behind it, past the lease's
/// deadline, then lets go without writing.
///
/// Asserts the verb queued while the lease was still live, so a pass means it
/// lapsed during the wait and not before the verb began.
async fn hold_past_deadline(
    fixtures: &Fixtures,
    held: Transaction<'_, Postgres>,
    pid: i32,
    lease: &str,
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
        live(&mut probe, lease).await,
        "the verb must queue before the lease lapses"
    );
    tokio::time::sleep(LAPSES_IN).await;
    assert!(
        !live(&mut probe, lease).await,
        "the lease lapses while the row lock is held"
    );
    held.rollback()
        .await
        .expect("the holder lets go without writing");
}

/// The write fence, proved on its own transaction behind the held lock.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_write_fence_refuses_a_lease_lapsed_in_the_wait() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();
    let fence = WriteFence::new(
        held.runner.clone(),
        held.issued.lease_id.clone(),
        held.fence.as_u64(),
    );
    let mut holder = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut locked = holder.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&held.fixtures, &mut locked, lease).await;
    let mut writer = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut write = writer.begin().await.expect("write transaction");

    let (holds, ()) = tokio::join!(
        fence.holds(&mut write),
        hold_past_deadline(&held.fixtures, locked, pid, lease)
    );
    write.rollback().await.expect("the write lets go");

    assert!(
        !holds.expect("the fence reads"),
        "a lease that lapsed in the wait no longer fences a write"
    );
    held.fixtures.cleanup().await;
}

/// The tool-call records, posted behind the held lock.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_tool_call_records_refuse_a_lease_lapsed_in_the_wait() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();
    let post = format!(
        r#"{{"fencing_token":{},"calls":[{RECORD}]}}"#,
        held.fence.as_u64()
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
    let pid = lapse_and_lock(&held.fixtures, &mut locked, lease).await;

    let (kept, ()) = tokio::join!(
        plane.record_tool_calls(&held.runner, lease, &post, held.now),
        hold_past_deadline(&held.fixtures, locked, pid, lease)
    );

    assert_eq!(
        kept.err().map(|error| error.code()),
        Some(afd_fleet::Error::lease_not_found().code()),
        "a lease that lapsed in the wait keeps no record"
    );
    held.fixtures.cleanup().await;
}

/// The interim message count, taken behind the held lock after the unlocked
/// standing read has already passed.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_message_count_refuses_a_lease_lapsed_in_the_wait() {
    let stage = stage().await;
    let reply = Reply::To {
        connector: Provider::Slack.id(),
        address: THREAD,
    };
    stage.admit(Producer::Webhook, "lapses", reply).await;
    let (lease, issued) = stage.lease_next(stage.fixtures.enrolled_at()).await;
    let lease_id = Uuid7::parse(&lease).expect("a lease id is a v7 spelling");
    let request = MessageRequest {
        fencing_token: issued.fence.as_u64(),
        text: Cow::Borrowed("still here?"),
    };
    let plane = stage.fixtures.plane();
    let mut holder = stage
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let mut locked = holder.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&stage.fixtures, &mut locked, &lease).await;

    let (said, ()) = tokio::join!(
        plane.message(&stage.runner, lease_id, &request),
        hold_past_deadline(&stage.fixtures, locked, pid, &lease)
    );

    assert_eq!(
        said.err().map(|error| error.code()),
        Some(afd_fleet::Error::lease_not_found().code()),
        "a lease that lapsed in the wait takes no message slot"
    );
    stage.fixtures.cleanup().await;
}
