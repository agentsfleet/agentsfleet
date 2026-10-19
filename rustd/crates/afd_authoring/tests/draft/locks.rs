//! Lock choreography for interleaving proofs: wait until raced work queues
//! behind a holder, and file a receipt another claim would file.

use super::{fixture::Fixture, mint};
use afd_core::id::Uuid7;
use sqlx::pool::PoolConnection;
use std::time::Duration;

/// How often [`Fixture::waits_on`] reads the lock queue.
const POLL: Duration = Duration::from_millis(10);

/// How many reads [`Fixture::waits_on`] makes before failing the proof: ten
/// seconds at [`POLL`], past any queueing a loaded lane takes.
const POLLS: u32 = 1_000;

impl Fixture {
    /// Waits until `waiting` backends are blocked behind the session `holder`,
    /// directly or behind another waiter, so a proof releases its lock only
    /// once the work it races is queued on it.
    pub(super) async fn waits_on(&self, holder: i32, waiting: i64) {
        const BLOCKED: &str = "WITH RECURSIVE queued(pid) AS (SELECT $1::int UNION \
             SELECT a.pid FROM pg_stat_activity a JOIN queued q \
             ON q.pid = ANY(pg_blocking_pids(a.pid))) SELECT count(*) - 1 FROM queued";
        for _ in 0..POLLS {
            let blocked: i64 = sqlx::query_scalar(BLOCKED)
                .bind(holder)
                .fetch_one(&mut *self.connection().await)
                .await
                .expect("read lock waits");
            if blocked >= waiting {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        unreachable_wait(holder, waiting);
    }

    /// Files, uncommitted on the returned session, the receipt another claim
    /// under `scope` and `operation` would file, dated past its replay window so
    /// it never reads back as a replay. Answers the session and its backend;
    /// [`commit_receipt`] lands it once the raced request is queued behind it.
    pub(super) async fn foreign_receipt(
        &self,
        scope: &str,
        operation: &Uuid7,
    ) -> (PoolConnection<sqlx::Postgres>, i32) {
        const RECEIPT: &str = "INSERT INTO authoring.replay_receipts \
             (id,workspace_id,user_id,scope,operation_id,request_digest,resource_id,created_at) \
             VALUES($1::uuid,$2::uuid,$3::uuid,$4,$5::uuid,'authoring:v1:sha256:foreign', \
             $1::uuid,now()-interval '2 days')";
        let mut holder = self.connection().await;
        sqlx::query("BEGIN")
            .execute(&mut *holder)
            .await
            .expect("holder transaction");
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *holder)
            .await
            .expect("holder session");
        sqlx::query(RECEIPT)
            .bind(mint().as_str())
            .bind(self.workspace.as_str())
            .bind(self.owner.as_str())
            .bind(scope)
            .bind(operation.as_str())
            .execute(&mut *holder)
            .await
            .expect("another claim's receipt");
        (holder, pid)
    }
}

#[expect(
    clippy::panic,
    reason = "work that never queues on the lock fails the proof"
)]
fn unreachable_wait(holder: i32, waiting: i64) -> ! {
    panic!("{waiting} backends never queued behind session {holder}")
}

/// Commits the receipt [`Fixture::foreign_receipt`] held.
pub(super) async fn commit_receipt(mut holder: PoolConnection<sqlx::Postgres>) {
    sqlx::query("COMMIT")
        .execute(&mut *holder)
        .await
        .expect("the other claim commits");
}
